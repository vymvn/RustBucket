use serde::{Deserialize, Serialize};

// #[derive(Debug, Serialize, Deserialize)]
// pub struct CommandRequest {
//     pub command: String,
//     pub args: Vec<String>,
//     pub id: String, // Request ID for matching responses
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// pub struct CommandResponse {
//     pub id: String, // Matching request ID
//     pub status: ResponseStatus,
//     pub result: Option<String>,
//     pub error: Option<String>,
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// pub enum ResponseStatus {
//     Success,
//     Error,
// }

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
