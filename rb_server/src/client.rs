// use std::net::TcpStream;
use tokio::net::TcpStream;

pub struct Client {
    pub addr: String,
    // username: String, // To be added when authentication is implemented
    pub stream: TcpStream,
}

impl Client {
    pub fn new(stream: TcpStream) -> Client {
        let addr = stream.peer_addr().unwrap().to_string();
        Client { addr, stream }
    }
}
