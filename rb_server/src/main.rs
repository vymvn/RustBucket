mod client;
mod config;
mod listener;
mod server;

use std::sync::Arc;
use tokio::signal;

#[actix_web::main]
async fn main() {
    // Initialize the logger
    simple_logger::SimpleLogger::new().env().init().unwrap();

    // Create server
    let conf = config::RbServerConfig::new("localhost".to_string(), 6666, false);
    let mut c2 = server::RbServer::new(conf);

    // Start C2 server
    c2.start();

    // Wait for Ctrl+C
    match signal::ctrl_c().await {
        Ok(()) => {
            println!("\nReceived Ctrl+C, shutting down gracefully...");
        }
        Err(err) => {
            eprintln!("Error setting up Ctrl+C handler: {}", err);
        }
    }

    // Stop C2 server
    c2.stop();

    log::info!("All services stopped successfully");
}
