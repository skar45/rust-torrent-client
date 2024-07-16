pub mod tracker {
    use reqwest::{self};
    pub use std::fmt::Display;
    use std::{borrow::Borrow, error::Error, u8};
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
        fn get_id(id: i32) -> MessageId {
            match id {
                0 => MessageId::Choke,
                1 => MessageId::Unchoke,
                2 => MessageId::Interested,
                3 => MessageId::Interested,
                4 => MessageId::NotInterested,
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
                MessageId::NotInterested => 4,
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
        length: i32,
        id: MessageId,
        payload: Option<Vec<u8>>,
    }

    impl Message {
        // Serialize message into bit pattern: <length><id><paid>
        // length must be big endian
        fn byte_serialize(&self) -> () {
            let length = (&self.length >> 16) & (&self.length & 0xFFFF << 16);
            let id = &self.id.convert();
        }
    }

    pub struct Handshake {
        // length of the pstr, always 0x13
        pstrlen: i32,
        // name of the protocol: `BitTorrent protocol`
        pstr: String,
        reserved_bytes: [u8; 8],
        info_hash: [u8; 20],
        peer_id: String,
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
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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

    pub fn handshake_serialize(info_hash: &Vec<u8>, peer_id: &str) -> String {
        // 0x13 is the length of the message.
        let protocol_identifier = "x13BitTorrent protocol";
        let empty_bits = "0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00";
        let mut hash_string = String::from("");
        for b in info_hash {
            hash_string.push_str(&format!("\\{:#04x}", b));
        }
        println!("handhake message");
        println!("{}", hash_string);
        let handshake_message = format!(
            "\\{}\\{}{}{}",
            protocol_identifier, empty_bits, hash_string, peer_id
        );
        return handshake_message;
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
    use tracker::handshake_serialize;

    #[test]
    fn test_message_serialize() {
        let message = handshake_serialize(
            &vec![
                255, 12, 45, 0, 1, 2, 3, 10, 9, 21, 78, 123, 231, 34, 122, 99, 56, 100, 255, 34,
            ],
            "-TR2940-k8hj0wgej6ch",
        );
        let expected_result =  "\\x13BitTorrent protocol\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0x00\\0xff\\0x0c\\0x2d\\0x00\\0x01\\0x02\\0x03\\0x0a\\0x09\\0x15\\0x4e\\0x7b\\0xe7\\0x22\\0x7a\\0x63\\0x38\\0x64\\0xff\\0x22-TR2940-k8hj0wgej6ch";
        assert_eq!(message, expected_result);
    }
}
