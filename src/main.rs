mod config;
mod error;
mod proxy;
mod sandbox;

use std::process::ExitCode;

use clap::Parser;

/// sandme — run an IDE or code agent inside a macOS Seatbelt sandbox
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The command to run sandboxed (e.g. "zed ~/Workspace/")
    command: String,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Load configuration
    let config = match config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sandme: config error: {e}");
            return ExitCode::FAILURE;
        }
    };

    println!("sandme: running sandboxed command: {}", cli.command);
    println!("sandme: shared paths: {:?}", config.shared_paths);
    println!("sandme: proxy port: {}", config.proxy_port);

    // Run the command under the sandbox
    match sandbox::run(&config, &cli.command) {
        Ok(status) => {
            if status.success() {
                ExitCode::SUCCESS
            } else {
                // ExitCode only accepts u8; clamp i32 to valid range
                status
                    .code()
                    .map(|c| ExitCode::from(c.clamp(0, 255) as u8))
                    .unwrap_or(ExitCode::FAILURE)
            }
        }
        Err(e) => {
            eprintln!("sandme: {e}");
            ExitCode::FAILURE
        }
    }
}
