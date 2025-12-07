use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use url::Url;

/// A simple, blocking HTTP client for tracker communication.
/// Implements basic GET requests using raw TCP sockets to avoid external HTTP libraries.
/// https://wiki.theory.org/BitTorrentSpecification#Tracker_HTTP.2FHTTPS_Protocol
pub struct HttpClient;

impl HttpClient {
    /// Sends an HTTP GET request to the specified URL and returns the response body bytes.
    /// Handles DNS resolution, TCP connection, request formatting, and basic response parsing.
    pub fn get(url_str: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let url = Url::parse(url_str)?;

        let host = url.host_str().ok_or("Missing host in URL")?;
        let port = url.port().unwrap_or(80);

        // 1. Connect to the server
        let addr = format!("{}:{}", host, port);
        let mut stream = TcpStream::connect(&addr)?;

        // 2. Construct HTTP request
        // Note: We must include the query string in the path
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

        // 3. Send request
        stream.write_all(request.as_bytes())?;

        // 4. Read response
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
