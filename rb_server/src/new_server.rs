use crate::config::RbServerConfig;
use crate::listener;
use rb::client::Client;
use rb::session::Session;

use core::str;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use uuid::Uuid;

use rb::command::{CommandRegistry, CommandResult};

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<Mutex<Vec<Client>>>,
    listeners: Arc<Mutex<Vec<Box<dyn listener::Listener + Send + Sync>>>>,
    sessions: Arc<Mutex<Vec<Session>>>,
    running: Arc<AtomicBool>,
    server_task: Mutex<Option<JoinHandle<()>>>,
    command_registry: Arc<CommandRegistry>,
}

impl RbServer {
    /// Create a new RbServer instance with the given configuration
    pub fn new(config: RbServerConfig) -> Self {
        RbServer {
            config,
            clients: Arc::new(Mutex::new(Vec::new())),
            listeners: Arc::new(Mutex::new(Vec::new())),
            sessions: Arc::new(Mutex::new(Vec::new())),
            running: Arc::new(AtomicBool::new(false)),
            server_task: Mutex::new(None),
            command_registry: Arc::new(CommandRegistry::new()),
        }
    }

    /// Start the C2 server
    pub async fn start(&self) -> io::Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);

        // Bind to the address specified in the config
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("Server listening on {}", addr);

        let running = self.running.clone();
        let clients = self.clients.clone();
        let sessions = self.sessions.clone();
        let command_registry = self.command_registry.clone();

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        println!("New connection from: {}", addr);

                        let client = Client::new(socket);
                        let client_id = client.id();

                        {
                            let mut client_list = clients.lock().unwrap();
                            client_list.push(client.clone());
                        }

                        let client_list = clients.clone();
                        let session_list = sessions.clone();
                        let command_registry_clone = command_registry.clone();

                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_client(
                                client,
                                client_id,
                                client_list,
                                session_list,
                                command_registry_clone,
                            )
                            .await
                            {
                                eprintln!("Error handling client {}: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {}", e);
                    }
                }
            }
        });

        // Store the server task handle
        *self.server_task.lock().unwrap() = Some(handle);

        Ok(())
    }

    /// Stop the C2 server
    pub async fn stop(&self) -> io::Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Set running flag to false to stop the accept loop
        self.running.store(false, Ordering::SeqCst);

        // Wait for the server task to complete
        if let Some(handle) = self.server_task.lock().unwrap().take() {
            // It's generally a good idea to have a timeout here
            match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                Ok(_) => println!("Server shutdown completed"),
                Err(_) => {
                    println!("Server shutdown timed out");
                    // You might want to abort the task or take additional actions
                }
            }
        }

        // Clean up any remaining clients
        let mut clients = self.clients.lock().unwrap();
        clients.clear();

        // Clean up any remaining sessions
        let mut sessions = self.sessions.lock().unwrap();
        sessions.clear();

        Ok(())
    }

    /// Add a listener to the server
    pub fn add_listener(&self, listener: Box<dyn listener::Listener + Send + Sync>) {
        let mut listeners = self.listeners.lock().unwrap();
        listeners.push(listener);
    }

    /// Get the current number of connected clients
    pub fn client_count(&self) -> usize {
        self.clients.lock().unwrap().len()
    }

    /// Get the current number of active sessions
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    /// Handle an individual client connection
    async fn handle_client(
        mut client: Client,
        client_id: Uuid,
        clients: Arc<Mutex<Vec<Client>>>,
        sessions: Arc<Mutex<Vec<Session>>>,
        command_registry: Arc<CommandRegistry>,
    ) -> io::Result<()> {
        // Extract the stream from the client
        let mut stream = match client.take_stream() {
            Some(stream) => stream,
            None => {
                return Err(io::Error::new(io::ErrorKind::Other, "Client has no stream"));
            }
        };

        // Basic read/write loop - customize based on your protocol
        let mut buffer = [0u8; 1024];

        loop {
            match stream.read(&mut buffer).await {
                Ok(0) => {
                    // Connection closed
                    println!("Client {} disconnected", client.addr());
                    break;
                }
                Ok(n) => {
                    // Process the received data - this is where you would
                    // implement your C2 protocol parsing
                    println!("Received {} bytes from {}", n, client.addr());

                    // let (cmd, args) =
                    //     command::CommandParser::parse(str::from_utf8(&buffer[..n]).unwrap());

                    // log::info!("received cmd: {:?} with args {:?}", cmd, args);
                    //

                    let result: CommandResult = command_registry
                        .execute(str::from_utf8(&buffer[..n]).unwrap())
                        .await;

                    print!("cmd result: {:?}", result);
                }
                Err(e) => {
                    eprintln!("Error reading from socket: {}", e);
                    break;
                }
            }
        }

        // Clean up when the client disconnects
        {
            let mut client_list = clients.lock().unwrap();
            client_list.retain(|c| c.id() != client_id);
        }

        Ok(())
    }

    /// Check if the server is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
