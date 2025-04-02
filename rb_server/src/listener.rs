use actix_web::{web, App, HttpResponse, HttpServer, Responder};
//use rand::distr::Alphanumeric;
//use rand::Rng;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::oneshot;

pub struct Listener {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub running: bool,
    shutdown_tx: Option<oneshot::Sender<()>>, // Used to signal shutdown
    handle: Option<thread::JoinHandle<()>>,   // Thread handle for the server
}

impl Listener {
    pub fn new(host: String, port: u16, name: String) -> Listener {
        //let name: String = gen_name();
        log::info!("Generated new listener with name: {}", name);
        Listener {
            name,
            host,
            port,
            running: false,
            shutdown_tx: None,
            handle: None,
        }
    }

    pub fn start(&mut self, address: &str) {
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let address = address.to_string();

        let handle = thread::spawn(move || {
            let system = actix_web::rt::System::new();

            let server = HttpServer::new(|| App::new().route("/", web::get().to(index)))
                .bind(&address)
                .expect("Failed to bind server")
                .run();

            let server_handle = server.handle();
            let shutdown_future = async move {
                shutdown_rx.await.ok(); // Wait for shutdown signal
                println!("Shutting down server...");
                server_handle.stop(true).await;
            };

            system.block_on(async {
                tokio::spawn(shutdown_future);
                server.await.expect("Server failed");
            });
        });

        self.shutdown_tx = Some(shutdown_tx);
        self.handle = Some(handle);
    }

    //pub async fn start(&mut self) -> std::io::Result<()> {
    //    if self.running {
    //        log::warn!("Listener {} is already running", self.name);
    //        return Ok(());
    //    }
    //
    //    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    //
    //    let listener_data = Arc::new(Mutex::new(ListenerData {
    //        name: self.name.clone(),
    //        connections: Vec::new(),
    //    }));
    //
    //    let data = listener_data.clone();
    //    let host = self.host.clone();
    //    let port = self.port;
    //
    //    log::info!("Starting listener {} on {}:{}", self.name, host, port);
    //
    //    let handle = thread::spawn(move || {
    //        let server = HttpServer::new(|| {
    //            App::new()
    //                .app_data(web::Data::new(data.clone()))
    //                .route("/", web::get().to(index))
    //                .route("/beacon", web::post().to(beacon_handler))
    //                .route("/tasks/{agent_name}", web::get().to(get_tasks))
    //        });
    //
    //        let server_handle = server.run();
    //
    //        let shutdown_future = async {
    //            shutdown_rx.await.ok(); // Wait for shutdown signal
    //            println!("Shutting down server...");
    //            server_handle.stop(true);
    //        };
    //
    //        actix_web::rt::System::new().block_on(shutdown_future);
    //    });
    //
    //    self.shutdown_tx = Some(shutdown_tx);
    //    self.handle = Some(handle);
    //
    //    self.running = true;
    //    Ok(())
    //}

    pub fn stop(&mut self) {
        if !self.running {
            log::warn!("Listener {} is not running", self.name);
            return;
        }

        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(()); // Send shutdown signal
        }

        if let Some(handle) = self.handle {
            let _ = handle.join(); // Wait for the thread to finish
        }

        log::info!("Stopping listener {}", self.name);
        self.running = false;
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
}

// Data structure to track connected agents/beacons
struct ListenerData {
    name: String,
    connections: Vec<AgentConnection>,
}

struct AgentConnection {
    agent_name: String,
    last_seen: std::time::SystemTime,
    tasks: Vec<Task>,
}

struct Task {
    name: String,
    command: String,
    status: TaskStatus,
    result: Option<String>,
}

enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

// Route handlers
async fn index() -> impl Responder {
    log::info!("Hit index");
    HttpResponse::Ok().body("Server running")
}

async fn beacon_handler(
    data: web::Data<Arc<Mutex<ListenerData>>>,
    payload: web::Json<BeaconPayload>,
) -> impl Responder {
    let mut data = data.lock().unwrap();

    // Find existing connection or create new one
    let agent_conn = match data
        .connections
        .iter_mut()
        .find(|c| c.agent_name == payload.agent_name)
    {
        Some(conn) => {
            log::info!("Existing agent {} checked in", payload.agent_name);
            conn.last_seen = std::time::SystemTime::now();
            conn
        }
        None => {
            log::info!("New agent connection: {}", payload.agent_name);
            let new_conn = AgentConnection {
                agent_name: payload.agent_name.clone(),
                last_seen: std::time::SystemTime::now(),
                tasks: Vec::new(),
            };
            data.connections.push(new_conn);
            data.connections.last_mut().unwrap()
        }
    };

    // Process task results if any
    if let Some(results) = &payload.results {
        for result in results {
            if let Some(task) = agent_conn
                .tasks
                .iter_mut()
                .find(|t| t.name == result.task_name)
            {
                task.status = TaskStatus::Completed;
                task.result = Some(result.output.clone());
                log::info!(
                    "Task {} completed for agent {}",
                    task.name,
                    agent_conn.agent_name
                );
            }
        }
    }

    // Return a simple response
    HttpResponse::Ok().json(web::Json(BeaconResponse {
        listener_name: data.name.clone(),
        message: "Beacon received".to_string(),
    }))
}

async fn get_tasks(
    data: web::Data<Arc<Mutex<ListenerData>>>,
    path: web::Path<String>,
) -> impl Responder {
    let agent_name = path.into_inner();
    let data = data.lock().unwrap();

    if let Some(conn) = data.connections.iter().find(|c| c.agent_name == agent_name) {
        // Filter for pending tasks only
        let pending_tasks: Vec<&Task> = conn
            .tasks
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Pending))
            .collect();

        HttpResponse::Ok().json(web::Json(TaskResponse {
            tasks: pending_tasks
                .iter()
                .map(|t| TaskInfo {
                    name: t.name.clone(),
                    command: t.command.clone(),
                })
                .collect(),
        }))
    } else {
        HttpResponse::NotFound().body(format!("Agent {} not found", agent_name))
    }
}

// Data structures for JSON serialization/deserialization
#[derive(serde::Deserialize)]
struct BeaconPayload {
    agent_name: String,
    hostname: Option<String>,
    username: Option<String>,
    os_info: Option<String>,
    results: Option<Vec<TaskResult>>,
}

#[derive(serde::Deserialize)]
struct TaskResult {
    task_name: String,
    output: String,
}

#[derive(serde::Serialize)]
struct BeaconResponse {
    listener_name: String,
    message: String,
}

#[derive(serde::Serialize)]
struct TaskResponse {
    tasks: Vec<TaskInfo>,
}

#[derive(serde::Serialize)]
struct TaskInfo {
    name: String,
    command: String,
}

//fn gen_name() -> String {
//let name: String = rand::thread_rng()
//        .sample_iter(&Alphanumeric)
//        .take(8)
//        .map(char::from)
//        .collect();
//
//    name
//}
