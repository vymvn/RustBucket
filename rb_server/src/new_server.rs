use crate::certs::{CrlUpdater, TestPki};
use crate::config::RbServerConfig;
use crate::listener;
use futures::{SinkExt, StreamExt};
use rb::client::Client;
use rb::command::CommandContext;
use rb::session::Session;
use rustls::server::Acceptor;
use rustls::ServerConfig;
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use uuid::Uuid;

use rb::command::CommandRegistry;
use rb::message::CommandResult;

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<Mutex<Vec<Client>>>,
    listeners: Arc<Mutex<Vec<Box<dyn listener::Listener>>>>,
    // sessions: Arc<Mutex<Vec<Session>>>,
    sessions: Arc<std::sync::Mutex<HashMap<Uuid, Arc<std::sync::Mutex<rb::session::Session>>>>>,
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
            // sessions: Arc::new(Mutex::new(Vec::new())),
            sessions: Arc::new(std::sync::Mutex::new(HashMap::<
                Uuid,
                Arc<std::sync::Mutex<rb::session::Session>>,
            >::new())),
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

        // Check if mTLS is enabled
        if self.config.mtls.enabled {
            self.start_mtls_server(&addr).await?;
        } else {
            self.start_plain_server(&addr).await?;
        }

        Ok(())
    }

    /// Start a non-TLS server
    async fn start_plain_server(&self, addr: &str) -> io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        log::info!("Server listening on {} (plain TCP)", addr);

        let running = self.running.clone();
        let clients = self.clients.clone();
        let sessions = self.sessions.clone();
        let command_registry = self.command_registry.clone();

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        log::info!("New connection from: {}", addr);

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

    /// Start an mTLS server
    async fn start_mtls_server(&self, addr: &str) -> io::Result<()> {
        // Generate PKI
        let test_pki = Arc::new(TestPki::new());

        // Write the certificates and keys to disk
        test_pki.write_to_disk(
            &self.config.mtls.ca_path,
            &self.config.mtls.client_cert_path,
            &self.config.mtls.client_key_path,
            &self.config.mtls.crl_path,
            self.config.mtls.crl_update_seconds,
        );

        // Start the CRL updater in a tokio task instead of a standard thread
        let crl_updater = CrlUpdater::new(
            std::time::Duration::from_secs(self.config.mtls.crl_update_seconds),
            self.config.mtls.crl_path.clone(),
            test_pki.clone(),
        );
        tokio::spawn(async move {
            // run is not async, so we don't await it
            crl_updater.run();
        });

        // Bind to the address
        let listener = TcpListener::bind(addr).await?;
        log::info!("Server listening on {} (mTLS)", addr);

        let running = self.running.clone();
        let clients = self.clients.clone();
        let sessions = self.sessions.clone();
        let command_registry = self.command_registry.clone();
        let crl_path = self.config.mtls.crl_path.clone();

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match listener.accept().await {
                    Ok((mut stream, addr)) => {
                        log::info!("New TLS connection attempt from: {}", addr);

                        let mut acceptor = Acceptor::default();
                        let test_pki = test_pki.clone();
                        let crl_path = crl_path.clone();

                        // Process the TLS handshake in a separate task
                        let client_list = clients.clone();
                        let session_list = sessions.clone();
                        let running_clone = running.clone();
                        let command_registry_clone = command_registry.clone();

                        tokio::spawn(async move {
                            // Read TLS packets until we've consumed a full client hello
                            let accepted = loop {
                                // Use tokio's AsyncReadExt to read into a buffer first
                                let mut buf = vec![0u8; 8192]; // Reasonable buffer size
                                match stream.read(&mut buf).await {
                                    Ok(0) => {
                                        // Connection closed
                                        log::error!("Connection closed during TLS handshake");
                                        return;
                                    }
                                    Ok(n) => {
                                        // Feed the data into the acceptor
                                        if let Err(e) = acceptor.read_tls(&mut &buf[..n]) {
                                            log::error!("Error reading TLS hello: {}", e);
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Error reading from socket: {}", e);
                                        return;
                                    }
                                }

                                match acceptor.accept() {
                                    Ok(Some(accepted)) => break accepted,
                                    Ok(None) => continue,
                                    Err((e, mut alert)) => {
                                        // Write the alert to the stream using AsyncWriteExt
                                        let mut alert_bytes = Vec::new();
                                        if let Err(write_err) = alert.write_all(&mut alert_bytes) {
                                            log::error!("Failed to write alert: {}", write_err);
                                        }
                                        // Send alert bytes via tokio's AsyncWriteExt
                                        if let Err(send_err) = stream.write_all(&alert_bytes).await {
                                            log::error!("Failed to send alert: {}", send_err);
                                        }
                                        log::error!("Error accepting connection: {}", e);
                                        return;
                                    }
                                }
                            };

                            // Generate a server config for the accepted connection
                            let config = test_pki.server_config(&crl_path, accepted.client_hello());
                            
                            // Complete the TLS handshake - we need to convert the rustls::ServerConfig to tokio_rustls::ServerConfig
                            let tls_stream = match accepted.into_connection(config.clone()) {
                                Ok(conn) => conn,
                                Err((e, mut alert)) => {
                                    // Write the alert using standard Write first
                                    let mut alert_bytes = Vec::new();
                                    if let Err(write_err) = alert.write_all(&mut alert_bytes) {
                                        log::error!("Failed to write alert: {}", write_err);
                                    }
                                    // Send the alert bytes using tokio's AsyncWriteExt
                                    if let Err(send_err) = stream.write_all(&alert_bytes).await {
                                        log::error!("Failed to send alert: {}", send_err);
                                    }
                                    log::error!("Error completing TLS handshake: {}", e);
                                    return;
                                }
                            };

                            // TLS connection established, use it to create a client
                            log::info!("TLS connection established with: {}", addr);

                            // We currently can't directly create a Client with a TLS stream
                            // since Client::new expects TcpStream. We'd need to modify the Client
                            // struct to accept different types of streams, but for now let's
                            // use what we have.
                            let client = Client::new(stream);
                            let client_id = client.id();

                            {
                                let mut client_list = client_list.lock().unwrap();
                                client_list.push(client.clone());
                            }

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
                                log::error!("Error handling client {}: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Error accepting connection: {}", e);
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
                Ok(_) => log::info!("Server shutdown completed"),
                Err(_) => {
                    log::error!("Server shutdown timed out");
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
        // sessions: Arc<Mutex<Vec<Session>>>,
        sessions: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Session>>>>>,
        running: Arc<AtomicBool>,
        command_registry: Arc<CommandRegistry>,
    ) -> io::Result<()> {
        log::debug!("Handling client: {}", client.addr());

        // Extract the TCP stream first
        let mut tcp_stream = client.take_tcp().unwrap();
        // Then split it to avoid ownership issues
        let (reader, writer) = tcp_stream.split();
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
                let mut cmd_context = CommandContext {
                    sessions: sessions.clone(),
                    command_registry: command_registry.clone(),
                };
                let result: CommandResult = command_registry
                    .execute(&mut cmd_context, msg.as_str())
                    .await;

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
