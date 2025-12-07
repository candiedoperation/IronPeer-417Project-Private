mod download;
mod peer;
mod protocol;
mod protodef;
mod storage;
mod structs;
mod tracker;

use crate::peer::handshake::Handshake;
use crate::protodef::meta_info::MetaInfo;
use crate::tracker::announce::{AnnounceEvent, ClientStats, TrackerClient};
use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static DEBUG: AtomicBool = AtomicBool::new(false);

pub fn is_debug() -> bool {
    DEBUG.load(Ordering::Relaxed)
}

#[derive(Parser, Debug)]
#[command(name = "IronPeer")]
#[command(about = "A BitTorrent client written in Rust", long_about = None)]
struct Args {
    /// Path to the .torrent file
    #[arg(short, long)]
    torrent: Option<String>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

fn main() {
    let args = Args::parse();
    
    DEBUG.store(args.debug, Ordering::Relaxed);
    
    let torrent_path = args.torrent.unwrap_or_else(|| {
        "/Users/atheesh/Downloads/raspios-2025-12-04-trixie-arm64.img.xz.torrent".to_string()
    });
    
    let torrent_path = PathBuf::from(torrent_path);
    let output_dir = PathBuf::from("downloads");
    const TARGET_PEER_COUNT: usize = 50;

    match MetaInfo::from_file(&torrent_path) {
        Ok(meta_info) => {
            println!("Successfully parsed torrent file!");
            println!("Torrent name: {}", meta_info.info.name);
            
            let torrent_info = meta_info.to_torrent_info();
            println!("Total size: {} bytes", torrent_info.total_size);
            
            let file_manager = crate::storage::files::FileManager::new(output_dir, &torrent_info);
            let mut download_manager = crate::download::manager::DownloadManager::new(&torrent_info, *TrackerClient::with_default_peer_id(6881).peer_id());
            
            // Verify existing data on disk
            download_manager.verify_existing_pieces(&file_manager);
            
            // Tracker announce
            let tracker_client = TrackerClient::new(download_manager.peer_id, 6881);
            let udp_client = crate::tracker::udp::UdpTrackerClient::new().ok();
            let stats = ClientStats {
                uploaded: 0,
                downloaded: 0,
                left: torrent_info.total_size,
            };
            
            let tracker_urls = torrent_info.all_tracker_urls();
            let mut all_peers = Vec::new();
            
            for tracker_url in &tracker_urls {
                println!("Announcing to: {}", tracker_url);
                
                if tracker_url.starts_with("udp://") {
                    // Use UDP tracker client
                    if let Some(ref udp) = udp_client {
                        match udp.announce(
                            tracker_url,
                            &torrent_info.info_hash,
                            &download_manager.peer_id,
                            0, // downloaded
                            torrent_info.total_size, // left
                            0, // uploaded
                            6881, // port
                        ) {
                            Ok(new_peers) => {
                                println!("Found {} peers from {}", new_peers.len(), tracker_url);
                                all_peers.extend(new_peers);
                                if all_peers.len() >= 50 { break; }
                            }
                            Err(e) => {
                                if is_debug() {
                                    eprintln!("UDP tracker error: {}", e);
                                }
                            }
                        }
                    }
                } else {
                    // Use HTTP tracker client
                    if let Ok(response) = tracker_client.announce(tracker_url, &torrent_info.info_hash, &stats, AnnounceEvent::Started) {
                        let new_peers = response.parse_peers();
                        println!("Found {} peers from {}", new_peers.len(), tracker_url);
                        all_peers.extend(new_peers);
                        if all_peers.len() >= 50 { break; } 
                    }
                }
            }
            
            if all_peers.is_empty() {
                eprintln!("No peers found!");
                return;
            }
            
            println!("Total unique peers found: {}", all_peers.len());
            println!("Starting download loop...");
            
            // Create channel for receiving successful connections from background threads
            let (conn_tx, conn_rx) = std::sync::mpsc::channel();
            
            // Spawn initial batch of connection attempts (parallel)
            let initial_batch_size = std::cmp::min(TARGET_PEER_COUNT * 2, all_peers.len());
            for i in 0..initial_batch_size {
                let peer = all_peers[i].clone();
                let info_hash = torrent_info.info_hash.clone();
                let peer_id = download_manager.peer_id.clone();
                let tx = conn_tx.clone();
                let debug = is_debug();
                
                std::thread::spawn(move || {
                    if debug {
                        println!("Connecting to {}...", peer.addr);
                    }
                    match Handshake::connect_and_handshake(&peer, &info_hash, &peer_id, Duration::from_millis(500)) {
                        Ok((connection, _)) => {
                            if debug {
                                println!("Connected to {}", peer.addr);
                            }
                            if let Ok(_) = connection.stream().set_nonblocking(true) {
                                let _ = tx.send(connection);
                            }
                        },
                        Err(_) => {
                            if debug {
                                println!("Failed to connect to {}", peer.addr);
                            }
                        }
                    }
                });
            }

            // Start TCP Listener for incoming connections
            let listener_tx = conn_tx.clone();
            let listener_info_hash = torrent_info.info_hash.clone();
            let listener_peer_id = download_manager.peer_id.clone();
            
            std::thread::spawn(move || {
                let listener = std::net::TcpListener::bind("0.0.0.0:6881");
                match listener {
                    Ok(l) => {
                        println!("Listening for incoming connections on 0.0.0.0:6881");
                        for stream in l.incoming() {
                            match stream {
                                Ok(stream) => {
                                    let tx = listener_tx.clone();
                                    let info_hash = listener_info_hash.clone();
                                    let peer_id = listener_peer_id.clone();
                                    
                                    std::thread::spawn(move || {
                                        if let Ok(addr) = stream.peer_addr() {
                                            // Create Peer struct for incoming connection
                                            // peer id is unknow atp but we're good anyways
                                            let peer = crate::structs::peer::Peer {
                                                addr,
                                            };
                                            
                                            match crate::peer::connection::PeerConnection::from_stream(stream, peer) {
                                                Ok(mut connection) => {
                                                    // Perform handshake
                                                    match Handshake::perform(&mut connection, &info_hash, &peer_id) {
                                                        Ok(_) => {
                                                            if let Ok(_) = connection.stream().set_nonblocking(true) {
                                                                let _ = tx.send(connection);
                                                            }
                                                        }
                                                        Err(e) => {
                                                            if is_debug() {
                                                                println!("Handshake failed with incoming peer {}: {}", addr, e);
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    if is_debug() {
                                                        println!("Failed to create connection from stream {}: {}", addr, e);
                                                    }
                                                }
                                            }
                                        }
                                    });
                                }
                                Err(e) => {
                                    if is_debug() {
                                        println!("Error accepting connection: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to bind TCP listener: {}", e);
                    }
                }
            });
            
            let mut last_log_time = std::time::Instant::now();
            let mut peer_index = initial_batch_size;
            let mut last_downloaded_bytes = 0;
            let mut last_speed_calc_time = std::time::Instant::now();
            let mut announced_completed = false;

            loop {
                // 1. Check for new connections from background threads (non-blocking)
                while let Ok(connection) = conn_rx.try_recv() {
                    // Check if already connected by address
                    let peer_addr = connection.peer().addr;
                    let already_connected = download_manager.peers.iter()
                        .any(|p| p.connection.peer().addr == peer_addr);
                    
                    if !already_connected {
                        download_manager.add_peer(connection);
                    }
                }
                
                // 2. Spawn more connection attempts if needed
                let active_count = download_manager.peers.len();
                if active_count < TARGET_PEER_COUNT && peer_index < all_peers.len() {
                    // Spawn a few more connection attempts in parallel
                    let batch_size = std::cmp::min(5, all_peers.len() - peer_index);
                    for _ in 0..batch_size {
                        if peer_index >= all_peers.len() {
                            break;
                        }
                        
                        let peer = all_peers[peer_index].clone();
                        peer_index += 1;
                        
                        let info_hash = torrent_info.info_hash.clone();
                        let peer_id = download_manager.peer_id.clone();
                        let tx = conn_tx.clone();
                        let debug = is_debug();
                        
                        std::thread::spawn(move || {
                            if debug {
                                println!("Connecting to {}...", peer.addr);
                            }
                            match Handshake::connect_and_handshake(&peer, &info_hash, &peer_id, Duration::from_millis(500)) {
                                Ok((connection, _)) => {
                                    if debug {
                                        println!("Connected to {}", peer.addr);
                                    }
                                    if let Ok(_) = connection.stream().set_nonblocking(true) {
                                        let _ = tx.send(connection);
                                    }
                                },
                                Err(_) => {
                                    if debug {
                                        println!("Failed to connect to {}", peer.addr);
                                    }
                                }
                            }
                        });
                    }
                }

                // 3. Tick download manager
                download_manager.tick(&file_manager);
                
                // 4. Check completion and announce
                if download_manager.piece_manager.is_complete() && !announced_completed {
                    println!("\n✓ Download complete! Announcing to trackers and continuing to seed...");
                    
                    // Announce completion to all trackers
                    let completed_stats = ClientStats {
                        uploaded: 0, // TODO: track actual uploaded bytes
                        downloaded: torrent_info.total_size,
                        left: 0,
                    };
                    
                    for tracker_url in &tracker_urls {
                        if tracker_url.starts_with("udp://") {
                            // afaik UDP trackers don't support completed event so, 
                            // we're just gonna announce as usual
                        } else {
                            // HTTP tracker - send completed event
                            if let Ok(_) = tracker_client.announce(
                                tracker_url,
                                &torrent_info.info_hash,
                                &completed_stats,
                                AnnounceEvent::Completed,
                            ) {
                                if is_debug() {
                                    println!("Announced completion to {}", tracker_url);
                                }
                            }
                        }
                    }
                    
                    announced_completed = true;
                }
                
                // 5. Log progress
                if last_log_time.elapsed() >= Duration::from_secs(1) {
                    let completed_pieces = download_manager.piece_manager.pieces_complete;
                    let downloaded_bytes = completed_pieces as u64 * torrent_info.piece_length; // Approximate
                    let total_bytes = torrent_info.total_size;
                    let percentage = (downloaded_bytes as f64 / total_bytes as f64) * 100.0;
                    
                    // Calculate speed
                    let now = std::time::Instant::now();
                    let elapsed = now.duration_since(last_speed_calc_time).as_secs_f64();
                    let speed = if elapsed > 0.0 {
                        (downloaded_bytes.saturating_sub(last_downloaded_bytes)) as f64 / elapsed
                    } else {
                        0.0
                    };
                    
                    last_downloaded_bytes = downloaded_bytes;
                    last_speed_calc_time = now;

                    // Clear line and print progress
                    if announced_completed {
                        print!("\r🌱 Seeding: 100% ({}/{} MB) - Peers: {} - Press Ctrl+C to stop    ", 
                            downloaded_bytes / 1_000_000, 
                            total_bytes / 1_000_000,
                            download_manager.peers.len()
                        );
                    } else {
                        print!("\rProgress: [{:<20}] {:.2}% ({}/{} MB) - Speed: {:.2} Mb/s - Peers: {}    ", 
                            "=".repeat((percentage / 5.0) as usize),
                            percentage, 
                            downloaded_bytes / 1_000_000, 
                            total_bytes / 1_000_000,
                            (speed / 1_000_000.0) * 8.0,
                            download_manager.peers.len()
                        );
                    }
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                    
                    last_log_time = std::time::Instant::now();
                }
                
                // Sleep a bit to avoid busy loop
                std::thread::sleep(Duration::from_millis(10));
            }
        },
        Err(e) => {
            eprintln!("Error parsing torrent file: {}", e);
            std::process::exit(1);
        }
    }
}
