mod config;

pub struct RbServer {
    config: RbServerConfig,
}

impl RbServer {
    pub fn new(config: RbServerConfig) -> RbServer {
        RbServer { config }
    }

    pub fn start(&self) {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(addr).unwrap();
        println!("Server started at {}:{}", self.config.host, self.config.port);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    thread::spawn(move || {
                        handle_client(stream);
                    });
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
    }
}
