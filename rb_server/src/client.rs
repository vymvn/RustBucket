use std::net::TcpStream;

pub struct Client {
    pub addr: String,
    // username: String, // To be added when authentication is implemented
    pub stream: TcpStream,
}

impl Client {
    pub fn new(addr: String, stream: TcpStream) -> Client {
        Client { addr, stream }
    }
}
