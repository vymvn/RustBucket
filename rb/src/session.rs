use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::task::{Task, TaskResult, TaskStatus};

/// Status of a session
#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,
    Idle,
    Disconnected,
    Terminated,
}

impl ToString for SessionStatus {
    fn to_string(&self) -> String {
        match self {
            SessionStatus::Active => "Active".to_string(),
            SessionStatus::Idle => "Idle".to_string(),
            SessionStatus::Disconnected => "Disconnected".to_string(),
            SessionStatus::Terminated => "Terminated".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct Session {
    /// Unique ID for this session
    id: usize,

    /// Agent identifier (could be a name, hostname, or another UUID)
    agent_hostname: String,

    /// Address the agent connected from
    ip_address: String,

    /// When the session was created
    created_at: SystemTime,

    /// Last time communication was received from this session
    last_seen: SystemTime,

    /// Current status of the session
    status: Arc<Mutex<SessionStatus>>,

    /// Flag indicating if session is active
    active: AtomicBool,

    /// Channel for sending commands to the agent
    command_tx: Option<mpsc::Sender<String>>,

    /// Channel for receiving responses from the agent
    response_rx: Option<mpsc::Receiver<String>>,

    /// Channel for receiving heartbeats or other events from the agent
    event_rx: Option<mpsc::Receiver<SessionEvent>>,

    /// Tasks assigned to this session
    tasks: Arc<Mutex<Vec<Task>>>,

    /// Task results received from this session
    results: Arc<Mutex<Vec<TaskResult>>>,
}

/// Events that can be sent from an agent to the server
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Heartbeat,
    Error(String),
    Info(String),
    Disconnect,
}

impl Session {
    /// Create a new session
    pub fn new(id: usize, agent_hostname: String, ip_address: String) -> Self {
        let now = SystemTime::now();

        Self {
            id,
            agent_hostname,
            ip_address,
            created_at: now,
            last_seen: now,
            status: Arc::new(Mutex::new(SessionStatus::Active)),
            active: AtomicBool::new(true),
            command_tx: None,
            response_rx: None,
            event_rx: None,
            tasks: Arc::new(Mutex::new(Vec::new())),
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Initialize communication channels
    pub fn init_channels(&mut self) -> mpsc::Sender<String> {
        // Create channels for command/response communication
        let (command_tx, command_rx) = mpsc::channel(100);
        let (response_tx, response_rx) = mpsc::channel(100);
        let (event_tx, event_rx) = mpsc::channel(100);

        // Store our end of the channels
        self.command_tx = Some(command_tx.clone());
        self.response_rx = Some(response_rx);
        self.event_rx = Some(event_rx);

        // Return the command receiver that should be passed to the agent handler
        command_tx
    }

    /// Send a command to the agent
    pub async fn send_command(&self, command: String) -> Result<(), String> {
        match &self.command_tx {
            Some(tx) => tx
                .send(command)
                .await
                .map_err(|e| format!("Failed to send command: {}", e)),
            None => Err("Session has no command channel".to_string()),
        }
    }

    /// Receive a response from the agent with timeout
    pub async fn receive_response(&mut self, timeout: Duration) -> Result<String, String> {
        match &mut self.response_rx {
            Some(rx) => {
                tokio::select! {
                    response = rx.recv() => {
                        match response {
                            Some(msg) => {
                                // Update last seen timestamp
                                self.update_last_seen();
                                Ok(msg)
                            },
                            None => Err("Agent disconnected".to_string()),
                        }
                    }
                    _ = tokio::time::sleep(timeout) => {
                        Err("Response timeout".to_string())
                    }
                }
            }
            None => Err("Session has no response channel".to_string()),
        }
    }

    /// Process events from the agent
    pub async fn process_events(&mut self) {
        if let Some(mut rx) = self.event_rx.take() {
            let id = self.id;
            let status = Arc::clone(&self.status);

            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    match event {
                        SessionEvent::Heartbeat => {
                            // Update status to Active
                            if let Ok(mut status_guard) = status.lock() {
                                if *status_guard == SessionStatus::Idle {
                                    *status_guard = SessionStatus::Active;
                                }
                            }
                            log::debug!("Received heartbeat from session {}", id);
                        }
                        SessionEvent::Error(error) => {
                            log::error!("Error from session {}: {}", id, error);
                        }
                        SessionEvent::Info(info) => {
                            log::info!("Info from session {}: {}", id, info);
                        }
                        SessionEvent::Disconnect => {
                            // Update status to Disconnected
                            if let Ok(mut status_guard) = status.lock() {
                                *status_guard = SessionStatus::Disconnected;
                            }
                            log::info!("Session {} disconnected", id);
                            break;
                        }
                    }
                }

                // Channel closed
                if let Ok(mut status_guard) = status.lock() {
                    *status_guard = SessionStatus::Terminated;
                }
                log::info!("Event channel for session {} closed", id);
            });
        }
    }

    /// Add a new task to this session
    pub fn add_task(&self, action: String) -> Uuid {
        let task_id = Uuid::new_v4();
        let task = Task {
            id: task_id,
            beacon_id: Uuid::new_v4(), // Or use some other ID that maps to this session
            session_id: self.id,
            action,
            created_at: SystemTime::now(),
            status: TaskStatus::Pending,
        };

        // Add task to the session's task list
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.push(task);
        }

        task_id
    }

    /// Get all pending tasks
    pub fn get_pending_tasks(&self) -> Vec<Task> {
        if let Ok(tasks) = self.tasks.lock() {
            tasks
                .iter()
                .filter(|task| matches!(task.status, TaskStatus::Pending))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Mark a task as in progress
    pub fn mark_task_in_progress(&self, task_id: &Uuid) -> bool {
        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(task) = tasks.iter_mut().find(|t| &t.id == task_id) {
                task.status = TaskStatus::InProgress;
                return true;
            }
        }
        false
    }

    /// Add a task result
    pub fn add_task_result(&self, result: TaskResult) -> bool {
        // Update the task status
        let mut updated = false;
        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(task) = tasks.iter_mut().find(|t| t.id == result.task_id) {
                task.status = result.status.clone();
                updated = true;
            }
        }

        // Store the result
        if updated {
            if let Ok(mut results) = self.results.lock() {
                results.push(result);
                return true;
            }
        }

        false
    }

    /// Get a specific task by ID
    pub fn get_task(&self, task_id: &Uuid) -> Option<Task> {
        if let Ok(tasks) = self.tasks.lock() {
            tasks.iter().find(|t| &t.id == task_id).cloned()
        } else {
            None
        }
    }

    /// Get all task results
    pub fn get_results(&self) -> Vec<TaskResult> {
        if let Ok(results) = self.results.lock() {
            results.clone()
        } else {
            Vec::new()
        }
    }

    /// Update the last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = SystemTime::now();
    }

    /// Mark session as idle
    pub fn set_idle(&self) {
        if let Ok(mut status) = self.status.lock() {
            *status = SessionStatus::Idle;
        }
    }

    /// Mark session as active
    pub fn set_active(&self) {
        if let Ok(mut status) = self.status.lock() {
            *status = SessionStatus::Active;
        }
    }

    /// Terminate the session
    pub fn terminate(&self) {
        self.active.store(false, Ordering::SeqCst);
        if let Ok(mut status) = self.status.lock() {
            *status = SessionStatus::Terminated;
        }
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Get session ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Get agent hostname
    pub fn agent_hostname(&self) -> &str {
        &self.agent_hostname
    }

    /// Get address
    pub fn address(&self) -> &str {
        &self.ip_address
    }

    /// Get last seen timestamp as a string
    pub fn last_seen(&self) -> String {
        match self.last_seen.elapsed() {
            Ok(elapsed) => {
                if elapsed.as_secs() < 60 {
                    "Just now".to_string()
                } else if elapsed.as_secs() < 3600 {
                    format!("{} minutes ago", elapsed.as_secs() / 60)
                } else if elapsed.as_secs() < 86400 {
                    format!("{} hours ago", elapsed.as_secs() / 3600)
                } else {
                    format!("{} days ago", elapsed.as_secs() / 86400)
                }
            }
            Err(_) => "Time error".to_string(),
        }
    }

    /// Get session status
    pub fn status(&self) -> String {
        match self.status.lock() {
            Ok(status) => status.to_string(),
            Err(_) => "Unknown".to_string(),
        }
    }
}

/// SessionManager handles creation and tracking of sessions
pub struct SessionManager {
    sessions: Arc<Mutex<Vec<Arc<Session>>>>,
    next_id: AtomicUsize,
    beacon_to_session: Arc<Mutex<HashMap<Uuid, usize>>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            next_id: AtomicUsize::new(0),
            beacon_to_session: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new session
    pub fn create_session(&self, agent_hostname: String, ip_address: String) -> Arc<Session> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let session = Arc::new(Session::new(id, agent_hostname, ip_address));

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.push(session.clone());
        }

        session
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &usize) -> Option<Arc<Session>> {
        if let Ok(sessions) = self.sessions.lock() {
            sessions.iter().find(|s| s.id() == *id).cloned()
        } else {
            None
        }
    }

    /// Get all sessions
    pub fn get_all_sessions(&self) -> Vec<Arc<Session>> {
        if let Ok(sessions) = self.sessions.lock() {
            sessions.clone()
        } else {
            Vec::new()
        }
    }

    /// Delete a session by ID
    pub fn remove_session(&self, id: &usize) -> bool {
        // First remove from sessions list
        let removed = if let Ok(mut sessions) = self.sessions.lock() {
            let len_before = sessions.len();
            sessions.retain(|s| s.id() != *id);
            sessions.len() < len_before
        } else {
            false
        };

        // Then clean up any beacon mappings
        if removed {
            if let Ok(mut mapping) = self.beacon_to_session.lock() {
                // Remove all entries where the value is the session ID
                mapping.retain(|_, session_id| session_id != id);
            }
        }

        removed
    }

    /// Activate a session
    /// Returns result
    pub fn activate_session(&self, id: &usize) -> Result<(), String> {
        if let Some(session) = self.get_session(id) {
            session.set_active();
            Ok(())
        } else {
            Err("Session not found".to_string())
        }
    }

    /// Kill all sessions
    pub fn kill_all_sessions(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            for session in sessions.iter() {
                session.terminate();
            }
            sessions.clear();
        }
    }

    // Implement the lookup method
    pub fn get_session_id_by_beacon(&self, beacon_id: &Uuid) -> Result<usize, String> {
        if let Ok(mapping) = self.beacon_to_session.lock() {
            if let Some(session_id) = mapping.get(beacon_id) {
                Ok(*session_id)
            } else {
                Err("No session found for this beacon ID".to_string())
            }
        } else {
            Err("Failed to acquire lock on beacon mapping".to_string())
        }
    }

    pub fn add_task_by_beacon(&self, beacon_id: Uuid, action: String) -> Result<Uuid, String> {
        let session_id = self.get_session_id_by_beacon(&beacon_id)?;
        if let Some(session) = self.get_session(&session_id) {
            Ok(session.add_task(action))
        } else {
            Err("Session not found".to_string())
        }
    }

    pub fn submit_task_result(&self, beacon_id: Uuid, result: TaskResult) -> Result<bool, String> {
        let session_id = self.get_session_id_by_beacon(&beacon_id)?;
        if let Some(session) = self.get_session(&session_id) {
            Ok(session.add_task_result(result))
        } else {
            Err("Session not found".to_string())
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_session_creation() {
        // let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let addr = "192.168.0.1";
        let session = Session::new(0, "test-agent".to_string(), addr.to_string());

        assert_eq!(session.agent_hostname(), "test-agent");
        assert_eq!(session.address(), addr);
        assert_eq!(session.status(), "Active");
        assert!(session.is_active());
    }

    #[test]
    fn test_session_status_changes() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let session = Session::new(1, "test-agent".to_string(), addr.to_string());

        // Initially active
        assert_eq!(session.status(), "Active");

        // Set to idle
        session.set_idle();
        assert_eq!(session.status(), "Idle");

        // Set back to active
        session.set_active();
        assert_eq!(session.status(), "Active");

        // Terminate
        session.terminate();
        assert_eq!(session.status(), "Terminated");
        assert!(!session.is_active());
    }

    #[tokio::test]
    async fn test_session_manager() {
        let manager = SessionManager::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);

        // Create a session
        let session = manager.create_session("test-agent".to_string(), addr.to_string());
        let id = session.id();

        // Get session by ID
        let retrieved = manager.get_session(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id(), id);

        // Remove session
        assert!(manager.remove_session(&id));

        // Session should be gone
        let retrieved = manager.get_session(&id);
        assert!(retrieved.is_none());
    }
}
