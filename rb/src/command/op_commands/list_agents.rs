pub struct ListAgentsCommand;

impl Command for ListAgentsCommand {
    fn name(&self) -> &str {
        "agents"
    }
    
    fn description(&self) -> &str {
        "List all connected agents"
    }
    
    fn execute(&self, _args: Vec<String>, context: &mut ServerContext) -> CommandResult {
        if context.connected_agents.is_empty() {
            return Ok("No agents currently connected.".to_string());
        }
        
        let mut response = String::from("Connected agents:\n");
        for (i, agent) in context.connected_agents.iter().enumerate() {
            response.push_str(&format!("{}. {}\n", i + 1, agent));
        }
        
        Ok(response)
    }
