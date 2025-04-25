use std::net::SocketAddr;
use uuid::Uuid;

use crate::message::*;

pub mod http_listener;

/// Trait defining the interface for all listener types
pub trait Listener: Send + Sync {
    /// Get the name of this listener
    fn name(&self) -> &str;
    
    /// Get the unique ID of this listener
    fn id(&self) -> Uuid;
    
    /// Get the socket address this listener is bound to
    fn addr(&self) -> SocketAddr;
    
    /// Start the listener
    async fn start(&mut self) -> Result<(), String>;
    
    /// Stop the listener
    async fn stop(&mut self) -> Result<(), String>;
    
    /// Check if the listener is currently running
    fn is_running(&self) -> bool;
    
    /// Get the count of implants connected to this listener
    fn implant_count(&self) -> usize;
    
    /// Get information about all implants connected to this listener
    fn get_implants(&self) -> Vec<ImplantInfo>;
    
    /// Type of the listener for display purposes
    fn listener_type(&self) -> &str;
}

