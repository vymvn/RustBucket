use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, RwLock,
};
use std::time::{Duration, SystemTime};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::listener::*;
use crate::message::*;
use crate::session::SessionManager;
use crate::task::*;

// Server state
pub struct ServerState {
    pub beacons: Arc<Mutex<HashMap<Uuid, BeaconInfo>>>,
}

pub struct HttpListener {
    name: String,
    id: Uuid,
    addr: SocketAddr,
    // sessions: Arc<RwLock<HashMap<Uuid, Arc<Session>>>>,
    session_manager: Arc<RwLock<SessionManager>>,
    running: Arc<AtomicBool>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
    state: Arc<ServerState>,
}

impl HttpListener {
    pub fn new(name: &str, addr: SocketAddr, session_manager: Arc<RwLock<SessionManager>>) -> Self {
        HttpListener {
            name: name.to_string(),
            id: Uuid::new_v4(),
            addr,
            session_manager,
            running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
            handle: None,
            state: Arc::new(ServerState {
                beacons: Arc::new(Mutex::new(HashMap::new())),
                // tasks: Arc::new(Mutex::new(HashMap::new())),
                // results: Arc::new(Mutex::new(HashMap::new())),
            }),
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

    pub fn get_state(&self) -> Arc<ServerState> {
        self.state.clone()
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
        let server_state = self.state.clone();
        let session_manager = self.session_manager.clone();

        // Spawn Actix Web server in a separate task
        let handle = tokio::spawn(async move {
            // Create shared data for route handlers
            let listener_data = web::Data::new(ListenerData {
                name: listener_name.clone(),
                id: listener_id,
                state: server_state,
                session_manager: session_manager.clone(),
            });

            // Start server in a separate task
            let server_task = tokio::spawn(async move {
                // Define route handlers
                async fn index(data: web::Data<ListenerData>) -> impl Responder {
                    HttpResponse::Ok()
                        .body(format!("RustBucket Listener: {} ({})", data.name, data.id))
                }

                // Beacon registration/check-in endpoint
                async fn beacon_checkin(
                    checkin: web::Json<BeaconCheckin>,
                    data: web::Data<ListenerData>,
                ) -> impl Responder {
                    let now = SystemTime::now();
                    let mut beacons = data.state.beacons.lock().unwrap();

                    // Check if this is a new beacon or an existing one
                    let beacon_id = if let Some(id) = checkin.id {
                        // Existing beacon - update last_seen
                        if let Some(beacon) = beacons.get_mut(&id) {
                            beacon.last_seen = now;
                            beacon.ip_address = checkin.ip_address.clone();
                            id
                        } else {
                            // ID provided but not found - register as new
                            let new_id = Uuid::new_v4();
                            let beacon_info = BeaconInfo {
                                id: new_id,
                                hostname: checkin.hostname.clone(),
                                ip_address: checkin.ip_address.clone(),
                                os_info: checkin.os_info.clone(),
                                username: checkin.username.clone(),
                                process_id: checkin.process_id,
                                first_seen: now,
                                last_seen: now,
                            };
                            beacons.insert(new_id, beacon_info);

                            // Create a new session
                            let mgr = data.session_manager.write().unwrap();
                            let session = mgr.create_session(
                                checkin.hostname.to_string(),
                                checkin.ip_address.to_string(),
                            );

                            new_id
                        }
                    } else {
                        // New beacon - register
                        let new_id = Uuid::new_v4();
                        let beacon_info = BeaconInfo {
                            id: new_id,
                            hostname: checkin.hostname.clone(),
                            ip_address: checkin.ip_address.clone(),
                            os_info: checkin.os_info.clone(),
                            username: checkin.username.clone(),
                            process_id: checkin.process_id,
                            first_seen: now,
                            last_seen: now,
                        };

                        beacons.insert(new_id, beacon_info);

                        // Create a new session
                        let mgr = data.session_manager.write().unwrap();
                        mgr.create_session(
                            checkin.hostname.to_string(),
                            checkin.ip_address.to_string(),
                        );

                        new_id
                    };

                    log::info!("Beacon check-in: {}", beacon_id);

                    HttpResponse::Ok().json(serde_json::json!({
                        "status": "success",
                        "beacon_id": beacon_id,
                    }))
                }

                // Get tasks for a specific beacon
                async fn get_tasks(
                    path: web::Path<String>,
                    data: web::Data<ListenerData>,
                ) -> impl Responder {
                    let beacon_id = match Uuid::parse_str(&path.into_inner()) {
                        Ok(id) => id,
                        Err(_) => return HttpResponse::BadRequest().body("Invalid beacon ID"),
                    };

                    // Update last_seen
                    {
                        let mut beacons = data.state.beacons.lock().unwrap();
                        if let Some(beacon) = beacons.get_mut(&beacon_id) {
                            beacon.last_seen = SystemTime::now();
                        } else {
                            return HttpResponse::NotFound().body("Beacon not found");
                        }
                    }

                    // Get session manager and find tasks for this beacon
                    let session_manager = data.session_manager.read().unwrap();

                    // Find session ID from beacon ID (you'll need to implement this mapping)
                    let session_id = match session_manager.get_session_id_by_beacon(&beacon_id) {
                        Ok(id) => id,
                        Err(_) => {
                            return HttpResponse::NotFound().body("No session found for beacon")
                        }
                    };

                    // Get pending tasks and mark them as in progress
                    let session = match session_manager.get_session(&session_id) {
                        Some(s) => s,
                        None => return HttpResponse::NotFound().body("Session not found"),
                    };

                    let pending_tasks = session.get_pending_tasks();

                    // Mark tasks as in progress
                    for task in &pending_tasks {
                        session.mark_task_in_progress(&task.id);
                    }

                    HttpResponse::Ok().json(pending_tasks)
                }

                // Upload task results
                async fn upload_results(
                    result: web::Json<TaskResult>,
                    data: web::Data<ListenerData>,
                ) -> impl Responder {
                    let task_result = result.into_inner();

                    // Use session manager to submit the result
                    let session_manager = data.session_manager.read().unwrap();
                    let beacon_id = task_result.beacon_id;
                    match session_manager.submit_task_result(beacon_id, task_result) {
                        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
                            "status": "success",
                            "message": "Task result received"
                        })),
                        Err(e) => HttpResponse::BadRequest().body(e),
                    }
                }

                // Create a new task for a beacon
                async fn create_task(
                    task_data: web::Json<serde_json::Value>,
                    data: web::Data<ListenerData>,
                ) -> impl Responder {
                    // Extract beacon ID and command from request
                    let beacon_id_str = match task_data.get("beacon_id") {
                        Some(val) => match val.as_str() {
                            Some(s) => s,
                            None => {
                                return HttpResponse::BadRequest().body("Invalid beacon ID format")
                            }
                        },
                        None => return HttpResponse::BadRequest().body("Missing beacon ID"),
                    };

                    let beacon_id = match Uuid::parse_str(beacon_id_str) {
                        Ok(id) => id,
                        Err(_) => return HttpResponse::BadRequest().body("Invalid beacon ID"),
                    };

                    let command = match task_data.get("command") {
                        Some(val) => match val.as_str() {
                            Some(s) => s.to_string(),
                            None => {
                                return HttpResponse::BadRequest().body("Invalid command format")
                            }
                        },
                        None => return HttpResponse::BadRequest().body("Missing command"),
                    };

                    // Extract args (optional)
                    let args = match task_data.get("args") {
                        Some(val) => match val.as_array() {
                            Some(arr) => arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect(),
                            None => Vec::new(),
                        },
                        None => Vec::new(),
                    };

                    // Verify beacon exists
                    {
                        let beacons = data.state.beacons.lock().unwrap();
                        if !beacons.contains_key(&beacon_id) {
                            return HttpResponse::NotFound().body("Beacon not found");
                        }
                    }

                    // // Create new task
                    // let task_id = Uuid::new_v4();
                    // let task = Task {
                    //     id: task_id,
                    //     beacon_id,
                    //     command,
                    //     args,
                    //     created_at: SystemTime::now(),
                    //     status: TaskStatus::Pending,
                    // };

                    // Use session manager to create the task
                    let session_manager = data.session_manager.read().unwrap();
                    let result = session_manager.add_task_by_beacon(beacon_id, command, args);

                    match result {
                        Ok(task_id) => {
                            // Get the task details to return
                            // You'll need a way to look up the task by ID across sessions
                            // For now, let's assume we can get the session by beacon ID
                            match session_manager.get_session_id_by_beacon(&beacon_id) {
                                Ok(session_id) => {
                                    if let Some(session) = session_manager.get_session(&session_id)
                                    {
                                        if let Some(task) = session.get_task(&task_id) {
                                            return HttpResponse::Ok().json(task);
                                        }
                                    }
                                    HttpResponse::InternalServerError()
                                        .body("Task created but could not be retrieved")
                                }
                                Err(_) => HttpResponse::InternalServerError()
                                    .body("Session not found for beacon"),
                            }
                        }
                        Err(e) => HttpResponse::BadRequest().body(e),
                    }
                }

                // List all active beacons
                async fn list_beacons(data: web::Data<ListenerData>) -> impl Responder {
                    let beacons = data.state.beacons.lock().unwrap();
                    let beacon_list: Vec<BeaconInfo> = beacons.values().cloned().collect();
                    HttpResponse::Ok().json(beacon_list)
                }

                let server = HttpServer::new(move || {
                    App::new()
                        .app_data(listener_data.clone())
                        .route("/", web::get().to(index))
                        .route("/beacon/checkin", web::post().to(beacon_checkin))
                        .route("/beacon/tasks/{beacon_id}", web::get().to(get_tasks))
                        .route("/beacon/results", web::post().to(upload_results))
                        .route("/tasks", web::post().to(create_task))
                        .route("/beacons", web::get().to(list_beacons))
                })
                .bind(server_addr)
                .expect("Failed to bind server to address");

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

    // Helper method to add a task to a beacon
    // pub fn add_beacon_task(
    //     &self,
    //     beacon_id: Uuid,
    //     command: String,
    //     args: Vec<String>,
    // ) -> Result<Uuid, String> {
    //     let beacons = self.state.beacons.lock().unwrap();
    //     if !beacons.contains_key(&beacon_id) {
    //         return Err("Beacon not found".to_string());
    //     }
    //     drop(beacons);
    //
    //     let task_id = Uuid::new_v4();
    //     let task = Task {
    //         id: task_id,
    //         beacon_id,
    //         command,
    //         args,
    //         created_at: SystemTime::now(),
    //         status: TaskStatus::Pending,
    //     };
    //
    //     let mut tasks = self.state.tasks.lock().unwrap();
    //     tasks.insert(task_id, task);
    //
    //     Ok(task_id)
    // }

    // Helper method to clean up stale beacons
    pub fn cleanup_stale_beacons(&self, timeout: Duration) -> usize {
        let now = SystemTime::now();
        let mut beacons = self.state.beacons.lock().unwrap();

        let stale_ids: Vec<Uuid> = beacons
            .iter()
            .filter_map(|(id, beacon)| match beacon.last_seen.elapsed() {
                Ok(elapsed) if elapsed > timeout => Some(*id),
                _ => None,
            })
            .collect();

        for id in &stale_ids {
            beacons.remove(id);
        }

        stale_ids.len()
    }
}

// Shared data structure for route handlers
struct ListenerData {
    name: String,
    id: Uuid,
    state: Arc<ServerState>,
    session_manager: Arc<RwLock<SessionManager>>, // <-- Add this line
}
