use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub enum CommandError {
    InvalidArguments(String),
    ExecutionFailure(String),
    NotImplemented,
    NetworkError(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CommandError::InvalidArguments(msg) => write!(f, "Invalid arguments: {}", msg),
            CommandError::ExecutionFailure(msg) => write!(f, "Execution failed: {}", msg),
            CommandError::NotImplemented => write!(f, "Command not implemented"),
            CommandError::NetworkError(msg) => write!(f, "Network error: {}", msg),
        }
    }
}

pub type CommandResult = Result<String, CommandError>;

// Generic context trait that both server and client contexts can implement
pub trait Context {}

// Command trait defining the interface for all commands
pub trait Command {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn execute(&self, args: Vec<String>, context: &mut dyn Context) -> CommandResult;
    // fn execute(&self, args: Vec<String>) -> CommandResult;

    fn help(&self) -> String {
        format!("{} - {}", self.name(), self.description())
    }

    fn validate(&self, args: &[String]) -> Result<(), CommandError> {
        Ok(())
    }
}
