/// BitTorrent peer wire protocol message types.
/// All messages follow the format: <length prefix><message ID><payload>
/// where length prefix is 4 bytes (big-endian), message ID is 1 byte (except keep-alive).
/// https://wiki.theory.org/BitTorrentSpecification#Messages
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have {
        piece_index: u32,
    },
    Bitfield {
        bits: Vec<u8>,
    },
    Request {
        index: u32,  // Piece index
        begin: u32,  // Byte offset within piece
        length: u32, // Block length (typically 16KB)
    },

    Piece {
        index: u32,     // Piece index
        begin: u32,     // Byte offset within piece
        block: Vec<u8>, // Block data
    },

    Cancel {
        index: u32,
        begin: u32,
        length: u32,
    },
    Port {
        listen_port: u16,
    },
}

impl Message {
    /// Returns the message ID for encoding.
    pub fn id(&self) -> Option<u8> {
        match self {
            Message::KeepAlive => None,
            Message::Choke => Some(0),
            Message::Unchoke => Some(1),
            Message::Interested => Some(2),
            Message::NotInterested => Some(3),
            Message::Have { .. } => Some(4),
            Message::Bitfield { .. } => Some(5),
            Message::Request { .. } => Some(6),
            Message::Piece { .. } => Some(7),
            Message::Cancel { .. } => Some(8),
            Message::Port { .. } => Some(9),
        }
    }

    /// Returns the payload length (excluding length prefix and message ID).
    pub fn payload_len(&self) -> usize {
        match self {
            Message::KeepAlive => 0,
            Message::Choke | Message::Unchoke | Message::Interested | Message::NotInterested => 0,
            Message::Have { .. } => 4,
            Message::Bitfield { bits } => bits.len(),
            Message::Request { .. } | Message::Cancel { .. } => 12,
            Message::Piece { block, .. } => 8 + block.len(),
            Message::Port { .. } => 2,
        }
    }

    /// Returns the total message length (4 bytes length + 1 byte ID + payload).
    #[allow(dead_code)]
    pub fn total_len(&self) -> usize {
        let payload = self.payload_len();
        let id_len = if self.id().is_some() { 1 } else { 0 };
        4 + id_len + payload
    }
}
