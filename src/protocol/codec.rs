use crate::protocol::messages::Message;
use std::io::Read;

/// Encodes a BitTorrent message into bytes for transmission.
/// Format: [4-byte length (big-endian)][1-byte message ID][payload]
/// Keep-alive messages have length 0 and no ID or payload.
pub struct MessageEncoder;

impl MessageEncoder {
    /// Encodes a message into a byte vector ready for transmission.
    pub fn encode(message: &Message) -> Vec<u8> {
        let payload_len = message.payload_len();
        let id = message.id();
        let total_len = if id.is_some() { 1 + payload_len } else { payload_len };

        let mut buf = Vec::with_capacity(4 + total_len);

        // Length prefix (4 bytes, big-endian)
        buf.extend_from_slice(&(total_len as u32).to_be_bytes());

        // Message ID (1 byte, except for keep-alive)
        if let Some(id) = id {
            buf.push(id);
        }

        // Payload
        match message {
            Message::KeepAlive => {}
            Message::Choke | Message::Unchoke | Message::Interested | Message::NotInterested => {}
            Message::Have { piece_index } => {
                buf.extend_from_slice(&piece_index.to_be_bytes());
            }
            Message::Bitfield { bits } => {
                buf.extend_from_slice(bits);
            }
            Message::Request { index, begin, length } => {
                buf.extend_from_slice(&index.to_be_bytes());
                buf.extend_from_slice(&begin.to_be_bytes());
                buf.extend_from_slice(&length.to_be_bytes());
            }
            Message::Piece { index, begin, block } => {
                buf.extend_from_slice(&index.to_be_bytes());
                buf.extend_from_slice(&begin.to_be_bytes());
                buf.extend_from_slice(block);
            }
            Message::Cancel { index, begin, length } => {
                buf.extend_from_slice(&index.to_be_bytes());
                buf.extend_from_slice(&begin.to_be_bytes());
                buf.extend_from_slice(&length.to_be_bytes());
            }
            Message::Port { listen_port } => {
                buf.extend_from_slice(&listen_port.to_be_bytes());
            }
        }

        buf
    }
}

/// Decodes bytes from a peer connection into BitTorrent messages.
/// Handles partial reads and keeps state for incomplete messages.
pub struct MessageDecoder {
    buffer: Vec<u8>,
}

impl MessageDecoder {
    /// Creates a new message decoder.
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
        }
    }

    /// Reads and decodes the next complete message from the connection.
    /// Returns None if no complete message is available yet (need more data).
    /// Returns Some(Ok(message)) on success, Some(Err) on protocol error.
    pub fn read_message<R: Read>(
        &mut self,
        reader: &mut R,
    ) -> Result<Option<Message>, Box<dyn std::error::Error>> {
        // Ensure we have at least 4 bytes for length prefix
        while self.buffer.len() < 4 {
            let mut temp = [0u8; 1024];
            match reader.read(&mut temp) {
                Ok(0) => {
                    if self.buffer.is_empty() {
                        return Ok(None); // Connection closed, no data
                    }
                    return Err("Connection closed with incomplete message".into());
                }
                Ok(n) => self.buffer.extend_from_slice(&temp[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if self.buffer.len() < 4 {
                        return Ok(None); // Need more data
                    }
                    break; // We have enough
                }
                Err(e) => return Err(Box::new(e)),
            }
        }

        // Read length prefix
        let len_bytes = [self.buffer[0], self.buffer[1], self.buffer[2], self.buffer[3]];
        let message_len = u32::from_be_bytes(len_bytes) as usize;
        self.buffer.drain(0..4);

        // Keep-alive message
        if message_len == 0 {
            return Ok(Some(Message::KeepAlive));
        }

        // Read message payload (ID + data)
        while self.buffer.len() < message_len {
            let needed = message_len - self.buffer.len();
            let mut temp = vec![0u8; needed.min(1024)];
            match reader.read(&mut temp) {
                Ok(0) => return Err("Connection closed while reading message".into()),
                Ok(n) => self.buffer.extend_from_slice(&temp[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Partial read, put length prefix back and return
                    let mut restored = len_bytes.to_vec();
                    restored.extend_from_slice(&self.buffer);
                    self.buffer = restored;
                    return Ok(None);
                }
                Err(e) => return Err(Box::new(e)),
            }
        }

        // Extract and decode message
        let payload = self.buffer.drain(0..message_len).collect::<Vec<_>>();
        Self::decode_message(&payload)
    }

    /// Decodes a message payload (after length prefix) into a Message.
    fn decode_message(payload: &[u8]) -> Result<Option<Message>, Box<dyn std::error::Error>> {
        if payload.is_empty() {
            return Err("Empty message payload".into());
        }

        let id = payload[0];
        let data = &payload[1..];

        let message = match id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => {
                if data.len() < 4 {
                    return Err("Have message too short".into());
                }
                let piece_index = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                Message::Have { piece_index }
            }
            5 => {
                Message::Bitfield {
                    bits: data.to_vec(),
                }
            }
            6 => {
                if data.len() < 12 {
                    return Err("Request message too short".into());
                }
                let index = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let begin = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let length = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                Message::Request { index, begin, length }
            }
            7 => {
                if data.len() < 8 {
                    return Err("Piece message too short".into());
                }
                let index = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let begin = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let block = data[8..].to_vec();
                Message::Piece { index, begin, block }
            }
            8 => {
                if data.len() < 12 {
                    return Err("Cancel message too short".into());
                }
                let index = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let begin = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let length = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
                Message::Cancel { index, begin, length }
            }
            9 => {
                if data.len() < 2 {
                    return Err("Port message too short".into());
                }
                let listen_port = u16::from_be_bytes([data[0], data[1]]);
                Message::Port { listen_port }
            }
            _ => return Err(format!("Unknown message ID: {}", id).into()),
        };

        Ok(Some(message))
    }
}

impl Default for MessageDecoder {
    fn default() -> Self {
        Self::new()
    }
}

