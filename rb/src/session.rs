use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicBool};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;
use std::time::SystemTime;


/// Represents a connection session with a client
pub struct Session {
    id: Uuid,
    target: SocketAddr,
    last_active: SystemTime,
    status: SessionStatus,
}

impl Session {
    pub fn new(client_addr: SocketAddr, connection_type: String) -> Self {
        let now = SystemTime::now();
        Session {
            id: Uuid::new_v4(),
            client_addr,
            created_at: now,
            last_active: now,
            metadata: std::collections::HashMap::new(),
            connection_type,
        }
    }
    
    pub fn id(&self) -> Uuid {
        self.id
    }
    
    pub fn client_addr(&self) -> SocketAddr {
        self.client_addr
    }
    
    pub fn created_at(&self) -> SystemTime {
        self.created_at
    }
    
    pub fn last_active(&self) -> SystemTime {
        self.last_active
    }
    
    pub fn update_last_active(&mut self) {
        self.last_active = SystemTime::now();
    }
    
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
    
    pub fn get_metadata(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }
    
    pub fn connection_type(&self) -> &str {
        &self.connection_type
    }
}
