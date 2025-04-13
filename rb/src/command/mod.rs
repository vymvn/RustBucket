// File: rb/src/command/mod.rs
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

use crate::session::Session;

// Re-exports
mod builtin;
pub use builtin::*;

// Command execution result
pub type CommandResult = Result<CommandOutput, CommandError>;

// Command definition - represents a C2 operator command
pub struct Command {
    // Command name (used to invoke the command)
    pub name: String,

    // Short description of what the command does
    pub description: String,

    // Detailed usage information
    pub usage: String,

    // Example usage
    pub examples: Vec<String>,

    // Whether this command requires an active session
    pub requires_session: bool,

    // Handler function that executes the command
    handler: Arc<dyn Fn(Option<&mut Session>, Vec<String>) -> CommandResult + Send + Sync>,
}

impl Command {
    pub fn new<F>(
        name: &str,
        description: &str,
        usage: &str,
        examples: Vec<String>,
        requires_session: bool,
        handler: F,
    ) -> Self
    where
        F: Fn(Option<&mut Session>, Vec<String>) -> CommandResult + Send + Sync + 'static,
    {
        Command {
            name: name.to_string(),
            description: description.to_string(),
            usage: usage.to_string(),
            examples,
            requires_session,
            handler: Arc::new(handler),
        }
    }

    pub fn execute(&self, session: Option<&mut Session>, args: Vec<String>) -> CommandResult {
        // Check if the command requires a session
        if self.requires_session && session.is_none() {
            return Err(CommandError::NoActiveSession(
                "This command requires an active session".to_string(),
            ));
        }

        // Execute the command handler
        (self.handler)(session, args)
    }
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("requires_session", &self.requires_session)
            .finish()
    }
}

impl Clone for Command {
    fn clone(&self) -> Self {
        Command {
            name: self.name.clone(),
            description: self.description.clone(),
            usage: self.usage.clone(),
            examples: self.examples.clone(),
            requires_session: self.requires_session,
            handler: self.handler.clone(),
        }
    }
}

// Command output data - flexible output format
#[derive(Debug, Clone, serde::Serialize)]
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
#[derive(Debug, thiserror::Error, serde::Serialize)]
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

// Command registry to store and manage commands
#[derive(Debug, Clone)]
pub struct CommandRegistry {
    commands: Arc<RwLock<HashMap<String, Command>>>,
    active_session_id: Arc<RwLock<Option<uuid::Uuid>>>,
    // sessions: Arc<RwLock<HashMap<uuid::Uuid, Arc<tokio::sync::Mutex<Session>>>>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = CommandRegistry {
            commands: Arc::new(RwLock::new(HashMap::new())),
            active_session_id: Arc::new(RwLock::new(None)),
            // sessions: Arc::new(RwLock::new(HashMap::new())),
        };

        // Register built-in commands
        registry.register_builtin_commands();

        registry
    }

    pub fn register(&self, command: Command) -> Result<(), String> {
        let mut commands = self.commands.write().unwrap();

        if commands.contains_key(&command.name) {
            return Err(format!("Command '{}' is already registered", command.name));
        }

        commands.insert(command.name.clone(), command);
        Ok(())
    }

    pub fn unregister(&self, name: &str) -> Result<Command, String> {
        let mut commands = self.commands.write().unwrap();

        commands
            .remove(name)
            .ok_or_else(|| format!("Command '{}' not found", name))
    }

    pub fn get(&self, name: &str) -> Option<Command> {
        let commands = self.commands.read().unwrap();
        commands.get(name).cloned()
    }

    pub fn list(&self) -> Vec<Command> {
        let commands = self.commands.read().unwrap();
        commands.values().cloned().collect()
    }

    // Register a session with this registry
    // pub fn register_session(&self, session: Session) {
    //     let session_id = session.id;
    //     let session_mutex = Arc::new(tokio::sync::Mutex::new(session));
    //
    //     let mut sessions = self.sessions.write().unwrap();
    //     sessions.insert(session_id, session_mutex);
    // }
    //
    // // Remove a session from this registry
    // pub fn unregister_session(&self, session_id: uuid::Uuid) -> bool {
    //     let mut sessions = self.sessions.write().unwrap();
    //     let removed = sessions.remove(&session_id).is_some();
    //
    //     // If we removed the active session, clear the active session
    //     if removed {
    //         let mut active = self.active_session_id.write().unwrap();
    //         if active.as_ref() == Some(&session_id) {
    //             *active = None;
    //         }
    //     }
    //
    //     removed
    // }

    // Set the active session
    // pub fn set_active_session(&self, session_id: Option<uuid::Uuid>) -> Result<(), String> {
    //     if let Some(id) = session_id {
    //         // Check if the session exists
    //         let sessions = self.sessions.read().unwrap();
    //         if !sessions.contains_key(&id) {
    //             return Err(format!("Session not found: {}", id));
    //         }
    //     }
    //
    //     // Set the active session
    //     let mut active = self.active_session_id.write().unwrap();
    //     *active = session_id;
    //
    //     Ok(())
    // }
    //
    // // Get the active session if there is one
    // pub fn get_active_session(&self) -> Option<Arc<tokio::sync::Mutex<Session>>> {
    //     let active_id = self.active_session_id.read().unwrap();
    //
    //     match *active_id {
    //         Some(id) => {
    //             let sessions = self.sessions.read().unwrap();
    //             sessions.get(&id).cloned()
    //         }
    //         None => None,
    //     }
    // }
    //
    // // List all sessions
    // pub fn list_sessions(&self) -> Vec<uuid::Uuid> {
    //     let sessions = self.sessions.read().unwrap();
    //     sessions.keys().cloned().collect()
    // }

    // Execute a command in the current context
    pub async fn execute(&self, command_line: &str) -> CommandResult {
        // Parse command and arguments
        let mut parts = command_line.split_whitespace();
        let command_name = match parts.next() {
            Some(name) => name,
            None => {
                return Err(CommandError::InvalidArguments(
                    "No command specified".into(),
                ))
            }
        };

        let args: Vec<String> = parts.map(String::from).collect();

        // Get command
        let command = self.get(command_name).ok_or_else(|| {
            CommandError::TargetNotFound(format!("Command '{}' not found", command_name))
        })?;

        // If command requires a session, get the active session
        let mut session_option = None;

        if command.requires_session {
            println!("havent done this yet");
            // if let Some(session_arc) = self.get_active_session() {
            //     let mut session = session_arc
            //         .try_lock()
            //         .map_err(|_| CommandError::Internal("Failed to lock active session".into()))?;
            //     session_option = Some(&mut *session);
            // } else {
            //     return Err(CommandError::NoActiveSession(
            //         "No active session. Use 'sessions interact <id>' to select a session.".into(),
            //     ));
            // }
        }

        // Execute command
        command.execute(session_option, args)
    }

    // Register built-in commands like help, exit, etc.
    fn register_builtin_commands(&mut self) {
        // Register help command
        self.register(builtin::help_command(self.clone()))
            .expect("Failed to register help command");

        // Register other built-in commands
        for cmd in builtin::get_builtin_commands(self.clone()) {
            self.register(cmd)
                .expect("Failed to register built-in command");
        }
    }
}

// Default implementation for CommandRegistry
impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Parser for command-line input
pub struct CommandParser;

impl CommandParser {
    pub fn parse(input: &str) -> (String, Vec<String>) {
        // TODO: Implement more sophisticated parsing with quoted arguments, etc.
        let mut parts = input.trim().split_whitespace();
        let command = parts.next().unwrap_or("").to_string();
        let args = parts.map(String::from).collect();

        (command, args)
    }
}
