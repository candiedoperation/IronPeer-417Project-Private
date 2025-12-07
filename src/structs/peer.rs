use std::net::{IpAddr, SocketAddr};

/// Represents a peer connection endpoint in the BitTorrent network.
/// Stores the IP address and port number where a peer can be reached.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Peer {
    pub addr: SocketAddr,
}

impl Peer {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self {
            addr: SocketAddr::new(ip, port),
        }
    }

    /// Parses a peer from compact binary format used in tracker responses.
    /// The compact format encodes peers as binary strings: IPv4 uses 6 bytes (4 IP + 2 port),
    /// IPv6 uses 18 bytes (16 IP + 2 port). Ports are stored in big-endian format.
    /// https://wiki.theory.org/BitTorrentSpecification#Tracker_Response
    pub fn from_compact(compact: &[u8]) -> Option<Self> {
        if compact.len() == 6 {
            let ip = IpAddr::from([compact[0], compact[1], compact[2], compact[3]]);
            let port = u16::from_be_bytes([compact[4], compact[5]]);
            Some(Self::new(ip, port))
        } else if compact.len() == 18 {
            let ip_bytes: [u8; 16] = compact[0..16].try_into().ok()?;
            let ip = IpAddr::from(ip_bytes);
            let port = u16::from_be_bytes([compact[16], compact[17]]);
            Some(Self::new(ip, port))
        } else {
            None
        }
    }

    pub fn ip(&self) -> IpAddr {
        self.addr.ip()
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Parses multiple peers from compact binary format.
    /// Used for UDP tracker responses where all peers are in compact format.
    pub fn from_compact_bytes(data: &[u8]) -> Result<Vec<Self>, Box<dyn std::error::Error>> {
        let mut peers = Vec::new();
        let mut offset = 0;

        while offset + 6 <= data.len() {
            if let Some(peer) = Self::from_compact(&data[offset..offset + 6]) {
                peers.push(peer);
            }
            offset += 6;
        }

        Ok(peers)
    }
}

impl From<SocketAddr> for Peer {
    fn from(addr: SocketAddr) -> Self {
        Self { addr }
    }
}
