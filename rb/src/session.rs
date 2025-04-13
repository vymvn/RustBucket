// File: rb/src/session.rs
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::client::Client;
use crate::command::{CommandOutput, CommandRegistry};

// Session represents an active connection between the C2 operator and an agent/beacon
pub struct Session {
    // Unique identifier for this session
    pub id: Uuid,

    // Descriptive name for the session (can be set by the operator)
    pub name: String,

    // ID of the client (agent/beacon) associated with this session
    pub client_id: Uuid,

    // When the session was established
    pub established_at: SystemTime,

    // When the session last communicated
    pub last_check_in: SystemTime,

    // Session metadata
    pub metadata: SessionMetadata,

    // Session state
    pub status: SessionStatus,

    // Command registry for executing commands
    command_registry: Arc<CommandRegistry>,

    // Communication channels for sending commands and receiving responses
    command_tx: mpsc::Sender<String>,
    response_rx: mpsc::Receiver<String>,
}

// Metadata about the agent/beacon
#[derive(Debug, Clone)]
pub struct SessionMetadata {
    // Operating system information
    pub os_type: String,
    pub os_version: String,
    pub hostname: String,
    pub username: String,

    // Agent information
    pub agent_version: String,
    pub agent_build: String,

    // Network information
    pub internal_ip: String,
    pub external_ip: String,

    // Permissions/privilege level
    pub privilege_level: PrivilegeLevel,

    // Supported features/capabilities
    pub capabilities: Vec<String>,

    // Custom fields for any additional info
    pub custom: std::collections::HashMap<String, String>,
}
//
// Privilege level of the agent
#[derive(Debug, Clone, PartialEq)]
pub enum PrivilegeLevel {
    User,
    Admin,
    System,
    Root,
    Unknown,
}

// Status of the session
#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,
    Sleeping,
    Disconnected,
    Terminated,
}
//
// impl Session {
//     // Create a new session for a client
//     pub fn new(
//         client: &Client,
//         command_registry: Arc<CommandRegistry>,
//         metadata: SessionMetadata,
//     ) -> (Self, mpsc::Sender<String>) {
//         // Create communication channels
//         let (cmd_tx, cmd_rx) = mpsc::channel(100);
//         let (resp_tx, resp_rx) = mpsc::channel(100);
//
//         let now = SystemTime::now();
//
//         let session = Session {
//             id: Uuid::new_v4(),
//             name: format!("{}_{}", metadata.hostname, Uuid::new_v4().as_u128() % 10000),
//             client_id: client.id,
//             established_at: now,
//             last_check_in: now,
//             metadata,
//             status: SessionStatus::Active,
//             command_registry,
//             command_tx: resp_tx,
//             response_rx: resp_rx,
//         };
//
//         // Spawn a task to handle the communication with the agent
//         Self::start_session_handler(cmd_rx, resp_tx.clone(), client.stream.clone());
//
//         (session, cmd_tx)
//     }
//
//     // Start the background task that handles communication with the agent
//     fn start_session_handler(
//         mut cmd_rx: mpsc::Receiver<String>,
//         resp_tx: mpsc::Sender<String>,
//         mut stream_opt: Option<TcpStream>,
//     ) {
//         tokio::spawn(async move {
//             let mut stream = match stream_opt.take() {
//                 Some(s) => s,
//                 None => return, // No stream available
//             };
//
//             // Simple protocol: send commands and receive responses
//             // A real implementation would use a more robust protocol
//             while let Some(command) = cmd_rx.recv().await {
//                 // Send command to agent
//                 if let Err(e) = stream.write_all(command.as_bytes()).await {
//                     eprintln!("Failed to send command to agent: {}", e);
//                     break;
//                 }
//
//                 // Read response
//                 let mut buffer = [0u8; 4096];
//                 match stream.read(&mut buffer).await {
//                     Ok(0) => {
//                         // Connection closed
//                         break;
//                     }
//                     Ok(n) => {
//                         // Forward response
//                         let response = String::from_utf8_lossy(&buffer[..n]).to_string();
//                         if let Err(e) = resp_tx.send(response).await {
//                             eprintln!("Failed to forward agent response: {}", e);
//                             break;
//                         }
//                     }
//                     Err(e) => {
//                         eprintln!("Error reading from agent: {}", e);
//                         break;
//                     }
//                 }
//             }
//
//             // Session handler terminated
//             eprintln!("Session handler terminated");
//         });
//     }
//
//     // Execute a command on this session
//     pub async fn execute_command(&mut self, command_str: &str) -> Result<CommandOutput, String> {
//         // Update last check-in time
//         self.last_check_in = SystemTime::now();
//
//         // Check if session is active
//         if self.status != SessionStatus::Active {
//             return Err(format!(
//                 "Session is not active (current status: {:?})",
//                 self.status
//             ));
//         }
//
//         // Send command to agent
//         if let Err(e) = self.command_tx.send(command_str.to_string()).await {
//             return Err(format!("Failed to send command: {}", e));
//         }
//
//         // Wait for response with timeout
//         match tokio::time::timeout(Duration::from_secs(30), self.response_rx.recv()).await {
//             Ok(Some(response)) => {
//                 // Process response
//                 Ok(CommandOutput::Text(response))
//             }
//             Ok(None) => {
//                 // Channel closed
//                 self.status = SessionStatus::Disconnected;
//                 Err("Session disconnected".to_string())
//             }
//             Err(_) => {
//                 // Timeout
//                 Err("Command timed out".to_string())
//             }
//         }
//     }
//
//     // Check if the session is alive by sending a ping
//     pub async fn check_alive(&mut self) -> bool {
//         if self.status == SessionStatus::Terminated {
//             return false;
//         }
//
//         // Try to send a ping command
//         match self.execute_command("ping").await {
//             Ok(_) => {
//                 self.status = SessionStatus::Active;
//                 true
//             }
//             Err(_) => {
//                 // If we can't ping, mark as disconnected but not terminated
//                 // It might come back online later
//                 self.status = SessionStatus::Disconnected;
//                 false
//             }
//         }
//     }
//
//     // Terminate the session
//     pub async fn terminate(&mut self) -> Result<(), String> {
//         // Send exit command to agent if still connected
//         if self.status == SessionStatus::Active {
//             // Best effort to tell the agent to exit
//             let _ = self.execute_command("exit").await;
//         }
//
//         // Mark as terminated regardless of agent response
//         self.status = SessionStatus::Terminated;
//
//         Ok(())
//     }
//
//     // Get the time since the last check-in
//     pub fn time_since_last_check_in(&self) -> Duration {
//         SystemTime::now()
//             .duration_since(self.last_check_in)
//             .unwrap_or(Duration::from_secs(0))
//     }
//
//     // Get a display name for the session
//     pub fn display_name(&self) -> String {
//         format!(
//             "{} ({}@{})",
//             self.name, self.metadata.username, self.metadata.hostname
//         )
//     }
// }
//
// // Create a new session with default metadata for testing
// impl Default for SessionMetadata {
//     fn default() -> Self {
//         SessionMetadata {
//             os_type: "unknown".to_string(),
//             os_version: "unknown".to_string(),
//             hostname: "unknown".to_string(),
//             username: "unknown".to_string(),
//             agent_version: "1.0".to_string(),
//             agent_build: "debug".to_string(),
//             internal_ip: "0.0.0.0".to_string(),
//             external_ip: "0.0.0.0".to_string(),
//             privilege_level: PrivilegeLevel::Unknown,
//             capabilities: vec![],
//             custom: std::collections::HashMap::new(),
//         }
//     }
// }
