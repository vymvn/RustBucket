use crate::config::RbServerConfig;
use crate::listener;
use futures::{SinkExt, StreamExt};
use rb::client::Client;
use rb::session::Session;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use uuid::Uuid;

use rb::command::{CommandOutput, CommandRegistry, CommandResult};

use crate::cert_management::CertManager;

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<Mutex<Vec<Client>>>,
    listeners: Arc<Mutex<Vec<Box<dyn listener::Listener>>>>,
    sessions: Arc<Mutex<Vec<Session>>>,
    running: Arc<AtomicBool>,
    server_task: Mutex<Option<JoinHandle<()>>>,
    command_registry: Arc<CommandRegistry>,
    cert_manager: Arc<Mutex<Option<CertManager>>>,
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
            cert_manager: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the C2 server
    pub async fn start(&self) -> io::Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.running.store(true, Ordering::SeqCst);

        // Initialize certificate manager
        {
            let mut cert_manager = self.cert_manager.lock().unwrap();
            *cert_manager = Some(CertManager::new("./certs"));

            if let Some(manager) = &mut *cert_manager {
                manager.generate_certificates().map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Failed to generate certificates: {}", e),
                    )
                })?;

                log::info!("Successfully generated mTLS certificates");
            }
        }

        // Bind to the address specified in the config
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = TcpListener::bind(&addr).await?;
        log::info!("Server listening on {}", addr);

        let running = self.running.clone();
        let clients = self.clients.clone();
        let sessions = self.sessions.clone();
        let command_registry = self.command_registry.clone();
        let cert_manager = self.cert_manager.clone();

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
                        let running_clone = running.clone();
                        let command_registry_clone = command_registry.clone();

                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_client(
                                client,
                                client_id,
                                client_list,
                                session_list,
                                running_clone,
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
        running: Arc<AtomicBool>,
        command_registry: Arc<CommandRegistry>,
        cert_manager: Arc<Mutex<Option<CertManager>>>,
    ) -> io::Result<()> {
        log::debug!("Handling client: {}", client.addr());

        // Extract the TCP stream first
        let mut tcp_stream = client.take_tcp().unwrap();

        // Get TLS configuration
        let server_config = if let Some(manager) = &*cert_manager.lock().unwrap() {
            manager.create_server_config()?
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Certificate manager not initialized",
            ));
        };

        // Accept TLS connection
        let tls_conn = tokio_rustls::TlsAcceptor::from(server_config)
            .accept(tcp_stream)
            .await
            .map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("TLS handshake failed: {}", e))
            })?;

        log::info!("TLS handshake completed with client: {}", client.addr());

        // Then split it to avoid ownership issues
        let (reader, writer) = tokio::io::split(tls_conn);
        let mut stream = FramedRead::new(reader, LinesCodec::new());
        let mut sink = FramedWrite::new(writer, LinesCodec::new());

        // Maybe will add this later for the client to have autocomplete features
        // let commands: Vec<String> = command_registry
        //     .list()
        //     .iter()
        //     .map(|cmd| cmd.name.clone())
        //     .collect();
        //
        // let serialized_cmds = serde_json::to_string(&commands).unwrap();
        //
        // // Send the serialized result to the client
        // if let Err(e) = sink.send(serialized_cmds).await {
        //     log::error!("Failed to send commands to client: {}", e);
        // }

        while running.load(Ordering::SeqCst) {
            while let Some(Ok(msg)) = stream.next().await {
                let result: CommandResult = command_registry.execute(msg.as_str()).await;

                // Serialize the result
                let serialized = match result {
                    Ok(output) => {
                        // Serialize the output
                        serde_json::to_string(&output).unwrap_or_else(|e| {
                            format!("{{\"error\": \"Failed to serialize output: {}\"}}", e)
                        })
                    }
                    Err(err) => {
                        // Serialize the error
                        serde_json::to_string(&err).unwrap_or_else(|e| {
                            format!("{{\"error\": \"Failed to serialize error: {}\"}}", e)
                        })
                    }
                };

                // Send the serialized result to the client
                if let Err(e) = sink.send(serialized).await {
                    log::error!("Failed to send response to client: {}", e);
                    break;
                }

                // Log the command execution
                log::debug!("Executed command: {}", msg);
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
