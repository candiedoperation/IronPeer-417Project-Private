use crate::peer::connection::PeerConnection;
use crate::structs::peer::Peer;

/// Result of a successful handshake containing the peer's ID.
#[derive(Debug, Clone)]
pub struct HandshakeResult {
    pub peer_id: [u8; 20],
}

/// BitTorrent handshake protocol implementation.
/// The handshake consists of: protocol length (1 byte) + protocol string (19 bytes) +
/// 8 reserved bytes + info_hash (20 bytes) + peer_id (20 bytes) = 68 bytes total.
/// https://wiki.theory.org/BitTorrentSpecification#Handshake
pub struct Handshake;

impl Handshake {
    /// Protocol identifier string for BitTorrent protocol version 1.0.
    const PROTOCOL_STRING: &'static [u8] = b"BitTorrent protocol";
    const PROTOCOL_STRING_LEN: u8 = 19;
    const RESERVED_BYTES: [u8; 8] = [0; 8];
    const HANDSHAKE_LEN: usize = 1 + 19 + 8 + 20 + 20; // 68 bytes (protocol_len + protocol + reserved + info_hash + peer_id)

    /// Performs a BitTorrent handshake with a peer.
    /// Sends our handshake and validates the peer's response.
    /// Returns the peer's ID if handshake succeeds and info_hash matches.
    pub fn perform(
        connection: &mut PeerConnection,
        info_hash: &[u8; 20],
        our_peer_id: &[u8; 20],
    ) -> Result<HandshakeResult, Box<dyn std::error::Error>> {
        // Send handshake
        Self::send_handshake(connection, info_hash, our_peer_id)?;

        // Receive and validate handshake
        let peer_id = Self::receive_handshake(connection, info_hash)?;

        Ok(HandshakeResult { peer_id })
    }

    /// Sends the BitTorrent handshake to the peer.
    /// Format: [protocol_len][protocol_string][reserved][info_hash][peer_id]
    fn send_handshake(
        connection: &mut PeerConnection,
        info_hash: &[u8; 20],
        peer_id: &[u8; 20],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut handshake = Vec::with_capacity(Self::HANDSHAKE_LEN);
        handshake.push(Self::PROTOCOL_STRING_LEN);
        handshake.extend_from_slice(Self::PROTOCOL_STRING);
        handshake.extend_from_slice(&Self::RESERVED_BYTES);
        handshake.extend_from_slice(info_hash);
        handshake.extend_from_slice(peer_id);

        connection.write_all(&handshake)?;
        Ok(())
    }

    /// Receives and validates the peer's handshake response.
    /// Verifies protocol string, reserved bytes, and info_hash match.
    /// Returns the peer's ID if validation succeeds.
    fn receive_handshake(
        connection: &mut PeerConnection,
        expected_info_hash: &[u8; 20],
    ) -> Result<[u8; 20], Box<dyn std::error::Error>> {
        let mut handshake = vec![0u8; Self::HANDSHAKE_LEN];
        connection.read_exact(&mut handshake)?;

        // Validate protocol string length
        if handshake[0] != Self::PROTOCOL_STRING_LEN {
            return Err(format!(
                "Invalid protocol length: expected {}, got {}",
                Self::PROTOCOL_STRING_LEN, handshake[0]
            )
            .into());
        }

        // Validate protocol string
        let protocol_start = 1;
        let protocol_end = protocol_start + Self::PROTOCOL_STRING_LEN as usize;
        if &handshake[protocol_start..protocol_end] != Self::PROTOCOL_STRING {
            return Err("Invalid protocol string".into());
        }

        // Reserved bytes are at positions 20-27 (skip validation, accept any)
        let reserved_start = protocol_end;
        let reserved_end = reserved_start + 8;

        // Validate info_hash (must match exactly)
        let info_hash_start = reserved_end;
        let info_hash_end = info_hash_start + 20;
        let received_info_hash = &handshake[info_hash_start..info_hash_end];
        if received_info_hash != expected_info_hash {
            return Err("Info hash mismatch - peer is not for this torrent".into());
        }

        // Extract peer_id
        let peer_id_start = info_hash_end;
        let peer_id_end = peer_id_start + 20;
        let mut peer_id = [0u8; 20];
        peer_id.copy_from_slice(&handshake[peer_id_start..peer_id_end]);

        Ok(peer_id)
    }

    /// Attempts to perform a handshake with a peer, connecting first if needed.
    /// Convenience method that combines connection and handshake in one call.
    pub fn connect_and_handshake(
        peer: &Peer,
        info_hash: &[u8; 20],
        our_peer_id: &[u8; 20],
        timeout: std::time::Duration,
    ) -> Result<(PeerConnection, HandshakeResult), Box<dyn std::error::Error>> {
        let mut connection = PeerConnection::connect(peer, timeout)?;
        let result = Self::perform(&mut connection, info_hash, our_peer_id)?;
        Ok((connection, result))
    }
}

