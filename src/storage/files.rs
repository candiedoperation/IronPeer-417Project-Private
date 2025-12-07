use crate::structs::torrent_info::{TorrentFile, TorrentInfo};
use sha1::{Digest, Sha1};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

/// Manages file I/O for the torrent.
/// Handles reading/writing blocks to the correct file(s) and verifying piece hashes.
pub struct FileManager {
    pub output_dir: PathBuf,
    pub files: Vec<TorrentFile>,
    pub piece_length: u64,
    pub total_size: u64,
    pub total_pieces: usize,
}

impl FileManager {
    pub fn new(output_dir: PathBuf, torrent_info: &TorrentInfo) -> Self {
        Self {
            output_dir,
            files: torrent_info.files.clone(),
            piece_length: torrent_info.piece_length,
            total_size: torrent_info.total_size,
            total_pieces: torrent_info.piece_hashes.len(),
        }
    }

    /// Calculate the actual length of a piece (last piece may be smaller)
    fn get_piece_length(&self, piece_index: usize) -> u64 {
        if piece_index == self.total_pieces - 1 {
            // Last piece
            let remainder = self.total_size % self.piece_length;
            if remainder == 0 {
                self.piece_length
            } else {
                remainder
            }
        } else {
            self.piece_length
        }
    }

    /// Writes a block of data to the appropriate file(s).
    /// A block might span across two files.
    pub fn write_block(
        &self,
        piece_index: usize,
        begin: u32,
        data: &[u8],
    ) -> Result<(), std::io::Error> {
        let global_offset = (piece_index as u64 * self.piece_length) + begin as u64;
        let mut current_offset = global_offset;
        let mut remaining_data = data;

        for file_info in &self.files {
            // Calculate file bounds in global space
            // We need to know the start offset of this file.
            // Since we don't store it, we have to calculate it or store it in TorrentFile.
            // For now, let's assume we iterate and track offset.
            // Wait, TorrentFile has `start_piece` but not byte offset.
            // Let's recalculate offsets on the fly or improve TorrentFile.
            // Actually, let's just iterate all files until we find the right one.
            // Optimization: Store global offset in TorrentFile or FileManager.
            // For this MVP, I'll calculate it.

            // NOTE: This is inefficient for many files, but fine for MVP.
            // Better: Pre-calculate file offsets.
            // Let's assume we can calculate it.

            // Actually, let's just use a helper that finds the file(s) for a range.
            // But I can't easily do that without storing offsets.
            // Let's just iterate.

            // Wait, I can't easily know the file start offset without summing previous files.
            // I'll implement a helper to get file + offset for a global offset.
            break; // Placeholder
        }

        // Real implementation:
        let mut file_start_offset = 0;
        for file_info in &self.files {
            let file_end_offset = file_start_offset + file_info.length;

            if current_offset >= file_start_offset && current_offset < file_end_offset {
                // This file contains at least part of the block
                let file_path = self.output_dir.join(&file_info.path);

                // Ensure parent directories exist
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .open(&file_path)?;

                let offset_in_file = current_offset - file_start_offset;
                file.seek(SeekFrom::Start(offset_in_file))?;

                let bytes_to_write = std::cmp::min(
                    remaining_data.len() as u64,
                    file_info.length - offset_in_file,
                ) as usize;

                file.write_all(&remaining_data[..bytes_to_write])?;

                remaining_data = &remaining_data[bytes_to_write..];
                current_offset += bytes_to_write as u64;

                if remaining_data.is_empty() {
                    break;
                }
            }

            file_start_offset += file_info.length;
        }

        Ok(())
    }

    /// Reads a full piece from disk for verification.
    pub fn read_piece(&self, piece_index: usize) -> Result<Vec<u8>, std::io::Error> {
        // Calculate the actual length of this piece
        let piece_len = self.get_piece_length(piece_index);

        let mut buffer = vec![0u8; piece_len as usize];
        let global_offset = piece_index as u64 * self.piece_length;

        let mut current_offset = global_offset;
        let mut buf_offset = 0;
        let mut file_start_offset = 0;

        for file_info in &self.files {
            let file_end_offset = file_start_offset + file_info.length;

            if current_offset < file_end_offset
                && (current_offset + (buffer.len() - buf_offset) as u64) > file_start_offset
            {
                // Overlap
                let file_path = self.output_dir.join(&file_info.path);
                if !file_path.exists() {
                    // If file doesn't exist, we can't read it. Return error or zeros?
                    // For verification, missing file means invalid piece.
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "File missing",
                    ));
                }

                let mut file = File::open(&file_path)?;

                let start_in_file = if current_offset > file_start_offset {
                    current_offset - file_start_offset
                } else {
                    0
                };

                file.seek(SeekFrom::Start(start_in_file))?;

                let end_in_file = std::cmp::min(
                    file_info.length,
                    start_in_file + (buffer.len() - buf_offset) as u64,
                );

                let bytes_to_read = (end_in_file - start_in_file) as usize;
                file.read_exact(&mut buffer[buf_offset..buf_offset + bytes_to_read])?;

                buf_offset += bytes_to_read;
                current_offset += bytes_to_read as u64;

                if buf_offset >= buffer.len() {
                    break;
                }
            }

            file_start_offset += file_info.length;
        }

        Ok(buffer)
    }

    pub fn verify_piece(
        &self,
        piece_index: usize,
        expected_hash: &[u8; 20],
    ) -> Result<bool, std::io::Error> {
        let data = self.read_piece(piece_index)?;

        let mut hasher = Sha1::new();
        hasher.update(&data);
        let hash = hasher.finalize();

        let hash_bytes: [u8; 20] = hash.into();
        Ok(&hash_bytes == expected_hash)
    }
}
