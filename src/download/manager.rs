use crate::peer::connection::PeerConnection;
use crate::protocol::messages::Message;
use crate::storage::files::FileManager;
use crate::structs::torrent_info::TorrentInfo;

/// Status of a piece download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceStatus {
    Missing,
    Downloading,
    Complete,
}

pub const BLOCK_SIZE: u32 = 16384; // 16KB

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub index: u32,
    pub begin: u32,
    pub length: u32,
    pub downloaded: bool,
    pub requested: bool,
}

pub struct PieceState {
    pub index: usize,
    pub blocks: Vec<BlockInfo>,
    pub downloaded_count: usize,
}

impl PieceState {
    pub fn new(index: usize, piece_length: u64) -> Self {
        let mut blocks = Vec::new();
        let mut offset = 0;

        while offset < piece_length {
            let length = std::cmp::min(BLOCK_SIZE as u64, piece_length - offset) as u32;
            blocks.push(BlockInfo {
                index: index as u32,
                begin: offset as u32,
                length,
                downloaded: false,
                requested: false,
            });
            offset += length as u64;
        }

        Self {
            index,
            blocks,
            downloaded_count: 0,
        }
    }
}

/// Manages the state of all pieces in the torrent.
pub struct PieceManager {
    pub piece_status: Vec<PieceStatus>,
    pub pieces_complete: usize,
    pub total_pieces: usize,
    pub downloading_pieces: Vec<PieceState>,
    pub piece_length: u64,
    pub last_piece_length: u64,
}

impl PieceManager {
    pub fn new(total_pieces: usize, piece_length: u64, total_size: u64) -> Self {
        let last_piece_length = total_size % piece_length;
        let last_piece_length = if last_piece_length == 0 {
            piece_length
        } else {
            last_piece_length
        };

        Self {
            piece_status: vec![PieceStatus::Missing; total_pieces],
            pieces_complete: 0,
            total_pieces,
            downloading_pieces: Vec::new(),
            piece_length,
            last_piece_length,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.pieces_complete == self.total_pieces
    }

    pub fn mark_complete(&mut self, index: usize) {
        if self.piece_status[index] != PieceStatus::Complete {
            self.piece_status[index] = PieceStatus::Complete;
            self.pieces_complete += 1;
            // Remove from downloading
            self.downloading_pieces.retain(|p| p.index != index);
        }
    }

    pub fn get_next_needed_piece(
        &mut self,
        peer_have: &[bool],
        availability: &[u16],
    ) -> Option<usize> {
        // Rarest-First Strategy:
        // Find missing pieces that the peer has.
        // Sort them by availability (rarity).
        // Pick the rarest one.

        let mut candidates: Vec<usize> = self
            .piece_status
            .iter()
            .enumerate()
            .filter(|(i, status)| {
                **status == PieceStatus::Missing && peer_have.get(*i).copied().unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();

        if candidates.is_empty() {
            return None;
        }

        // Sort by availability (ascending) -> rarest first
        // If availability is equal, random or sequential (stable sort preserves order)
        candidates.sort_by_key(|&i| availability.get(i).copied().unwrap_or(0));

        Some(candidates[0])
    }

    pub fn start_downloading(&mut self, index: usize) {
        if self.piece_status[index] == PieceStatus::Missing {
            self.piece_status[index] = PieceStatus::Downloading;
            let len = if index == self.total_pieces - 1 {
                self.last_piece_length
            } else {
                self.piece_length
            };
            self.downloading_pieces.push(PieceState::new(index, len));
        }
    }

    pub fn reset_piece(&mut self, index: usize) {
        // Reset piece to missing status so it can be retried
        self.piece_status[index] = PieceStatus::Missing;
        // Remove from downloading pieces
        self.downloading_pieces.retain(|p| p.index != index);
    }

    pub fn get_bitfield(&self) -> Vec<u8> {
        // Create bitfield representing which pieces we have
        let num_bytes = (self.total_pieces + 7) / 8;
        let mut bitfield = vec![0u8; num_bytes];

        for (i, status) in self.piece_status.iter().enumerate() {
            if *status == PieceStatus::Complete {
                let byte_index = i / 8;
                let bit_index = 7 - (i % 8);
                bitfield[byte_index] |= 1 << bit_index;
            }
        }

        bitfield
    }
}

pub struct ActivePeer {
    pub connection: PeerConnection,
    pub peer_choking: bool,
    pub peer_interested: bool,
    pub am_choking: bool,
    pub am_interested: bool,
    pub have_pieces: Vec<bool>,
    pub inflight_requests: usize,
    pub requested_blocks: Vec<(u32, u32)>, // (index, begin)
}

impl ActivePeer {
    pub fn new(connection: PeerConnection, num_pieces: usize) -> Self {
        Self {
            connection,
            peer_choking: true,
            peer_interested: false,
            am_choking: true,
            am_interested: false,
            have_pieces: vec![false; num_pieces],
            inflight_requests: 0,
            requested_blocks: Vec::new(),
        }
    }
}

/// Orchestrates the download process, managing peers and piece requests.
pub struct DownloadManager {
    pub peers: Vec<ActivePeer>,
    pub piece_manager: PieceManager,
    pub peer_id: [u8; 20],
    #[allow(dead_code)]
    pub info_hash: [u8; 20],
    pub piece_hashes: Vec<[u8; 20]>,
    pub unchoked_peers: usize, // Track how many peers we're uploading to (max 5)
}

impl DownloadManager {
    pub fn new(torrent_info: &TorrentInfo, peer_id: [u8; 20]) -> Self {
        let piece_manager = PieceManager::new(
            torrent_info.num_pieces,
            torrent_info.piece_length,
            torrent_info.total_size,
        );
        Self {
            peers: Vec::new(),
            piece_manager,
            peer_id,
            info_hash: torrent_info.info_hash,
            piece_hashes: torrent_info.piece_hashes.clone(),
            unchoked_peers: 0,
        }
    }

    pub fn add_peer(&mut self, mut connection: PeerConnection) {
        let num_pieces = self.piece_manager.piece_status.len();

        // Send bitfield to tell peer what pieces we have
        let bitfield = self.piece_manager.get_bitfield();
        if let Err(_) = connection.send_message(&Message::Bitfield { bits: bitfield }) {
            return; // Failed to send bitfield, don't add peer
        }

        self.peers.push(ActivePeer::new(connection, num_pieces));
    }

    pub fn verify_existing_pieces(&mut self, file_manager: &FileManager) -> usize {
        let mut verified_count = 0;
        println!("Verifying existing data...");

        for i in 0..self.piece_manager.total_pieces {
            if i < self.piece_hashes.len() {
                let hash = &self.piece_hashes[i];
                // We only verify if we can read the piece (file exists)
                // FileManager::verify_piece handles reading.
                // If file is missing, it returns error/false.
                match file_manager.verify_piece(i, hash) {
                    Ok(true) => {
                        self.piece_manager.piece_status[i] = PieceStatus::Complete;
                        self.piece_manager.pieces_complete += 1;
                        verified_count += 1;
                    }
                    _ => {
                        // Not complete or error, leave as Missing
                    }
                }
            }

            // Optional: Print progress for large files
            if i % 100 == 0 && i > 0 {
                print!(
                    "\rVerified {}/{} pieces",
                    i, self.piece_manager.total_pieces
                );
                use std::io::Write;
                std::io::stdout().flush().unwrap();
            }
        }
        println!(
            "\rVerification complete: {}/{} pieces found.",
            verified_count, self.piece_manager.total_pieces
        );
        verified_count
    }

    pub fn tick(&mut self, file_manager: &FileManager) {
        let mut peers_to_remove = Vec::new();
        let mut completed_pieces: Vec<u32> = Vec::new(); // Track completed pieces to broadcast
        let endgame = self
            .piece_manager
            .piece_status
            .iter()
            .all(|s| *s != PieceStatus::Missing);

        // Calculate piece availability (for rarest-first)
        let mut piece_availability = vec![0u16; self.piece_manager.total_pieces];
        for peer in &self.peers {
            for (i, has_piece) in peer.have_pieces.iter().enumerate() {
                if *has_piece && i < piece_availability.len() {
                    piece_availability[i] = piece_availability[i].saturating_add(1);
                }
            }
        }

        for i in 0..self.peers.len() {
            let peer = &mut self.peers[i];

            // 1. Read messages
            match peer.connection.read_message() {
                Ok(Some(msg)) => {
                    match msg {
                        Message::Choke => {
                            // println!("Peer choked us");
                            peer.peer_choking = true;
                        }
                        Message::Unchoke => {
                            // println!("Peer unchoked us");
                            peer.peer_choking = false;
                        }
                        Message::Have { piece_index } => {
                            if (piece_index as usize) < peer.have_pieces.len() {
                                peer.have_pieces[piece_index as usize] = true;
                            }
                        }
                        Message::Bitfield { bits } => {
                            for (byte_idx, byte) in bits.iter().enumerate() {
                                for bit_idx in 0..8 {
                                    let piece_idx = byte_idx * 8 + bit_idx;
                                    if piece_idx < peer.have_pieces.len() {
                                        if (byte >> (7 - bit_idx)) & 1 != 0 {
                                            peer.have_pieces[piece_idx] = true;
                                        }
                                    }
                                }
                            }
                        }
                        Message::Piece {
                            index,
                            begin,
                            block,
                        } => {
                            peer.inflight_requests = peer.inflight_requests.saturating_sub(1);
                            peer.requested_blocks
                                .retain(|(i, b)| !(*i == index && *b == begin));

                            // Write block
                            if let Err(e) = file_manager.write_block(index as usize, begin, &block)
                            {
                                eprintln!("Failed to write block: {}", e);
                            }

                            // Update piece state
                            if let Some(piece_state) = self
                                .piece_manager
                                .downloading_pieces
                                .iter_mut()
                                .find(|p| p.index == index as usize)
                            {
                                if let Some(block_info) =
                                    piece_state.blocks.iter_mut().find(|b| b.begin == begin)
                                {
                                    if !block_info.downloaded {
                                        block_info.downloaded = true;
                                        piece_state.downloaded_count += 1;
                                    }
                                }

                                // Check if piece is complete
                                if piece_state.downloaded_count == piece_state.blocks.len() {
                                    // Verify
                                    if (index as usize) < self.piece_hashes.len() {
                                        let hash = &self.piece_hashes[index as usize];
                                        match file_manager.verify_piece(index as usize, hash) {
                                            Ok(true) => {
                                                if crate::is_debug() {
                                                    println!("Piece {} verified!", index);
                                                }
                                                self.piece_manager.mark_complete(index as usize);

                                                // Collect piece index to broadcast later
                                                completed_pieces.push(index);
                                            } // Reuse for completed pieces
                                            Ok(false) => {
                                                eprintln!(
                                                    "Piece {} verification failed! Retrying...",
                                                    index
                                                );
                                                // Reset piece to missing so it can be retried
                                                self.piece_manager.reset_piece(index as usize);
                                            }
                                            Err(e) => {
                                                eprintln!("Error verifying piece {}: {}", index, e)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Message::Request {
                            index,
                            begin,
                            length,
                        } => {
                            // Handle upload request from peer
                            if !peer.am_choking {
                                // Read the block from disk
                                match file_manager.read_block(
                                    index as usize,
                                    begin,
                                    length as usize,
                                ) {
                                    Ok(block) => {
                                        // Send the block to the peer
                                        if let Err(_) =
                                            peer.connection.send_message(&Message::Piece {
                                                index,
                                                begin,
                                                block,
                                            })
                                        {
                                            peers_to_remove.push(i);
                                        }
                                    }
                                    Err(e) => {
                                        if crate::is_debug() {
                                            eprintln!("Failed to read block for upload: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Message::Interested => {
                            peer.peer_interested = true;
                        }
                        _ => {}
                    }
                }
                Ok(None) => {}
                Err(_e) => {
                    // eprintln!("Peer error: {}", _e);
                    peers_to_remove.push(i);
                }
            }

            // 2. Manage choking (simple algorithm: unchoke up to 5 peers)
            if peer.peer_interested && peer.am_choking && self.unchoked_peers < 5 {
                if let Ok(_) = peer.connection.send_message(&Message::Unchoke) {
                    peer.am_choking = false;
                    self.unchoked_peers += 1;
                }
            }

            // 3. Manage interest
            if !peer.am_interested {
                // If peer has something we need, get interested
                // For simplicity, always be interested if we are not done
                if !self.piece_manager.is_complete() {
                    if let Err(_) = peer.connection.send_message(&Message::Interested) {
                        peers_to_remove.push(i);
                        continue;
                    }
                    peer.am_interested = true;
                }
            }

            // 4. Request blocks
            if !peer.peer_choking && peer.am_interested && peer.inflight_requests < 10 {
                // Find a block to request
                // 1. Continue current pieces
                // 2. Start new piece

                let mut request_made = false;

                // Try to find a block in currently downloading pieces
                for piece_state in &mut self.piece_manager.downloading_pieces {
                    // Check if peer has this piece
                    if peer
                        .have_pieces
                        .get(piece_state.index)
                        .copied()
                        .unwrap_or(false)
                    {
                        for block in &mut piece_state.blocks {
                            // Check if we already asked THIS peer for this block
                            let already_asked_peer =
                                peer.requested_blocks.contains(&(block.index, block.begin));

                            if !block.downloaded && !already_asked_peer {
                                // In normal mode, we request if nobody else requested it (!block.requested).
                                // In endgame mode, we request even if someone else requested it,
                                // as long as WE haven't asked THIS peer yet.
                                if endgame || !block.requested {
                                    if let Ok(_) = peer.connection.send_message(&Message::Request {
                                        index: block.index,
                                        begin: block.begin,
                                        length: block.length,
                                    }) {
                                        block.requested = true;
                                        peer.inflight_requests += 1;
                                        peer.requested_blocks.push((block.index, block.begin));
                                        request_made = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if request_made {
                        break;
                    }
                }

                if !request_made && peer.inflight_requests < 10 && !endgame {
                    // Start a new piece
                    if let Some(index) = self
                        .piece_manager
                        .get_next_needed_piece(&peer.have_pieces, &piece_availability)
                    {
                        self.piece_manager.start_downloading(index);
                        // Now try to request from it
                        if let Some(piece_state) = self
                            .piece_manager
                            .downloading_pieces
                            .iter_mut()
                            .find(|p| p.index == index)
                        {
                            if let Some(block) = piece_state.blocks.first_mut() {
                                if let Ok(_) = peer.connection.send_message(&Message::Request {
                                    index: block.index,
                                    begin: block.begin,
                                    length: block.length,
                                }) {
                                    block.requested = true;
                                    peer.inflight_requests += 1;
                                    peer.requested_blocks.push((block.index, block.begin));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove dead peers (in reverse order to maintain indices)
        for i in peers_to_remove.into_iter().rev() {
            self.peers.remove(i);
        }

        // Broadcast Have messages for completed pieces
        for piece_index in completed_pieces {
            self.broadcast_have(piece_index);
        }
    }

    fn broadcast_have(&mut self, piece_index: u32) {
        // Send Have message to all peers
        let msg = Message::Have { piece_index };
        for peer in &mut self.peers {
            let _ = peer.connection.send_message(&msg);
        }
    }
}
