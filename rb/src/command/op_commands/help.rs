use crate::command::types::{Command, CommandError, CommandResult, Context};
use crate::command::CommandRegistry;

pub struct HelpCommand {
    registry: Option<CommandRegistry>,
}

impl HelpCommand {
    pub fn new() -> Self {
        Self { registry: None }
    }

    pub fn with_registry(registry: CommandRegistry) -> Self {
        Self {
            registry: Some(registry),
        }
    }
}

impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "Display help information about available commands"
    }

    fn execute(&self, args: Vec<String>, _context: &mut dyn Context) -> CommandResult {
        match &self.registry {
            Some(registry) => {
                if args.is_empty() {
                    // List all commands
                    let commands = registry.list_commands();
                    let mut output = String::from("Available commands:\n");
                    for (name, desc) in commands {
                        output.push_str(&format!("  {}: {}\n", name, desc));
                    }
                    Ok(output)
                } else {
                    // Show help for specific command
                    match registry.get_help(&args[0]) {
                        Some(help) => Ok(help),
                        None => Err(CommandError::ExecutionFailure(format!(
                            "Command '{}' not found",
                            args[0]
                        ))),
                    }
                }
            }
            None => Err(CommandError::ExecutionFailure(
                "Help command not properly initialized with registry".to_string(),
            )),
        }
    }

    fn validate(&self, args: &[String]) -> Result<(), CommandError> {
        if args.len() > 1 {
            return Err(CommandError::InvalidArguments(
                "Help takes at most one argument".to_string(),
            ));
        }
        Ok(())
    }
}
