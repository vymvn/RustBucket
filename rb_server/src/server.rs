use crate::config::RbServerConfig;
use crate::listener::Listener;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

pub struct Client {
    addr: String,
    // username: String, // To be added when authentication is implemented
    stream: TcpStream,
}

impl Client {
    pub fn new(addr: String, stream: TcpStream) -> Client {
        Client { addr, stream }
    }
}

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<Mutex<Vec<Client>>>,
    pub listeners: Arc<Mutex<Vec<Listener>>>,
    running: Arc<AtomicBool>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl RbServer {
    pub fn new(config: RbServerConfig) -> RbServer {
        RbServer {
            config,
            clients: Arc::new(Mutex::new(Vec::new())),
            listeners: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            server_thread: None,
        }
    }

    pub fn start(&mut self) {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = match TcpListener::bind(&addr) {
            Ok(listener) => listener,
            Err(e) => {
                log::error!("Failed to bind to address {}: {}", addr, e);
                return;
            }
        };

        // Set non-blocking mode to allow for graceful shutdown
        listener
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode");

        log::info!("Listening on {}:{}", self.config.host, self.config.port);

        // Set the running flag to true
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let clients = self.clients.clone();
        let listeners = self.listeners.clone();

        // Spawn the server in a separate thread
        self.server_thread = Some(thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        log::info!("New connection from: {}", addr);
                        let addr_str = addr.to_string();

                        // Create a new client
                        let client_stream = stream.try_clone().expect("Failed to clone stream");
                        let client = Client::new(addr_str.clone(), client_stream);

                        // Add client to the shared list
                        {
                            let mut clients_lock = clients.lock().unwrap();
                            clients_lock.push(client);
                        }

                        // Handle client in a separate thread
                        let connection_running = running.clone();
                        let clients_for_thread = clients.clone();
                        let listeners_for_thread = listeners.clone();
                        thread::spawn(move || {
                            Self::handle_client(
                                stream,
                                addr_str,
                                connection_running,
                                clients_for_thread,
                                listeners_for_thread,
                            );
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No incoming connections, sleep briefly to avoid CPU spin
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::error!("Error accepting connection: {}", e);
                        // Only break if we're shutting down, otherwise continue
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                }
            }

            log::info!("Server thread terminated");
        }));
    }

    pub fn stop(&mut self) {
        log::info!("Stopping RbServer...");

        // Set the running flag to false to signal threads to stop
        self.running.store(false, Ordering::SeqCst);

        // Close all open client connections
        let mut clients = self.clients.lock().unwrap();
        for client in clients.iter_mut() {
            let _ = client.stream.write_all(b"Server shutting down...\n");
            let _ = client.stream.shutdown(std::net::Shutdown::Both);
        }
        clients.clear();

        // Stop all listeners
        let mut listeners = self.listeners.lock().unwrap();
        for listener in listeners.iter_mut() {
            listener.stop();
        }
        listeners.clear();

        // Wait for the server thread to terminate
        if let Some(server_thread) = self.server_thread.take() {
            match server_thread.join() {
                Ok(_) => log::info!("Server thread joined successfully"),
                Err(_) => log::error!("Error joining server thread"),
            }
        }

        log::info!("RbServer stopped");
    }

    async fn parse_and_execute_command(
        command: &str,
        stream: &mut TcpStream,
        listeners: Arc<Mutex<Vec<Listener>>>,
    ) -> Result<bool, std::io::Error> {
        let parts: Vec<&str> = command.trim().split_whitespace().collect();
        if parts.is_empty() {
            stream.write_all(b"No command received\n")?;
            return Ok(false);
        }

        match parts[0] {
            "ping" => {
                stream.write_all(b"pong\n")?;
            }
            "echo" => {
                let response = format!("{}\n", parts[1..].join(" "));
                stream.write_all(response.as_bytes())?;
            }
            "exit" => {
                stream.write_all(b"Closing connection...\n")?;
                return Ok(true);
            }
            "help" => {
                let help_text = "\
                Available commands:\n\
                - ping                       : Test server connectivity\n\
                - echo <message>             : Echo back the message\n\
                - listener <port> [name]     : Start a new listener on specified port with optional name\n\
                - list listeners             : List all active listeners\n\
                - stop listener <id>         : Stop a listener by ID\n\
                - help                       : Show this help message\n\
                - exit                       : Close the connection\n";
                stream.write_all(help_text.as_bytes())?;
            }
            "listener" => {
                if parts.len() < 2 {
                    stream.write_all(b"Error: Port number required\n")?;
                    return Ok(false);
                }

                let port = match parts[1].parse::<u16>() {
                    Ok(p) => p,
                    Err(_) => {
                        stream.write_all(b"Error: Invalid port number\n")?;
                        return Ok(false);
                    }
                };

                // Use custom name if provided, otherwise generate one
                let name = if parts.len() >= 3 {
                    parts[2].to_string()
                } else {
                    format!("listener_{}", port)
                };

                // Create and start the new listener
                let mut listener = Listener::new("0.0.0.0".to_string(), port, name);

                match listener.start().await {
                    Ok(_) => {
                        let listener_name = listener.name.to_string();
                        let response = format!(
                            "Listener {} started successfully on port {}\n",
                            listener_name, port
                        );
                        stream.write_all(response.as_bytes())?;

                        // Add to the listeners list
                        let mut listeners_lock = listeners.lock().unwrap();
                        listeners_lock.push(listener);
                    }
                    Err(e) => {
                        let response =
                            format!("Failed to start listener on port {}: {}\n", port, e);
                        stream.write_all(response.as_bytes())?;
                    }
                }
            }
            "list" => {
                if parts.len() < 2 || parts[1] != "listeners" {
                    stream.write_all(b"Unknown command. Did you mean 'list listeners'?\n")?;
                    return Ok(false);
                }

                let listeners_lock = listeners.lock().unwrap();
                if listeners_lock.is_empty() {
                    stream.write_all(b"No active listeners\n")?;
                } else {
                    stream.write_all(b"Active listeners:\n")?;
                    for (i, listener) in listeners_lock.iter().enumerate() {
                        let line = format!(
                            "{}) ID: {}, Address: {}:{}\n",
                            i + 1,
                            listener.name,
                            "0.0.0.0",     // Assuming this is the binding address
                            listener.port  // Assuming this field is accessible
                        );
                        stream.write_all(line.as_bytes())?;
                    }
                }
            }
            "stop" => {
                if parts.len() < 3 || parts[1] != "listener" {
                    stream.write_all(b"Unknown command. Did you mean 'stop listener <name>'?\n")?;
                    return Ok(false);
                }

                let listener_name = parts[2];
                let mut listeners_lock = listeners.lock().unwrap();

                let position = listeners_lock.iter().position(|l| l.name == listener_name);
                match position {
                    Some(pos) => {
                        // Remove and stop the listener
                        let mut listener = listeners_lock.remove(pos);
                        listener.stop();
                        stream.write_all(
                            format!("Listener {} stopped successfully\n", listener_name).as_bytes(),
                        )?;
                    }
                    None => {
                        stream.write_all(
                            format!("Listener with ID {} not found\n", listener_name).as_bytes(),
                        )?;
                    }
                }
            }
            _ => {
                stream.write_all(b"Unknown command. Type 'help' for available commands.\n")?;
            }
        }

        Ok(false)
    }

    fn handle_client(
        mut stream: TcpStream,
        addr: String,
        running: Arc<AtomicBool>,
        clients: Arc<Mutex<Vec<Client>>>,
        listeners: Arc<Mutex<Vec<Listener>>>,
    ) {
        log::info!("Handling client: {}", addr);

        // Set read timeout to periodically check if server is still running
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .expect("Failed to set read timeout");

        let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
        let mut line = String::new();

        // Send welcome message
        let _ = stream.write_all(
            b"Welcome to RbServer Command Console. Type 'help' for available commands.\n",
        );

        // Create a runtime for handling async commands
        let rt = tokio::runtime::Runtime::new().unwrap();

        while running.load(Ordering::SeqCst) {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    log::info!("Connection closed by client: {}", addr);
                    break;
                }
                Ok(_) => {
                    // Parse and execute command with async support
                    let result = rt.block_on(Self::parse_and_execute_command(
                        &line,
                        &mut stream,
                        listeners.clone(),
                    ));

                    match result {
                        Ok(true) => break, // Exit command received
                        Ok(false) => {}    // Continue with next command
                        Err(e) => {
                            log::error!("Error executing command: {}", e);
                            break;
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                {
                    // Timeout occurred, check if server is still running
                    continue;
                }
                Err(e) => {
                    log::error!("Error reading from socket: {}", e);
                    break;
                }
            }
        }

        // Close the connection when thread terminates
        let _ = stream.shutdown(std::net::Shutdown::Both);

        // Remove client from the list
        {
            let mut clients_lock = clients.lock().unwrap();
            clients_lock.retain(|client| client.addr != addr);
        }

        log::info!("Client handler for {} terminated", addr);
    }
}
