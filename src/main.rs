//! sandme — run a coding IDE or code agent inside a macOS Seatbelt sandbox,
//! with its network egress routed through a proxy sandme manages.

mod app_bundle;
mod config;
mod error;
mod proxy;
mod sandbox;

use std::os::unix::process::ExitStatusExt;
use std::process::{ExitCode, ExitStatus};

use clap::Parser;

use crate::error::SandmeError;

/// The status reserved for a failure of sandme's own (FR-201).
///
/// `env(1)` and `timeout(1)` reserve the same one, for the same reason: `1` is
/// what `wc` returns for a missing file and `grep` for no match, so a wrapper
/// that fails on its own account and exits `1` is indistinguishable from the
/// command it was asked to run.
const SANDME_FAILURE: u8 = 125;

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

    match run(&cli.command).await {
        Ok(status) => exit_code(status),
        Err(error) => {
            // The prefix is what tells a caller the line is sandme's and not
            // the command's (posix.md §3), and with a reserved status it is
            // the only channel that says so unambiguously (FR-202).
            eprintln!("sandme: {error}");
            failure_code(&error)
        }
    }
}

/// Bring up the proxy and run `command` under the sandbox behind it.
async fn run(command: &[String]) -> Result<ExitStatus, SandmeError> {
    let config = config::load()?;

    // The proxy comes up first: a sandbox without its proxy is a broken
    // sandbox, so the invocation fails instead (FR-005).
    let server = proxy::serve(config.proxy_port)?;

    sandbox::run(&config, server.addr(), command).await
    // `server` is dropped here: the proxy's lifetime follows the command's (T-007).
}

/// Map the sandboxed command's status to sandme's exit code.
///
/// Follows the exec-wrapper convention (docs/guidelines/architecture/posix.md
/// §4): the child's status is propagated unchanged, and termination by a
/// signal becomes `128+n`. A command that chose `125` for itself keeps it —
/// reserving a status binds sandme, not the child (FR-205).
fn exit_code(status: ExitStatus) -> ExitCode {
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

/// Map a failure to the status reserved for it (SPEC-0004).
///
/// The arms are written out rather than defaulted, so that a new error variant
/// has to state which status it means instead of silently inheriting one.
fn failure_code(error: &SandmeError) -> ExitCode {
    match error {
        SandmeError::ConfigRead { .. }
        | SandmeError::ConfigParse { .. }
        | SandmeError::Execute(_)
        | SandmeError::ProxyStartup { .. } => ExitCode::from(SANDME_FAILURE),
    }
}
