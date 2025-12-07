use rustls::pki_types::ServerName;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use url::Url;

/// A simple, blocking HTTP client for tracker communication.
/// Implements basic GET requests using raw TCP sockets to avoid external HTTP libraries.
/// Supports both HTTP and HTTPS (via rustls).
/// https://wiki.theory.org/BitTorrentSpecification#Tracker_HTTP.2FHTTPS_Protocol
pub struct HttpClient;

impl HttpClient {
    /// Sends an HTTP GET request to the specified URL and returns the response body bytes.
    /// Handles DNS resolution, TCP connection, TLS handshake (if HTTPS), request formatting, and basic response parsing.
    pub fn get(url_str: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = Url::parse(url_str)?;
        let host = url.host_str().ok_or("Missing host in URL")?;
        let scheme = url.scheme();

        // Construct HTTP request
        let path = url.path();
        let query = url.query().map(|q| format!("?{}", q)).unwrap_or_default();
        let request_path = if path.is_empty() { "/" } else { path };

        let request = format!(
            "GET {}{} HTTP/1.1\r\n\
             Host: {}\r\n\
             Connection: close\r\n\
             User-Agent: IronPeer/0.1.0\r\n\
             \r\n",
            request_path, query, host
        );

        match scheme {
            "http" => {
                let port = url.port().unwrap_or(80);
                let addr = format!("{}:{}", host, port);
                let mut stream = TcpStream::connect(&addr)?;
                Self::perform_request(&mut stream, &request)
            }
            "https" => {
                let port = url.port().unwrap_or(443);
                let addr = format!("{}:{}", host, port);
                let mut sock = TcpStream::connect(&addr)?;

                // Configure TLS
                let root_store = rustls::RootCertStore::from_iter(
                    webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
                );

                let config = rustls::ClientConfig::builder()
                    .with_root_certificates(root_store)
                    .with_no_client_auth();

                let server_name = ServerName::try_from(host)?.to_owned();
                let mut conn = rustls::ClientConnection::new(Arc::new(config), server_name)?;
                let mut stream = rustls::Stream::new(&mut conn, &mut sock);

                Self::perform_request(&mut stream, &request)
            }
            _ => Err(format!("Unsupported scheme: {}", scheme).into()),
        }
    }

    fn perform_request<S: Read + Write>(
        stream: &mut S,
        request: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Send request
        stream.write_all(request.as_bytes())?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line)?;

        // Parse status code (e.g., "HTTP/1.1 200 OK")
        if !status_line.starts_with("HTTP/1.1 200") && !status_line.starts_with("HTTP/1.0 200") {
            return Err(format!("HTTP request failed: {}", status_line.trim()).into());
        }

        // Read headers until empty line
        let mut content_length = None;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;

            if line == "\r\n" || line == "\n" {
                break;
            }

            let line_lower = line.to_lowercase();
            if line_lower.starts_with("content-length:") {
                if let Some(val) = line.split(':').nth(1) {
                    if let Ok(len) = val.trim().parse::<usize>() {
                        content_length = Some(len);
                    }
                }
            }
        }

        // Read body
        let mut body = Vec::new();
        if let Some(len) = content_length {
            body.resize(len, 0);
            reader.read_exact(&mut body)?;
        } else {
            // No content-length, read until EOF
            reader.read_to_end(&mut body)?;
        }

        Ok(body)
    }
}
