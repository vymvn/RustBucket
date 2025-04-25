use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
// use tokio::sync::mpsc;
use uuid::Uuid;

use crate::task::*;

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

    /// Implant ID
    implant_id: Uuid,

    /// implant hostname
    implant_hostname: String,

    /// Address the implant connected from
    ip_address: String,

    /// When the session was created
    created_at: SystemTime,

    /// Last time communication was received from this session
    last_seen: SystemTime,

    /// Current status of the session
    status: Arc<Mutex<SessionStatus>>,

    /// Flag indicating if session is active
    active: AtomicBool,

    /// Map of session ID to tasks
    tasks: Arc<Mutex<HashMap<usize, Vec<Task>>>>,

    /// Map of task ID to session ID for quick lookups
    task_to_session: Arc<Mutex<HashMap<Uuid, usize>>>,

    /// Map of task ID to task results
    results: Arc<Mutex<HashMap<Uuid, TaskResult>>>,
    // /// Map of implant ID to session ID
    // implant_to_session: Arc<Mutex<HashMap<Uuid, usize>>>,
    // /// Channel for sending commands to the implant
    // command_tx: Option<mpsc::Sender<String>>,
    //
    // /// Channel for receiving responses from the implant
    // response_rx: Option<mpsc::Receiver<String>>,
    //
    // /// Channel for receiving heartbeats or other events from the implant
    // event_rx: Option<mpsc::Receiver<SessionEvent>>,
    // /// Tasks assigned to this session
    // tasks: Arc<Mutex<Vec<Task>>>,
    //
    // /// Task results received from this session
    // results: Arc<Mutex<Vec<TaskResult>>>,
}

/// Events that can be sent from an implant to the server
#[derive(Debug, Clone)]
pub enum SessionEvent {
    Heartbeat,
    Error(String),
    Info(String),
    Disconnect,
}

impl Session {
    /// Create a new session
    pub fn new(id: usize, implant_id: Uuid, implant_hostname: String, ip_address: String) -> Self {
        let now = SystemTime::now();

        let new_session = Session {
            id,
            implant_id,
            implant_hostname,
            ip_address,
            created_at: now,
            last_seen: now,
            status: Arc::new(Mutex::new(SessionStatus::Active)),
            active: AtomicBool::new(true),
            tasks: Arc::new(Mutex::new(HashMap::new())),
            task_to_session: Arc::new(Mutex::new(HashMap::new())),
            results: Arc::new(Mutex::new(HashMap::new())),
            // implant_to_session: Arc::new(Mutex::new(HashMap::new())),
        };

        // new_session
        //     .map_implant_to_session(new_session.implant_id)
        //     .unwrap_or_else(|e| {
        //         log::error!("Failed to map implant to session: {}", e);
        //     });
        new_session
    }

    // /// Map a implant ID to a session ID
    // pub fn map_implant_to_session(&self, implant_id: Uuid) -> Result<(), String> {
    //     if let Ok(mut mapping) = self.implant_to_session.lock() {
    //         mapping.insert(implant_id, self.id);
    //         Ok(())
    //     } else {
    //         Err("Failed to acquire lock on implant mapping".to_string())
    //     }
    // }

    // /// Get session ID for a implant
    // pub fn get_session_id_by_implant(&self, implant_id: &Uuid) -> Result<usize, String> {
    //     if let Ok(mapping) = self.implant_to_session.lock() {
    //         if let Some(&session_id) = mapping.get(implant_id) {
    //             Ok(session_id)
    //         } else {
    //             Err(format!("No session found for implant ID {}", implant_id))
    //         }
    //     } else {
    //         Err("Failed to acquire lock on implant mapping".to_string())
    //     }
    // }

    /// Get session ID for a task
    pub fn get_session_id_by_task(&self, task_id: &Uuid) -> Result<usize, String> {
        if let Ok(mapping) = self.task_to_session.lock() {
            if let Some(&session_id) = mapping.get(task_id) {
                Ok(session_id)
            } else {
                Err(format!("No session found for task ID {}", task_id))
            }
        } else {
            Err("Failed to acquire lock on task mapping".to_string())
        }
    }

    /// Create a new task for a session
    pub fn create_task(&self, command: String, args: Vec<String>) -> Result<Uuid, String> {
        let task_id = Uuid::new_v4();

        let task = Task {
            id: task_id,
            implant_id: self.implant_id,
            session_id: self.id,
            command,
            args,
            created_at: SystemTime::now(),
            status: TaskStatus::Pending,
        };

        // Store the task
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks
                .entry(self.id)
                .or_insert_with(Vec::new)
                .push(task.clone());
        } else {
            return Err("Failed to acquire lock on tasks".to_string());
        }

        // Add mapping from task ID to session ID
        if let Ok(mut mapping) = self.task_to_session.lock() {
            mapping.insert(task_id, self.id);
        } else {
            // If we fail to add the mapping, try to remove the task
            if let Ok(mut tasks) = self.tasks.lock() {
                if let Some(session_tasks) = tasks.get_mut(&self.id) {
                    session_tasks.retain(|t| t.id != task_id);
                }
            }
            return Err("Failed to acquire lock on task mapping".to_string());
        }

        Ok(task_id)
    }

    /// Get a task by ID
    pub fn get_task(&self, task_id: &Uuid) -> Result<Task, String> {
        // First find which session this task belongs to
        let session_id = self.get_session_id_by_task(task_id)?;

        // Then get the task from that session
        if let Ok(tasks) = self.tasks.lock() {
            if let Some(session_tasks) = tasks.get(&session_id) {
                if let Some(task) = session_tasks.iter().find(|t| &t.id == task_id) {
                    return Ok(task.clone());
                }
            }
        }

        Err(format!("Task with ID {} not found", task_id))
    }

    /// Get all tasks for a session
    pub fn get_tasks_for_session(&self, session_id: usize) -> Result<Vec<Task>, String> {
        if let Ok(tasks) = self.tasks.lock() {
            if let Some(session_tasks) = tasks.get(&session_id) {
                Ok(session_tasks.clone())
            } else {
                Ok(Vec::new()) // No tasks for this session yet
            }
        } else {
            Err("Failed to acquire lock on tasks".to_string())
        }
    }

    /// Get all pending tasks for a session
    pub fn get_pending_tasks_for_session(&self, session_id: usize) -> Result<Vec<Task>, String> {
        let all_tasks = self.get_tasks_for_session(session_id)?;
        Ok(all_tasks
            .into_iter()
            .filter(|task| task.status == TaskStatus::Pending)
            .collect())
    }

    /// Get all pending tasks for a implant
    pub fn get_pending_tasks_for_implant(&self, implant_id: &Uuid) -> Result<Vec<Task>, String> {
        let all_tasks = self.get_tasks_for_session(self.id)?;

        Ok(all_tasks
            .into_iter()
            .filter(|task| task.status == TaskStatus::Pending && task.implant_id == *implant_id)
            .collect())
    }

    /// Update task status
    pub fn update_task_status(&self, task_id: &Uuid, status: TaskStatus) -> Result<(), String> {
        let session_id = self.get_session_id_by_task(task_id)?;

        if let Ok(mut tasks) = self.tasks.lock() {
            if let Some(session_tasks) = tasks.get_mut(&session_id) {
                if let Some(task) = session_tasks.iter_mut().find(|t| &t.id == task_id) {
                    task.status = status;
                    return Ok(());
                }
            }
        }

        Err(format!("Failed to update status for task {}", task_id))
    }

    /// Submit a task result
    pub fn submit_task_result(&self, result: TaskResult) -> Result<(), String> {
        let task_id = result.task_id;

        // Update the task status
        self.update_task_status(&task_id, result.status.clone())?;

        // Store the result
        if let Ok(mut results) = self.results.lock() {
            results.insert(task_id, result);
            Ok(())
        } else {
            Err("Failed to store task result".to_string())
        }
    }

    /// Get task result by task ID
    pub fn get_task_result(&self, task_id: &Uuid) -> Result<TaskResult, String> {
        if let Ok(results) = self.results.lock() {
            if let Some(result) = results.get(task_id) {
                Ok(result.clone())
            } else {
                Err(format!("No result found for task {}", task_id))
            }
        } else {
            Err("Failed to acquire lock on results".to_string())
        }
    }

    /// Get all results for a session
    pub fn get_results_for_session(&self, session_id: usize) -> Result<Vec<TaskResult>, String> {
        // First get all task IDs for this session
        let task_ids = if let Ok(tasks) = self.tasks.lock() {
            if let Some(session_tasks) = tasks.get(&session_id) {
                session_tasks.iter().map(|t| t.id).collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            return Err("Failed to acquire lock on tasks".to_string());
        };

        // Then get all results for these tasks
        if let Ok(results) = self.results.lock() {
            let session_results = task_ids
                .iter()
                .filter_map(|task_id| results.get(task_id).cloned())
                .collect();
            Ok(session_results)
        } else {
            Err("Failed to acquire lock on results".to_string())
        }
    }

    /// Get all tasks across all sessions
    pub fn get_all_tasks(&self) -> Vec<Task> {
        let mut all_tasks = Vec::new();

        if let Ok(tasks) = self.tasks.lock() {
            for session_tasks in tasks.values() {
                all_tasks.extend(session_tasks.clone());
            }
        }

        all_tasks
    }

    /// Get all results across all sessions
    pub fn get_all_results(&self) -> Vec<TaskResult> {
        if let Ok(results) = self.results.lock() {
            results.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Remove all tasks and results for a session
    pub fn remove_session(&self, session_id: usize) -> Result<(), String> {
        // Get all task IDs for this session before removing them
        let task_ids = if let Ok(tasks) = self.tasks.lock() {
            if let Some(session_tasks) = tasks.get(&session_id) {
                session_tasks.iter().map(|t| t.id).collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            return Err("Failed to acquire lock on tasks".to_string());
        };

        // Remove tasks for this session
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.remove(&session_id);
        } else {
            return Err("Failed to acquire lock on tasks for removal".to_string());
        }

        // Remove task-to-session mappings
        if let Ok(mut mapping) = self.task_to_session.lock() {
            for task_id in &task_ids {
                mapping.remove(task_id);
            }
        } else {
            return Err("Failed to acquire lock on task mapping".to_string());
        }

        // Remove results for these tasks
        if let Ok(mut results) = self.results.lock() {
            for task_id in &task_ids {
                results.remove(task_id);
            }
        } else {
            return Err("Failed to acquire lock on results".to_string());
        }

        Ok(())
    }

    /// Clear all tasks and results
    pub fn clear_all(&self) -> Result<(), String> {
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.clear();
        } else {
            return Err("Failed to acquire lock on tasks".to_string());
        }

        if let Ok(mut task_mapping) = self.task_to_session.lock() {
            task_mapping.clear();
        } else {
            return Err("Failed to acquire lock on task mapping".to_string());
        }

        if let Ok(mut results) = self.results.lock() {
            results.clear();
        } else {
            return Err("Failed to acquire lock on results".to_string());
        }

        Ok(())
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

    /// Get implant hostname
    pub fn implant_hostname(&self) -> &str {
        &self.implant_hostname
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
    sessions: Arc<Mutex<HashMap<usize, Arc<Session>>>>,
    next_id: AtomicUsize,
    implant_to_session: Arc<Mutex<HashMap<Uuid, usize>>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: AtomicUsize::new(0),
            implant_to_session: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new session
    pub fn create_session(
        &self,
        implant_id: Uuid,
        implant_hostname: String,
        ip_address: String,
    ) -> Arc<Session> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let session = Arc::new(Session::new(id, implant_id, implant_hostname, ip_address));

        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(id, session.clone());
        }

        // Map implant ID to session ID
        if let Ok(mut mapping) = self.implant_to_session.lock() {
            mapping.insert(implant_id, id);
        }

        session
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &usize) -> Option<Arc<Session>> {
        if let Ok(sessions) = self.sessions.lock() {
            sessions.get(id).cloned()
        } else {
            None
        }
    }

    /// Get all sessions
    pub fn get_all_sessions(&self) -> Vec<Arc<Session>> {
        if let Ok(sessions) = self.sessions.lock() {
            sessions.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Delete a session by ID
    pub fn remove_session(&self, id: &usize) -> bool {
        // First remove from sessions map
        let removed = if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(id).is_some()
        } else {
            false
        };

        // Then clean up any implant mappings
        if removed {
            if let Ok(mut mapping) = self.implant_to_session.lock() {
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
            for session in sessions.values() {
                session.terminate();
            }
            sessions.clear();
        }
    }

    /// Get session ID by implant ID
    pub fn get_session_id_by_implant(&self, implant_id: &Uuid) -> Result<usize, String> {
        if let Ok(mapping) = self.implant_to_session.lock() {
            if let Some(session_id) = mapping.get(implant_id) {
                Ok(*session_id)
            } else {
                Err("No session found for this implant ID".to_string())
            }
        } else {
            Err("Failed to acquire lock on implant mapping".to_string())
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
        let implant_id = Uuid::new_v4();
        let session = Session::new(0, implant_id, "test-implant".to_string(), addr.to_string());

        assert_eq!(session.implant_hostname(), "test-implant");
        assert_eq!(session.address(), addr);
        assert_eq!(session.status(), "Active");
        assert!(session.is_active());
    }

    #[test]
    fn test_session_status_changes() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let implant_id = Uuid::new_v4();
        let session = Session::new(1, implant_id, "test-implant".to_string(), addr.to_string());

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
        let implant_id = Uuid::new_v4();

        // Create a session
        let session = manager.create_session(implant_id, "hostname".to_string(), addr.to_string());
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
