mod connect_tracker;
mod parse_torrent;
use crate::connect_tracker::tracker;
use crate::parse_torrent::torrent_info::TorrentInfo;
use bendy::decoding::FromBencode;
use clap::Parser;
use tokio::runtime::Runtime;

/**
 * TODO
 * - http error handling
 * - bencode parsing error handling
 * - file integrity check
 * - Cli error message output
 * - Unit tests
 */

#[derive(Parser)]
struct Cli {
    torrent: std::path::PathBuf,
}

fn main() {
    let arg = Cli::parse();
    let file = std::fs::read(arg.torrent).expect("could not read file");
    let torrent_info = TorrentInfo::from_bencode(&file).unwrap();
    let mut req_data = tracker::url_builder(
        torrent_info.announce.clone(),
        // TODO: generate random 20 byte string for peer_id
        "4DE13F0-RWtU(34AhWe8".to_string(),
        torrent_info.info.length,
    );

    let request = tracker::get_data(&mut req_data, torrent_info.info_hash);
    let rt = Runtime::new().unwrap();
    rt.block_on(request).unwrap();
    // TODO: connect to the peers
}
