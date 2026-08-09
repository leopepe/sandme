//! Seatbelt sandbox profile generation and sandboxed command execution.

use std::fmt::Write;
use std::net::SocketAddr;
use std::process::ExitStatus;

use crate::config::Config;
use crate::error::SandmeError;

/// Generate the Seatbelt (SBPL) profile applied to the sandboxed command.
///
/// Everything is denied by default (FR-004): the command keeps the process
/// mechanics macOS needs to start, read-only access to the system runtime,
/// read-write access to the shared paths, and network egress to the proxy
/// and nothing else (FR-006).
pub fn generate_profile(config: &Config, proxy: SocketAddr) -> String {
    let mut sbpl = String::from(
        "(version 1)\n\
         (deny default)\n\
         (allow process-exec)\n\
         (allow process-fork)\n\
         (allow process-info-pidinfo)\n\
         (allow signal (target self))\n\
         (allow sysctl-read)\n\
         (allow mach-lookup)\n\
         (allow ipc-posix-shm*)\n\
         (allow file-read-metadata)\n\
         (allow file-read* (literal \"/dev/urandom\") (literal \"/dev/tty\") (literal \"/dev/null\"))\n\
         (allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\"))\n\
         (allow file-read* (literal \"/\"))\n\
         (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library\"))\n\
         (allow file-read* (subpath \"/private/etc\") (subpath \"/private/var/db/dyld\") (subpath \"/private/var/run\"))\n",
    );

    for path in &config.shared_paths {
        let expanded = expand_path(path);
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{expanded}\"))"
        );
    }

    let _ = writeln!(
        sbpl,
        "(allow network-outbound (remote ip \"localhost:{}\"))",
        proxy.port()
    );
    sbpl
}

/// Expand a path starting with ~ to the home directory, and resolve
/// symlinks so the profile matches the kernel's view of the path.
fn expand_path(path: &str) -> String {
    let expanded = if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        format!("{home}/{rest}")
    } else {
        path.to_string()
    };
    std::fs::canonicalize(&expanded).map_or(expanded, |resolved| resolved.display().to_string())
}

/// Execute a command under the Seatbelt sandbox and wait for it.
///
/// The operands reach the child unshelled and unquoted: `program` is
/// executed directly with `args` (posix.md §5). The profile is handed to
/// `sandbox-exec -p`, so nothing sensitive touches a temp file. The
/// command's egress is wired to the proxy without any configuration of its
/// own (FR-006): `HTTP_PROXY`/`HTTPS_PROXY` point at `proxy`, and the
/// profile allows no other network destination. Ctrl-C kills the child so
/// sandme can shut down with it (T-007).
pub async fn run(
    config: &Config,
    proxy: SocketAddr,
    command: &[String],
) -> Result<ExitStatus, SandmeError> {
    // clap's required trailing operand guarantees at least one word.
    let (program, args) = command
        .split_first()
        .expect("clap requires at least one operand");

    let profile = generate_profile(config, proxy);
    let proxy_url = format!("http://{proxy}");

    let mut child = tokio::process::Command::new("sandbox-exec")
        .arg("-p")
        .arg(&profile)
        .arg(program)
        .args(args)
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("https_proxy", &proxy_url)
        .spawn()
        .map_err(SandmeError::Execute)?;

    let status = tokio::select! {
        status = child.wait() => status.map_err(SandmeError::Execute)?,
        _ = tokio::signal::ctrl_c() => {
            let _ = child.kill().await;
            child.wait().await.map_err(SandmeError::Execute)?
        }
    };

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    fn config_with(paths: &[&str]) -> Config {
        Config {
            shared_paths: paths.iter().map(|p| (*p).to_string()).collect(),
            proxy_port: 8787,
        }
    }

    #[test]
    fn denies_everything_by_default() {
        let profile = generate_profile(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        assert!(profile.starts_with("(version 1)\n(deny default)\n"));
    }

    #[test]
    fn shares_configured_paths_read_write() {
        let profile = generate_profile(
            &config_with(&["/tmp/sandme-test"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        assert!(profile.contains("(allow file-read* file-write* (subpath \"/tmp/sandme-test\"))"));
    }

    #[test]
    fn routes_network_only_to_the_proxy() {
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 8787));
        let profile = generate_profile(&config_with(&[]), proxy);

        assert!(profile.contains("(allow network-outbound (remote ip \"localhost:8787\"))"));
        assert!(!profile.contains("network-bind"));
    }
}
