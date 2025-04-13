use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub struct HttpListener {
    name: String,
    id: Uuid,
    addr: SocketAddr,
    running: Arc<AtomicBool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl HttpListener {
    pub fn new(name: &str, addr: SocketAddr) -> Self {
        HttpListener {
            name: name.to_string(),
            id: Uuid::new_v4(),
            addr,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            handle: None,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub async fn start(&mut self) -> Result<(), String> {
        // Don't start if already running
        if self.running.load(Ordering::SeqCst) {
            return Err("Listener is already running".to_string());
        }

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);

        let server_addr = self.addr;
        let listener_name = self.name.clone();
        let listener_id = self.id;

        // Spawn Actix Web server in a separate task
        let handle = tokio::spawn(async move {
            // Create shared data for route handlers
            let listener_data = web::Data::new(ListenerData {
                name: listener_name.clone(),
                id: listener_id,
            });

            // Start server in a separate task
            let server_task = tokio::spawn(async move {
                // Define route handlers
                async fn index(data: web::Data<ListenerData>) -> impl Responder {
                    HttpResponse::Ok()
                        .body(format!("RustBucket Listener: {} ({})", data.name, data.id))
                }

                async fn command_handler(
                    command: web::Json<serde_json::Value>,
                    data: web::Data<ListenerData>,
                ) -> impl Responder {
                    log::info!("Listener {} received command: {:?}", data.name, command);
                    HttpResponse::Ok().json(serde_json::json!({
                        "status": "received",
                        "message": "Command processed",
                        "listener": data.name,
                        "listener_id": data.id
                    }))
                }

                let server = HttpServer::new(move || {
                    App::new()
                        .app_data(listener_data.clone())
                        .route("/", web::get().to(index))
                        .route("/command", web::post().to(command_handler))
                    // Add more routes as needed
                })
                .bind(server_addr)
                .expect("Failed to bind server to address");

                // println!(
                //     "HTTP listener '{}' ({}) started on {}",
                //     listener_data.name, listener_data.id, server_addr
                // );

                // Run the server
                if let Err(e) = server.run().await {
                    eprintln!("Server error: {}", e);
                }
            });

            // Wait for shutdown signal
            let _ = shutdown_rx.await;
            log::info!("Shutdown signal received for listener '{}'", listener_name);

            // Abort the server task
            server_task.abort();

            // Clean up
            running.store(false, Ordering::SeqCst);
            log::info!(
                "HTTP listener '{}' ({}) stopped",
                listener_name,
                listener_id
            );
        });

        self.handle = Some(handle);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), String> {
        if !self.running.load(Ordering::SeqCst) {
            return Err("Listener is not running".to_string());
        }

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Wait for the task to complete
        if let Some(handle) = self.handle.take() {
            match handle.await {
                Ok(_) => {}
                Err(e) => return Err(format!("Error joining task: {}", e)),
            }
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// Shared data structure for route handlers
struct ListenerData {
    name: String,
    id: Uuid,
}
