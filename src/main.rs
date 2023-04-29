mod connect_tracker;
mod parse_tracker_res;
mod parse_torrent;
use crate::connect_tracker::tracker;
use crate::parse_torrent::torrent_info::TorrentInfo;
use crate::parse_tracker_res::peers::PeerList;
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
    let tracker_res = rt.block_on(request).unwrap();
    let peers = PeerList::from_bencode(&tracker_res).unwrap();

    println!("peers: {:#?}", peers);
    // TODO: connect to the peers
}
