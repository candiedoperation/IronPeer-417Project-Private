use serde::{Deserialize, Serialize};
use crate::structs::peer::Peer;

/// Bencoded dictionary returned by tracker in response to an announce request.
/// Contains peer list, statistics, and re-announce interval information.
/// https://wiki.theory.org/BitTorrentSpecification#Tracker_Response
#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse {
    #[serde(rename = "warning message")]
    pub warning_message: Option<String>,
    #[serde(rename = "min interval")]
    pub min_interval: Option<u64>,
    #[serde(default)]
    pub interval: u64,
    #[serde(rename = "tracker id")]
    pub tracker_id: Option<String>,
    pub complete: Option<u64>,
    pub incomplete: Option<u64>,
    #[serde(default)]
    pub peers: PeerList,
    #[serde(rename = "failure reason")]
    pub failure_reason: Option<String>,
}

/// Represents the peer list format from tracker responses.
/// Trackers can return peers in compact binary format (more efficient) or as a list
/// of dictionaries. The compact format is preferred and more commonly used.
#[derive(Debug, Clone)]
pub enum PeerList {
    Compact(Vec<u8>),
    NonCompact(Vec<PeerDict>),
}

impl Default for PeerList {
    fn default() -> Self {
        PeerList::Compact(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDict {
    pub peer_id: Option<String>,
    pub ip: String,
    pub port: u16,
}

impl TrackerResponse {
    /// Checks if the response is empty (no peers).
    pub fn is_empty(&self) -> bool {
        match &self.peers {
            PeerList::Compact(data) => data.is_empty(),
            PeerList::NonCompact(peers) => peers.is_empty(),
        }
    }

    /// Extracts and parses peer addresses from the tracker response.
    /// Handles both compact (binary) and non-compact (dictionary list) formats.
    /// For compact format, attempts IPv4 parsing first (6 bytes) then falls back to IPv6 (18 bytes).
    pub fn parse_peers(&self) -> Vec<Peer> {
        match &self.peers {
            PeerList::Compact(data) => {
                let mut peers = Vec::new();
                let mut offset = 0;
                while offset + 6 <= data.len() {
                    if let Some(peer) = Peer::from_compact(&data[offset..offset + 6]) {
                        peers.push(peer);
                        offset += 6;
                    } else if offset + 18 <= data.len() {
                        if let Some(peer) = Peer::from_compact(&data[offset..offset + 18]) {
                            peers.push(peer);
                            offset += 18;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                peers
            }
            PeerList::NonCompact(peer_dicts) => {
                peer_dicts
                    .iter()
                    .filter_map(|p| {
                        p.ip.parse::<std::net::IpAddr>()
                            .ok()
                            .map(|ip| Peer::new(ip, p.port))
                    })
                    .collect()
            }
        }
    }
}

/// Custom deserializer for PeerList enum. The tracker response can contain peers
/// as either a binary string (compact format) or a list of dictionaries (non-compact).
/// This visitor pattern handles both cases during deserialization.
impl<'de> serde::Deserialize<'de> for PeerList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PeerListVisitor;

        impl<'de> serde::de::Visitor<'de> for PeerListVisitor {
            type Value = PeerList;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string (compact) or list (non-compact)")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PeerList::Compact(v.as_bytes().to_vec()))
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(PeerList::Compact(v.to_vec()))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut peers = Vec::new();
                while let Some(peer) = seq.next_element::<PeerDict>()? {
                    peers.push(peer);
                }
                Ok(PeerList::NonCompact(peers))
            }
        }

        deserializer.deserialize_any(PeerListVisitor)
    }
}

