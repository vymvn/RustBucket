use clap::{Arg, Command};
use colored::*;
use reedline::{DefaultPrompt, Reedline, Signal};
use reedline::{Prompt, PromptEditMode, PromptHistorySearch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::io::{self, Read, Write};
use std::net::TcpStream;

use rb::command::{CommandError, CommandOutput};

// Represents the result from server
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ServerResponse {
    Success(CommandOutput),
    Error(CommandError),
}

// Display command output with nice formatting and colors
fn display_command_output(output: CommandOutput) {
    match output {
        CommandOutput::Text(text) => {
            // Text output
            println!("{}", text);
        }
        CommandOutput::Table { headers, rows } => {
            // Enhanced table display with colors
            println!();
            let header_line = headers
                .iter()
                .map(|h| h.bright_green().bold().to_string())
                .collect::<Vec<_>>()
                .join(" | ");

            println!("{}", header_line);
            println!("{}", "=".repeat(header_line.len()).dimmed());

            // Alternate row colors for better readability
            for (i, row) in rows.iter().enumerate() {
                let row_str = row.join(" | ");
                if i % 2 == 0 {
                    println!("{}", row_str.cyan());
                } else {
                    println!("{}", row_str.blue());
                }
            }

            println!();
        }
        CommandOutput::Json(value) => {
            // Pretty-print JSON with yellow color
            if let Ok(pretty) = serde_json::to_string_pretty(&value) {
                // Add syntax highlighting to JSON
                // This is a simple version - a real JSON highlighter would be more sophisticated
                let highlighted = pretty
                    .replace("{", "{".bright_yellow().to_string().as_str())
                    .replace("}", "}".bright_yellow().to_string().as_str())
                    .replace("[", "[".bright_yellow().to_string().as_str())
                    .replace("]", "]".bright_yellow().to_string().as_str())
                    .replace(":", ":".bright_yellow().to_string().as_str())
                    .replace(",", ",".bright_yellow().to_string().as_str());

                println!("{}", highlighted);
            } else {
                println!("{:?}", value.to_string().yellow());
            }
        }
        CommandOutput::Binary(data) => {
            println!("{}", "Binary data:".bright_green());

            // Display binary data as a hex dump with colors
            for (i, chunk) in data.chunks(16).enumerate() {
                // Print offset
                print!("{:08x}  ", i * 16);

                // Print hex values
                for (j, byte) in chunk.iter().enumerate() {
                    if j == 8 {
                        print!(" "); // Extra space in the middle
                    }
                    print!("{:02x} ", byte);
                }

                // Fill remaining space if chunk is not full
                for _ in chunk.len()..16 {
                    print!("   ");
                }

                // Extra space for alignment
                if chunk.len() <= 8 {
                    print!(" ");
                }

                // Print ASCII representation
                print!(" │");
                for &byte in chunk {
                    if byte >= 32 && byte <= 126 {
                        // Printable ASCII
                        print!("{}", (byte as char).to_string().blue());
                    } else {
                        // Non-printable
                        print!("{}", ".".dimmed());
                    }
                }
                println!("│");
            }
            println!("\n{} bytes", data.len().to_string().green());
        }
        CommandOutput::None => {
            println!(
                "{}",
                "Command executed successfully with no output.".bright_green()
            );
        }
    }
}

// Display command errors with appropriate colors
fn display_command_error(error: CommandError) {
    match error {
        CommandError::InvalidArguments(msg) => {
            eprintln!("{}: {}", "Invalid Arguments".bright_red().bold(), msg);
        }
        CommandError::PermissionDenied(msg) => {
            eprintln!("{}: {}", "Permission Denied".bright_red().bold(), msg);
        }
        CommandError::ExecutionFailed(msg) => {
            eprintln!("{}: {}", "Execution Failed".bright_red().bold(), msg);
        }
        CommandError::TargetNotFound(msg) => {
            eprintln!("{}: {}", "Target Not Found".bright_yellow().bold(), msg);
        }
        CommandError::NoActiveSession(msg) => {
            eprintln!("{}: {}", "No Active Session".bright_yellow().bold(), msg);
        }
        CommandError::SessionError(msg) => {
            eprintln!("{}: {}", "Session Error".bright_red().bold(), msg);
        }
        CommandError::Internal(msg) => {
            eprintln!("{}: {}", "Internal Error".bright_red().bold(), msg);
        }
    }
}

// Custom prompt implementation with the requested name
struct RustBucketPrompt;

impl Prompt for RustBucketPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Owned(format!("{} ", "RustBucket>".white().bold()))
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

fn main() -> io::Result<()> {
    // Define command line arguments using clap
    let matches = Command::new("RustBucket Client")
        .version("1.0")
        .author("RustBucket Developer")
        .about("Command and Control Client for RustBucket Server")
        .arg(
            Arg::new("host")
                .short('H')
                .long("host")
                .value_name("HOST")
                .help("C2 server hostname or IP address")
                .default_value("localhost"),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("C2 server port")
                .default_value("6666"),
        )
        .get_matches();

    // Get server address from command line arguments
    let host = matches.get_one::<String>("host").unwrap();
    let port = matches.get_one::<String>("port").unwrap();
    let server_address = format!("{}:{}", host, port);

    // Connect to the C2 server
    let mut stream = match TcpStream::connect(&server_address) {
        Ok(stream) => {
            println!(
                "{} {}",
                "Connected to RustBucket C2 server at".green(),
                server_address.bright_green()
            );
            stream
        }
        Err(e) => {
            eprintln!(
                "{} {}: {}",
                "Failed to connect to RustBucket C2 server at".bright_red(),
                server_address.bright_red(),
                e
            );
            return Err(e);
        }
    };

    // Set TCP_NODELAY to disable Nagle's algorithm
    stream.set_nodelay(true)?;

    // Create a new Reedline engine
    let mut line_editor = Reedline::create();
    let prompt = RustBucketPrompt;

    let banner = r"
     (                                             
 )\ )           )  (              )         )  
(()/(  (     ( /(( )\   (      ( /(   (  ( /(  
 /(_))))\ (  )\())((_) ))\  (  )\()) ))\ )\()) 
(_)) /((_))\(_))((_)_ /((_) )\((_)\ /((_|_))/  
| _ (_))(((_) |_ | _ |_))( ((_) |(_|_)) | |_   
|   / || (_-<  _|| _ \ || / _|| / // -_)|  _|  
|_|_\\_,_/__/\__||___/\_,_\__||_\_\\___| \__|  
                                               
";

    println!("{}", banner.bright_yellow().bold());

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                if buffer.trim().is_empty() {
                    continue;
                }

                // Send the command to the server with a newline terminator
                let command = format!("{}\n", buffer);
                stream.write_all(command.as_bytes())?;
                stream.flush()?;

                // Read the response from the server
                let mut response_data = Vec::new();
                let mut buffer = [0; 1024];

                // Read in a loop until we have a complete response
                loop {
                    match stream.read(&mut buffer) {
                        Ok(0) => {
                            eprintln!("{}", "Server closed the connection".bright_red());
                            return Ok(());
                        }
                        Ok(n) => {
                            response_data.extend_from_slice(&buffer[..n]);

                            // Try to parse what we have so far to see if it's complete
                            if let Ok(response) =
                                serde_json::from_slice::<ServerResponse>(&response_data)
                            {
                                // We have a complete response
                                match response {
                                    ServerResponse::Success(output) => {
                                        // Print a separator for clarity
                                        // println!("{}", "─".repeat(50).dimmed());
                                        display_command_output(output);
                                        // println!("{}", "─".repeat(50).dimmed());
                                    }
                                    ServerResponse::Error(error) => {
                                        // Print a separator for clarity
                                        // println!("{}", "─".repeat(50).dimmed());
                                        display_command_error(error);
                                        // println!("{}", "─".repeat(50).dimmed());
                                    }
                                }
                                break;
                            }

                            // If we couldn't parse yet, continue reading
                        }
                        Err(e) => {
                            eprintln!("{}: {}", "Failed to receive data".bright_red(), e);
                            return Err(e);
                        }
                    }
                }
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\n{}", "Disconnecting from server...".yellow());
                break Ok(());
            }
        }
    }
}
