use reedline::{DefaultCompleter, Prompt, PromptEditMode, PromptHistorySearch, Reedline, Signal};
use nu_ansi_term::Color;
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpStream, SocketAddr};
use std::str;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub struct RustBucketPrompt {
    pub context: PromptContext,
}

pub enum PromptContext {
    Server,
    Agent {
        hostname: String,
    },
    Connected {
        hostname: String,
        connected_to: String,
    },
}

impl Prompt for RustBucketPrompt {
    fn render_prompt_left(&self) -> Cow<str> {
        match &self.context {
            PromptContext::Server => {
                let label = Color::Cyan.paint("[Server]");
                let app = Color::Green.paint("RustBucket> ");
                Cow::Owned(format!("{} {}", label, app))
            }
            PromptContext::Agent { hostname } => {
                let label = Color::Yellow.paint(format!("[{}]", hostname));
                let app = Color::Green.paint("RustBucket> ");
                Cow::Owned(format!("{} {}", label, app))
            }
            PromptContext::Connected { hostname, connected_to } => {
                let label = Color::Yellow.paint(format!("[{}]", hostname));
                let connection = Color::Red.paint(format!("({})", connected_to));
                let app = Color::Green.paint("RustBucket> ");
                Cow::Owned(format!("{} {} {}", label, connection, app))
            }
        }
    }

    fn render_prompt_right(&self) -> Cow<str> {
        "".into()
    }

    fn render_prompt_indicator(&self, _mode: PromptEditMode) -> Cow<str> {
        "".into()
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<str> {
        "... ".into()
    }

    fn render_prompt_history_search_indicator(
        &self,
        _history_search: PromptHistorySearch,
    ) -> Cow<str> {
        ": ".into()
    }
}

struct TcpClientState {
    stream: Option<TcpStream>,
    buffer: Vec<u8>,
}

impl TcpClientState {
    fn new() -> Self {
        Self {
            stream: None,
            buffer: Vec::with_capacity(4096),
        }
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn connect(&mut self, addr: &str) -> io::Result<()> {
        if self.is_connected() {
            self.disconnect()?;
        }

        let stream = TcpStream::connect(addr)?;
        stream.set_nonblocking(true)?;
        self.stream = Some(stream);
        Ok(())
    }

    fn disconnect(&mut self) -> io::Result<()> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> io::Result<usize> {
        if let Some(stream) = &mut self.stream {
            stream.write(data)
        } else {
            Err(io::Error::new(io::ErrorKind::NotConnected, "Not connected to server"))
        }
    }

    fn receive(&mut self) -> io::Result<Option<Vec<u8>>> {
        if let Some(stream) = &mut self.stream {
            let mut buf = [0; 1024];
            match stream.read(&mut buf) {
                Ok(0) => {
                    // Connection closed by server
                    self.disconnect()?;
                    Ok(None)
                }
                Ok(n) => {
                    self.buffer.extend_from_slice(&buf[0..n]);
                    let result = self.buffer.clone();
                    self.buffer.clear();
                    Ok(Some(result))
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data available right now
                    Ok(None)
                }
                Err(e) => Err(e),
            }
        } else {
            Err(io::Error::new(io::ErrorKind::NotConnected, "Not connected to server"))
        }
    }
}

fn main() {
    // Extended command list for tab-completion
    let commands = vec![
        "use agent", "use server", "exit", "help", "list", "download", "upload",
        "connect", "disconnect", "send", "recv", "status", "clear",
    ];

    let completer = Box::new(DefaultCompleter::new(commands.iter().map(|s| s.to_string()).collect()));

    // Initialize REPL editor
    let mut line_editor = Reedline::create().with_completer(completer);

    // Start in server context
    let mut prompt = RustBucketPrompt {
        context: PromptContext::Server,
    };

    // TCP client state
    let tcp_client = Arc::new(Mutex::new(TcpClientState::new()));

    // Create a background thread for polling data from the TCP server
    let tcp_client_clone = Arc::clone(&tcp_client);
    let receiver = thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(100));
            
            let mut client = tcp_client_clone.lock().unwrap();
            if client.is_connected() {
                match client.receive() {
                    Ok(Some(data)) => {
                        if !data.is_empty() {
                            match str::from_utf8(&data) {
                                Ok(text) => println!("\nReceived: {}", text),
                                Err(_) => {
                                    println!("\nReceived binary data: {:?}", data);
                                }
                            }
                        }
                    }
                    Ok(None) => {}, // No data available
                    Err(e) => {
                        if e.kind() != io::ErrorKind::NotConnected {
                            eprintln!("\nError receiving data: {}", e);
                        }
                    }
                }
            }
        }
    });

    // Display welcome message
    println!("RustBucket Terminal v0.2.0");
    println!("Type 'help' for available commands");

    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(input)) => {
                let input = input.trim();
                
                if input.is_empty() {
                    continue;
                }
                
                // Parse command and arguments
                let parts: Vec<&str> = input.splitn(2, ' ').collect();
                let command = parts[0];
                let args = parts.get(1).unwrap_or(&"");

                match command {
                    "exit" => {
                        println!("Exiting...");
                        break;
                    }
                    "use" if *args == "agent" => {
                        println!("Switched to agent context.");
                        match prompt.context {
                            PromptContext::Connected { hostname, connected_to } => {
                                prompt.context = PromptContext::Connected {
                                    hostname: "dev-pc".to_string(),
                                    connected_to,
                                };
                            }
                            _ => {
                                prompt.context = PromptContext::Agent {
                                    hostname: "dev-pc".to_string(),
                                };
                            }
                        }
                    }
                    "use" if *args == "server" => {
                        println!("Switched to server context.");
                        match prompt.context {
                            PromptContext::Connected { hostname: _, connected_to } => {
                                prompt.context = PromptContext::Connected {
                                    hostname: "server".to_string(),
                                    connected_to,
                                };
                            }
                            _ => {
                                prompt.context = PromptContext::Server;
                            }
                        }
                    }
                    "connect" => {
                        let addr = if args.is_empty() { "localhost:6666" } else { args };
                        println!("Connecting to {}...", addr);
                        
                        let mut client = tcp_client.lock().unwrap();
                        match client.connect(addr) {
                            Ok(_) => {
                                println!("Successfully connected to {}", addr);
                                // Update prompt context to show connection status
                                match &prompt.context {
                                    PromptContext::Server => {
                                        prompt.context = PromptContext::Connected {
                                            hostname: "server".to_string(),
                                            connected_to: addr.to_string(),
                                        };
                                    }
                                    PromptContext::Agent { hostname } => {
                                        prompt.context = PromptContext::Connected {
                                            hostname: hostname.clone(),
                                            connected_to: addr.to_string(),
                                        };
                                    }
                                    PromptContext::Connected { hostname, connected_to: _ } => {
                                        prompt.context = PromptContext::Connected {
                                            hostname: hostname.clone(),
                                            connected_to: addr.to_string(),
                                        };
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to connect: {}", e);
                            }
                        }
                    }
                    "disconnect" => {
                        let mut client = tcp_client.lock().unwrap();
                        if client.is_connected() {
                            match client.disconnect() {
                                Ok(_) => {
                                    println!("Disconnected from server");
                                    // Update prompt context to remove connection status
                                    match &prompt.context {
                                        PromptContext::Connected { hostname, connected_to: _ } => {
                                            if hostname == "server" {
                                                prompt.context = PromptContext::Server;
                                            } else {
                                                prompt.context = PromptContext::Agent {
                                                    hostname: hostname.clone(),
                                                };
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error disconnecting: {}", e);
                                }
                            }
                        } else {
                            println!("Not currently connected to any server");
                        }
                    }
                    "send" => {
                        if args.is_empty() {
                            println!("Usage: send <message>");
                            continue;
                        }
                        
                        let mut client = tcp_client.lock().unwrap();
                        if client.is_connected() {
                            match client.send(args.as_bytes()) {
                                Ok(n) => {
                                    println!("Sent {} bytes", n);
                                }
                                Err(e) => {
                                    eprintln!("Failed to send data: {}", e);
                                }
                            }
                        } else {
                            println!("Not connected to any server. Use 'connect' first.");
                        }
                    }
                    "status" => {
                        let client = tcp_client.lock().unwrap();
                        if client.is_connected() {
                            match &prompt.context {
                                PromptContext::Connected { hostname: _, connected_to } => {
                                    println!("Connected to {}", connected_to);
                                }
                                _ => {
                                    println!("Connected to server");
                                }
                            }
                        } else {
                            println!("Not connected to any server");
                        }
                    }
                    "clear" => {
                        // Clear screen with ANSI escape code
                        print!("\x1B[2J\x1B[1;1H");
                    }
                    "help" => {
                        println!("Available commands:");
                        println!("  connect [addr]   - Connect to TCP server (default: localhost:6666)");
                        println!("  disconnect       - Disconnect from server");
                        println!("  send <message>   - Send data to connected server");
                        println!("  status           - Show connection status");
                        println!("  use agent        - Switch to agent context");
                        println!("  use server       - Switch to server context");
                        println!("  clear            - Clear screen");
                        println!("  help             - Display this help message");
                        println!("  exit             - Exit the application");
                    }
                    other => {
                        println!("Unknown command: {}. Type 'help' for available commands.", other);
                    }
                }
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                println!("\nCaught exit signal. Goodbye!");
                break;
            }
            Err(err) => {
                eprintln!("Error: {}", err);
            }
        }
    }

    // Clean up TCP connection before exiting
    let mut client = tcp_client.lock().unwrap();
    if client.is_connected() {
        let _ = client.disconnect();
    }
    
    // We're intentionally not joining the receiver thread here since we want to exit immediately
    // In a production app, you might want to use a channel to signal thread termination
}
