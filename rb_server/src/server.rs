use crate::config::RbServerConfig;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

pub struct RbServer {
    config: RbServerConfig,
    connections: Vec<TcpStream>,
}

impl RbServer {
    pub fn new(config: RbServerConfig) -> RbServer {
        let connections = Vec::new();
        RbServer {
            config,
            connections,
        }
    }

    pub fn start(&mut self) {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(addr).unwrap();

        log::info!("Listening on {}:{}", self.config.host, self.config.port);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    self.connections.push(stream.try_clone().unwrap());
                    thread::spawn(move || {
                        Self::handle_client(stream);
                    });
                }
                Err(e) => {
                    log::error!("Error: {}", e);
                }
            }
        }
    }

    fn handle_client(mut stream: TcpStream) {
        let peer_addr = stream.peer_addr().unwrap();
        log::info!("New connection from: {}", peer_addr);

        let mut buffer = [0; 1024];

        // Read from the client and echo back the data
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    log::info!("Connection closed by client: {}", peer_addr);
                    break;
                }
                Ok(n) => {
                    log::info!("Received {} bytes from {}", n, peer_addr);

                    if let Err(e) = stream.write_all(&buffer[0..n]) {
                        log::error!("Failed to write to socket: {}", e);
                        break;
                    }

                    if let Err(e) = stream.flush() {
                        log::error!("Failed to flush socket: {}", e);
                        break;
                    }

                    log::info!("Echoed back {} bytes to {}", n, peer_addr);
                }
                Err(e) => {
                    log::error!("Error reading from socket: {}", e);
                    break;
                }
            }
        }
    }
}
