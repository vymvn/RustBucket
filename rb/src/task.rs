use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use uuid::Uuid;

/// Status of a task
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl ToString for TaskStatus {
    fn to_string(&self) -> String {
        match self {
            TaskStatus::Pending => "Pending".to_string(),
            TaskStatus::InProgress => "In Progress".to_string(),
            TaskStatus::Completed => "Completed".to_string(),
            TaskStatus::Failed => "Failed".to_string(),
            TaskStatus::Cancelled => "Cancelled".to_string(),
        }
    }
}

/// A task to be executed by an agent/beacon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique ID for this task
    pub id: Uuid,

    /// Beacon/Agent ID this task is assigned to
    pub beacon_id: Uuid,

    /// Session ID for this task
    pub session_id: usize,

    /// Action to do
    pub action: String,

    /// When the task was created
    pub created_at: SystemTime,

    /// Current status of the task
    pub status: TaskStatus,
}

/// Task result from an agent/beacon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Task ID this result belongs to
    pub task_id: Uuid,

    /// Beacon/Agent ID that executed the task
    pub beacon_id: Uuid,

    /// Session ID for this task
    pub session_id: usize,

    /// Output from the command
    pub output: String,

    /// Error output, if any
    pub error: Option<String>,

    /// Status code returned by the command
    pub status_code: Option<i32>,

    /// Status of the task
    pub status: TaskStatus,

    /// When the result was created
    pub completed_at: SystemTime,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// TaskManager handles task operations across sessions
pub struct TaskManager {
    /// Map of session ID to tasks
    tasks: Arc<Mutex<HashMap<usize, Vec<Task>>>>,

    /// Map of task ID to session ID for quick lookups
    task_to_session: Arc<Mutex<HashMap<Uuid, usize>>>,

    /// Map of task ID to task results
    results: Arc<Mutex<HashMap<Uuid, TaskResult>>>,

    /// Map of beacon ID to session ID
    beacon_to_session: Arc<Mutex<HashMap<Uuid, usize>>>,
}

impl TaskManager {
    /// Create a new task manager
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            task_to_session: Arc::new(Mutex::new(HashMap::new())),
            results: Arc::new(Mutex::new(HashMap::new())),
            beacon_to_session: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Map a beacon ID to a session ID
    pub fn map_beacon_to_session(&self, beacon_id: Uuid, session_id: usize) -> Result<(), String> {
        if let Ok(mut mapping) = self.beacon_to_session.lock() {
            mapping.insert(beacon_id, session_id);
            Ok(())
        } else {
            Err("Failed to acquire lock on beacon mapping".to_string())
        }
    }

    /// Get session ID for a beacon
    pub fn get_session_id_by_beacon(&self, beacon_id: &Uuid) -> Result<usize, String> {
        if let Ok(mapping) = self.beacon_to_session.lock() {
            if let Some(&session_id) = mapping.get(beacon_id) {
                Ok(session_id)
            } else {
                Err(format!("No session found for beacon ID {}", beacon_id))
            }
        } else {
            Err("Failed to acquire lock on beacon mapping".to_string())
        }
    }

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

    /// Get beacon ID from session id
    pub fn get_beacon_id_by_session(&self, session_id: usize) -> Result<Uuid, String> {
        if let Ok(mapping) = self.beacon_to_session.lock() {
            for (beacon_id, sid) in mapping.iter() {
                if *sid == session_id {
                    return Ok(*beacon_id);
                }
            }
        }
        Err(format!("No beacon found for session ID {}", session_id))
    }

    /// Create a new task for a session
    pub fn create_task(&self, session_id: usize, action: String) -> Result<Uuid, String> {
        let task_id = Uuid::new_v4();

        let task = Task {
            id: task_id,
            beacon_id: self.get_beacon_id_by_session(session_id)?,
            session_id,
            action,
            created_at: SystemTime::now(),
            status: TaskStatus::Pending,
        };

        // Store the task
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks
                .entry(session_id)
                .or_insert_with(Vec::new)
                .push(task.clone());
        } else {
            return Err("Failed to acquire lock on tasks".to_string());
        }

        // Add mapping from task ID to session ID
        if let Ok(mut mapping) = self.task_to_session.lock() {
            mapping.insert(task_id, session_id);
        } else {
            // If we fail to add the mapping, try to remove the task
            if let Ok(mut tasks) = self.tasks.lock() {
                if let Some(session_tasks) = tasks.get_mut(&session_id) {
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

    /// Get all pending tasks for a beacon
    pub fn get_pending_tasks_for_beacon(&self, beacon_id: &Uuid) -> Result<Vec<Task>, String> {
        let session_id = self.get_session_id_by_beacon(beacon_id)?;
        let all_tasks = self.get_tasks_for_session(session_id)?;

        Ok(all_tasks
            .into_iter()
            .filter(|task| task.status == TaskStatus::Pending && task.beacon_id == *beacon_id)
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

        // Remove beacon-to-session mappings
        if let Ok(mut beacon_mapping) = self.beacon_to_session.lock() {
            beacon_mapping.retain(|_, &mut sid| sid != session_id);
        } else {
            return Err("Failed to acquire lock on beacon mapping".to_string());
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

        if let Ok(mut beacon_mapping) = self.beacon_to_session.lock() {
            beacon_mapping.clear();
        } else {
            return Err("Failed to acquire lock on beacon mapping".to_string());
        }

        Ok(())
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Task request from the operator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    /// Beacon/Agent ID this task is for
    pub beacon_id: Uuid,

    /// Command to execute
    pub command: String,

    /// Command arguments
    pub args: Vec<String>,

    /// Optional timeout in seconds
    pub timeout: Option<u64>,
}

/// Serializable task response for API endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    /// Task ID
    pub id: Uuid,

    /// Beacon ID
    pub beacon_id: Uuid,

    /// Action
    pub action: String,

    /// Status
    pub status: String,

    /// Created timestamp (ISO format)
    pub created_at: String,
}

impl From<Task> for TaskResponse {
    fn from(task: Task) -> Self {
        // Convert SystemTime to ISO string (this is a simplification)
        let created_at = match task.created_at.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => {
                let secs = n.as_secs();
                format!("{}", secs)
            }
            Err(_) => "Invalid time".to_string(),
        };

        Self {
            id: task.id,
            beacon_id: task.beacon_id,
            action: task.action,
            status: task.status.to_string(),
            created_at,
        }
    }
}

/// Serializable task result response for API endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResultResponse {
    /// Task ID
    pub task_id: Uuid,

    /// Output
    pub output: String,

    /// Error (if any)
    pub error: Option<String>,

    /// Status
    pub status: String,

    /// Status code
    pub status_code: Option<i32>,

    /// Completed timestamp (ISO format)
    pub completed_at: String,

    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

impl From<TaskResult> for TaskResultResponse {
    fn from(result: TaskResult) -> Self {
        // Convert SystemTime to ISO string (this is a simplification)
        let completed_at = match result.completed_at.duration_since(SystemTime::UNIX_EPOCH) {
            Ok(n) => {
                let secs = n.as_secs();
                format!("{}", secs)
            }
            Err(_) => "Invalid time".to_string(),
        };

        Self {
            task_id: result.task_id,
            output: result.output,
            error: result.error,
            status: result.status.to_string(),
            status_code: result.status_code,
            completed_at,
            execution_time_ms: result.execution_time_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task_manager = TaskManager::new();
        let session_id = 1;
        let beacon_id = Uuid::new_v4();

        // Map beacon to session
        task_manager
            .map_beacon_to_session(beacon_id, session_id)
            .unwrap();

        // Create a task
        let action = "ls".to_string();
        let task_id = task_manager.create_task(session_id, action).unwrap();

        // Check if we can get the task
        let task = task_manager.get_task(&task_id).unwrap();
        assert_eq!(task.action, "ls");
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[test]
    fn test_task_status_update() {
        let task_manager = TaskManager::new();
        let session_id = 1;
        let beacon_id = Uuid::new_v4();

        // Map beacon to session
        task_manager
            .map_beacon_to_session(beacon_id, session_id)
            .unwrap();

        // Create a task
        let task_id = task_manager
            .create_task(session_id, "ls".to_string())
            .unwrap();

        // Update task status
        task_manager
            .update_task_status(&task_id, TaskStatus::InProgress)
            .unwrap();

        // Check if status updated
        let task = task_manager.get_task(&task_id).unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_task_result_submission() {
        let task_manager = TaskManager::new();
        let session_id = 1;
        let beacon_id = Uuid::new_v4();

        // Map beacon to session
        task_manager
            .map_beacon_to_session(beacon_id, session_id)
            .unwrap();

        // Create a task
        let task_id = task_manager
            .create_task(session_id, "pwd".to_string())
            .unwrap();

        // Submit a result
        let result = TaskResult {
            task_id,
            beacon_id,
            session_id,
            output: "C:\\Users\\testuser\\Desktop\n".to_string(),
            error: None,
            status_code: Some(0),
            status: TaskStatus::Completed,
            completed_at: SystemTime::now(),
            execution_time_ms: 100,
        };

        task_manager.submit_task_result(result.clone()).unwrap();

        // Check if result is stored and task status updated
        let stored_result = task_manager.get_task_result(&task_id).unwrap();
        assert_eq!(stored_result.output, "C:\\Users\\testuser\\Desktop\n");

        let task = task_manager.get_task(&task_id).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn test_session_cleanup() {
        let task_manager = TaskManager::new();
        let session_id = 1;
        let beacon_id = Uuid::new_v4();

        // Map beacon to session
        task_manager
            .map_beacon_to_session(beacon_id, session_id)
            .unwrap();

        // Create some tasks
        let task_id1 = task_manager
            .create_task(session_id, "ls".to_string())
            .unwrap();

        let task_id2 = task_manager
            .create_task(session_id, "systeminfo".to_string())
            .unwrap();

        // Submit a result for one task
        let result = TaskResult {
            task_id: task_id1,
            beacon_id,
            session_id,
            output: "output".to_string(),
            error: None,
            status_code: Some(0),
            status: TaskStatus::Completed,
            completed_at: SystemTime::now(),
            execution_time_ms: 100,
        };
        task_manager.submit_task_result(result).unwrap();

        // Remove session
        task_manager.remove_session(session_id).unwrap();

        // Check that tasks and results are gone
        assert!(task_manager.get_task(&task_id1).is_err());
        assert!(task_manager.get_task(&task_id2).is_err());
        assert!(task_manager.get_task_result(&task_id1).is_err());

        // Check that beacon mapping is gone
        assert!(task_manager.get_session_id_by_beacon(&beacon_id).is_err());
    }
}
