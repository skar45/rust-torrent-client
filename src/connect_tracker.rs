pub mod tracker {
    use reqwest::{self};
    pub use std::fmt::Display;
    use std::{borrow::Borrow, error::Error, str::from_utf8, u8, vec};
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

    #[derive(Debug)]
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

    #[derive(Debug)]
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
        pstrlen: usize,
        // name of the protocol: `BitTorrent protocol`
        pstr: String,
        // 8 empty bytes
        reserved_bytes: Vec<u8>,
        info_hash: Vec<u8>,
        peer_id: String,
    }

    impl Handshake {
        pub fn new(info_hash: Vec<u8>, peer_id: &str) -> Self {
            Handshake {
                pstrlen: 0x13,
                pstr: String::from("BitTorrent protocol"),
                reserved_bytes: vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                info_hash,
                peer_id: String::from(peer_id),
            }
        }

        // handshake: <pstrlen><pstr><reserved><info_hash><peer_id>
        pub fn serialize(&self) -> Vec<u8> {
            let mut s_bytes: Vec<u8> = vec![];
            s_bytes.reserve_exact(49 + self.pstrlen);
            s_bytes.push(self.pstrlen as u8);
            s_bytes.append(&mut self.pstr.as_bytes().to_vec());
            s_bytes.append(&mut self.reserved_bytes.clone());
            s_bytes.append(&mut self.info_hash.clone());
            s_bytes.append(&mut self.peer_id.as_bytes().to_vec());
            return s_bytes;
        }

        pub fn deserialize(message: Vec<u8>) -> Result<Self, Box<dyn Error>> {
            let hash = message.get((message.len() - 40)..(message.len() - 20));
            let peer_id = message.get((message.len() - 20)..message.len());

            if let Some(h) = hash {
                if let Some(p_id) = peer_id {
                    let handshake = Handshake::new(h.to_vec(), from_utf8(p_id).unwrap());
                    return Ok(handshake);
                }
            }
            panic!("Error parsing handshake!");
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
    pub async fn fetch_tracker_data(
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
        println!("url: {}", url);
        let response = &client.get(url).send().await?.bytes().await?;

        Ok(response.to_vec())
    }

    pub async fn handshake_with_peer(
        handshake_message: &Handshake,
        ip: &str,
        port: i32,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        if let Ok(mut stream) = TcpStream::connect(format!("{}:{}", ip, port)).await {
            println!("Connected to ip: {}", ip);
            stream
                .write_all(&handshake_message.serialize())
                .await
                .expect("Could not send message!");

            let mut buffer = Vec::new();
            let m = stream.read_to_end(&mut buffer).await;
            return Ok(buffer[..m.expect("Could not read response!")].to_vec());
        }
        panic!("Could not connect to peer");
    }

    pub async fn send_messsage_to_peer(
        message: &Message,
        ip: &str,
        port: i32,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        if let Ok(mut stream) = TcpStream::connect(format!("{}:{}", ip, port)).await {
            println!("Connected to ip: {}", ip);
            stream
                .write_all(&message.byte_serialize())
                .await
                .expect("Could not send message!");

            let mut buffer = Vec::new();
            let m = stream.read_to_end(&mut buffer).await;
            return Ok(buffer[..m.expect("Could not read response!")].to_vec());
        }
        panic!("Could not connect to peer");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracker::Handshake;
    use tracker::Message;

    #[test]
    fn message_serialize() {
        let peer_id = "-TR2940-k8hj0wgej6ch";
        let mut payload: Vec<u8> = vec![0x13];
        payload.append(&mut "BitTorrent protocol".as_bytes().to_vec());
        payload.append(&mut [0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0].to_vec());
        payload.append(
            &mut [
                255, 12, 45, 0, 1, 2, 3, 10, 9, 21, 78, 123, 231, 34, 122, 99, 56, 100, 255, 34,
            ]
            .to_vec(),
        );
        payload.append(&mut peer_id.as_bytes().to_vec());
        let handshake = Handshake::new(
            vec![
                255, 12, 45, 0, 1, 2, 3, 10, 9, 21, 78, 123, 231, 34, 122, 99, 56, 100, 255, 34,
            ],
            peer_id,
        );
        assert_eq!(handshake.serialize(), payload);
        assert_eq!(
            Handshake::deserialize(handshake.serialize())
                .unwrap()
                .serialize(),
            payload
        );
    }
}
