use std::{
    cmp,
    sync::{Arc, Mutex},
};

use crate::{
    connect_tracker::tracker::{Handshake, Message, MessageId, PeerConnection},
    parse_torrent::torrent_info::TorrentInfo,
    parse_tracker_res::peers::{Peer, PeerList},
};

const MAX_PIECE_SIZE: usize = 1024;

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

#[derive(Debug)]
pub struct Piece {
    index: usize,
    length: usize,
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
        if let Some(piece) = self.get_next_required_piece_index() {
            let byte_index = piece.index / 8;
            let shift = 7 - (piece.index % 8);
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
        let get_ceil = |x: usize| if x % 8 != 0 { (x / 8) + 1 } else { x / 8 };
        let byte_index = index / 8;
        let byte_len = get_ceil(length + index);
        println!(
            "before bf: {:?}, index: {}, length: {}, byte_length: {} byte_index: {}",
            self.bitfield, index, length, byte_len, byte_index
        );
        for (i, byte) in self.bitfield[byte_index..byte_len].iter_mut().enumerate() {
            if i == 0 {
                let payload_index = index % 8;
                let payload_len = cmp::min(8 - payload_index, length);
                let shift = 8 - (payload_len + payload_index);
                let payload = !0x0 % (u32::pow(2, payload_len as u32));
                *byte = *byte | (payload << shift) as u8;
                println!(
                    "0 value: payload: {} shift: {} payload_len: {} payload_index: {}",
                    payload, shift, payload_len, payload_index
                );
            } else if i != (byte_len - byte_index - 1) {
                println!("yoooo");
                *byte = 0xff;
            } else {
                let payload_len = if (length + index) % 8 == 0 {
                    8
                } else {
                    (length + index) % 8
                };
                let shift = 8 - payload_len;
                let payload = !0x0 % (u32::pow(2, payload_len as u32));
                *byte = *byte | (payload << shift) as u8;
                println!(
                    "end value: payload {} shift {} payload len {}",
                    payload, shift, payload_len
                );
            }
        }
        println!("after bf: {:?}", self.bitfield);
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
    pub fn get_next_required_piece_index(&self) -> Option<Piece> {
        let mut length = 0;
        let mut index = None;
        for (i, byte) in self.bitfield.iter().enumerate() {
            if *byte == 0xff {
                continue;
            };
            for j in 0..8 {
                if (*byte >> (7 - j)) & 0x1 == 0x0 {
                    if length == MAX_PIECE_SIZE {
                        if let Some(index) = index {
                            return Some(Piece { index, length });
                        } else {
                            return None;
                        }
                    } else {
                        length += 1;
                    }
                    if index.is_none() {
                        index = Some((i * 8) + j);
                    }
                } else {
                    if let Some(index) = index {
                        return Some(Piece { index, length });
                    }
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

    pub fn get_required_piece(&self) -> Option<Piece> {
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
                                    if !state.check_peer_bitfield(&bitfield) {
                                        continue;
                                    };
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
                                            payload: None,
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
                                                    payload: None,
                                                });
                                            }
                                        }
                                    }
                                    MessageId::Piece => {}
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
                        continue;
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
        let create_state = || -> TorrentState {
            let t_metadata = TorrentMetadata {
                pieces: vec![],
                piece_length: 2,
                length: 48,
                name: String::from(""),
            };
            TorrentState::new(
                TorrentInfo {
                    announce: String::from(""),
                    comment: String::from(""),
                    creation_date: 0,
                    created_by: String::from(""),
                    url_list: vec![],
                    info_data: t_metadata,
                    info_hash: vec![],
                },
                &PeerList {
                    interval: 0,
                    peers: vec![],
                },
            )
        };
        let mut torrent_state = create_state();
        let mut torrent_state2 = create_state();
        let mut torrent_state3 = create_state();
        let mut torrent_state4 = create_state();
        let mut torrent_state5 = create_state();
        let mut torrent_state6 = create_state();

        torrent_state.set_bitfield_on(1, 16);
        assert_eq!(torrent_state.bitfield[0], 0x7f);
        assert_eq!(torrent_state.bitfield[1], 0xff);
        assert_eq!(torrent_state.bitfield[2], 0x80);
        torrent_state2.set_bitfield_on(2, 4);
        assert_eq!(torrent_state2.bitfield[0], 0x3c);
        assert_eq!(torrent_state2.bitfield[1], 0x00);
        assert_eq!(torrent_state2.bitfield[2], 0x00);
        torrent_state3.set_bitfield_on(9, 15);
        assert_eq!(torrent_state3.bitfield[0], 0x00);
        assert_eq!(torrent_state3.bitfield[1], 0x7f);
        assert_eq!(torrent_state3.bitfield[2], 0xff);
        torrent_state4.set_bitfield_on(12, 4);
        assert_eq!(torrent_state4.bitfield[0], 0x00);
        assert_eq!(torrent_state4.bitfield[1], 0x0f);
        assert_eq!(torrent_state4.bitfield[2], 0x00);
        torrent_state5.set_bitfield_on(18, 6);
        assert_eq!(torrent_state5.bitfield[0], 0x00);
        assert_eq!(torrent_state5.bitfield[1], 0x00);
        assert_eq!(torrent_state5.bitfield[2], 0x3f);
        torrent_state6.set_bitfield_on(0, 24);
        assert_eq!(torrent_state6.bitfield[0], 0xff);
        assert_eq!(torrent_state6.bitfield[1], 0xff);
        assert_eq!(torrent_state6.bitfield[2], 0xff);
        // torrent_queue.set_bitfield_on(22, 1);
        // assert_eq!(torrent_queue.bitfield[2], 0x02);
        // assert!(!torrent_queue.check_piece(23));
        // torrent_queue.set_bitfield_on(8, 9);
        // assert_eq!(torrent_queue.bitfield[2], 0x80);
        // assert!(!torrent_queue.check_piece(23));
        // assert_eq!(torrent_queue.get_next_required_piece_index(), Some(1));
        // torrent_queue.set_bitfield_on(1);
        // assert_eq!(torrent_queue.get_next_required_piece_index(), Some(2));
    }
}
