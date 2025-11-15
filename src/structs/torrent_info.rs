use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TorrentInfo {
    pub info_hash: [u8; 20],
    pub name: String,
    pub total_size: u64,
    pub piece_length: u64,
    pub num_pieces: usize,
    pub piece_hashes: Vec<[u8; 20]>,
    pub files: Vec<TorrentFile>,
    pub trackers: Vec<Vec<String>>,
    pub is_private: bool,
    pub creation_date: Option<i64>,
    pub created_by: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TorrentFile {
    pub path: PathBuf,
    pub length: u64,
    pub start_piece: usize,
    pub end_piece: usize,
    pub piece_offset_start: u64,
    pub piece_offset_end: u64,
    pub md5sum: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PieceInfo {
    pub index: usize,
    pub length: u64,
    pub hash: [u8; 20],
    pub file_spans: Vec<FileSpan>,
}

#[derive(Debug, Clone)]
pub struct FileSpan {
    pub file_index: usize,
    pub file_offset: u64,
    pub length: u64,
}

impl TorrentInfo {
    pub fn piece_length(&self, piece_index: usize) -> u64 {
        if piece_index == self.num_pieces - 1 {
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
    
    pub fn piece_hash(&self, piece_index: usize) -> Option<&[u8; 20]> {
        self.piece_hashes.get(piece_index)
    }
    
    pub fn get_piece_info(&self, piece_index: usize) -> Option<PieceInfo> {
        if piece_index >= self.num_pieces {
            return None;
        }
        
        let piece_start = piece_index as u64 * self.piece_length;
        let piece_end = piece_start + self.piece_length(piece_index);
        let hash = *self.piece_hash(piece_index)?;
        
        let mut file_spans = Vec::new();
        
        for (file_idx, file) in self.files.iter().enumerate() {
            let file_start = if file_idx == 0 {
                0
            } else {
                self.files[..file_idx].iter().map(|f| f.length).sum()
            };
            let file_end = file_start + file.length;
            
            if piece_start < file_end && piece_end > file_start {
                let span_start = piece_start.max(file_start);
                let span_end = piece_end.min(file_end);
                let file_offset = span_start - file_start;
                let length = span_end - span_start;
                
                file_spans.push(FileSpan {
                    file_index: file_idx,
                    file_offset,
                    length,
                });
            }
        }
        
        Some(PieceInfo {
            index: piece_index,
            length: self.piece_length(piece_index),
            hash,
            file_spans,
        })
    }
    
    pub fn blocks_per_piece(&self, block_size: u32) -> usize {
        ((self.piece_length + block_size as u64 - 1) / block_size as u64) as usize
    }
    
    pub fn all_tracker_urls(&self) -> Vec<String> {
        self.trackers.iter().flatten().cloned().collect()
    }
}

impl TorrentFile {
    pub fn contains_piece(&self, piece_index: usize) -> bool {
        piece_index >= self.start_piece && piece_index <= self.end_piece
    }
    
    pub fn piece_range(&self) -> std::ops::RangeInclusive<usize> {
        self.start_piece..=self.end_piece
    }
}