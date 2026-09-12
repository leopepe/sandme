//! sandme — run a coding IDE or code agent inside a macOS Seatbelt sandbox,
//! with its network egress routed through a proxy sandme manages.

mod app_bundle;
mod config;
mod credential;
mod egress;
mod error;
mod executable;
mod profile;
mod proxy;
mod sandbox;
mod tunnel;

use std::os::unix::process::ExitStatusExt;
use std::process::{ExitCode, ExitStatus};

use clap::Parser;

use crate::error::SandmeError;

/// The status reserved for a failure of sandme's own (FR-801).
///
/// `env(1)` and `timeout(1)` reserve the same one, for the same reason: `1` is
/// what `wc` returns for a missing file and `grep` for no match, so a wrapper
/// that fails on its own account and exits `1` is indistinguishable from the
/// command it was asked to run.
const SANDME_FAILURE: u8 = 125;

/// The status for a command that was found but could not be executed (FR-804).
const NOT_EXECUTABLE: u8 = 126;

/// The status for a command that could not be found (FR-803).
const NOT_FOUND: u8 = 127;

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
    // ssh's `ProxyCommand` re-executes sandme with this sentinel to open the
    // SSH tunnel (issue #33). It is intercepted before the CLI is parsed
    // because it is sandme's own private convention, not a documented flag, and
    // it must not bring up a second proxy or sandbox — it runs *inside* one.
    let raw: Vec<String> = std::env::args().collect();
    if let [_, sentinel, host, port] = raw.as_slice()
        && sentinel == tunnel::PROXY_COMMAND_ARG
    {
        return match tunnel::run(host, port).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("sandme: {error}");
                ExitCode::from(SANDME_FAILURE)
            }
        };
    }

    let cli = Cli::parse();

    match run(&cli.command).await {
        Ok(status) => exit_code(status),
        Err(error) => {
            // The prefix is what tells a caller the line is sandme's and not
            // the command's (posix.md §3), and with a reserved status it is
            // the only channel that says so unambiguously (FR-802).
            eprintln!("sandme: {error}");
            failure_code(&error)
        }
    }
}

/// Bring up the proxy and run `command` under the sandbox behind it.
async fn run(command: &[String]) -> Result<ExitStatus, SandmeError> {
    let loaded = config::load()?;

    // A widening setting sourced from the environment can have been planted by a
    // previous sandboxed command's shell rc, so it is called out on stderr —
    // never stdout, which carries only the child's output (posix.md §3, issue
    // #30). The run still proceeds: the warning informs, it does not block.
    for warning in &loaded.warnings {
        eprintln!("sandme: {warning}");
    }
    let config = loaded.config;

    // The proxy comes up first: a sandbox without its proxy is a broken
    // sandbox, so the invocation fails instead (FR-005).
    // `serve` takes the whole config now: the port, and the credential the
    // child must present (SPEC-0003/FR-302).
    let server = proxy::serve(&config)?;

    // The profile is built here rather than inside `sandbox::run`, so that
    // running a command under a profile does not require holding the config
    // that produced it.
    let profile = profile::generate_profile(&config, server.addr())?;

    // The server, not just its address: the child's proxy URL carries the
    // credential, so `run` needs both.
    sandbox::run(&profile, &server, command).await
    // `server` is dropped here: the proxy's lifetime follows the command's (T-007).
}

/// Map the sandboxed command's status to sandme's exit code.
///
/// Follows the exec-wrapper convention (docs/guidelines/architecture/posix.md
/// §4): the child's status is propagated unchanged, and termination by a
/// signal becomes `128+n`. A command that chose `125`, `126` or `127` for
/// itself keeps it — reserving a status binds sandme, not the child (FR-805).
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
        SandmeError::CommandNotFound { .. } => ExitCode::from(NOT_FOUND),
        SandmeError::CommandNotExecutable { .. } => ExitCode::from(NOT_EXECUTABLE),
        SandmeError::ConfigRead { .. }
        | SandmeError::ConfigParse { .. }
        | SandmeError::Execute(_)
        | SandmeError::ProxyStartup { .. }
        | SandmeError::TunnelProxyUnset
        | SandmeError::TunnelUnreachable { .. }
        | SandmeError::TunnelRefused { .. }
        | SandmeError::UnsafeProfilePath { .. } => ExitCode::from(SANDME_FAILURE),
    }
}
