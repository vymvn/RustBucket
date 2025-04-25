mod agent;
mod certs;
mod config;
// mod context;
// mod handler;
mod server;
// mod server;

use tokio::signal;
use tokio::spawn;
use std::sync::Arc;


use std::net::{SocketAddr, IpAddr, Ipv4Addr};

use rb::listener::http_listener::HttpListener;

#[tokio::main]
async fn main() {
    // 1) Initialize logger
    simple_logger::SimpleLogger::new().env().init().unwrap();

    // 2) Build your mTLS config & server config
    let mtls_config = config::MtlsConfig::new(
        false,
        "certs/ca-cert.pem".to_string(),
        "certs/client-cert.pem".to_string(),
        "certs/client-key.pem".to_string(),
        "certs/crl.der".to_string(),
        5,
    );
    let conf = config::RbServerConfig::with_mtls(
        "0.0.0.0".to_string(),
        6666,
        false,
        mtls_config,
    );

    // 3) Instantiate your C2 server
    let server = server::RbServer::new(conf);

    // 4) Spin up the HTTP listener in the same process
    let http_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080);

    // now call HttpListener::new(name: &str, addr: SocketAddr, session_mgr)
    let mut http_listener = HttpListener::new(
        "http_listener",        // an arbitrary name
        http_addr,              // correctly-typed SocketAddr
        server.session_manager(), // your shared SessionManager
    );

    // spawn it in the background
    spawn(async move {
        if let Err(e) = http_listener.start().await {
            log::error!("HTTP listener failed: {}", e);
        }
    });
    log::info!("HTTP listener running on port {}", http_addr.port());

    // 5) Start your TCP C2 listener (this blocks until shutdown)
    server
        .start()
        .await
        .expect("C2 server crashed");

    // 6) Graceful shutdown on Ctrl+C
    signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    log::info!("Shutting down C2 server...");
    server.stop().await.expect("Failed to stop server");
    log::info!("All services stopped");
}

