use rand::Rng;
use std::net::UdpSocket;
use std::time::Duration;

const UDP_CONNECT_ACTION: u32 = 0;
const UDP_ANNOUNCE_ACTION: u32 = 1;
const UDP_PROTOCOL_ID: u64 = 0x41727101980; // Magic constant for UDP tracker protocol

#[derive(Debug)]
pub struct UdpTrackerClient {
    socket: UdpSocket,
}

impl UdpTrackerClient {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(Duration::from_secs(5)))?;
        socket.set_write_timeout(Some(Duration::from_secs(5)))?;

        Ok(Self { socket })
    }

    fn parse_udp_url(url: &str) -> Result<(String, u16), Box<dyn std::error::Error>> {
        // Parse UDP tracker URL (udp://tracker.example.com:6969/announce)
        if !url.starts_with("udp://") {
            return Err("Not a UDP URL".into());
        }

        let without_protocol = &url[6..]; // Remove "udp://"
        let parts: Vec<&str> = without_protocol.split('/').collect();
        let host_port = parts[0];

        let host_port_parts: Vec<&str> = host_port.split(':').collect();
        if host_port_parts.len() != 2 {
            return Err("Invalid UDP URL format".into());
        }

        let host = host_port_parts[0].to_string();
        let port = host_port_parts[1].parse::<u16>()?;

        Ok((host, port))
    }

    fn send_connect_request(&self, server_addr: &str) -> Result<u64, Box<dyn std::error::Error>> {
        let transaction_id: u32 = rand::thread_rng().gen();

        let mut request = Vec::new();
        request.extend_from_slice(&UDP_PROTOCOL_ID.to_be_bytes());
        request.extend_from_slice(&UDP_CONNECT_ACTION.to_be_bytes());
        request.extend_from_slice(&transaction_id.to_be_bytes());

        self.socket.send_to(&request, server_addr)?;

        let mut response = vec![0u8; 16];
        let (len, _) = self.socket.recv_from(&mut response)?;

        if len < 16 {
            return Err("Invalid connect response length".into());
        }

        let response_action =
            u32::from_be_bytes([response[0], response[1], response[2], response[3]]);
        let response_transaction_id =
            u32::from_be_bytes([response[4], response[5], response[6], response[7]]);

        if response_action != UDP_CONNECT_ACTION {
            return Err("Invalid connect response action".into());
        }

        if response_transaction_id != transaction_id {
            return Err("Transaction ID mismatch".into());
        }

        let connection_id = u64::from_be_bytes([
            response[8],
            response[9],
            response[10],
            response[11],
            response[12],
            response[13],
            response[14],
            response[15],
        ]);

        Ok(connection_id)
    }

    pub fn announce(
        &self,
        url: &str,
        info_hash: &[u8; 20],
        peer_id: &[u8; 20],
        downloaded: u64,
        left: u64,
        uploaded: u64,
        port: u16,
    ) -> Result<Vec<crate::structs::peer::Peer>, Box<dyn std::error::Error>> {
        let (host, tracker_port) = Self::parse_udp_url(url)?;
        let server_addr = format!("{}:{}", host, tracker_port);

        // Step 1: Connect
        let connection_id = self.send_connect_request(&server_addr)?;

        // Step 2: Announce
        let transaction_id: u32 = rand::thread_rng().gen();

        let mut request = Vec::new();
        request.extend_from_slice(&connection_id.to_be_bytes());
        request.extend_from_slice(&UDP_ANNOUNCE_ACTION.to_be_bytes());
        request.extend_from_slice(&transaction_id.to_be_bytes());
        request.extend_from_slice(info_hash);
        request.extend_from_slice(peer_id);
        request.extend_from_slice(&downloaded.to_be_bytes());
        request.extend_from_slice(&left.to_be_bytes());
        request.extend_from_slice(&uploaded.to_be_bytes());
        request.extend_from_slice(&0u32.to_be_bytes()); // event: 0 = none, 2 = started
        request.extend_from_slice(&0u32.to_be_bytes()); // IP address (0 = default)
        request.extend_from_slice(&0u32.to_be_bytes()); // key
        request.extend_from_slice(&(-1i32).to_be_bytes()); // num_want (-1 = default)
        request.extend_from_slice(&port.to_be_bytes());

        self.socket.send_to(&request, &server_addr)?;

        let mut response = vec![0u8; 2048];
        let (len, _) = self.socket.recv_from(&mut response)?;

        if len < 20 {
            return Err("Invalid announce response length".into());
        }

        let response_action =
            u32::from_be_bytes([response[0], response[1], response[2], response[3]]);
        let response_transaction_id =
            u32::from_be_bytes([response[4], response[5], response[6], response[7]]);

        if response_action != UDP_ANNOUNCE_ACTION {
            return Err("Invalid announce response action".into());
        }

        if response_transaction_id != transaction_id {
            return Err("Transaction ID mismatch".into());
        }

        // Parse peers (starts at offset 20)
        let peers_data = &response[20..len];
        let peers = crate::structs::peer::Peer::from_compact_bytes(peers_data)?;

        Ok(peers)
    }
}
