pub mod tracker {
    use reqwest::{self};
    pub use std::fmt::Display;
    use std::{borrow::Borrow, error::Error, u8, vec};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
    };
    use url::form_urlencoded::byte_serialize;

    use crate::parse_tracker_res::peers::PeerList;

    const LISTENING_PORT: i32 = 8000;

    enum Event {
        Started,
        Stopped,
        Completed,
    }

    impl Display for Event {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Event::Started => write!(f, "started"),
                Event::Stopped => write!(f, "stopped"),
                Event::Completed => write!(f, "completed"),
            }
        }
    }

    pub struct AnnounceURL {
        url: String,
        peer_id: String,
        port: i32,
        uploaded: i32,
        downloaded: i32,
        left: i32,
        event: Event,
    }

    impl AnnounceURL {
        pub fn new(url: String, peer_id: String, left: i32) -> AnnounceURL {
            AnnounceURL {
                url,
                peer_id,
                port: LISTENING_PORT,
                uploaded: 0,
                downloaded: 0,
                left, // TODO: extract the total size of the file
                event: Event::Started,
            }
        }
    }

    enum MessageId {
        KeepAlive,
        Choke,
        Unchoke,
        Interested,
        NotInterested,
        Have,
        Bitfield,
        Request,
        Piece,
        Cancel,
        Port,
    }

    impl MessageId {
        fn get_id(id: u8) -> MessageId {
            match id {
                0 => MessageId::Choke,
                1 => MessageId::Unchoke,
                2 => MessageId::Interested,
                3 => MessageId::Interested,
                5 => MessageId::Have,
                6 => MessageId::Bitfield,
                7 => MessageId::Request,
                8 => MessageId::Piece,
                9 => MessageId::Cancel,
                10 => MessageId::Port,
                _ => MessageId::KeepAlive,
            }
        }

        fn convert(&self) -> u8 {
            match &self {
                MessageId::Choke => 0,
                MessageId::Unchoke => 1,
                MessageId::Interested => 2,
                MessageId::Interested => 3,
                MessageId::Have => 5,
                MessageId::Bitfield => 6,
                MessageId::Request => 7,
                MessageId::Piece => 8,
                MessageId::Cancel => 9,
                MessageId::Port => 10,
                _ => 0,
            }
        }
    }

    pub struct Message {
        length: u32,
        id: Option<MessageId>,
        payload: Option<Vec<u8>>,
    }

    impl Message {
        /**
         * Serialize message into bit pattern: <length><id><payload>.
         * Length must be big endian.
         */
        pub fn byte_serialize(&self) -> Vec<u8> {
            match &self.id {
                None => return vec![0x00, 0x00],
                Some(id) => {
                    let length = &self.length.to_be_bytes();
                    let id = id.convert();
                    let mut ret = vec![];
                    ret.append(&mut length.to_vec());
                    ret.push(id);
                    match &self.payload {
                        None => {
                            return ret;
                        }
                        Some(m) => {
                            ret.append(&mut m.clone());
                            return ret;
                        }
                    }
                }
            }
        }

        pub fn read(message: Vec<u8>) -> Result<Self, Box<dyn Error>> {
            let length = u32::from_be_bytes(
                message[0..4]
                    .try_into()
                    .unwrap_or_else(|_| panic!("Message is less than 4 bytes!")),
            );
            let id = u8::from(message[5])
                .try_into()
                .unwrap_or_else(|_| panic!("Message doesn't have the 5th byte!"));
            if message.len() < (length as usize + 5) {
                panic!("Message length is less than the encoded length: {}", length);
            }
            let payload = message[5..length as usize].to_vec();

            Ok(Message {
                length,
                id: Some(MessageId::get_id(id)),
                payload: Some(payload),
            })
        }
    }

    #[derive(Debug)]
    pub struct Handshake {
        // length of the pstr, always 0x13
        pstrlen: String,
        // name of the protocol: `BitTorrent protocol`
        pstr: String,
        // 8 empty bytes
        reserved_bytes: String,
        info_hash: Vec<u8>,
        peer_id: String,
    }

    impl Handshake {
        pub fn new(info_hash: Vec<u8>, peer_id: &str) -> Self {
            Handshake {
                pstrlen: String::from("0x13"),
                pstr: String::from("BitTorrent protocol"),
                reserved_bytes: String::from("0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00"),
                info_hash,
                peer_id: String::from(peer_id),
            }
        }

        pub fn serialize(&self) -> String {
            let mut hash_string = String::from("");
            for b in &self.info_hash {
                hash_string.push_str(&format!("\\{:#04x}", b));
            }
            let handshake_message = format!(
                "\\{}{}\\{}{}{}",
                self.pstrlen, self.pstr, self.reserved_bytes, hash_string, self.peer_id
            );
            return handshake_message;
        }

        pub fn deserialize(message: &str) -> Result<Self, Box<dyn Error>> {
            let mut id_hash = message
                .split("x13BitTorrent protocol\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00");
            let id_hash = id_hash
                .nth(1)
                .expect("Could not parse info hash and peer id from message.");
            let mut hash = vec![];
            for c in id_hash[..(id_hash.len() - 20)].split("\\") {
                if c.len() < 4 {
                    continue;
                };
                let hex_str = u8::from_str_radix(&c[2..4], 16);
                match hex_str {
                    Ok(v) => hash.push(v),
                    Err(e) => println!("Could not parse hex string: {}", c),
                }
            }
            let peer_id = &id_hash[(id_hash.len() - 20)..];

            Ok(Handshake::new(hash, peer_id))
        }

        pub fn get_hash(&self) -> &Vec<u8> {
            &self.info_hash
        }

        pub fn get_peer_id(&self) -> &str {
            &self.peer_id
        }
    }

    /**
     * Parse url query from key value pairs
     */
    fn parse_query(params: &[(&str, String)]) -> String {
        let mut query_string = String::from("");
        for pair in params {
            let (ref k, ref v) = pair.borrow();
            if query_string.len() == 0 {
                query_string = format!("?{k}={v}");
            } else {
                query_string = format!("{query_string}&{k}={v}");
            }
        }
        query_string
    }

    /**
     * Connect to the tracker and get metadata
     */
    pub async fn get_data(
        request: &mut AnnounceURL,
        hash: &Vec<u8>,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let client = reqwest::Client::new();
        let url = &request.url;
        let info_hash = byte_serialize(&hash).collect::<String>();

        let query = parse_query(&[
            ("info_hash", info_hash),
            ("peer_id", request.peer_id.to_owned()),
            ("uploaded", request.uploaded.to_string()),
            ("downloaded", request.downloaded.to_string()),
            ("left", request.left.to_string()),
            ("event", request.event.to_string()),
        ]);

        let url = format!("{url}{query}");
        let response = &client.get(url).send().await?.bytes().await?;

        Ok(response.to_vec())
    }

    pub async fn connect_to_peer(
        handshake_message: &str,
        peer_list: &PeerList,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        for p in peer_list.peers.iter() {
            if let Ok(mut stream) = TcpStream::connect(format!("{}:{}", p.ip, p.port)).await {
                println!("Connected to ip: {}", p.ip);
                stream
                    .write_all(handshake_message.as_bytes())
                    .await
                    .expect("Could not send message!");

                let mut buffer = Vec::new();
                let m = stream.read_to_end(&mut buffer).await;
                return Ok(buffer[..m.expect("Could not read response!")].to_vec());
            }
        }
        panic!("Could not connect to peer");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker::Handshake;
    use tracker::Message;

    const PAYLOAD: &str = "\\0x13BitTorrent protocol\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0xff\\0x0c\\0x2d\\0x00\\0x01\\0x02\\0x03\\0x0a\\0x09\\0x15\\0x4e\\0x7b\\0xe7\\0x22\\0x7a\\0x63\\0x38\\0x64\\0xff\\0x22-TR2940-k8hj0wgej6ch";
    const PEER_ID: &str = "-TR2940-k8hj0wgej6ch";

    #[test]
    fn message_serialize() {
        let peer_id = PEER_ID;
        let message = Handshake::new(
            vec![
                255, 12, 45, 0, 1, 2, 3, 10, 9, 21, 78, 123, 231, 34, 122, 99, 56, 100, 255, 34,
            ],
            peer_id,
        )
        .serialize();
        let handshake = Handshake::deserialize(&message);
        assert_eq!(message, PAYLOAD);
    }

    #[test]
    fn message_deserialize() {
        let peer_id = PEER_ID;
        let handshake = Handshake::deserialize(PAYLOAD).unwrap();
        assert_eq!(handshake.get_peer_id(), peer_id);
    }

    //     #[test]
    //     fn bitfield_check() {
    //         assert!(!Message::check_piece(vec![0xff, 0xff], 16));
    //         assert!(Message::check_piece(vec![0x00, 0x80], 8) );
    //         assert!(!Message::check_piece(vec![0xff, 0xfe], 15));
    //     }

    //     #[test]
    //     fn bitfield_set() {
    //         let mut bitfield = vec![0x7f, 0xfe, 0x00];
    //         Message::set_bitfield(&mut bitfield, 0);
    //         assert_eq!(bitfield[0], 0xff);
    //         Message::set_bitfield(&mut bitfield, 15);
    //         assert_eq!(bitfield[1], 0xff);
    //         Message::set_bitfield(&mut bitfield, 22);
    //         assert_eq!(bitfield[2], 0x02);
    //     }
}
