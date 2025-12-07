use crate::protocol::{
    codec::{MessageDecoder, MessageEncoder},
    messages::Message,
};
use crate::structs::peer::Peer;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// Manages a TCP connection to a BitTorrent peer.
/// Handles connection establishment, timeouts, and basic I/O operations.
/// The connection must be used for handshake before any message exchange.
pub struct PeerConnection {
    stream: TcpStream,
    peer: Peer,
    decoder: MessageDecoder,
}

impl PeerConnection {
    /// Establishes a TCP connection to the peer with a timeout.
    /// Returns an error if connection fails or times out.
    pub fn connect(peer: &Peer, timeout: Duration) -> Result<Self, Box<dyn std::error::Error>> {
        let stream = std::net::TcpStream::connect_timeout(&peer.addr, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.set_nodelay(true)?;

        Ok(Self {
            stream,
            peer: peer.clone(),
            decoder: MessageDecoder::new(),
        })
    }

    /// Creates a connection from an already-established TcpStream.
    pub fn from_stream(stream: TcpStream, peer: Peer) -> Result<Self, Box<dyn std::error::Error>> {
        stream.set_nodelay(true)?;
        Ok(Self {
            stream,
            peer,
            decoder: MessageDecoder::new(),
        })
    }

    /// Reads exactly `len` bytes from the connection.
    /// Blocks until all bytes are received or an error occurs.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Box<dyn std::error::Error>> {
        let mut total_read = 0;
        while total_read < buf.len() {
            match self.stream.read(&mut buf[total_read..]) {
                Ok(0) => return Err("Connection closed by peer".into()),
                Ok(n) => total_read += n,
                Err(e) => return Err(Box::new(e)),
            }
        }
        Ok(())
    }

    /// Writes all bytes to the connection.
    /// Blocks until all bytes are sent or an error occurs.
    pub fn write_all(&mut self, buf: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.stream.write_all(buf)?;
        Ok(())
    }

    /// Gets a reference to the underlying TcpStream for advanced operations.
    pub fn stream(&self) -> &TcpStream {
        &self.stream
    }

    /// Gets a mutable reference to the underlying TcpStream (prolly use it later idk? kept for reference)
    #[allow(dead_code)]
    pub fn stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
    }

    /// Gets the peer address this connection is connected to.
    pub fn peer(&self) -> &Peer {
        &self.peer
    }

    /// Sends a BitTorrent protocol message to the peer.
    /// Encodes the message and writes it to the connection.
    pub fn send_message(&mut self, message: &Message) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = MessageEncoder::encode(message);
        self.write_all(&encoded)?;
        Ok(())
    }

    /// Reads the next complete message from the peer.
    /// Returns None if no complete message is available yet (non-blocking).
    /// Returns Some(Ok(message)) on success, Some(Err) on protocol error.
    pub fn read_message(&mut self) -> Result<Option<Message>, Box<dyn std::error::Error>> {
        self.decoder.read_message(&mut self.stream)
    }
}
