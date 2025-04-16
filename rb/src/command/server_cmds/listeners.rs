use crate::command::*;
use crate::message::*;

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
    fn name(&self) -> &str {
        "listeners"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Server
    }

    fn description(&self) -> &str {
        "Manage C2 listeners"
    }

    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
        // Yes I am hardcoding the name and description because I had weird ownership issues
        // Problem??
        let cmd = clap::Command::new("listeners")
            .about("Manage RustBucket C2 listeners")
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
        let args_str = command_line
            .trim_start()
            .strip_prefix(self.name())
            .unwrap_or("")
            .trim_start();

        // Parse the arguments
        let matches = cmd.try_get_matches_from(
            std::iter::once(self.name()).chain(args_str.split_whitespace()),
        )?;

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
                log::debug!("{:?}", args);
            }
            Err(_) => todo!(),
        };

        Ok(CommandOutput::Text("Listener command executed".to_string()))
    }
}
