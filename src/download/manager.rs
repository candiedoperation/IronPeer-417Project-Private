use crate::structs::peer::Peer;
use crate::structs::torrent_info::TorrentInfo;

/// Status of a piece download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceStatus {
    Missing,
    Downloading,
    Complete,
}

/// Manages the state of all pieces in the torrent.
pub struct PieceManager {
    pub piece_status: Vec<PieceStatus>,
    pub pieces_complete: usize,
    pub total_pieces: usize,
}

impl PieceManager {
    pub fn new(total_pieces: usize) -> Self {
        Self {
            piece_status: vec![PieceStatus::Missing; total_pieces],
            pieces_complete: 0,
            total_pieces,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.pieces_complete == self.total_pieces
    }

    pub fn mark_complete(&mut self, index: usize) {
        if self.piece_status[index] != PieceStatus::Complete {
            self.piece_status[index] = PieceStatus::Complete;
            self.pieces_complete += 1;
        }
    }
}

pub const BLOCK_SIZE: u32 = 16384; // 16KB

#[derive(Debug, Clone)]
pub struct Block {
    pub index: u32,
    pub begin: u32,
    pub length: u32,
    pub data: Option<Vec<u8>>,
}

pub struct PieceDownload {
    pub index: usize,
    pub blocks: Vec<Block>,
    pub downloaded: usize,
    pub requested: usize,
}

impl PieceDownload {
    pub fn new(index: usize, piece_length: u64) -> Self {
        let mut blocks = Vec::new();
        let mut offset = 0;

        while offset < piece_length {
            let length = std::cmp::min(BLOCK_SIZE as u64, piece_length - offset) as u32;
            blocks.push(Block {
                index: index as u32,
                begin: offset as u32,
                length,
                data: None,
            });
            offset += length as u64;
        }

        Self {
            index,
            blocks,
            downloaded: 0,
            requested: 0,
        }
    }
}

/// Orchestrates the download process, managing peers and piece requests.
pub struct DownloadManager {
    pub piece_manager: PieceManager,
    pub active_peers: Vec<Peer>,
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}

impl DownloadManager {
    pub fn new(torrent_info: &TorrentInfo, peer_id: [u8; 20]) -> Self {
        Self {
            piece_manager: PieceManager::new(torrent_info.num_pieces),
            active_peers: Vec::new(),
            info_hash: torrent_info.info_hash,
            peer_id,
        }
    }

    pub fn add_peer(&mut self, peer: Peer) {
        self.active_peers.push(peer);
    }
}
