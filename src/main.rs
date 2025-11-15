mod protodef;
mod structs;
mod tracker;

use std::path::PathBuf;
use crate::protodef::meta_info::MetaInfo;
use crate::tracker::announce::{TrackerClient, ClientStats, AnnounceEvent};

fn main() {
    let torrent_path = PathBuf::from("/Users/atheesh/Downloads/ubuntu-25.10-desktop-amd64.iso.torrent");
    
    match MetaInfo::from_file(&torrent_path) {
        Ok(meta_info) => {
            println!("Successfully parsed torrent file!");
            println!("Torrent name: {}", meta_info.info.name);
            println!("Announce URL: {}", meta_info.announce);
            
            // Convert to TorrentInfo
            let torrent_info = meta_info.to_torrent_info();
            
            println!("\nTorrent Info:");
            println!("  Info hash: {:x?}", torrent_info.info_hash);
            println!("  Total size: {} bytes ({:.2} GB)", 
                     torrent_info.total_size, 
                     torrent_info.total_size as f64 / 1_000_000_000.0);
            println!("  Piece length: {} bytes", torrent_info.piece_length);
            println!("  Number of pieces: {}", torrent_info.num_pieces);
            println!("  Number of files: {}", torrent_info.files.len());
            println!("  Number of trackers: {}", torrent_info.trackers.len());
            
            // Test tracker announce
            println!("\n=== Testing Tracker Announce ===");
            let tracker_client = TrackerClient::with_default_peer_id(6881);
            let stats = ClientStats {
                uploaded: 0,
                downloaded: 0,
                left: torrent_info.total_size,
            };
            
            // Get all tracker URLs
            let tracker_urls: Vec<String> = torrent_info.all_tracker_urls();
            println!("Announcing to {} tracker(s)...", tracker_urls.len());
            
            for tracker_url in &tracker_urls {
                println!("\nAnnouncing to: {}", tracker_url);
                match tracker_client.announce(
                    tracker_url,
                    &torrent_info.info_hash,
                    &stats,
                    AnnounceEvent::Started,
                ) {
                    Ok(response) => {
                        println!("  Interval: {} seconds", response.interval);
                        if let Some(complete) = response.complete {
                            println!("  Seeders: {}", complete);
                        }
                        if let Some(incomplete) = response.incomplete {
                            println!("  Leechers: {}", incomplete);
                        }
                        let peers = response.parse_peers();
                        println!("  Peers found: {}", peers.len());
                        for (i, peer) in peers.iter().take(5).enumerate() {
                            println!("    {}. {}", i + 1, peer.addr);
                        }
                        if peers.len() > 5 {
                            println!("    ... and {} more", peers.len() - 5);
                        }
                    }
                    Err(e) => {
                        eprintln!("  Error: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error parsing torrent file: {}", e);
            std::process::exit(1);
        }
    }
}
