// File: rb/src/command/builtin.rs
use super::{Command, CommandError, CommandOutput, CommandRegistry, CommandResult};
use crate::session::{Session, SessionStatus};
use std::time::{Duration, SystemTime};

// Create the help command (needs access to the registry to list all commands)
pub fn help_command(registry: CommandRegistry) -> Command {
    Command::new(
        "help",
        "Display help information for commands",
        "help [command]",
        vec!["help".to_string(), "help sessions".to_string()],
        false, // Does not require a session
        move |_session, args| {
            if args.is_empty() {
                // List all commands
                let commands = registry.list();
                let mut headers = vec![
                    "Command".to_string(),
                    "Description".to_string(),
                    "Requires Session".to_string(),
                ];
                let mut rows = Vec::new();

                for cmd in commands {
                    rows.push(vec![
                        cmd.name,
                        cmd.description,
                        if cmd.requires_session {
                            "Yes".to_string()
                        } else {
                            "No".to_string()
                        },
                    ]);
                }

                Ok(CommandOutput::Table { headers, rows })
            } else {
                // Show help for specific command
                let cmd_name = &args[0];
                match registry.get(cmd_name) {
                    Some(cmd) => {
                        let mut help_text = format!("Command: {}\n", cmd.name);
                        help_text.push_str(&format!("Description: {}\n", cmd.description));
                        help_text.push_str(&format!("Usage: {}\n", cmd.usage));
                        help_text.push_str(&format!(
                            "Requires active session: {}\n",
                            if cmd.requires_session { "Yes" } else { "No" }
                        ));

                        if !cmd.examples.is_empty() {
                            help_text.push_str("\nExamples:\n");
                            for example in cmd.examples {
                                help_text.push_str(&format!("  {}\n", example));
                            }
                        }

                        Ok(CommandOutput::Text(help_text))
                    }
                    None => Err(CommandError::TargetNotFound(format!(
                        "Command '{}' not found",
                        cmd_name
                    ))),
                }
            }
        },
    )
}

// Get all built-in commands
pub fn get_builtin_commands(registry: CommandRegistry) -> Vec<Command> {
    vec![
        exit_command(),
        // sessions_command(registry.clone()),
        // interact_command(registry),
        info_command(),
        // shell_command(),
        // kill_command(),
    ]
}

// Exit command - exit the C2 console (not used on sessions)
fn exit_command() -> Command {
    Command::new(
        "exit",
        "Exit the C2 console",
        "exit",
        vec!["exit".to_string()],
        false, // Does not require a session
        |_session, _args| {
            // This would normally signal the console app to exit
            Ok(CommandOutput::Text("Exiting C2 console...".to_string()))
        },
    )
}

// Sessions command - list all sessions
// fn sessions_command(registry: CommandRegistry) -> Command {
//     Command::new(
//         "sessions",
//         "List all agent sessions",
//         "sessions",
//         vec!["sessions".to_string()],
//         false, // Does not require a session
//         move |_session, _args| {
//             let session_ids = registry.list_sessions();
//
//             if session_ids.is_empty() {
//                 return Ok(CommandOutput::Text("No active sessions".to_string()));
//             }
//
//             let mut headers = vec![
//                 "ID".to_string(),
//                 "Name".to_string(),
//                 "Host".to_string(),
//                 "User".to_string(),
//                 "OS".to_string(),
//                 "Status".to_string(),
//                 "Last Check-in".to_string(),
//             ];
//
//             let mut rows = Vec::new();
//             let sessions = registry.sessions.read().unwrap();
//
//             for id in session_ids {
//                 if let Some(session_arc) = sessions.get(&id) {
//                     // Try to lock the session, skip if we can't
//                     if let Ok(session) = session_arc.try_lock() {
//                         let status_str = match session.status {
//                             SessionStatus::Active => "Active",
//                             SessionStatus::Sleeping => "Sleeping",
//                             SessionStatus::Disconnected => "Disconnected",
//                             SessionStatus::Terminated => "Terminated",
//                         };
//
//                         let last_check = format_duration(session.time_since_last_check_in());
//
//                         rows.push(vec![
//                             id.to_string(),
//                             session.name.clone(),
//                             session.metadata.hostname.clone(),
//                             session.metadata.username.clone(),
//                             format!(
//                                 "{} {}",
//                                 session.metadata.os_type, session.metadata.os_version
//                             ),
//                             status_str.to_string(),
//                             last_check,
//                         ]);
//                     }
//                 }
//             }
//
//             Ok(CommandOutput::Table { headers, rows })
//         },
//     )
// }

// Interact command - interact with a session
// fn interact_command(registry: CommandRegistry) -> Command {
//     Command::new(
//         "interact",
//         "Interact with a session",
//         "interact <session_id>",
//         vec!["interact 550e8400-e29b-41d4-a716-446655440000".to_string()],
//         false, // Does not require a session
//         move |_session, args| {
//             if args.is_empty() {
//                 return Err(CommandError::InvalidArguments("Session ID required".into()));
//             }
//
//             let session_id_str = &args[0];
//             let session_id = match uuid::Uuid::parse_str(session_id_str) {
//                 Ok(id) => id,
//                 Err(_) => {
//                     return Err(CommandError::InvalidArguments(
//                         "Invalid session ID format".into(),
//                     ))
//                 }
//             };
//
//             // Set the active session
//             if let Err(e) = registry.set_active_session(Some(session_id)) {
//                 return Err(CommandError::TargetNotFound(e));
//             }
//
//             // Get session info to display
//             let sessions = registry.sessions.read().unwrap();
//             if let Some(session_arc) = sessions.get(&session_id) {
//                 if let Ok(session) = session_arc.try_lock() {
//                     return Ok(CommandOutput::Text(format!(
//                         "Now interacting with session {} ({}@{})",
//                         session.name, session.metadata.username, session.metadata.hostname
//                     )));
//                 }
//             }
//
//             Err(CommandError::Internal(
//                 "Failed to get session information".into(),
//             ))
//         },
//     )
// }

// Info command - get info about the current session
fn info_command() -> Command {
    Command::new(
        "info",
        "Display information about the current session",
        "info",
        vec!["info".to_string()],
        true, // Requires a session
        |session, _args| {
            let session = session.unwrap(); // Safe because requires_session is true

            let mut info = Vec::new();
            info.push(("Session ID", session.id.to_string()));
            info.push(("Name", session.name.clone()));
            info.push(("Hostname", session.metadata.hostname.clone()));
            info.push(("Username", session.metadata.username.clone()));
            info.push((
                "OS",
                format!(
                    "{} {}",
                    session.metadata.os_type, session.metadata.os_version
                ),
            ));
            info.push(("Internal IP", session.metadata.internal_ip.clone()));
            info.push(("External IP", session.metadata.external_ip.clone()));
            info.push(("Agent Version", session.metadata.agent_version.clone()));
            info.push((
                "Privilege Level",
                format!("{:?}", session.metadata.privilege_level),
            ));
            info.push(("Established", format_system_time(session.established_at)));
            info.push(("Last Check-in", format_system_time(session.last_check_in)));
            info.push(("Status", format!("{:?}", session.status)));

            let mut text = String::new();
            for (key, value) in info {
                text.push_str(&format!("{}: {}\n", key, value));
            }

            Ok(CommandOutput::Text(text))
        },
    )
}

// Shell command - execute shell command on agent
// fn shell_command() -> Command {
//     Command::new(
//         "shell",
//         "Execute a shell command on the agent",
//         "shell <command>",
//         vec!["shell whoami".to_string(), "shell ls -la".to_string()],
//         true, // Requires a session
//         |session, args| {
//             let session = session.unwrap(); // Safe because requires_session is true
//
//             if args.is_empty() {
//                 return Err(CommandError::InvalidArguments("Command required".into()));
//             }
//
//             let command = args.join(" ");
//
//             // In a real implementation, this would send the command to the agent
//             // and wait for a response
//
//             Ok(CommandOutput::Text(format!(
//                 "Executing on {}: {}\n\n[Command output would appear here]",
//                 session.display_name(),
//                 command
//             )))
//         },
//     )
// }

// Kill command - terminate the session/agent
// fn kill_command() -> Command {
//     Command::new(
//         "kill",
//         "Terminate the current session",
//         "kill",
//         vec!["kill".to_string()],
//         true, // Requires a session
//         |session, _args| {
//             let session = session.unwrap(); // Safe because requires_session is true
//
//             // In a real implementation, this would send a termination command to the agent
//
//             Ok(CommandOutput::Text(format!(
//                 "Terminating session {}",
//                 session.display_name()
//             )))
//         },
//     )
// }

// Helper function to format durations in a human-readable way
fn format_duration(duration: Duration) -> String {
    if duration.as_secs() < 60 {
        return format!("{} secs ago", duration.as_secs());
    } else if duration.as_secs() < 3600 {
        return format!("{} mins ago", duration.as_secs() / 60);
    } else if duration.as_secs() < 86400 {
        return format!("{} hours ago", duration.as_secs() / 3600);
    } else {
        return format!("{} days ago", duration.as_secs() / 86400);
    }
}

// Helper function to format system time in a human-readable way
fn format_system_time(time: SystemTime) -> String {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();

            // Use chrono to format the time (in a real implementation)
            // For now, just return the timestamp
            format!("{} (unix timestamp)", secs)
        }
        Err(_) => "Invalid time".to_string(),
    }
}
