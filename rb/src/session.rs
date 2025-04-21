use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use uuid::Uuid;

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
    agent_id: String,

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
    pub fn new(id: usize, agent_id: String, ip_address: String) -> Self {
        let now = SystemTime::now();

        Self {
            id,
            agent_id,
            ip_address,
            created_at: now,
            last_seen: now,
            status: Arc::new(Mutex::new(SessionStatus::Active)),
            active: AtomicBool::new(true),
            command_tx: None,
            response_rx: None,
            event_rx: None,
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

    /// Get agent ID
    pub fn agent_id(&self) -> &str {
        &self.agent_id
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
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            next_id: AtomicUsize::new(0),
        }
    }

    /// Create a new session
    pub fn create_session(&self, agent_id: String, ip_address: String) -> Arc<Session> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let session = Arc::new(Session::new(id, agent_id, ip_address));

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

    /// Remove a session
    pub fn remove_session(&self, id: &usize) -> bool {
        if let Ok(mut sessions) = self.sessions.lock() {
            let len_before = sessions.len();
            sessions.retain(|s| s.id() != *id);
            sessions.len() < len_before
        } else {
            false
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

    /// Clean up inactive sessions
    pub fn cleanup_inactive(&self, timeout: Duration) -> usize {
        let mut removed = 0;

        if let Ok(mut sessions) = self.sessions.lock() {
            let len_before = sessions.len();

            sessions.retain(|session| {
                // Keep active sessions
                session.is_active()
            });

            removed = len_before - sessions.len();
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

        assert_eq!(session.agent_id(), "test-agent");
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
