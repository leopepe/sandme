//! sandme — run a coding IDE or code agent inside a macOS Seatbelt sandbox,
//! with its network egress routed through a proxy sandme manages.

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

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = match config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("sandme: {error}");
            return ExitCode::FAILURE;
        }
    };

    // The proxy comes up first: a sandbox without its proxy is a broken
    // sandbox, so the invocation fails instead (FR-005).
    let server = match proxy::serve(config.proxy_port) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("sandme: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("sandme: proxy listening on {}", server.addr());
    println!("sandme: shared paths: {:?}", config.shared_paths);

    match sandbox::run(&config, server.addr(), &cli.command).await {
        Ok(status) => exit_code(status),
        Err(error) => {
            eprintln!("sandme: {error}");
            ExitCode::FAILURE
        }
    }
    // `server` is dropped here: the proxy's lifetime follows the command's (T-007).
}

/// Map the sandboxed command's status to sandme's exit code.
fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    if status.success() {
        return ExitCode::SUCCESS;
    }
    // FR-002: propagate the child's exit code. `ExitCode` holds a u8, so the
    // code is clamped into range; a child killed by a signal maps to 1.
    #[allow(
        clippy::cast_sign_loss,
        reason = "FR-002: clamped into ExitCode's u8 range"
    )]
    status.code().map_or(ExitCode::FAILURE, |code| {
        ExitCode::from(code.clamp(0, 255) as u8)
    })
}
