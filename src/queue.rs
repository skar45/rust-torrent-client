use std::sync::{Arc, Mutex};

use crate::{
    connect_tracker::tracker::{self, Handshake},
    parse_torrent::torrent_info::TorrentInfo,
    parse_tracker_res::peers::{Peer, PeerList},
};

// Exchanging pieces described in `TorrentMetadata`:
// Maintain state with peer: client is choking peer, peer is interested, client is interested, peer is choking client.
// A piece is downloaded when the client is interested in peer and the peer is not choking the client.
// `Handshake` is established first and the the client can begin exchanging `Message` with peers
// Strategy
// Create a queue of pieces to download and then for each peer:
// - Establish handshake with peer
// - Send/Recieve bitfield
// - Potential have message form peer?
// - Track rare pieces (optional)
// - Unchoke
// - Interested
// - Request a piece
// - Download the piece
// - Check if the piece is valid
// - Update bitfield
//
// Seeding:
// - If a request is received, send piece if the piece exists

struct PeerState {
    is_interested: bool,
    is_choked: bool,
    client_interested: bool,
    client_choked: bool,
    peer_info: Peer,
}

pub struct TorrentState {
    bitfield: Vec<u8>,
    info: TorrentInfo,
    peers: Vec<PeerState>,
}

impl TorrentState {
    pub fn new(info: TorrentInfo, peer_list: &PeerList) -> Self {
        let peer_state: Vec<PeerState> = peer_list
            .peers
            .iter()
            .map(|p| PeerState {
                is_interested: false,
                is_choked: false,
                client_choked: false,
                client_interested: true,
                peer_info: p.clone(),
            })
            .collect();

        let bitfield_len: usize =
            (info.info_data.length / info.info_data.piece_length / 8) as usize;
        let mut bitfield: Vec<u8> = Vec::with_capacity(bitfield_len);
        for i in 0..bitfield_len {
            bitfield.push(0x00);
        }

        TorrentState {
            bitfield,
            info: info.clone(),
            peers: peer_state,
        }
    }

    pub fn check_piece(&self, index: usize) -> bool {
        let byte_index = index / 8;
        let shift = 7 - (index % 8);
        match self.bitfield.get(byte_index) {
            Some(v) => {
                return ((*v >> shift) & 0x1) != 0;
            }
            None => {
                return false;
            }
        };
    }

    pub fn set_bitfield_on(&mut self, index: usize) {
        let byte_index = index / 8;
        let shift = 7 - (index % 8);
        if let Some(v) = self.bitfield.get_mut(byte_index) {
            *v = *v | (0x1 << shift);
        };
    }

    pub fn set_bitfield_off(&mut self, index: usize) {
        let byte_index = index / 8;
        let shift = 7 - (index % 8);
        if let Some(v) = self.bitfield.get_mut(byte_index) {
            *v = *v & !(0x1 << shift);
        };
    }

    pub fn get_next_required_piece(&self) -> Option<usize> {
        for (i, byte) in self.bitfield.iter().enumerate() {
            if *byte == 0xff {
                continue;
            };
            for offset in 7..0 {
                if (*byte >> offset) & 0x1 == 0x0 {
                    return Some((i * 8) + (7 - offset));
                }
            }
        }
        return None;
    }
}

pub struct SharedTorrentState {
    mutex: Arc<Mutex<TorrentState>>
}

impl SharedTorrentState {
    pub fn new(state: TorrentState) -> Self {
        SharedTorrentState { mutex: Arc::new(Mutex::new(state)) }
    }

    pub fn get_handshake_payload(&self, peer_index: usize, peer_id: String) -> Option<(Handshake, i32, String)> {
            let lock = &self.mutex.lock().unwrap();
            let peer = lock.peers.get(peer_index);
            if let Some(p) = peer {
                let ip = p.peer_info.ip.clone();
                let port = p.peer_info.port;
                let handshake = Handshake::new(lock.info.info_hash.clone(), &peer_id);
                 Some((handshake, port, ip));
            };
            None
    }
}

pub async fn create_queue(state: TorrentState, client_id: String) {
    let total_peers: usize = state.peers.len();
    let shared_state = SharedTorrentState::new(state);

    for i in 0..(total_peers - 1) {
        let hpi = shared_state.get_handshake_payload(i, client_id.clone());
        tokio::spawn(async move {
            if let Some((handshake, port, ip)) = hpi {
                println!("yooo");
                let res = tracker::handshake_with_peer(&handshake, &ip, port).await;
                println!("yooo");
                let parsed_res = tracker::Message::read(res.unwrap());
                println!("parsed res {:?}", parsed_res.unwrap());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_torrent::torrent_info::TorrentMetadata;

    #[test]
    fn bitfield_set() {
        let peerlist = PeerList {
            interval: 0,
            peers: vec![],
        };

        let t_metadata = TorrentMetadata {
            pieces: vec![],
            piece_length: 2,
            length: 48,
            name: String::from(""),
        };

        let torrent_info = TorrentInfo {
            announce: String::from(""),
            comment: String::from(""),
            creation_date: 0,
            created_by: String::from(""),
            url_list: vec![],
            info_data: t_metadata,
            info_hash: vec![],
        };

        let mut torrent_queue: TorrentState = TorrentState::new(torrent_info, &peerlist);

        torrent_queue.set_bitfield_on(0);
        assert_eq!(torrent_queue.bitfield[0], 0x80);
        assert!(torrent_queue.check_piece(0));
        torrent_queue.set_bitfield_on(15);
        assert_eq!(torrent_queue.bitfield[1], 0x01);
        assert!(torrent_queue.check_piece(15));
        torrent_queue.set_bitfield_on(22);
        assert_eq!(torrent_queue.bitfield[2], 0x02);
        assert!(!torrent_queue.check_piece(23));
    }
}
