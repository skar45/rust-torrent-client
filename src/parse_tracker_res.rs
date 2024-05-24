pub mod peers {
    pub use bendy::decoding::{Error, FromBencode, Object, ResultExt};

    #[derive(Debug)]
    pub struct Peer {
        pub ip: String,
        pub port: i32,
    }

    #[derive(Debug)]
    pub struct PeerList {
        pub interval: i32,
        pub peers: Vec<Peer>,
    }

    impl FromBencode for PeerList {
        fn decode_bencode_object(object: Object) -> Result<Self, Error>
        where
            Self: Sized,
        {
            let mut interval = None;
            let mut peers = Vec::new();

            let mut decoder = object.try_into_dictionary()?;

            while let Some(pair) = decoder.next_pair()? {
                match pair {
                    (b"interval", obj) => {
                        interval = i32::decode_bencode_object(obj)
                            .context("interval")
                            .map(Some)?
                    }
                    (b"peers", obj) => {
                        let mut list = obj.try_into_list()?;
                        while let Ok(item) = list.next_object() {
                            if let Some(v) = item {
                                let mut ip = None;
                                let mut port = None;
                                let mut peer_dict = v.try_into_dictionary()?;
                                while let Some(peer_dict_pair) = peer_dict.next_pair()? {
                                    match peer_dict_pair {
                                        (b"ip", ip_obj) => {
                                            ip = String::decode_bencode_object(ip_obj)
                                                .context("ip")
                                                .map(Some)?
                                        }
                                        (b"port", port_obj) => {
                                            port = i32::decode_bencode_object(port_obj)
                                                .context("port")
                                                .map(Some)?
                                        }
                                        _ => {
                                            return Err(Error::unexpected_field(
                                                "[PeerList]: excessive fields",
                                            ))
                                        }
                                    }
                                }
                                let ip = ip.ok_or_else(|| Error::missing_field("ip"))?;
                                let port = port.ok_or_else(|| Error::missing_field("port"))?;

                                peers.push(Peer { ip, port });
                            } else {
                                break;
                            }
                        }
                    }
                    _ => return Err(Error::unexpected_field("[TrackerData]: excessive fields")),
                }
            }

            let interval = interval.ok_or_else(|| Error::missing_field("interval"))?;

            Ok(PeerList { interval, peers })
        }
    }
}
