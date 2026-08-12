//! Configuration from `~/.sandme/config.toml`, overridden by environment variables.

use std::env;
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::SandmeError;

/// sandme configuration loaded from config file and environment variables.
///
/// Precedence: environment variables override config file values (FR-009).
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Filesystem paths the sandboxed command may access.
    #[serde(default)]
    pub shared_paths: Vec<String>,

    /// Port for the HTTP proxy server.
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,

    /// Allow GUI applications to write to temporary directories.
    ///
    /// When `true`, the sandbox grants write access to `/private/tmp` and
    /// `/private/var/folders`, which GUI apps need for state and caches.
    /// Defaults to `false` to preserve the strict security model.
    #[serde(default)]
    pub gui_mode: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shared_paths: vec!["~/".to_string()],
            proxy_port: default_proxy_port(),
            gui_mode: false,
        }
    }
}

fn default_proxy_port() -> u16 {
    8787
}

/// Resolve the config file path (~/.sandme/config.toml).
pub fn config_path() -> PathBuf {
    let Some(home) = env::var("HOME").ok().map(PathBuf::from) else {
        return PathBuf::from("./config.toml");
    };
    home.join(".sandme").join("config.toml")
}

/// Load configuration from file and environment (FR-007, FR-008, FR-009).
///
/// 1. Start with defaults.
/// 2. Load the config file (`~/.sandme/config.toml`) if it exists; a file
///    that does not parse is reported, not silently replaced.
/// 3. Override with environment variables (`SANDME_SHARED_PATHS`, `SANDME_PROXY_PORT`).
pub fn load() -> Result<Config, SandmeError> {
    let mut config = Config::default();

    let path = config_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|source| SandmeError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        config = toml::from_str(&content).map_err(|source| SandmeError::ConfigParse { source })?;
    }

    apply_env(&mut config);
    Ok(config)
}

/// Apply environment-variable overrides; the environment wins (FR-009).
fn apply_env(config: &mut Config) {
    if let Ok(paths) = env::var("SANDME_SHARED_PATHS") {
        config.shared_paths = paths
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if let Ok(port) = env::var("SANDME_PROXY_PORT")
        && let Ok(port) = port.parse::<u16>()
    {
        config.proxy_port = port;
    }

    if let Ok(gui) = env::var("SANDME_GUI_MODE") {
        config.gui_mode = gui == "1" || gui.to_lowercase() == "true";
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn keeps_defaults_for_absent_settings() {
        // Given a HOME directory with no config file
        // (one test owns HOME to keep the suite parallel-safe;
        // the calls are safe here: this single test is its only writer)
        let dir = std::env::temp_dir().join("sandme-test-keeps-defaults");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original_home = std::env::var("HOME").ok();
        unsafe {
            std::env::set_var("HOME", &dir);
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_PROXY_PORT");
        }

        // When the configuration is loaded
        let config = load().unwrap();

        // Then the defaults are returned
        assert_eq!(config.shared_paths, vec!["~/".to_string()]);
        assert_eq!(config.proxy_port, 8787);

        // Restore HOME
        unsafe {
            match original_home {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[serial_test::serial]
    fn environment_overrides_previous_values() {
        // Given a config with file-sourced values and override env vars set
        // (one test owns these env vars to keep the suite parallel-safe;
        // the calls are safe here: this single test is their only writer)
        unsafe {
            std::env::set_var("SANDME_SHARED_PATHS", "/x, /y ,");
            std::env::set_var("SANDME_PROXY_PORT", "7070");
        }
        let mut config = Config {
            shared_paths: vec!["/file".to_string()],
            proxy_port: 1234,
            gui_mode: false,
        };

        // When the environment is applied
        apply_env(&mut config);
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_PROXY_PORT");
        }

        // Then the environment wins and blank entries are dropped
        assert_eq!(config.shared_paths, vec!["/x", "/y"]);
        assert_eq!(config.proxy_port, 7070);
    }
}
