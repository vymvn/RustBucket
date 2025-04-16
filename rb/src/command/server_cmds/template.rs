use crate::command::*;
use crate::message::*;

use clap;

pub struct ServerHelpCommand {}

impl RbCommand for ServerHelpCommand {
    fn name(&self) -> &str {
        todo!()
    }

    fn command_type(&self) -> CommandType {
        todo!()
    }

    fn description(&self) -> &str {
        todo!()
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
