pub mod http_listener;
use std::net::SocketAddr;
use std::sync::{Arc, atomic::AtomicBool};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;
mod http_listener;


/// Trait defining the common interface for all listener types
pub trait Listener {
    /// Returns the unique identifier for this listener
    fn id(&self) -> Uuid;
    
    /// Returns the name of this listener
    fn name(&self) -> &str;
    
    /// Returns the socket address this listener is bound to
    fn addr(&self) -> SocketAddr;
    
    /// Checks if the listener is currently running
    fn is_running(&self) -> bool;
    
    /// Starts the listener
    fn start(&mut self) -> Result<(), String>;
    
    /// Stops the listener
    fn stop(&mut self) -> Result<(), String>;
    
    /// Accepts a new connection and returns a Session
    /// This would typically be implemented as an async method in a real application
    fn accept(&mut self) -> Result<Session, String>;
}
