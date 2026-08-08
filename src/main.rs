//! sandme — run a coding IDE or code agent inside a macOS Seatbelt sandbox,
//! with its network egress routed through a proxy sandme manages.

mod config;
mod error;
mod proxy;
mod sandbox;

use std::os::unix::process::ExitStatusExt;
use std::process::ExitCode;

use clap::Parser;

/// sandme — run an IDE or code agent inside a macOS Seatbelt sandbox
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Command to run sandboxed, with its arguments
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
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
///
/// Follows the exec-wrapper convention (docs/guidelines/architecture/posix.md
/// §4): the child's status is propagated unchanged, and termination by a
/// signal becomes `128+n`.
fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    if status.success() {
        return ExitCode::SUCCESS;
    }
    if let Some(signal) = status.signal() {
        return ExitCode::from(u8::try_from(128 + signal).unwrap_or(u8::MAX));
    }
    status.code().map_or(ExitCode::FAILURE, |code| {
        ExitCode::from(u8::try_from(code).unwrap_or(u8::MAX))
    })
}
