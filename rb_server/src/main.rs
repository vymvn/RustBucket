mod agent;
mod certs;
mod config;
// mod context;
// mod handler;
mod server;
// mod server;

use tokio::signal;

#[tokio::main]
async fn main() {
    // Initialize the logger
    simple_logger::SimpleLogger::new().env().init().unwrap();

    // Create mTLS configuration
    let mtls_config = config::MtlsConfig::new(
        false, // Enable mTLS
        "certs/ca-cert.pem".to_string(),
        "certs/client-cert.pem".to_string(),
        "certs/client-key.pem".to_string(),
        "certs/crl.der".to_string(),
        5, // CRL update interval in seconds
    );

    // Create server with mTLS enabled
    let conf = config::RbServerConfig::with_mtls("localhost".to_string(), 6666, false, mtls_config);
    // let conf = config::RbServerConfig::new("localhost".to_string(), 6666, false);
    let c2 = server::RbServer::new(conf);

    // Start C2 server
    match c2.start().await {
        Ok(_) => {
            log::info!("C2 server started successfully");
        }
        Err(err) => {
            log::error!("Error starting C2 server: {}", err);
            return;
        }
    }

    // Wait for Ctrl+C
    match signal::ctrl_c().await {
        Ok(()) => {
            log::info!("\nReceived Ctrl+C, shutting down gracefully...");
        }
        Err(err) => {
            log::error!("Error setting up Ctrl+C handler: {}", err);
        }
    }

    // Stop C2 server
    match c2.stop().await {
        Ok(_) => {
            log::info!("C2 server stopped successfully");
        }
        Err(err) => {
            log::error!("Error stopping C2 server: {}", err);
        }
    }

    log::info!("All services stopped successfully");
}
