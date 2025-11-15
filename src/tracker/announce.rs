use url::Url;
use crate::structs::{tracker::TrackerResponse, peer::Peer};

/// Tracks client statistics sent to tracker during announce requests.
/// These values are used by trackers to maintain swarm statistics and determine
/// seeding/leeching status of the client.
#[derive(Debug, Clone)]
pub struct ClientStats {
    pub uploaded: u64,
    pub downloaded: u64,
    pub left: u64,
}

/// Event type sent to tracker indicating the current state of the client.
/// Used to signal when download starts, completes, or stops. None is used for periodic updates.
/// https://wiki.theory.org/BitTorrentSpecification#Tracker_Request_Parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnounceEvent {
    Started,
    Completed,
    Stopped,
    None,
}

impl AnnounceEvent {
    fn as_str(&self) -> Option<&'static str> {
        match self {
            AnnounceEvent::Started => Some("started"),
            AnnounceEvent::Completed => Some("completed"),
            AnnounceEvent::Stopped => Some("stopped"),
            AnnounceEvent::None => None,
        }
    }
}

/// Client for communicating with BitTorrent trackers via HTTP/HTTPS announce protocol.
/// Handles URL construction, parameter encoding, and response parsing to retrieve peer lists.
/// https://wiki.theory.org/BitTorrentSpecification#Tracker_HTTP.2FHTTPS_Protocol
pub struct TrackerClient {
    peer_id: [u8; 20],
    port: u16,
}

impl TrackerClient {
    pub fn new(peer_id: [u8; 20], port: u16) -> Self {
        Self { peer_id, port }
    }

    /// Creates a tracker client with a peer ID generated from a UUIDv4.
    /// The UUID is hashed with SHA1 to produce a consistent 20-byte peer ID.
    /// In the future, this UUID can be saved to a config file for persistence.
    pub fn with_default_peer_id(port: u16) -> Self {
        let peer_id = Self::generate_peer_id_from_uuid();
        Self::new(peer_id, port)
    }

    /// Generates a 20-byte peer ID by hashing a UUIDv4 with SHA1.
    /// Creates a new UUIDv4 each time, which can later be persisted to config.
    fn generate_peer_id_from_uuid() -> [u8; 20] {
        use sha1::{Digest, Sha1};
        use uuid::Uuid;
        
        let uuid = Uuid::new_v4();
        let mut hasher = Sha1::new();
        hasher.update(uuid.as_bytes());
        let hash = hasher.finalize();
        let mut peer_id = [0u8; 20];
        peer_id.copy_from_slice(&hash);
        peer_id
    }

    /// Sends an HTTP GET announce request to the tracker and parses the bencoded response.
    /// Constructs the URL with all required parameters (info_hash, peer_id, stats, event) and
    /// handles URL encoding of binary data. Returns the tracker response containing peer list.
    pub fn announce(
        &self,
        announce_url: &str,
        info_hash: &[u8; 20],
        stats: &ClientStats,
        event: AnnounceEvent,
    ) -> Result<TrackerResponse, Box<dyn std::error::Error>> {
        let url = self.build_announce_url(announce_url, info_hash, stats, event)?;
        
        let response = reqwest::blocking::get(url.as_str())?;
        
        if !response.status().is_success() {
            return Err(format!("Tracker returned error: {}", response.status()).into());
        }

        let body = response.bytes()?;
        
        let tracker_response: TrackerResponse = serde_bencode::from_bytes(&body)
            .map_err(|e| format!("Failed to parse tracker response: {}", e))?;

        // Check for error response from tracker
        if let Some(ref reason) = tracker_response.failure_reason {
            return Err(format!("Tracker error: {}", reason).into());
        }

        // Validate required fields for success response
        if tracker_response.interval == 0 && tracker_response.is_empty() {
            return Err("Invalid tracker response: missing interval and peers".into());
        }

        Ok(tracker_response)
    }

    /// Constructs the tracker announce URL with all required query parameters.
    /// Manually constructs the query string to avoid double-encoding of binary fields.
    /// The url crate's query_pairs would encode our already-encoded binary data.
    fn build_announce_url(
        &self,
        announce_url: &str,
        info_hash: &[u8; 20],
        stats: &ClientStats,
        event: AnnounceEvent,
    ) -> Result<Url, Box<dyn std::error::Error>> {
        let mut url = Url::parse(announce_url)?;
        
        // Manually construct query string to avoid double-encoding binary data
        let mut query_parts = Vec::new();
        query_parts.push(format!("info_hash={}", self.url_encode_bytes(info_hash)));
        query_parts.push(format!("peer_id={}", self.url_encode_bytes(&self.peer_id)));
        query_parts.push(format!("port={}", self.port));
        query_parts.push(format!("uploaded={}", stats.uploaded));
        query_parts.push(format!("downloaded={}", stats.downloaded));
        query_parts.push(format!("left={}", stats.left));
        query_parts.push("compact=1".to_string());
        
        if let Some(event_str) = event.as_str() {
            query_parts.push(format!("event={}", event_str));
        }
        
        query_parts.push("numwant=50".to_string());
        
        url.set_query(Some(&query_parts.join("&")));
        
        Ok(url)
    }

    /// Converts raw bytes to URL-encoded string using percent encoding (%XX format).
    /// Required for info_hash and peer_id parameters since they contain arbitrary binary data
    /// that must be safely transmitted in HTTP query strings. Each byte becomes %XX.
    fn url_encode_bytes(&self, bytes: &[u8]) -> String {
        bytes.iter()
            .map(|&b| format!("%{:02X}", b))
            .collect()
    }

    pub fn peer_id(&self) -> &[u8; 20] {
        &self.peer_id
    }
}

/// Announces to multiple tracker URLs and aggregates unique peers from all responses.
/// Uses a HashSet to deduplicate peers by their socket address, ensuring each peer
/// appears only once in the final list even if multiple trackers return the same peer.
pub fn announce_to_trackers(
    client: &TrackerClient,
    tracker_urls: &[String],
    info_hash: &[u8; 20],
    stats: &ClientStats,
    event: AnnounceEvent,
) -> Vec<Peer> {
    let mut all_peers = Vec::new();
    let mut seen_peers = std::collections::HashSet::new();

    for tracker_url in tracker_urls {
        match client.announce(tracker_url, info_hash, stats, event) {
            Ok(response) => {
                let peers = response.parse_peers();
                for peer in peers {
                    if seen_peers.insert(peer.addr) {
                        all_peers.push(peer);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to announce to {}: {}", tracker_url, e);
            }
        }
    }

    all_peers
}

