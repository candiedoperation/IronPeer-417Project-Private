# IronPeer

IronPeer is a BitTorrent client implemented in Rust, for the CMSC417 Final Project.

## Features

### Core Features
- **Torrent Parsing**: Parses `.torrent` files using a custom Bencode implementation.
- **Tracker Communication**: Supports HTTP and UDP trackers using the compact peer list format.
- **Peer Wire Protocol**: Implements the full BitTorrent handshake and message exchange (Choke, Unchoke, Interested, NotInterested, Have, Bitfield, Request, Piece).
- **Download Management**: Orchestrates downloads from multiple peers simultaneously.
- **File Storage**: Supports both single-file and multi-file torrents.

### Extra Credit Features
- **HTTPS Support**: Implements secure tracker communication using `rustls` (wrapping a custom TCP HTTP client).
- **UDP Trackers**: Full support for the UDP tracker protocol.
- **Rarest-First Strategy**: Prioritizes downloading pieces that are rarest among connected peers to improve swarm health.
- **Multi-file Support**: Correctly handles reading/writing blocks that span across file boundaries in multi-file torrents.
- **Seeding**: Supports uploading blocks to other peers (seeding) after download completion or while downloading.

## Prerequisites

- **Rust**: Ensure you have Rust and Cargo installed. You can install them via [rustup](https://rustup.rs/).

## Building

To build the project, run:

```bash
cargo build --release
```

The executable will be located in `target/release/ironpeer`.

## Usage

To download a torrent, use the following command:

```bash
cargo run --release -- -t <path_to_torrent_file>
```

### Options

- `-t, --torrent <PATH>`: Path to the `.torrent` file (Required).
- `-d, --debug`: Enable debug logging for detailed output.

### Example

```bash
cargo run --release -- -t ubuntu-22.04.1-desktop-amd64.iso.torrent
```

## Implementation Details

- **Language**: Rust
- **Networking**: Uses standard `std::net` for TCP/UDP sockets. HTTP and HTTPS clients is handcoded.
- **TLS**: Uses `rustls` for HTTPS support.
- **Concurrency**: Uses threads for handling peer connections and the TCP listener for incoming connections.
