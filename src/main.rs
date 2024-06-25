mod connect_tracker;
mod parse_torrent;
mod parse_tracker_res;

use crate::connect_tracker::tracker;
use crate::parse_torrent::torrent_info::TorrentInfo;
use crate::parse_tracker_res::peers::PeerList;
use bendy::decoding::FromBencode;
use clap::Parser;
use rand::{self, distributions::Alphanumeric, thread_rng, Rng};
use tokio::runtime::Runtime;

/**
 * TODO
 * - [ ] connect to clients
 * - [ ] download data
 * - [ ] http error handling
 * - [ ] bencode parsing error handling
 * - [ ] file integrity check
 * - [ ] Cli error message output
 * - [ ] Unit tests
 */

#[derive(Parser)]
struct Cli {
    torrent: std::path::PathBuf,
}

fn main() {
    let args = Cli::parse();
    let file = std::fs::read(args.torrent).expect("could not read file");
    let torrent_info = TorrentInfo::from_bencode(&file).unwrap();

    let client_id: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect();

    let mut req_data = tracker::url_builder(
        torrent_info.announce.clone(),
        client_id.to_string(),
        torrent_info.info.length,
    );

    let request = tracker::get_data(&mut req_data, &torrent_info.info_hash);
    let rt = Runtime::new().unwrap();
    let tracker_res = rt.block_on(request).unwrap();
    let peer_list = PeerList::from_bencode(&tracker_res).unwrap();
    println!("peers: {:#?}", peer_list);
    // Connect to the first peer
    let handshake_msg = tracker::handshake_serialize(&torrent_info.info_hash, &client_id);
    let first_peer = &peer_list.peers[0];
    let port_string = &first_peer.port.to_string();
    let connect_to_tracker = tracker::connect_to_peer(&handshake_msg, &first_peer.ip, &port_string);
    rt.block_on(connect_to_tracker).unwrap();
}
