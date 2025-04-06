use super::types::{Command, CommandError, CommandResult, Context};
use std::collections::HashMap;

pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        CommandRegistry {
            commands: HashMap::new(),
        }
    }

    pub fn register<C: Command + 'static>(&mut self, command: C) {
        self.commands
            .insert(command.name().to_string(), Box::new(command));
    }

    pub fn execute(
        &self,
        name: &str,
        args: Vec<String>,
        context: &mut dyn Context,
    ) -> CommandResult {
        match self.commands.get(name) {
            Some(cmd) => {
                cmd.validate(&args)?;
                cmd.execute(args, context)
            }
            None => Err(CommandError::ExecutionFailure(format!(
                "Command '{}' not found",
                name
            ))),
        }
    }

    pub fn list_commands(&self) -> Vec<(&str, &str)> {
        self.commands
            .iter()
            .map(|(_, cmd)| (cmd.name(), cmd.description()))
            .collect()
    }

    pub fn get_help(&self, name: &str) -> Option<String> {
        self.commands.get(name).map(|cmd| cmd.help())
    }
}
