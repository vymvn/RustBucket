use crate::config::RbServerConfig;

use log;
use std::io::Write;
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

        println!(
            "RustBucket server started at {}:{}",
            self.config.host, self.config.port
        );

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    //self.connections.push(stream);
                    thread::spawn(move || {
                        Self::handle_client(stream);
                    });
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
    }

    fn handle_client(mut stream: TcpStream) {
        stream
            .write_all("hello vro".as_bytes())
            .expect("Couldn't send hello :(");

        loop {
            // input-output loop
        }
    }
}
