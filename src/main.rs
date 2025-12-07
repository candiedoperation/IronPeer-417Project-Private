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
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let torrent_path =
        PathBuf::from("/Users/atheesh/Downloads/raspios-2025-12-04-trixie-arm64.img.xz.torrent");
    let output_dir = PathBuf::from("downloads");

    match MetaInfo::from_file(&torrent_path) {
        Ok(meta_info) => {
            println!("Successfully parsed torrent file!");
            println!("Torrent name: {}", meta_info.info.name);

            let torrent_info = meta_info.to_torrent_info();
            println!("Total size: {} bytes", torrent_info.total_size);

            let file_manager = crate::storage::files::FileManager::new(output_dir, &torrent_info);
            let mut download_manager = crate::download::manager::DownloadManager::new(
                &torrent_info,
                *TrackerClient::with_default_peer_id(6881).peer_id(),
            );

            // Tracker announce
            let tracker_client = TrackerClient::new(download_manager.peer_id, 6881);
            let stats = ClientStats {
                uploaded: 0,
                downloaded: 0,
                left: torrent_info.total_size,
            };

            let tracker_urls = torrent_info.all_tracker_urls();
            let mut peers = Vec::new();

            for tracker_url in &tracker_urls {
                println!("Announcing to: {}", tracker_url);
                if let Ok(response) = tracker_client.announce(
                    tracker_url,
                    &torrent_info.info_hash,
                    &stats,
                    AnnounceEvent::Started,
                ) {
                    peers.extend(response.parse_peers());
                    if peers.len() >= 50 {
                        break;
                    }
                }
            }

            if peers.is_empty() {
                eprintln!("No peers found!");
                return;
            }

            println!("Found {} peers. Connecting to up to 10...", peers.len());

            // Connect to peers
            let mut connected_count = 0;
            for peer in peers {
                if connected_count >= 10 {
                    break;
                }

                println!("Connecting to {}", peer.addr);
                match Handshake::connect_and_handshake(
                    &peer,
                    &torrent_info.info_hash,
                    &download_manager.peer_id,
                    Duration::from_secs(3),
                ) {
                    Ok((mut connection, _)) => {
                        println!("Connected to {}", peer.addr);
                        // Set non-blocking for event loop
                        if let Err(e) = connection.stream().set_nonblocking(true) {
                            eprintln!("Failed to set non-blocking: {}", e);
                            continue;
                        }
                        download_manager.add_peer(connection);
                        connected_count += 1;
                    }
                    Err(e) => {
                        // eprintln!("Failed to connect to {}: {}", peer.addr, e);
                    }
                }
            }

            println!(
                "Connected to {} peers. Starting download loop...",
                connected_count
            );

            loop {
                download_manager.tick(&file_manager);

                if download_manager.piece_manager.is_complete() {
                    println!("Download complete!");
                    break;
                }

                // Sleep a bit to avoid busy loop
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Err(e) => {
            eprintln!("Error parsing torrent file: {}", e);
            std::process::exit(1);
        }
    }
}
