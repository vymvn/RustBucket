use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::SystemTime;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Session status enum to track the connection state
#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Connected,
    Disconnected,
    Idle,
}

/// Represents a connection session with a client
pub struct Session {
    id: Uuid,
    target: SocketAddr,
    created_at: SystemTime,
    last_active: SystemTime,
    connection_type: String,
    metadata: HashMap<String, String>,
    status: SessionStatus,
}

impl Session {
    pub fn new(target: SocketAddr, connection_type: String) -> Self {
        let now = SystemTime::now();
        Session {
            id: Uuid::new_v4(),
            target,
            created_at: now,
            last_active: now,
            connection_type,
            metadata: HashMap::new(),
            status: SessionStatus::Connected,
        }
    }
    
    pub fn id(&self) -> Uuid {
        self.id
    }
    
    pub fn target_addr(&self) -> SocketAddr {
        self.target
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
    
    pub fn status(&self) -> &SessionStatus {
        &self.status
    }
    
    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
    }
}
