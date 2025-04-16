use crate::message::{CommandError, CommandOutput, CommandResult};
use crate::session::Session;
use std::any::Any;
use std::collections::HashMap;
use std::result::Result;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

use clap;
// use rb_server::listener::*;

mod server_cmds;

// Define command types
pub enum CommandType {
    Server,  // Commands that control the server
    Implant, // Commands sent to implants
}

// Base Command trait that both types will implement
pub trait RbCommand: Send + Sync {
    fn name(&self) -> &str;
    fn command_type(&self) -> CommandType;
    fn description(&self) -> &str;
    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error>;
    fn execute_with_parsed_args(
        &self,
        context: &mut CommandContext,
        args: Box<dyn Any>,
    ) -> CommandResult;
}

// Context passed to commands (can contain server state, active session, etc.)
pub struct CommandContext {
    // pub active_session: Option<Arc<Mutex<Session>>>,
    pub sessions: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Session>>>>>,
    pub command_registry: Arc<CommandRegistry>,
    // pub listeners: Arc<Mutex<HashMap<Uuid, Arc<Mutex<Box<dyn Listener>>>>>>,
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
        // registry.register(Box::new(ServerSessionsCommand {}));
        registry.register(Box::new(server_cmds::ServerHelpCommand {}));

        // Register built-in implant commands
        // registry.register(Box::new(ImplantLsCommand {}));
        // registry.register(Box::new(ImplantPwdCommand {}));
        // registry.register(Box::new(ImplantCatCommand {}));

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
    pub async fn execute(&self, context: &mut CommandContext, command_line: &str) -> CommandResult {
        // Parse the command
        let parts: Vec<&str> = command_line.split_whitespace().collect();
        if parts.is_empty() {
            return Err(CommandError::InvalidArguments(
                "No command specified".into(),
            ));
        }

        let command_name = parts[0];

        // First check server commands
        if let Some(command) = self.get_server_command(command_name) {
            return self.execute_command(command, context, command_line).await;
        }

        // If there's an active session, check implant commands
        // if context.active_session.is_some() {
        //     if let Some(command) = self.get_implant_command(command_name) {
        //         return self
        //             .execute_implant_command(command, context, command_line)
        //             .await;
        //     }
        // }

        // Command not found
        Err(CommandError::TargetNotFound(format!(
            "Command '{}' not found",
            command_name
        )))
    }

    async fn execute_command(
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
    ) -> CommandResult {
        // Parse arguments first
        let args_result = command.parse_args(command_line);

        match args_result {
            Ok(parsed_args) => {
                // if let Some(session_arc) = &context.active_session {
                //     let mut session = session_arc.lock().map_err(|_| {
                //         CommandError::Internal("Failed to lock active session".into())
                //     })?;
                //
                //     // Send the command to the implant through the session
                //     return session.send_command(command.name(), command_line).await;
                // }

                Err(CommandError::NoActiveSession("No active session".into()))
            }
            Err(err) => Err(CommandError::InvalidArguments(format!(
                "Failed to parse arguments: {}",
                err
            ))),
        }
    }
}

// // Example server command implementation
// pub struct ServerListenersCommand {}
//
// impl Command for ServerListenersCommand {
//     fn name(&self) -> &str {
//         "listeners"
//     }
//
//     fn command_type(&self) -> CommandType {
//         CommandType::Server
//     }
//
//     fn description(&self) -> &str {
//         "Manage C2 listeners"
//     }
//
//     fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
//         use clap::{Arg, ArgAction, Command};
//
//         let cmd = Command::new(self.name())
//             .about(self.description())
//             .subcommand(Command::new("list").about("List all active listeners"))
//             .subcommand(
//                 Command::new("start")
//                     .about("Start a new listener")
//                     .arg(
//                         Arg::new("type")
//                             .help("Listener type (http, tcp)")
//                             .required(true),
//                     )
//                     .arg(
//                         Arg::new("bind")
//                             .short('b')
//                             .long("bind")
//                             .help("Address to bind to")
//                             .default_value("0.0.0.0"),
//                     )
//                     .arg(
//                         Arg::new("port")
//                             .short('p')
//                             .long("port")
//                             .help("Port to listen on")
//                             .required(true),
//                     ),
//             )
//             .subcommand(
//                 Command::new("stop")
//                     .about("Stop a listener")
//                     .arg(Arg::new("id").help("Listener ID to stop").required(true)),
//             );
//
//         // Get the arguments part (skip the command name)
//         let args_str = command_line
//             .trim_start()
//             .strip_prefix(self.name())
//             .unwrap_or("")
//             .trim_start();
//
//         // Parse the arguments
//         let matches = cmd.try_get_matches_from(
//             std::iter::once(self.name()).chain(args_str.split_whitespace()),
//         )?;
//
//         #[derive(Debug)]
//         struct ListenerArgs {
//             action: String,
//             listener_type: Option<String>,
//             bind_address: Option<String>,
//             port: Option<u16>,
//             id: Option<String>,
//         }
//
//         let mut args = ListenerArgs {
//             action: String::new(),
//             listener_type: None,
//             bind_address: None,
//             port: None,
//             id: None,
//         };
//
//         // Parse subcommands
//         if let Some(sub_matches) = matches.subcommand_matches("list") {
//             args.action = "list".to_string();
//         } else if let Some(sub_matches) = matches.subcommand_matches("start") {
//             args.action = "start".to_string();
//             args.listener_type = sub_matches.get_one::<String>("type").cloned();
//             args.bind_address = sub_matches.get_one::<String>("bind").cloned();
//             args.port = sub_matches.get_one::<u16>("port").copied();
//         } else if let Some(sub_matches) = matches.subcommand_matches("stop") {
//             args.action = "stop".to_string();
//             args.id = sub_matches.get_one::<String>("id").cloned();
//         }
//
//         Ok(Box::new(args))
//     }
//
//     fn execute_with_parsed_args(
//         &self,
//         context: &mut CommandContext,
//         args: Box<dyn Any>,
//     ) -> CommandResult {
//         // Implementation for executing the listeners command
//         // ...
//
//         Ok(CommandOutput::Text("Listener command executed".to_string()))
//     }
// }

// Example implant command
// pub struct ImplantLsCommand {}
//
// impl Command for ImplantLsCommand {
//     fn name(&self) -> &str {
//         "ls"
//     }
//
//     fn command_type(&self) -> CommandType {
//         CommandType::Implant
//     }
//
//     fn description(&self) -> &str {
//         "List files in directory on implant"
//     }
//
//     fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
//         use clap::{Arg, Command};
//
//         let cmd = Command::new(self.name())
//             .about(self.description())
//             .arg(Arg::new("path").help("Path to list").default_value("."));
//
//         // Get the arguments part
//         let args_str = command_line
//             .trim_start()
//             .strip_prefix(self.name())
//             .unwrap_or("")
//             .trim_start();
//
//         let matches = cmd.try_get_matches_from(
//             std::iter::once(self.name()).chain(args_str.split_whitespace()),
//         )?;
//
//         #[derive(Debug)]
//         struct LsArgs {
//             path: String,
//         }
//
//         let args = LsArgs {
//             path: matches.get_one::<String>("path").unwrap().clone(),
//         };
//
//         Ok(Box::new(args))
//     }
//
//     fn execute_with_parsed_args(
//         &self,
//         context: &mut CommandContext,
//         args: Box<dyn Any>,
//     ) -> CommandResult {
//         // For implant commands, this typically just prepares the command for sending
//         // The actual execution happens in the implant
//
//         // In a real implementation, this would be sent to the active session
//
//         Ok(CommandOutput::Text(
//             "ls command will be sent to implant".to_string(),
//         ))
//     }
// }
