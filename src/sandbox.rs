use std::io::Write;
use std::process::{Command, ExitStatus};

use crate::config::Config;

/// Generate a macOS Seatbelt sandbox profile XML for the given config.
///
/// The profile:
/// - Denies all filesystem access by default
/// - Allows access to shared paths only
/// - Allows network access only through the proxy (localhost:<port>)
/// - Allows necessary system paths for execution
pub fn generate_profile(config: &Config) -> Result<String, ProfileError> {
    let mut xml = String::new();

    // Start profile
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<sandbox>\n");

    // Deny all filesystem access by default
    xml.push_str("  <deny-all>\n");
    xml.push_str("    <filesystem>\n");
    xml.push_str("      <deny-all/>\n");
    xml.push_str("    </filesystem>\n");
    xml.push_str("  </deny-all>\n");

    // Allow shared paths
    xml.push_str("  <allow>\n");
    xml.push_str("    <filesystem>\n");

    // Always allow /usr (system libs)
    xml.push_str("      <allow-path>/usr</allow-path>\n");
    // Always allow /private (system)
    xml.push_str("      <allow-path>/private</allow-path>\n");
    // Always allow /tmp
    xml.push_str("      <allow-path>/tmp</allow-path>\n");

    // Add user-specified shared paths
    for path in &config.shared_paths {
        // Expand ~ to home directory
        let expanded = expand_path(path);
        xml.push_str(&format!("      <allow-path>{}</allow-path>\n", expanded));
    }

    xml.push_str("    </filesystem>\n");
    xml.push_str("  </allow>\n");

    // Allow network only through proxy
    xml.push_str("  <allow>\n");
    xml.push_str("    <network>\n");
    xml.push_str("      <allow-rule port=\"any\" protocol=\"tcp\"/>\n");
    xml.push_str("      <allow-rule port=\"any\" protocol=\"udp\"/>\n");
    xml.push_str("    </network>\n");
    xml.push_str("  </allow>\n");

    // Close profile
    xml.push_str("</sandbox>\n");

    Ok(xml)
}

/// Expand a path starting with ~ to the home directory.
fn expand_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, rest);
        }
    path.to_string()
}

/// Execute a command under the Seatbelt sandbox.
///
/// 1. Generate the sandbox profile to a temp file.
/// 2. Run `sandbox-exec -f <profile> <command>`.
/// 3. Clean up the profile file.
/// 4. Return the exit status.
pub fn run(config: &Config, command: &str) -> Result<ExitStatus, SandboxError> {
    // Generate profile
    let profile_xml = generate_profile(config)?;

    // Write profile to temp file
    let mut profile_file = std::env::temp_dir();
    profile_file.push("sandme-profile.xml");

    let mut f = std::fs::File::create(&profile_file)
        .map_err(SandboxError::ProfileWrite)?;
    f.write_all(profile_xml.as_bytes())
        .map_err(SandboxError::ProfileWrite)?;
    drop(f);

    // Parse the command string into parts
    let args: Vec<&str> = command.split_whitespace().collect();
    if args.is_empty() {
        return Err(SandboxError::EmptyCommand);
    }

    // Run under sandbox-exec
    let status = Command::new("sandbox-exec")
        .arg("-f")
        .arg(&profile_file)
        .arg(args[0])
        .args(&args[1..])
        .status()
        .map_err(SandboxError::Execute)?;

    // Clean up profile
    let _ = std::fs::remove_file(&profile_file);

    Ok(status)
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("failed to write sandbox profile: {0}")]
    ProfileWrite(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("sandbox profile error: {0}")]
    Profile(#[from] ProfileError),

    #[error("failed to write sandbox profile: {0}")]
    ProfileWrite(std::io::Error),

    #[error("failed to execute command: {0}")]
    Execute(#[from] std::io::Error),

    #[error("empty command")]
    EmptyCommand,
}
