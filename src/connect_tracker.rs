pub mod tracker {
    pub use std::fmt::Display;
    use reqwest::{self};
    use std::{
        borrow::Borrow,
        error::Error,
        u8,
    };
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream};
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
        Port
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
               _ => MessageId::KeepAlive
           } 
        }
    }

    pub struct Message {
        length: i32,
        id: MessageId,
        payload: Option<Vec<u8>>
    }

    impl Message {
        fn byte_serialize(&self) -> Vec<u8> {
             
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
        let mut hash_string  =  "";
        for b in info_hash {
            hash_string = &format!("{}\\{:x}", hash_string, b);
        }
        let handshake_message = format!(
            "\\{}\\{}\\{:?}\\{}",
            protocol_identifier, empty_bits, hash_string, peer_id
        );
        println!("handhake message");
        println!("{}", handshake_message);
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
