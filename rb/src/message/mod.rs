use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandRequest {
    pub command_line: String,
    pub session_id: Option<usize>, // Optional session ID for targeting specific sessions
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct CommandResponse {
//     pub id: String, // Matching request ID
//     pub status: ResponseStatus,
//     pub result: Option<String>,
//     pub error: Option<String>,
// }

#[derive(Debug, Serialize, Deserialize)]
pub enum ResponseStatus {
    Success,
    Error,
}

pub type CommandResult = Result<CommandOutput, CommandError>;

// Command output data - flexible output format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandOutput {
    Text(String),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Json(serde_json::Value),
    Binary(Vec<u8>),
    None,
}

// Command error types
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum CommandError {
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Target not found: {0}")]
    TargetNotFound(String),

    #[error("No active session: {0}")]
    NoActiveSession(String),

    #[error("Session error: {0}")]
    SessionError(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

// implant data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplantInfo {
    pub id: Uuid,
    pub hostname: String,
    pub ip_address: String,
    pub os_info: String,
    pub username: String,
    pub process_id: u32,
    pub first_seen: SystemTime,
    pub last_seen: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplantCheckin {
    pub id: Option<Uuid>, // Optional for first registration
    pub hostname: String,
    pub ip_address: String,
    pub os_info: String,
    pub username: String,
    pub process_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct CheckinResponse {
    pub implant_id: Uuid,
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Task {
//     pub id: Uuid,
//     pub implant_id: Uuid,
//     pub command: String,
//     pub args: Vec<String>,
//     pub created_at: SystemTime,
//     pub status: TaskStatus,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub enum TaskStatus {
//     Pending,
//     InProgress,
//     Completed,
//     Failed,
// }

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct TaskResult {
//     pub task_id: Uuid,
//     pub implant_id: Uuid,
//     pub output: String,
//     pub status: TaskStatus,
//     pub completed_at: SystemTime,
// }
