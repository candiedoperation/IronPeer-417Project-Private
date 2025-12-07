/// BitTorrent peer wire protocol message types.
/// All messages follow the format: <length prefix><message ID><payload>
/// where length prefix is 4 bytes (big-endian), message ID is 1 byte (except keep-alive).
/// https://wiki.theory.org/BitTorrentSpecification#Messages
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// Keep-alive message: <len=0000> (no message ID, no payload)
    KeepAlive,
    /// Choke message: <len=0001><id=0>
    /// Peer is choking us, we cannot request pieces.
    Choke,
    /// Unchoke message: <len=0001><id=1>
    /// Peer is unchoking us, we can request pieces.
    Unchoke,
    /// Interested message: <len=0001><id=2>
    /// We are interested in pieces the peer has.
    Interested,
    /// Not interested message: <len=0001><id=3>
    /// We are not interested in pieces the peer has.
    NotInterested,
    /// Have message: <len=0005><id=4><piece index>
    /// Peer announces it has completed a piece.
    Have {
        piece_index: u32,
    },
    /// Bitfield message: <len=0001+X><id=5><bitfield>
    /// Peer sends its bitfield indicating which pieces it has.
    /// Bitfield is a bit array where each bit represents a piece (1 = has, 0 = doesn't have).
    Bitfield {
        bits: Vec<u8>,
    },
    /// Request message: <len=0013><id=6><index><begin><length>
    /// Request a block of data from a piece.
    Request {
        index: u32,   // Piece index
        begin: u32,   // Byte offset within piece
        length: u32,  // Block length (typically 16KB)
    },
    /// Piece message: <len=0009+X><id=7><index><begin><block>
    /// Peer sends a block of data we requested.
    Piece {
        index: u32,   // Piece index
        begin: u32,   // Byte offset within piece
        block: Vec<u8>, // Block data
    },
    /// Cancel message: <len=0013><id=8><index><begin><length>
    /// Cancel a previous request (used in end-game mode).
    Cancel {
        index: u32,
        begin: u32,
        length: u32,
    },
    /// Port message: <len=0003><id=9><listen-port>
    /// DHT port announcement (for DHT support).
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
    pub fn total_len(&self) -> usize {
        let payload = self.payload_len();
        let id_len = if self.id().is_some() { 1 } else { 0 };
        4 + id_len + payload
    }
}

