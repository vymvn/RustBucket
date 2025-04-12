use rb::command::types::Context;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::agent::Agent;
use crate::client::Client;
use crate::listener::Listener;

pub struct ServerContext {
    pub connected_clients: Arc<std::sync::Mutex<Vec<Client>>>,
    pub connected_agents: Arc<std::sync::Mutex<Vec<Agent>>>,
    pub listeners: Arc<std::sync::Mutex<Vec<Box<dyn Listener>>>>,
    pub running: Arc<AtomicBool>,
}

impl Context for ServerContext {}
