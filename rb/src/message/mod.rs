use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    pub args: Vec<String>,
    pub id: String, // Request ID for matching responses
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResponse {
    pub id: String, // Matching request ID
    pub status: ResponseStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ResponseStatus {
    Success,
    Error,
}
