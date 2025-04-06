use crate::context::ServerContext;
use rb::command::CommandRegistry;
use rb::message::{CommandRequest, CommandResponse, ResponseStatus};
use std::sync::{Arc, Mutex};

pub struct CommandHandler {
    registry: CommandRegistry,
    context: Arc<Mutex<ServerContext>>,
}

impl CommandHandler {
    // pub fn new(registry: CommandRegistry, context: Arc<Mutex<ServerContext>>) -> Self {
    pub fn new(registry: CommandRegistry) -> Self {
        Self { registry }
    }

    pub fn handle_request(&self, request: CommandRequest) -> CommandResponse {
        let CommandRequest { command, args, id } = request;

        // Lock context for the duration of command execution
        // let mut context_guard = match self.context.lock() {
        //     Ok(guard) => guard,
        //     Err(_) => {
        //         return CommandResponse {
        //             id,
        //             status: ResponseStatus::Error,
        //             result: None,
        //             error: Some("Failed to acquire context lock".to_string()),
        //         }
        //     }
        // };

        // Execute the command
        // match self.registry.execute(&command, args, &mut *context_guard) {
        match self.registry.execute(&command, args) {
            Ok(result) => CommandResponse {
                id,
                status: ResponseStatus::Success,
                result: Some(result),
                error: None,
            },
            Err(err) => CommandResponse {
                id,
                status: ResponseStatus::Error,
                result: None,
                error: Some(format!("{}", err)),
            },
        }
    }
}
