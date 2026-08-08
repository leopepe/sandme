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
         (allow mach-register)\n\
         (allow mach-bootstrap)\n\
         (allow iokit-open)\n\
         (allow lsopen)\n\
         (allow ipc-posix-shm*)\n\
         (allow file-read-metadata)\n\
         (allow file-read* file-write* (literal \"/dev/ptmx\"))\n\
         (allow file-read* file-write* (subpath \"/dev/pts\"))\n\
         (allow file-read* file-write* (literal \"/dev/tty\") (literal \"/dev/null\"))\n\
         (allow file-write* (literal \"/dev/null\"))\n\
         (allow file-read* (literal \"/\"))\n\
         (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library\") (subpath \"/Applications\"))\n\
         (allow file-read* (subpath \"/private/etc\") (subpath \"/private/var/db/dyld\") (subpath \"/private/var/run\"))\n"
    );

    for path in &config.shared_paths {
        let expanded = expand_path(path);
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{expanded}\"))"
        );
    }

    // GUI applications need write access to ~/Library for state, caches, and preferences
    if let Ok(home) = std::env::var("HOME") {
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{home}/Library\"))"
        );
    }

    // GUI mode: allow write access to temporary directories
    if config.gui_mode {
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"/private/tmp\"))"
        );
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"/private/var/folders\"))"
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

/// Resolve an app bundle CLI wrapper to its actual executable.
///
/// macOS app bundles often provide CLI wrappers (e.g., `/usr/local/bin/zed`)
/// that use LaunchServices to open the app. Sandboxed processes cannot use
/// LaunchServices for arbitrary document types, so we detect these wrappers
/// and redirect to the bundle's main executable instead.
fn resolve_app_bundle_executable(program: &str) -> String {
    // Resolve symlinks to get the actual path
    let resolved = std::fs::canonicalize(program)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| program.to_string());
    
    // Check if the resolved path is inside an app bundle
    if let Some(contents_idx) = resolved.find(".app/Contents/") {
        let bundle_path = &resolved[..contents_idx + 4]; // Include ".app"
        let plist_path = format!("{bundle_path}/Contents/Info.plist");
        
        // Try to read the bundle identifier from Info.plist
        if let Ok(plist_content) = std::fs::read_to_string(&plist_path) {
            // Simple XML parsing for CFBundleExecutable
            if let Some(start) = plist_content.find("<key>CFBundleExecutable</key>") {
                let after_key = &plist_content[start..];
                if let Some(value_start) = after_key.find("<string>") {
                    let value_after = &after_key[value_start + 8..];
                    if let Some(value_end) = value_after.find("</string>") {
                        let executable_name = &value_after[..value_end];
                        let main_executable = format!("{bundle_path}/Contents/MacOS/{executable_name}");
                        
                        // If the main executable exists and differs from the resolved program, use it
                        if std::path::Path::new(&main_executable).exists() 
                            && main_executable != resolved 
                        {
                            return main_executable;
                        }
                    }
                }
            }
        }
    }
    
    resolved
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

    // Resolve app bundle CLI wrappers to their actual executables
    let resolved_program = resolve_app_bundle_executable(program);

    let profile = generate_profile(config, proxy);
    let proxy_url = format!("http://{proxy}");

    let mut child = tokio::process::Command::new("sandbox-exec")
        .arg("-p")
        .arg(&profile)
        .arg(&resolved_program)
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
            gui_mode: false,
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
    fn uses_macos_seatbelt_framework() {
        // Given a sandboxed command invocation
        // (this test documents that sandme uses sandbox-exec, the macOS
        // Seatbelt interface, as required by NFR-001)
        let profile = generate_profile(
            &config_with(&["/tmp/test"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 8787)),
        );

        // Then the profile is valid SBPL (Seatbelt Profile Language)
        // The integration tests in tests/cli.rs exercise sandbox-exec transitively
        assert!(profile.starts_with("(version 1)"));
        assert!(profile.contains("(deny default)"));
    }

    #[test]
    fn routes_network_only_to_the_proxy() {
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 8787));
        let profile = generate_profile(&config_with(&[]), proxy);

        assert!(profile.contains("(allow network-outbound (remote ip \"localhost:8787\"))"));
        assert!(!profile.contains("network-bind"));
    }

    #[test]
    fn gui_mode_allows_temp_writes() {
        let config = Config {
            shared_paths: vec![],
            proxy_port: 8787,
            gui_mode: true,
        };
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 8787));
        let profile = generate_profile(&config, proxy);
        assert!(profile.contains("/private/tmp"));
        assert!(profile.contains("/private/var/folders"));
    }
}
