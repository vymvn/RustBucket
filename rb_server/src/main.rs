mod agent;
mod config;
// mod context;
// mod handler;
mod listener;
mod new_server;
// mod server;

use tokio::signal;

#[tokio::main]
async fn main() {
    // Initialize the logger
    simple_logger::SimpleLogger::new().env().init().unwrap();

    // Create server
    let conf = config::RbServerConfig::new("localhost".to_string(), 6666, false);
    let c2 = new_server::RbServer::new(conf);

    // Start C2 server
    c2.start().await.expect("mrrp");

    // Wait for Ctrl+C
    match signal::ctrl_c().await {
        Ok(()) => {
            log::info!("\nReceived Ctrl+C, shutting down gracefully...");
        }
        Err(err) => {
            eprintln!("Error setting up Ctrl+C handler: {}", err);
        }
    }

    // Stop C2 server
    c2.stop().await.expect("meow");

    log::info!("All services stopped successfully");
}
