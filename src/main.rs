mod connect_tracker;
mod parse_torrent;
mod parse_tracker_res;
mod queue;

use crate::connect_tracker::tracker;
use crate::parse_torrent::torrent_info::TorrentInfo;
use crate::parse_tracker_res::peers::PeerList;
use bendy::decoding::FromBencode;
use clap::Parser;
use connect_tracker::tracker::AnnounceURL;
use queue::TorrentState;
use rand::{self, distributions::{Alphanumeric, Uniform}, thread_rng, Rng};

// TODO
// - [ ] Multifile support
// - [ ] Save state locally
// - [ ] Methods to control which pieces to download
// - [ ] Custom bencode parsing

#[derive(Parser)]
struct Cli {
    torrent: std::path::PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let file = std::fs::read(args.torrent).expect("could not read file");
    let torrent_info = TorrentInfo::from_bencode(&file).unwrap();

    let mut client_id = String::from("-aU0000-");
    let rand_num: String = thread_rng()
        .sample_iter(Uniform::from(0..9))
        .take(12)
        .map(|v| {
            match char::from_digit(v as u32, 10) {
                Some(num) => num,
                None => '0'
            }
        })
        .collect();
    client_id.push_str(&rand_num);
    println!("Client id: {} ", client_id);

    let mut req_data = AnnounceURL::new(
        torrent_info.announce.clone(),
        client_id.to_string(),
        torrent_info.info_data.length,
    );

    let request = tracker::fetch_tracker_data(&mut req_data, &torrent_info.info_hash);
    let tracker_res = request.await.unwrap();
    let peer_list = PeerList::from_bencode(&tracker_res).unwrap();
    println!(
        "tracker response: {}",
        torrent_info.info_data.length / torrent_info.info_data.piece_length
    );
    let torrent_state = TorrentState::new(torrent_info, &peer_list);
    queue::start_download(torrent_state, client_id).await;
    // rt.block_on(create_queue(torrent_state, client_id));
    //
    //     let handshake_msg =
    //         tracker::Handshake::new(torrent_info.info_hash.clone(), &client_id).serialize();
    //     let connect_to_tracker = tracker::connect_to_peer(&handshake_msg, &peer_list);
    //
    //     match rt.block_on(connect_to_tracker) {
    //         Ok(_) => {}
    //         Err(_) => {}
    //     };
}
