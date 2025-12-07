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
    const TARGET_PEER_COUNT: usize = 15;

    match MetaInfo::from_file(&torrent_path) {
        Ok(meta_info) => {
            println!("Successfully parsed torrent file!");
            println!("Torrent name: {}", meta_info.info.name);
            
            let torrent_info = meta_info.to_torrent_info();
            println!("Total size: {} bytes", torrent_info.total_size);
            
            let file_manager = crate::storage::files::FileManager::new(output_dir, &torrent_info);
            let mut download_manager = crate::download::manager::DownloadManager::new(&torrent_info, *TrackerClient::with_default_peer_id(6881).peer_id());
            
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
            
            let mut last_log_time = std::time::Instant::now();
            let mut peer_index = 0;
            let mut last_downloaded_bytes = 0;
            let mut last_speed_calc_time = std::time::Instant::now();

            loop {
                // 1. Maintain active peer count (non-blocking attempts)
                let active_count = download_manager.peers.len();
                if active_count < TARGET_PEER_COUNT {
                    // Try to connect to one more peer per tick if needed, to avoid blocking too long
                    if peer_index < all_peers.len() {
                        let peer = &all_peers[peer_index];
                        peer_index += 1;
                        
                        // Check if already connected (simple check by address)
                        let already_connected = download_manager.peers.iter().any(|p| p.connection.peer().addr == peer.addr);
                        if !already_connected {
                            if is_debug() {
                                println!("Connecting to {}...", peer.addr);
                            }
                            // Use a shorter timeout for the connect attempt to keep the loop moving
                            match Handshake::connect_and_handshake(peer, &torrent_info.info_hash, &download_manager.peer_id, Duration::from_secs(1)) {
                                Ok((connection, _)) => {
                                    if is_debug() {
                                        println!("Connected to {}", peer.addr);
                                    }
                                    if let Ok(_) = connection.stream().set_nonblocking(true) {
                                        download_manager.add_peer(connection);
                                    }
                                },
                                Err(_) => {
                                    if is_debug() {
                                        println!("Failed to connect to {}", peer.addr);
                                    }
                                }
                            }
                        }
                    } else {
                        // Reset index if we reached the end
                        peer_index = 0;
                    }
                }

                // 2. Tick download manager
                download_manager.tick(&file_manager);
                
                // 3. Check completion
                if download_manager.piece_manager.is_complete() {
                    println!("\nDownload complete!");
                    break;
                }
                
                // 4. Log progress
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
                    print!("\rProgress: [{:<50}] {:.2}% ({}/{} MB) - Speed: {:.2} MB/s - Peers: {}    ", 
                        "=".repeat((percentage / 2.0) as usize),
                        percentage, 
                        downloaded_bytes / 1_000_000, 
                        total_bytes / 1_000_000,
                        speed / 1_000_000.0,
                        download_manager.peers.len()
                    );
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
