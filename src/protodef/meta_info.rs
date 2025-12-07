use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use sha1::{Digest, Sha1};
use crate::structs::torrent_info::{TorrentInfo, TorrentFile};

/// Root structure of a .torrent file containing all metadata about the torrent.
/// This is the bencoded dictionary that gets deserialized from .torrent files.
/// https://wiki.theory.org/BitTorrentSpecification#Metainfo_File_Structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaInfo {
    pub announce: String,
    #[serde(rename = "announce-list")]
    pub announce_list: Option<Vec<Vec<String>>>,
    #[serde(rename = "creation date")]
    pub creation_date: Option<i64>,
    pub comment: Option<String>,
    #[serde(rename = "created by")]
    pub created_by: Option<String>,
    pub encoding: Option<String>,
    pub info: InfoDict,
}

/// The "info" dictionary from the metainfo file containing file and piece information.
/// This dictionary is bencoded and SHA1-hashed to produce the info_hash that uniquely
/// identifies the torrent. Key order and formatting must be preserved for hash correctness.
/// https://wiki.theory.org/BitTorrentSpecification#Info_Dictionary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoDict {
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: u64,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
    pub private: Option<u8>,
    pub length: Option<u64>,
    pub md5sum: Option<String>,
    pub files: Option<Vec<FileDict>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDict {
    pub length: u64,
    pub md5sum: Option<String>,
    pub path: Vec<String>,
}

impl MetaInfo {
    /// Reads a .torrent file from disk and deserializes it from bencode format.
    pub fn from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let data = fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Deserializes a MetaInfo structure from bencoded bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let meta_info: MetaInfo = serde_bencode::from_bytes(data)?;
        Ok(meta_info)
    }

    /// convert MetaInfo (protocol format) into TorrentInfo
    pub fn to_torrent_info(&self) -> TorrentInfo {
        let info_hash = compute_info_hash(&self.info); // info_hash is SHA1 of info dict
        let piece_length = self.info.piece_length;
        let num_pieces = self.info.pieces.len() / 20;
        let mut files = Vec::new();
        let mut trackers = Vec::new();
        if let Some(ref tiers) = self.announce_list {
            trackers.extend(tiers.iter().cloned()); // flatten tiers into trackers
        } else {
            trackers.push(vec![self.announce.clone()]);
        }
        if let Some(ref file_dicts) = self.info.files {
            let mut offset = 0;
            for f in file_dicts {
                let path = {
                    let mut b = PathBuf::from(&self.info.name);
                    for p in &f.path { b.push(p); }
                    b
                };
                files.push(TorrentFile {
                    path,
                    length: f.length,
                    start_piece: (offset / piece_length) as usize,
                    end_piece: ((offset + f.length - 1) / piece_length) as usize,
                    piece_offset_start: offset % piece_length,
                    piece_offset_end: (offset + f.length - 1) % piece_length,
                    md5sum: f.md5sum.clone(),
                });
                offset += f.length;
            }
        } else if let Some(length) = self.info.length {
            files.push(TorrentFile {
                path: PathBuf::from(&self.info.name),
                length,
                start_piece: 0,
                end_piece: ((length - 1) / piece_length) as usize,
                piece_offset_start: 0,
                piece_offset_end: (length - 1) % piece_length,
                md5sum: self.info.md5sum.clone(),
            });
        }
        let piece_hashes = self.info.pieces.chunks(20)
            .map(|chunk| {
                let mut h = [0u8; 20];
                h.copy_from_slice(chunk);
                h
            }).collect();
        let total_size = files.iter().map(|f| f.length).sum();
        TorrentInfo {
            info_hash,
            name: self.info.name.clone(),
            total_size,
            piece_length,
            num_pieces,
            piece_hashes,
            files,
            trackers,
            is_private: self.info.private == Some(1),
            creation_date: self.creation_date,
            created_by: self.created_by.clone(),
            comment: self.comment.clone(),
        }
    }
}

/// Computes the SHA1 hash of the bencoded info dictionary to produce the info_hash.
/// The info_hash uniquely identifies a torrent and must match exactly between clients.
/// Critical: The bencoding must preserve the exact key order and formatting from the original
/// torrent file, as any changes will produce a different hash. serde_bencode handles this correctly.
fn compute_info_hash(info: &InfoDict) -> [u8; 20] {
    let bencoded = serde_bencode::to_bytes(info)
        .expect("Failed to bencode info dict");
    let mut hasher = Sha1::new();
    hasher.update(&bencoded);
    hasher.finalize().into()
}
