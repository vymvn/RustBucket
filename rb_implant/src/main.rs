#[tokio::main]
async fn main() {
    if let Err(e) = rb_implant::run_implant().await {
        eprintln!("Fatal error: {}", e);
        std::process::exit(1);
    }
}

