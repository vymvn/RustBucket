use crate::command::*;

use clap;

pub struct ImplantSysteminfoCommand {}

impl RbCommand for ImplantSysteminfoCommand {
    fn name(&self) -> &'static str {
        "systeminfo"
    }

    fn command_type(&self) -> CommandType {
        CommandType::Implant
    }

    fn description(&self) -> &'static str {
        "Displays system information"
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
