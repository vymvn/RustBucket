use crate::command::types::{Command, CommandError, CommandResult, Context};

pub struct PingCommand;

impl Command for PingCommand {
    fn name(&self) -> &str {
        "ping"
    }

    fn description(&self) -> &str {
        "Check if server is responsive"
    }

    fn execute(&self, _args: Vec<String>, _context: &mut dyn Context) -> CommandResult {
        Ok("Pong!".to_string())
    }
}
