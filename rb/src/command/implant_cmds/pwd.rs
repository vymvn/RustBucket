use crate::command::*;
use crate::message::*;

use clap;

pub struct ImplantPwdCommand {}

impl RbCommand for ImplantPwdCommand {
    fn name(&self) -> &'static str {
        "pwd"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Implant
    }

    fn description(&self) -> &'static str {
        "Displays the current working directory"
    }

    fn parse_args(&self, command_line: &str) -> Result<Box<dyn Any>, clap::Error> {
        todo!()
    }

    fn execute_with_parsed_args(
        &self,
        context: &mut CommandContext,
        args: Box<dyn Any>,
    ) -> CommandResult {
        todo!()
    }
}
