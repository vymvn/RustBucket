mod config;
mod server;

fn main() {
    let conf = config::RbServerConfig::new("localhost".to_string(), 6666, true);
    let mut c2 = server::RbServer::new(conf);
    c2.start();
}
