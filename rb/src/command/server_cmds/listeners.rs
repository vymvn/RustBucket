/// Listeners command module
use crate::command::*;
use crate::listener::http_listener::HttpListener;
use crate::message::*;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::runtime::Runtime;

use super::get_arg_matches;
use clap;

#[derive(Debug)]
struct ListenerArgs {
    action: String,
    listener_type: Option<String>,
    bind_address: Option<String>,
    port: Option<u16>,
    id: Option<String>,
}

pub struct ServerListenersCommand {}

impl RbCommand for ServerListenersCommand {
    fn name(&self) -> &'static str {
        "listeners"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Server
    }

    fn description(&self) -> &'static str {
        "Manage C2 listeners"
    }

    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
        let cmd = clap::Command::new(self.name())
            .about(self.description().to_string())
            .subcommand(clap::Command::new("list").about("List all active listeners"))
            .subcommand(
                clap::Command::new("start")
                    .about("Start a new listener")
                    .arg(
                        clap::Arg::new("type")
                            .help("Listener type (http, tcp)")
                            .required(true),
                    )
                    .arg(
                        clap::Arg::new("bind")
                            .short('b')
                            .long("bind")
                            .help("Address to bind to")
                            .default_value("0.0.0.0"),
                    )
                    .arg(
                        clap::Arg::new("port")
                            .short('p')
                            .long("port")
                            .help("Port to listen on")
                            .required(true),
                    ),
            )
            .subcommand(
                clap::Command::new("stop").about("Stop a listener").arg(
                    clap::Arg::new("id")
                        .help("Listener ID to stop")
                        .required(true),
                ),
            );

        // Get the arguments part (skip the command name)
        // let args_str = command_line
        //     .trim_start()
        //     .strip_prefix(self.name())
        //     .unwrap_or("")
        //     .trim_start();
        //
        // // Parse the arguments
        // let matches = cmd.try_get_matches_from(
        //     std::iter::once(self.name()).chain(args_str.split_whitespace()),
        // )?;

        let matches = get_arg_matches(&cmd, command_line)?;

        let mut args = ListenerArgs {
            action: String::new(),
            listener_type: None,
            bind_address: None,
            port: None,
            id: None,
        };

        // Parse subcommands
        if let Some(sub_matches) = matches.subcommand_matches("list") {
            args.action = "list".to_string();
        } else if let Some(sub_matches) = matches.subcommand_matches("start") {
            args.action = "start".to_string();
            args.listener_type = sub_matches.get_one::<String>("type").cloned();
            args.bind_address = sub_matches.get_one::<String>("bind").cloned();
            args.port = sub_matches
                .get_one::<String>("port")
                .and_then(|p| p.parse::<u16>().ok());
        } else if let Some(sub_matches) = matches.subcommand_matches("stop") {
            args.action = "stop".to_string();
            args.id = sub_matches.get_one::<String>("id").cloned();
        }

        Ok(Box::new(args))
    }

    fn execute_with_parsed_args(
        &self,
        context: &mut CommandContext,
        args: Box<dyn Any>,
    ) -> CommandResult {
        match args.downcast::<ListenerArgs>() {
            Ok(args) => {
                log::debug!("Executing listener command with args: {:?}", args);

                match args.action.as_str() {
                    "list" => {
                        // Get all active listeners from context
                        let listeners = context.listeners.clone();

                        // Lock the mutex to access the HashMap
                        let listeners_guard = match listeners.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(), // Handle poisoned mutex
                        };

                        if listeners_guard.is_empty() {
                            return Ok(CommandOutput::Text("No active listeners".to_string()));
                        }

                        // Format as a table
                        let headers = vec![
                            "ID".to_string(),
                            "Name".to_string(),
                            "Address".to_string(),
                            "Status".to_string(),
                        ];

                        let mut rows = Vec::new();

                        for (id, listener_arc) in listeners_guard.iter() {
                            // Lock each listener to access its properties
                            if let Ok(listener) = listener_arc.lock() {
                                rows.push(vec![
                                    listener.id().to_string(),
                                    listener.name().to_string(),
                                    listener.addr().to_string(),
                                    listener.is_running().to_string(),
                                ]);
                            } else {
                                // If we can't lock a specific listener, add a row showing just the ID and an error
                                rows.push(vec![
                                    id.to_string(),
                                    "ERROR".to_string(),
                                    "Unable to access listener".to_string(),
                                    "".to_string(),
                                ]);
                            }
                        }

                        Ok(CommandOutput::Table { headers, rows })
                    }
                    "start" => {
                        // Validate required parameters
                        let listener_type = match &args.listener_type {
                            Some(t) => t,
                            None => {
                                return Err(CommandError::ExecutionFailed(
                                    "Listener type required".to_string(),
                                ))
                            }
                        };

                        let bind_address = args
                            .bind_address
                            .clone()
                            .unwrap_or_else(|| "0.0.0.0".to_string());

                        let port = match args.port {
                            Some(p) => p,
                            None => {
                                return Err(CommandError::ExecutionFailed(
                                    "Port required".to_string(),
                                ))
                            }
                        };

                        // Parse the bind address string into a SocketAddr
                        let socket_addr = match bind_address.parse::<std::net::IpAddr>() {
                            Ok(ip) => std::net::SocketAddr::new(ip, port),
                            Err(_) => {
                                return Err(CommandError::ExecutionFailed(format!(
                                    "Invalid bind address: {}",
                                    bind_address
                                )))
                            }
                        };

                        // Check if the listener type is supported
                        match listener_type.to_lowercase().as_str() {
                            "http" => {
                                // Create a new HTTP listener
                                let mut new_listener = HttpListener::new(
                                    format!("HTTP_{}:{}", bind_address, port).as_str(),
                                    socket_addr,
                                    context.session_manager.clone(),
                                );

                                // Get the mutex-protected listeners map
                                let listeners = context.listeners.clone();
                                let mut listeners_guard = match listeners.lock() {
                                    Ok(guard) => guard,
                                    Err(poisoned) => poisoned.into_inner(), // Handle poisoned mutex
                                };

                                // Store the listener ID before starting
                                let listener_id = new_listener.id();
                                let listener_name = new_listener.name().to_string();

                                // Instead of creating a new runtime, we'll spawn a task that will:
                                // 1. Start the listener
                                // 2. Update our command output via a channel

                                // Create a oneshot channel to get the result
                                let (tx, rx) = tokio::sync::oneshot::channel();

                                // Clone values needed for the closure
                                let listeners_clone = context.listeners.clone();

                                // Spawn the task to start the listener
                                tokio::spawn(async move {
                                    match new_listener.start().await {
                                        Ok(_) => {
                                            // Insert the listener into the map
                                            if let Ok(mut map) = listeners_clone.lock() {
                                                map.insert(
                                                    listener_id,
                                                    Arc::new(Mutex::new(Box::new(new_listener))),
                                                );
                                            }

                                            // Send success result
                                            let _ = tx.send(Ok(()));
                                        }
                                        Err(e) => {
                                            // Send error result
                                            let _ = tx.send(Err(e));
                                        }
                                    }
                                });

                                Ok(CommandOutput::Text(format!(
                                    "HTTP listener '{}' starting on {}:{} (ID: {})",
                                    listener_name, bind_address, port, listener_id
                                )))
                            }
                            "tcp" => {
                                // Currently only HTTP is implemented
                                Err(CommandError::ExecutionFailed(
                                    "TCP listener type not yet implemented".to_string(),
                                ))
                            }
                            _ => Err(CommandError::ExecutionFailed(format!(
                                "Unsupported listener type: {}",
                                listener_type
                            ))),
                        }
                    }
                    "stop" => {
                        // Validate required parameters
                        let id = match &args.id {
                            Some(id) => id,
                            None => {
                                return Err(CommandError::ExecutionFailed(
                                    "Listener ID required".to_string(),
                                ))
                            }
                        };

                        let listeners = context.listeners.clone();

                        // Lock the mutex to access the HashMap
                        let mut listeners_guard = match listeners.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(), // Handle poisoned mutex
                        };

                        // Check if listener exists
                        // if let Some(listener_arc) = listeners_guard.get(id) {
                        //     // Try to lock the listener to stop it
                        //     if let Ok(mut listener) = listener_arc.lock() {
                        //         // Create runtime to run async stop method
                        //         let rt = Runtime::new().unwrap();
                        //
                        //         // Call stop method and wait for result
                        //         return rt.block_on(async {
                        //             match listener.stop().await {
                        //                 Ok(_) => Ok(()),
                        //                 Err(e) => Err(format!("Failed to stop listener: {}", e)),
                        //             }
                        //         });
                        //     } else {
                        //         return Err("Unable to access listener".to_string());
                        //     }
                        // } else {
                        //     return Err(format!("Listener with ID '{}' not found", id));
                        // }

                        // Stop the listener using context
                        // match context.stop_listener(id) {
                        //     Ok(_) => Ok(CommandOutput::Text(format!(
                        //         "Listener with ID {} stopped",
                        //         id
                        //     ))),
                        //     Err(e) => Err(CommandError::ExecutionFailed(format!(
                        //         "Failed to stop listener: {}",
                        //         e
                        //     ))),
                        // }

                        Ok(CommandOutput::Text("meowmeow".to_string()))
                    }
                    _ => Err(CommandError::ExecutionFailed(format!(
                        "Unknown action: {}",
                        args.action
                    ))),
                }
            }
            Err(_) => Err(CommandError::ExecutionFailed(
                "Failed to process arguments".to_string(),
            )),
        }
    }
}
