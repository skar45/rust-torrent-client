use std::sync::{Arc, Mutex};

use crate::{
    connect_tracker::tracker::{Handshake, Message, MessageId, PeerConnection},
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
    am_interested: bool,
    am_choking: bool,
    peer_interested: bool,
    peer_choking: bool,
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
                am_interested: false,
                am_choking: true,
                peer_choking: false,
                peer_interested: true,
                peer_info: p.clone(),
            })
            .collect();

        let bitfield_len: usize =
            (info.info_data.length / info.info_data.piece_length / 8) as usize;
        let mut bitfield: Vec<u8> = Vec::with_capacity(bitfield_len);
        for _ in 0..bitfield_len {
            bitfield.push(0x00);
        }

        TorrentState {
            bitfield,
            info: info.clone(),
            peers: peer_state,
        }
    }

    /// Check if peer bitfield has the required piece
    pub fn check_peer_bitfield(&self, bitfield: &[u8]) -> bool {
        if let Some(index) = self.get_next_required_piece_index() {
            let byte_index = index / 8;
            let shift = 7 - (index % 8);
            match bitfield.get(byte_index) {
                Some(v) => {
                    return ((*v >> shift) & 0x1) != 0;
                }
                None => {
                    return false;
                }
            };
        };
        false
    }

    /// Check if the stored bitfield is on for the given index
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

    /// Set the bitfield on at the given index for a given length
    pub fn set_bitfield_on(&mut self, index: usize, length: usize) {
        let payload = 0x1 << length;
        let byte_index = index / 8;
        let shift = 7 - (index % 8);
        if let Some(v) = self.bitfield.get_mut(byte_index) {
            *v = *v | (0x1 << shift);
        };
    }

    /// Set the bitfield of at the given index
    pub fn set_bitfield_off(&mut self, index: usize) {
        let byte_index = index / 8;
        let shift = 7 - (index % 8);
        if let Some(v) = self.bitfield.get_mut(byte_index) {
            *v = *v & !(0x1 << shift);
        };
    }

    /// Get the first index of the bitfield that is off in a sequence
    /// Note: We want to download the pieces sequentially so the download speed will be slower
    pub fn get_next_required_piece_index(&self) -> Option<usize> {
        for (i, byte) in self.bitfield.iter().enumerate() {
            if *byte == 0xff {
                continue;
            };
            for j in 0..8 {
                if (*byte >> (7 - j)) & 0x1 == 0x0 {
                    return Some((i * 8) + (j));
                }
            }
        }
        return None;
    }
}

pub struct SharedTorrentState {
    mutex: Mutex<TorrentState>,
}

impl SharedTorrentState {
    pub fn new(state: TorrentState) -> Self {
        SharedTorrentState {
            mutex: Mutex::new(state),
        }
    }

    pub fn get_handshake(&self, client_id: &str) -> Handshake {
        let lock = self.mutex.lock().expect("Error unable to lock mutex!");
        let handshake = Handshake::new(lock.info.info_hash.clone(), client_id);
        return handshake;
    }

    pub fn get_ip_port(&self, peer_index: usize) -> (String, i32) {
        let lock = self.mutex.lock().expect("Error unable to lock mutex!");
        let peer = &lock.peers[peer_index];
        (peer.peer_info.ip.clone(), peer.peer_info.port)
    }

    pub fn get_required_piece(&self) -> Option<usize> {
        let lock = self.mutex.lock().expect("Error unable to lock mutex!");
        lock.get_next_required_piece_index()
    }

    pub fn set_choke(&self, status: bool, peer_index: usize) {
        let mut lock = self.mutex.lock().expect("Error unable to lock mutex!");
        lock.peers[peer_index].am_choking = status;
    }

    pub fn set_am_interested(&self, status: bool, peer_index: usize) {
        let mut lock = self.mutex.lock().expect("Error unable to lock mutex!");
        lock.peers[peer_index].am_interested = status;
    }

    pub fn set_peer_interested(&self, status: bool, peer_index: usize) {
        let mut lock = self.mutex.lock().expect("Error unable to lock mutex!");
        lock.peers[peer_index].peer_interested = status;
    }

    pub fn check_peer_bitfield(&self, bitfield: &[u8]) -> bool {
        let lock = self.mutex.lock().expect("Error unable to lock mutex!");
        lock.check_peer_bitfield(bitfield)
    }
}

pub async fn start_download(state: TorrentState, client_id: String) {
    let total_peers: usize = state.peers.len();
    let connections: usize = if total_peers < 100 { total_peers } else { 100 };
    println!("total connections: {}", connections);
    let state = Arc::new(SharedTorrentState::new(state));

    let mut threads = vec![];
    for i in 0..(connections - 1) {
        let state = state.clone();
        let shared_id = client_id.clone();
        let thread = tokio::spawn(async move {
            let handshake = state.get_handshake(&shared_id);
            let (ip, port) = state.get_ip_port(i);
            let mut connection = match PeerConnection::new(ip, port).await {
                Ok(stream) => stream,
                Err(e) => {
                    eprint!("Could not connect {}", e);
                    return;
                }
            };
            if let Err(e) = connection.handshake_with_peer(&handshake).await {
                eprint!("Could not send handshake {}", e);
                return;
            };

            loop {
                let response = connection.read_from_stream().await;
                let mut new_msg: Option<Message> = None;

                if response.len() > 0 {
                    // Handshake, Bitfield, Unchoke
                    if response.len() >= 68 {
                        match Handshake::deserialize(&response[0..68]) {
                            Ok(h) => {
                                let given_hash = h.get_hash();
                                for i in 0..given_hash.len() {
                                    if given_hash[i] != handshake.get_hash()[i] {
                                        println!("Hashes don't match!");
                                        return;
                                    }
                                }
                            }
                            Err(_) => {
                                panic!("Error desializing handshake!")
                            }
                        }

                        // TODO: refactor this
                        if response.len() > 68 {
                            let bitfield_msg = Message::read(&response[68..response.len() - 5]);
                            let choke_msg = Message::read(&response[response.len() - 5..]);

                            if let Ok(msg) = choke_msg {
                                if let Some(id) = msg.id {
                                    if id == MessageId::Unchoke {
                                        state.set_choke(false, i);
                                    } else if id == MessageId::Choke {
                                        state.set_choke(true, i);
                                    }
                                }
                            }
                            if let Ok(msg) = bitfield_msg {
                                if let Some(bitfield) = msg.payload {
                                    if !state.check_peer_bitfield(&bitfield) { continue };
                                }
                            }
                        }
                    } else {
                        if let Ok(msg) = Message::read(&response) {
                            if let Some(msg_id) = msg.id {
                                match msg_id {
                                    MessageId::Choke => {
                                        state.set_choke(true, i);
                                    }
                                    MessageId::Unchoke => {
                                        state.set_choke(false, i);
                                        new_msg = Some(Message {
                                            length: 1,
                                            id: Some(MessageId::Interested),
                                            payload: None
                                        })
                                    }
                                    MessageId::Interested => {
                                        state.set_peer_interested(true, i);
                                    }
                                    MessageId::NotInterested => {
                                        state.set_peer_interested(false, i);
                                    }
                                    MessageId::Have => {
                                        println!("Seeding has yet to be implemented");
                                    }
                                    MessageId::Bitfield => {
                                        if let Some(bitfield) = msg.payload {
                                            if !state.check_peer_bitfield(&bitfield) {
                                                continue;
                                            } else {
                                                state.set_am_interested(true, i);
                                                 new_msg = Some(Message {
                                                    length: 1,
                                                    id: Some(MessageId::Interested),
                                                    payload: None
                                                });
                                            }
                                        }
                                    },
                                    MessageId::Piece => {
                                    },
                                    _ => {
                                        println!("Message unsupported!");
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(m) = new_msg {
                    if let Err(e) = connection.send_messsage_to_peer(&m).await {
                        eprintln!("Unable to send payload! {}", e);
                        continue
                    };
                }
            }
        });
        threads.push(thread);
    }

    for t in threads {
        t.await.unwrap_err().is_cancelled();
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
        assert_eq!(torrent_queue.get_next_required_piece_index(), Some(1));
        torrent_queue.set_bitfield_on(1);
        assert_eq!(torrent_queue.get_next_required_piece_index(), Some(2));
    }
}
