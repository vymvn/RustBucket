use crate::client::Client;
use crate::config::RbServerConfig;
use crate::listener;
use rb::session::Session;

use std::sync::{Arc, Mutex};

pub struct RbServer {
    config: RbServerConfig,
    clients: Arc<Mutex<Vec<Client>>>,
    listeners: Arc<Mutex<Vec<Box<dyn listener::Listener>>>>,
    sessions: Arc<Mutex<Vec<Session>>>,
    running: Arc<AtomicBool>,
}
