// TODO: ls implant command

use crate::command::*;
use crate::message::*;

use clap;

pub struct ImplantLsCommand {}

impl RbCommand for ImplantLsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Implant
    }

    fn description(&self) -> &'static str {
        "Lists files and directories"
    }

    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
        // Create the command with a String, not a reference
        let cmd = clap::Command::new(self.name())
            .about(self.description().to_string())
            .arg(
                clap::Arg::new("path")
                    .help("Path to list")
                    .required(false)
                    .default_value("."),
            )
            .arg(
                clap::Arg::new("recursive")
                    .short('r')
                    .long("recursive")
                    .help("List directories recursively"),
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
        todo!();
    }
}
