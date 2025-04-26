use crate::certs::{CrlUpdater, TestPki};
use crate::config::RbServerConfig;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use rb::client::Client;
use rb::command::CommandContext;
use rb::listener::http_listener::HttpListener;
use rustls::server::Acceptor;
use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use uuid::Uuid;

use rb::command::CommandRegistry;
use rb::message::{CommandRequest, CommandResult};
use rb::session::SessionManager;

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<DashMap<Uuid, Client>>,
    client_handlers: Arc<Mutex<Vec<JoinHandle<()>>>>,
    // listeners: Arc<Mutex<Vec<Box<dyn Listener>>>>,
    // listeners: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Box<dyn Listener>>>>>>,
    listeners: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Box<HttpListener>>>>>>,
    // sessions: Arc<Mutex<Vec<Session>>>,
    // sessions: Arc<std::sync::RwLock<HashMap<Uuid, Arc<Session>>>>,
    session_manager: Arc<RwLock<SessionManager>>,
    running: Arc<AtomicBool>,
    server_task: Mutex<Option<JoinHandle<()>>>,
    command_registry: Arc<CommandRegistry>,
}

impl RbServer {
    /// Create a new RbServer instance with the given configuration
    pub fn new(config: RbServerConfig) -> Self {
        RbServer {
            config,
            // clients: Arc::new(Mutex::new(Vec::new())),
            clients: Arc::new(DashMap::new()),
            client_handlers: Arc::new(Mutex::new(Vec::new())),
            listeners: Arc::new(Mutex::new(HashMap::new())),
            // sessions: Arc::new(std::sync::RwLock::new(HashMap::new())),
            session_manager: Arc::new(RwLock::new(SessionManager::new())),
            // listeners_manager: Arc::new(RwLock::new(ListenersManager::new())), // yet to implement this
            running: Arc::new(AtomicBool::new(false)),
            server_task: Mutex::new(None),
            command_registry: Arc::new(CommandRegistry::new()),
        }
    }
    // pub fn session_manager(&self) -> Arc<RwLock<SessionManager>> {
    //     self.session_manager.clone()
    // }

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
        let session_manager = self.session_manager.clone();
        let command_registry = self.command_registry.clone();
        let listeners = self.listeners.clone();
        let client_handlers = self.client_handlers.clone();

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(1), // Check running flag every second
                    listener.accept(),
                )
                .await
                {
                    Ok(Ok((socket, addr))) => {
                        log::info!("New connection from: {}", addr);

                        let client = Client::new(socket);
                        let client_id = client.id();

                        clients.insert(client_id, client.clone());

                        let client_list = clients.clone();
                        let session_manager = session_manager.clone();
                        let running_clone = running.clone();
                        let command_registry_clone = command_registry.clone();
                        let listeners_clone = listeners.clone();

                        // Spawn and store the client handler
                        let handler = tokio::spawn(async move {
                            if let Err(e) = Self::handle_client(
                                client,
                                client_id,
                                client_list,
                                session_manager,
                                listeners_clone,
                                running_clone,
                                command_registry_clone,
                            )
                            .await
                            {
                                eprintln!("Error handling client {}: {}", addr, e);
                            }
                        });

                        // Store the handler reference
                        let mut handlers = client_handlers.lock().unwrap();
                        handlers.push(handler);
                    }
                    Ok(Err(e)) => {
                        eprintln!("Error accepting connection: {}", e);
                    }
                    Err(_) => {
                        // Timeout occurred, just continue to check the running flag
                        continue;
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
        // let sessions = self.sessions.clone();
        let session_manager = self.session_manager.clone();
        let command_registry = self.command_registry.clone();
        let crl_path = self.config.mtls.crl_path.clone();
        let listeners = self.listeners.clone();

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match listener.accept().await {
                    Ok((mut stream, addr)) => {
                        log::info!("New TLS connection attempt from: {}", addr);

                        let mut acceptor = Acceptor::default();
                        let test_pki = test_pki.clone();
                        let crl_path = crl_path.clone();

                        // Process the TLS handshake in a separate task
                        let clients_clone = clients.clone();
                        let session_manager = session_manager.clone();
                        let running_clone = running.clone();
                        let command_registry_clone = command_registry.clone();
                        let listeners_clone = listeners.clone();

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
                                        if let Err(send_err) = stream.write_all(&alert_bytes).await
                                        {
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

                            clients_clone.insert(client_id, client.clone());

                            if let Err(e) = Self::handle_client(
                                client,
                                client_id,
                                clients_clone,
                                session_manager,
                                listeners_clone,
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

    async fn handle_client(
        mut client: Client,
        client_id: Uuid,
        // clients: Arc<Mutex<Vec<Client>>>,
        clients: Arc<DashMap<Uuid, Client>>,
        session_manager: Arc<RwLock<SessionManager>>,
        listeners: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Box<HttpListener>>>>>>,
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

        // Create check interval for shutdown signal
        let mut shutdown_check = tokio::time::interval(std::time::Duration::from_millis(100));

        while running.load(Ordering::SeqCst) && !client.should_disconnect() {
            tokio::select! {
                _ = shutdown_check.tick() => {
                    // Just check the conditions in the while loop
                }
                Some(Ok(msg)) = stream.next() => {
                    log::info!("Received: {:?}", msg);
                    let mut cmd_context = CommandContext {
                        session_manager: session_manager.clone(),
                        command_registry: command_registry.clone(),
                        listeners: listeners.clone(),
                    };

                    let command_request: CommandRequest = match serde_json::from_str(msg.as_str()) {
                        Ok(request) => request,
                        Err(e) => {
                            log::error!("Failed to parse command request: {}", e);
                            let error_response =
                                format!("{{\"error\": \"Failed to parse command request: {}\"}}", e);
                            if let Err(e) = sink.send(error_response).await {
                                log::error!("Failed to send error response to client: {}", e);
                                break;
                            }
                            continue;
                        }
                    };

                    let result: CommandResult = command_registry
                        .execute(&mut cmd_context, command_request)
                        .await;

                    // Serialize the result
                    let serialized = match result {
                        Ok(output) => {
                            serde_json::to_string(&output).unwrap_or_else(|e| {
                                format!("{{\"error\": \"Failed to serialize output: {}\"}}", e)
                            })
                        }
                        Err(err) => {
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

                    log::debug!("Executed command: {}", msg);
                }
                else => break,
            }
        }

        // Log the disconnect reason
        if !running.load(Ordering::SeqCst) {
            log::info!("Client {} disconnected due to server shutdown", client_id);
        } else if client.should_disconnect() {
            log::info!("Client {} disconnected due to disconnect signal", client_id);
        } else {
            log::info!("Client {} disconnected", client_id);
        }

        // Clean up when the client disconnects
        clients.remove(&client_id);
        // {
        //     let mut client_list = clients.lock().unwrap();
        //     client_list.retain(|c| c.id() != client_id);
        // }

        Ok(())
    }

    // Modified stop method to properly clean up client handlers
    pub async fn stop(&self) -> io::Result<()> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        log::info!("Stopping server...");

        // Set running flag to false to stop the accept loop
        self.running.store(false, Ordering::SeqCst);

        // Signal all clients to disconnect
        {
            // let clients = self.clients.lock().unwrap();
            for client in self.clients.iter() {
                client.signal_disconnect();
            }
            log::info!("Signaled {} clients to disconnect", self.clients.len());
        }

        // Wait for the server task to complete with timeout
        if let Some(handle) = self.server_task.lock().unwrap().take() {
            match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                Ok(_) => log::info!("Server main task completed"),
                Err(_) => {
                    log::warn!("Server main task shutdown timed out");
                    // No need to abort - it will be dropped when the process exits
                }
            }
        }

        // Wait for all client handlers to complete
        {
            let mut handlers = self.client_handlers.lock().unwrap();
            let handler_count = handlers.len();
            log::info!("Waiting for {} client handlers to complete", handler_count);

            let mut completed = 0;
            let mut timed_out = 0;

            for handle in handlers.drain(..) {
                match tokio::time::timeout(std::time::Duration::from_secs(2), handle).await {
                    Ok(_) => completed += 1,
                    Err(_) => {
                        log::warn!("Client handler shutdown timed out");
                        timed_out += 1;
                    }
                }
            }

            log::info!(
                "Client handlers: {} completed, {} timed out",
                completed,
                timed_out
            );
        }

        // Clean up any remaining clients
        // let mut clients = self.clients.lock().unwrap();
        self.clients.clear();

        // Clean up any remaining sessions
        let session_manager = self.session_manager.write().unwrap();
        session_manager.kill_all_sessions();

        log::info!("Server stopped successfully");
        Ok(())
    }

    // /// Check if the server is currently running
    // pub fn is_running(&self) -> bool {
    //     self.running.load(Ordering::SeqCst)
    // }
}
