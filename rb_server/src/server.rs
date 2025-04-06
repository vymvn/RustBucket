use crate::config::RbServerConfig;
use crate::listener::HttpListener;
use std::io::{BufRead, BufReader, Write};
// use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use crate::client::Client;
use crate::context::ServerContext;
use crate::handler::CommandHandler;
use rb::command::op_commands;
use rb::command::CommandRegistry;

use rb::message::{CommandRequest, CommandResponse, ResponseStatus};

pub struct RbServer {
    config: RbServerConfig,
    context: Arc<std::sync::Mutex<ServerContext>>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl RbServer {
    pub fn new(config: RbServerConfig) -> RbServer {
        // Create server context
        let context = Arc::new(Mutex::new(ServerContext {
            connected_clients: Arc::new(Mutex::new(Vec::new())),
            connected_agents: Arc::new(Mutex::new(Vec::new())),
            listeners: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            // connected_clients: Vec::new(),
            // connected_agents: Vec::new(),
            // listeners: Vec::new(),
            // running: AtomicBool::new(false),
        }));

        RbServer {
            config,
            context,
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

        // Initialize command registry
        let registry = Self::initialize_commands();

        // Create command handler
        let command_handler = Arc::new(CommandHandler::new(registry, self.context));

        // Set non-blocking mode to allow for graceful shutdown
        listener
            .set_nonblocking(true)
            .expect("Failed to set non-blocking mode");

        log::info!("Listening on {}:{}", self.config.host, self.config.port);

        // Set the running flag to true
        let mut ctx = self.context.lock().unwrap();
        ctx.running.store(true, Ordering::SeqCst);

        // let running = self.running.clone();
        // let clients = self.clients.clone();
        // let listeners = self.listeners.clone();

        // Spawn the server in a separate thread
        self.server_thread = Some(thread::spawn(move || {
            while ctx.running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        log::info!("New connection from: {}", addr);
                        let addr_str = addr.to_string();

                        // Create a new client
                        let client_stream = stream.try_clone().expect("Failed to clone stream");
                        let client = Client::new(addr_str.clone(), client_stream);

                        // Add client to the shared list
                        {
                            let mut clients_lock = ctx.connected_clients.lock().unwrap();
                            clients_lock.push(client);
                        }

                        // Handle client in a separate thread
                        let context_for_thread = self.context.clone();
                        let handler_for_thread = command_handler.clone();

                        thread::spawn(move || {
                            Self::handle_client(stream, addr_str, self.context, handler_for_thread);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No incoming connections, sleep briefly to avoid CPU spin
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::error!("Error accepting connection: {}", e);
                        // Only break if we're shutting down, otherwise continue
                        if !ctx.running.load(Ordering::SeqCst) {
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

        let ctx = self.context.lock().unwrap();

        // Set the running flag to false to signal threads to stop
        ctx.running.store(false, Ordering::SeqCst);

        // Close all open client connections
        let mut clients = ctx.connected_clients.lock().unwrap();
        for client in clients.iter_mut() {
            let _ = client.stream.write_all(b"Server shutting down...\n");
            let _ = client.stream.shutdown(std::net::Shutdown::Both);
        }
        clients.clear();

        // Stop all listeners
        let mut listeners = ctx.listeners.lock().unwrap();
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

    fn initialize_commands() -> CommandRegistry {
        let mut registry = CommandRegistry::new();

        // Register commands
        registry.register(op_commands::PingCommand);

        // Initialize help command with registry reference
        let help_cmd = op_commands::HelpCommand::new();
        registry.register(help_cmd);

        registry
    }

    // async fn parse_and_execute_command(
    //     command: &str,
    //     stream: &mut TcpStream,
    //     listeners: Arc<Mutex<Vec<HttpListener>>>,
    // ) -> Result<bool, std::io::Error> {
    //     let parts: Vec<&str> = command.trim().split_whitespace().collect();
    //     if parts.is_empty() {
    //         stream.write_all(b"No command received\n")?;
    //         return Ok(false);
    //     }
    //
    //     match parts[0] {
    //         "ping" => {
    //             stream.write_all(b"pong\n")?;
    //         }
    //         "echo" => {
    //             let response = format!("{}\n", parts[1..].join(" "));
    //             stream.write_all(response.as_bytes())?;
    //         }
    //         "exit" => {
    //             stream.write_all(b"Closing connection...\n")?;
    //             return Ok(true);
    //         }
    //         "help" => {
    //             let help_text = "\
    //             Available commands:\n\
    //             - ping                       : Test server connectivity\n\
    //             - echo <message>             : Echo back the message\n\
    //             - listener <port> [name]     : Start a new listener on specified port with optional name\n\
    //             - list listeners             : List all active listeners\n\
    //             - stop listener <id>         : Stop a listener by ID\n\
    //             - help                       : Show this help message\n\
    //             - exit                       : Close the connection\n";
    //             stream.write_all(help_text.as_bytes())?;
    //         }
    //         "listener" => {
    //             if parts.len() < 2 {
    //                 stream.write_all(b"Error: Port number required\n")?;
    //                 return Ok(false);
    //             }
    //
    //             let port = match parts[1].parse::<u16>() {
    //                 Ok(p) => p,
    //                 Err(_) => {
    //                     stream.write_all(b"Error: Invalid port number\n")?;
    //                     return Ok(false);
    //                 }
    //             };
    //
    //             // Use custom name if provided, otherwise generate one
    //             let name = if parts.len() >= 3 {
    //                 parts[2].to_string()
    //             } else {
    //                 format!("listener_{}", port)
    //             };
    //
    //             // Create and start the new listener
    //             let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), port);
    //             let mut listener = HttpListener::new(&name, socket);
    //
    //             match listener.start().await {
    //                 Ok(_) => {
    //                     let listener_name = listener.name().to_string();
    //                     let response = format!(
    //                         "Listener {} started successfully on port {}\n",
    //                         listener_name, port
    //                     );
    //                     stream.write_all(response.as_bytes())?;
    //
    //                     // Add to the listeners list
    //                     let mut listeners_lock = listeners.lock().unwrap();
    //                     listeners_lock.push(listener);
    //                 }
    //                 Err(e) => {
    //                     let response =
    //                         format!("Failed to start listener on port {}: {}\n", port, e);
    //                     stream.write_all(response.as_bytes())?;
    //                 }
    //             }
    //         }
    //         "list" => {
    //             if parts.len() < 2 || parts[1] != "listeners" {
    //                 stream.write_all(b"Unknown command. Did you mean 'list listeners'?\n")?;
    //                 return Ok(false);
    //             }
    //
    //             let listeners_lock = listeners.lock().unwrap();
    //             if listeners_lock.is_empty() {
    //                 stream.write_all(b"No active listeners\n")?;
    //             } else {
    //                 stream.write_all(b"Active listeners:\n")?;
    //                 for listener in listeners_lock.iter() {
    //                     let line = format!(
    //                         "{}, ID: {}, Address: {}\n",
    //                         listener.name(),
    //                         listener.id(),
    //                         listener.addr()
    //                     );
    //                     stream.write_all(line.as_bytes())?;
    //                 }
    //             }
    //         }
    //         "stop" => {
    //             if parts.len() < 3 || parts[1] != "listener" {
    //                 stream.write_all(b"Unknown command. Did you mean 'stop listener <name>'?\n")?;
    //                 return Ok(false);
    //             }
    //
    //             let listener_name = parts[2];
    //             let mut listeners_lock = listeners.lock().unwrap();
    //
    //             let position = listeners_lock
    //                 .iter()
    //                 .position(|l| l.name() == listener_name);
    //             match position {
    //                 Some(pos) => {
    //                     // Remove and stop the listener
    //                     let mut listener = listeners_lock.remove(pos);
    //                     listener.stop().await.expect("fuck if I know");
    //                     stream.write_all(
    //                         format!("Listener {} stopped successfully\n", listener_name).as_bytes(),
    //                     )?;
    //                 }
    //                 None => {
    //                     stream.write_all(
    //                         format!("Listener with ID {} not found\n", listener_name).as_bytes(),
    //                     )?;
    //                 }
    //             }
    //         }
    //         _ => {
    //             stream.write_all(b"Unknown command. Type 'help' for available commands.\n")?;
    //         }
    //     }
    //
    //     Ok(false)
    // }

    fn handle_client(
        mut stream: TcpStream,
        addr: String,
        server_context: Arc<std::sync::Mutex<ServerContext>>,
        command_handler: Arc<CommandHandler>,
    ) {
        log::info!("Handling client: {}", addr);

        let ctx = server_context.lock().unwrap();

        // Set read timeout to periodically check if server is still running
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .expect("Failed to set read timeout");

        let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));

        // Send welcome message
        let _ = stream.write_all(
            b"Welcome to RbServer Command Console. Type 'help' for available commands.\n",
        );

        // Create a runtime for handling async commands
        let rt = tokio::runtime::Runtime::new().unwrap();

        while ctx.running.load(Ordering::SeqCst) {
            let mut buffer = String::new();
            match reader.read_line(&mut buffer) {
                Ok(0) => {
                    log::info!("Connection closed by client: {}", addr);
                    break;
                }
                Ok(_) => {
                    // Trim whitespace
                    let input = buffer.trim();

                    // Skip empty lines
                    if input.is_empty() {
                        continue;
                    }

                    // Handle raw command for backward compatibility or handle JSON
                    if input.starts_with("{") {
                        // Process as JSON command request
                        match serde_json::from_str::<CommandRequest>(input) {
                            Ok(request) => {
                                // Process request through command handler
                                let response = command_handler.handle_request(request);

                                // Send response
                                if let Ok(json) = serde_json::to_string(&response) {
                                    if let Err(e) = stream.write_all(json.as_bytes()) {
                                        log::error!("Error writing response: {}", e);
                                        break;
                                    }
                                    // Add newline for better client parsing
                                    if let Err(e) = stream.write_all(b"\n") {
                                        log::error!("Error writing newline: {}", e);
                                        break;
                                    }
                                    if let Err(e) = stream.flush() {
                                        log::error!("Error flushing stream: {}", e);
                                        break;
                                    }
                                } else {
                                    log::error!("Error serializing response");
                                    let _ =
                                        stream.write_all(b"Error: Failed to serialize response\n");
                                }
                            }
                            Err(e) => {
                                log::error!("Invalid JSON request: {}", e);
                                let _ = stream.write_all(
                                    format!("Error: Invalid JSON format - {}\n", e).as_bytes(),
                                );
                            }
                        }
                    } else {
                        // Process as plain text command (for interactive use)
                        let parts: Vec<String> =
                            input.split_whitespace().map(String::from).collect();

                        if !parts.is_empty() {
                            let command_name = &parts[0];
                            let args = parts[1..].to_vec();

                            // Special case for "exit" command to maintain backward compatibility
                            if command_name == "exit" {
                                let _ = stream.write_all(b"Goodbye!\n");
                                break;
                            }

                            // Create request and use command handler
                            let request = CommandRequest {
                                command: command_name.clone(),
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
                                    let _ = stream
                                        .write_all(format!("Error: {}\n", error_msg).as_bytes());
                                }
                            }

                            // // Special async commands handling via runtime
                            // if let Some(async_result) = rt.block_on(handle_async_aspects(
                            //     command_name,
                            //     &args,
                            //     listeners.clone(),
                            // )) {
                            //     if async_result {
                            //         // Special case for async command requesting termination
                            //         break;
                            //     }
                            // }
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
            let mut clients_lock = ctx.connected_clients.lock().unwrap();
            clients_lock.retain(|client| client.addr != addr);
        }

        log::info!("Client handler for {} terminated", addr);
    }

    // fn handle_client(
    //     mut stream: TcpStream,
    //     addr: String,
    //     running: Arc<AtomicBool>,
    //     clients: Arc<Mutex<Vec<Client>>>,
    //     listeners: Arc<Mutex<Vec<HttpListener>>>,
    // ) {
    //     log::info!("Handling client: {}", addr);
    //
    //     let mut context = ServerContext {
    //         connected_agents: vec![],
    //         server_config: Default::default(),
    //     };
    //
    //     let registry = initialize_commands();
    //
    //     // Command handling loop would go here
    //
    //     // Set read timeout to periodically check if server is still running
    //     stream
    //         .set_read_timeout(Some(Duration::from_millis(500)))
    //         .expect("Failed to set read timeout");
    //
    //     let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    //     let mut line = String::new();
    //
    //     // Send welcome message
    //     let _ = stream.write_all(
    //         b"Welcome to RbServer Command Console. Type 'help' for available commands.\n",
    //     );
    //
    //     // Create a runtime for handling async commands
    //     let rt = tokio::runtime::Runtime::new().unwrap();
    //
    //     while running.load(Ordering::SeqCst) {
    //         line.clear();
    //         match reader.read_line(&mut line) {
    //             Ok(0) => {
    //                 log::info!("Connection closed by client: {}", addr);
    //                 break;
    //             }
    //             Ok(_) => {
    //                 // Parse and execute command with async support
    //                 let result = rt.block_on(Self::parse_and_execute_command(
    //                     &line,
    //                     &mut stream,
    //                     listeners.clone(),
    //                 ));
    //
    //                 match result {
    //                     Ok(true) => break, // Exit command received
    //                     Ok(false) => {}    // Continue with next command
    //                     Err(e) => {
    //                         log::error!("Error executing command: {}", e);
    //                         break;
    //                     }
    //                 }
    //             }
    //             Err(e)
    //                 if e.kind() == std::io::ErrorKind::TimedOut
    //                     || e.kind() == std::io::ErrorKind::WouldBlock =>
    //             {
    //                 // Timeout occurred, check if server is still running
    //                 continue;
    //             }
    //             Err(e) => {
    //                 log::error!("Error reading from socket: {}", e);
    //                 break;
    //             }
    //         }
    //     }
    //
    //     // Close the connection when thread terminates
    //     let _ = stream.shutdown(std::net::Shutdown::Both);
    //
    //     // Remove client from the list
    //     {
    //         let mut clients_lock = clients.lock().unwrap();
    //         clients_lock.retain(|client| client.addr != addr);
    //     }
    //
    //     log::info!("Client handler for {} terminated", addr);
    // }
}
