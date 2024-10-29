pub mod torrent_error {
    use std::error;
    use std::fmt;

    #[derive(Debug)]
    pub enum NetworkError {
        ConnectionClosed,
        EpollError
    }

    impl fmt::Display for  NetworkError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match *self {
                Self::ConnectionClosed => write!(f, "Connection closed by the peer!"),
                Self::EpollError => write!(f, "Error with epoll!")
            }
        }
    }

    impl error::Error for NetworkError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            match *self {
                Self::ConnectionClosed => None,
                Self::EpollError => None
            }
        }
    }
}

