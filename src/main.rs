mod download;
mod peer;
mod protocol;
mod protodef;
mod storage;
mod structs;
mod tracker;

use crate::peer::handshake::Handshake;
use crate::protocol::messages::Message;
use crate::protodef::meta_info::MetaInfo;
use crate::tracker::announce::{AnnounceEvent, ClientStats, TrackerClient};
use std::path::PathBuf;
use std::time::Duration;

fn main() {
    let torrent_path =
        PathBuf::from("/Users/atheesh/Downloads/ubuntu-25.10-desktop-amd64.iso.torrent");
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
                    if !peers.is_empty() {
                        break;
                    } // Found peers, good enough for now
                }
            }

            if peers.is_empty() {
                eprintln!("No peers found!");
                return;
            }

            println!(
                "Found {} peers. Connecting to the first one...",
                peers.len()
            );
            let peer = &peers[0];

            match Handshake::connect_and_handshake(
                peer,
                &torrent_info.info_hash,
                &download_manager.peer_id,
                Duration::from_secs(5),
            ) {
                Ok((mut connection, _)) => {
                    println!("Connected to {}", peer.addr);

                    if let Err(e) = connection.send_message(&Message::Interested) {
                        eprintln!("Failed to send Interested: {}", e);
                        return;
                    }

                    let mut choked = true;
                    let mut current_piece_idx = 0;

                    loop {
                        // Read messages
                        match connection.read_message() {
                            Ok(Some(msg)) => {
                                match msg {
                                    Message::Unchoke => {
                                        println!("Unchoked!");
                                        choked = false;
                                    }
                                    Message::Choke => {
                                        println!("Choked!");
                                        choked = true;
                                    }
                                    Message::Piece {
                                        index,
                                        begin,
                                        block,
                                    } => {
                                        // Write block
                                        if let Err(e) =
                                            file_manager.write_block(index as usize, begin, &block)
                                        {
                                            eprintln!("Failed to write block: {}", e);
                                        }

                                        // Simple logic: assume we requested this.
                                        // In real impl, we'd track requests.
                                        // For now, let's just check if we have the full piece?
                                        // No, we need to track blocks.
                                        // But for this MVP loop, let's just request blocks sequentially.
                                    }
                                    _ => {}
                                }
                            }
                            Ok(None) => {} // No message
                            Err(e) => {
                                eprintln!("Connection error: {}", e);
                                break;
                            }
                        }

                        if !choked && current_piece_idx < torrent_info.num_pieces {
                            // Request blocks for current piece
                            // This is blocking and naive, but demonstrates logic.
                            // We need to request blocks.
                            // Let's use PieceDownload logic from manager?
                            // Or just manually request for now.

                            let piece_len = torrent_info.piece_length(current_piece_idx);
                            let mut offset = 0;
                            while offset < piece_len {
                                let len = std::cmp::min(16384, piece_len - offset);
                                if let Err(e) = connection.send_message(&Message::Request {
                                    index: current_piece_idx as u32,
                                    begin: offset as u32,
                                    length: len as u32,
                                }) {
                                    eprintln!("Failed to request block: {}", e);
                                    break;
                                }
                                offset += len;

                                // Wait for piece message (synchronous for this test)
                                // This is very slow but safe.
                                // Real impl needs pipelining.
                                loop {
                                    match connection.read_message() {
                                        Ok(Some(Message::Piece {
                                            index,
                                            begin,
                                            block,
                                        })) => {
                                            file_manager
                                                .write_block(index as usize, begin, &block)
                                                .unwrap();
                                            break; // Got the block
                                        }
                                        Ok(Some(msg)) => {
                                            println!("Received {:?} while waiting for piece", msg)
                                        }
                                        Ok(None) => {}
                                        Err(e) => panic!("Error: {}", e),
                                    }
                                }
                            }

                            println!("Downloaded piece {}", current_piece_idx);
                            // Verify
                            if let Some(hash) = torrent_info.piece_hash(current_piece_idx) {
                                if file_manager
                                    .verify_piece(current_piece_idx, hash)
                                    .unwrap_or(false)
                                {
                                    println!("Piece {} verified!", current_piece_idx);
                                    download_manager
                                        .piece_manager
                                        .mark_complete(current_piece_idx);
                                } else {
                                    println!("Piece {} verification failed!", current_piece_idx);
                                }
                            }

                            current_piece_idx += 1;
                        } else if current_piece_idx >= torrent_info.num_pieces {
                            println!("Download complete!");
                            break;
                        }
                    }
                }
                Err(e) => eprintln!("Handshake failed: {}", e),
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
