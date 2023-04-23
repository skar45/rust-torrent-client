pub mod tracker {
    use reqwest;
    use std::borrow::Borrow;
    pub use std::fmt::Display;
    use url::form_urlencoded::byte_serialize;

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
     * Torrent state data to be sent to the tracker
     */
    pub fn url_builder(url: String, peer_id: String, left: i32) -> AnnounceURL {
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

    /**
     * Connect to the tracker and get metadata
     */
    pub async fn get_data(
        request: &mut AnnounceURL,
        hash: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        // let url = Url::parse_with_params(
        //     &format!("{url}?info_hash={info_hash}"),
        // &[
        //     ("peer_id", request.peer_id.to_owned()),
        //     ("uploaded", request.uploaded.to_string()),
        //     ("downloaded", request.downloaded.to_string()),
        //     ("left", request.left.to_string()),
        //     ("event", request.event.to_string()),
        // ],
        // )?;

        println!("MAKING A REQUEST");
        println!("URL: {}", url);

        let response = &client.get(url).send().await?;

        println!("Request result: {:?}", response);

        Ok(())
    }
}
