pub mod torrent_info {
    pub use bendy::decoding::{Error, FromBencode, Object, ResultExt};
    use sha1_smol::Sha1;
    #[derive(Debug)]
    pub struct TorrentMetadata {
        pub pieces: Vec<u8>,
        pub piece_length: i32,
        pub length: i32,
        pub name: String,
    }

    #[derive(Debug)]
    pub struct TorrentInfo {
        pub announce: String,
        pub comment: String,
        pub creation_date: i32,
        pub created_by: String,
        pub url_list: Vec<String>,
        pub info: TorrentMetadata,
        pub info_hash: Vec<u8>,
    }

    impl FromBencode for TorrentMetadata {
        fn decode_bencode_object(object: Object) -> Result<Self, bendy::decoding::Error>
        where
            Self: Sized,
        {
            let mut pieces = None;
            let mut piece_length = None;
            let mut length = None;
            let mut name = None;

            let mut decoder = object.try_into_dictionary()?;

            while let Some(pair) = decoder.next_pair()? {
                match pair {
                    (b"pieces", value) => {
                        pieces = value
                            .bytes_or_else(|obj| {
                                Err(Error::unexpected_token("u8", obj.into_token().name()))
                            })
                            .map(Some)?;
                    }
                    (b"piece length", value) => {
                        piece_length = i32::decode_bencode_object(value)
                            .context("piece length")
                            .map(Some)?;
                    }
                    (b"length", value) => {
                        length = i32::decode_bencode_object(value)
                            .context("length")
                            .map(Some)?;
                    }
                    (b"name", value) => {
                        name = String::decode_bencode_object(value)
                            .context("name")
                            .map(Some)?;
                    }
                    _ => {
                        return Err(bendy::decoding::Error::unexpected_field(
                            "[TorrentMetadata]: excessive fields",
                        ))
                    }
                }
            }

            let pieces = (pieces.ok_or_else(|| Error::missing_field("pieces"))?).to_vec();
            let piece_length = piece_length.ok_or_else(|| Error::missing_field("piece_length"))?;
            let length = length.ok_or_else(|| Error::missing_field("length"))?;
            let name = name.ok_or_else(|| Error::missing_field("name"))?;

            Ok(TorrentMetadata {
                pieces,
                piece_length,
                length,
                name,
            })
        }
    }

    impl FromBencode for TorrentInfo {
        fn decode_bencode_object(object: Object) -> Result<Self, bendy::decoding::Error>
        where
            Self: Sized,
        {
            let mut announce = None;
            let mut comment = None;
            let mut creation_date = None;
            let mut created_by = None;
            let mut url_list = None;
            let mut info = None;
            let mut info_hash = None;

            let mut dict = object.try_into_dictionary()?;

            while let Some(pair) = dict.next_pair()? {
                match pair {
                    (b"announce", value) => {
                        announce = String::decode_bencode_object(value)
                            .context("announce")
                            .map(Some)?;
                    }
                    (b"comment", value) => {
                        comment = String::decode_bencode_object(value)
                            .context("comment")
                            .map(Some)?;
                    }
                    (b"creation date", value) => {
                        creation_date = i32::decode_bencode_object(value)
                            .context("creation date")
                            .map(Some)?;
                    }
                    (b"created by", value) => {
                        created_by = String::decode_bencode_object(value)
                            .context("created by")
                            .map(Some)?;
                    }
                    (b"url-list", value) => {
                        url_list = Vec::<String>::decode_bencode_object(value)
                            .context("creation date")
                            .map(Some)?;
                    }
                    (b"info", value) => {
                        let info_dict = value
                            .dictionary_or_else(|obj| Err(obj.into_token()))
                            .unwrap();

                        let info_bytes = info_dict.into_raw().unwrap();
                        let mut hasher = Sha1::new();
                        hasher.update(info_bytes);
                        info_hash = Some(hasher.digest().bytes().to_vec());
                        // info_hash = Some(hasher.digest().bytes());

                        info = TorrentMetadata::from_bencode(info_bytes)
                            .context("info")
                            .map(Some)?;
                    }
                    _ => {
                        return Err(bendy::decoding::Error::unexpected_field(
                            "[TorrentInfo]: excessive fields",
                        ))
                    }
                }
            }
            let announce = announce.ok_or_else(|| Error::missing_field("announce"))?;
            let comment = comment.ok_or_else(|| Error::missing_field("comment"))?;
            let creation_date =
                creation_date.ok_or_else(|| Error::missing_field("creation date"))?;
            let created_by = created_by.ok_or_else(|| Error::missing_field("created by"))?;
            let url_list = url_list.ok_or_else(|| Error::missing_field("url list"))?;
            let info = info.ok_or_else(|| Error::missing_field("info"))?;
            let info_hash = info_hash.ok_or_else(|| Error::missing_field("info"))?;

            Ok(TorrentInfo {
                announce,
                comment,
                creation_date,
                created_by,
                url_list,
                info,
                info_hash,
            })
        }
    }
}
