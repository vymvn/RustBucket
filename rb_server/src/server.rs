use crate::client::Client;
use crate::config::RbServerConfig;
use crate::listener::HttpListener;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use crate::handler::CommandHandler;
use rb::command::op_commands;
use rb::{CommandRegistry, ResponseStatus};
use rb::{CommandRequest, CommandResponse};

fn initialize_commands() -> CommandRegistry {
    let mut registry = CommandRegistry::new();

    // Register commands
    registry.register(op_commands::PingCommand);

    // Initialize help command with registry reference
    let help_cmd = op_commands::HelpCommand::new();
    registry.register(help_cmd);

    registry
}

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<Mutex<Vec<Client>>>,
    pub listeners: Arc<Mutex<Vec<HttpListener>>>,
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

        // Initialize command registry
        let registry = initialize_commands();

        // Create command handler
        // let command_handler = Arc::new(CommandHandler::new(registry, context.clone()));
        let command_handler = Arc::new(CommandHandler::new(registry));

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
                        let command_handler_for_thread = command_handler.clone();
                        thread::spawn(move || {
                            Self::handle_client(
                                stream,
                                addr_str,
                                connection_running,
                                clients_for_thread,
                                listeners_for_thread,
                                command_handler_for_thread,
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

    pub async fn stop(&mut self) {
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
            listener
                .stop()
                .await
                .unwrap_or_else(|e| panic!("couldn't stop listener {}: {}", listener.name(), e));
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
        listeners: Arc<Mutex<Vec<HttpListener>>>,
        command_handler: Arc<CommandHandler>,
    ) -> Result<bool, std::io::Error> {
        let parts: Vec<String> = command
            .trim()
            .split_whitespace()
            .map(String::from)
            .collect();

        if !parts.is_empty() {
            let command_name = &parts[0];
            let args = parts[1..].to_vec();

            // Create request and use command handler
            let request = CommandRequest {
                command: command_name.to_string(),
                args,
                id: uuid::Uuid::new_v4().to_string(),
            };

            let response = command_handler.handle_request(request);

            // Write response in human-readable format for interactive use
            match response.status {
                ResponseStatus::Success => {
                    if let Some(result) = response.result {
                        let _ = stream.write_all(result.as_bytes());
                        if !result.ends_with('\n') {
                            let _ = stream.write_all(b"\n");
                        }
                    }
                }
                ResponseStatus::Error => {
                    let error_msg = response
                        .error
                        .unwrap_or_else(|| "Unknown error".to_string());
                    let _ = stream.write_all(format!("Error: {}\n", error_msg).as_bytes());
                }
            }
        }

        Ok(false)
    }

    fn handle_client(
        mut stream: TcpStream,
        addr: String,
        running: Arc<AtomicBool>,
        clients: Arc<Mutex<Vec<Client>>>,
        listeners: Arc<Mutex<Vec<HttpListener>>>,
        command_handler: Arc<CommandHandler>,
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
                        command_handler.clone(),
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
