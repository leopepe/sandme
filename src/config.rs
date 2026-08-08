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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shared_paths: vec!["~/".to_string()],
            proxy_port: default_proxy_port(),
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
/// 2. Load the config file (`~/.sandme/config.toml`) if it exists.
/// 3. Override with environment variables (`SANDEME_SHARED_PATHS`, `SANDEME_PROXY_PORT`).
pub fn load() -> Result<Config, SandmeError> {
    let mut config = Config::default();

    let path = config_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|source| SandmeError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        let doc: toml::Value =
            toml::from_str(&content).map_err(|source| SandmeError::ConfigParse { source })?;
        apply_toml(&mut config, &doc);
    }

    apply_env(&mut config);
    Ok(config)
}

/// Apply settings present in a parsed config document.
fn apply_toml(config: &mut Config, doc: &toml::Value) {
    let Some(table) = doc.as_table() else {
        return;
    };

    if let Some(paths) = table.get("shared_paths").and_then(toml::Value::as_array) {
        config.shared_paths = paths
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }

    if let Some(port) = table.get("proxy_port").and_then(toml::Value::as_integer) {
        config.proxy_port = u16::try_from(port).unwrap_or_default();
    }
}

/// Apply environment-variable overrides; the environment wins (FR-009).
fn apply_env(config: &mut Config) {
    if let Ok(paths) = env::var("SANDEME_SHARED_PATHS") {
        config.shared_paths = paths
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    if let Ok(port) = env::var("SANDEME_PROXY_PORT")
        && let Ok(port) = port.parse::<u16>()
    {
        config.proxy_port = port;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_file_settings_when_present() {
        // Given a config document with both settings
        let doc: toml::Value =
            toml::from_str("shared_paths = [\"/a\", \"/b\"]\nproxy_port = 9090").unwrap();
        let mut config = Config::default();

        // When the document is applied
        apply_toml(&mut config, &doc);

        // Then both settings replace the defaults
        assert_eq!(config.shared_paths, vec!["/a", "/b"]);
        assert_eq!(config.proxy_port, 9090);
    }

    #[test]
    fn keeps_defaults_for_absent_settings() {
        // Given an empty config document
        let doc: toml::Value = toml::from_str("").unwrap();
        let mut config = Config::default();

        // When the document is applied
        apply_toml(&mut config, &doc);

        // Then the defaults survive
        assert_eq!(config.shared_paths, vec!["~/"]);
        assert_eq!(config.proxy_port, 8787);
    }

    #[test]
    fn environment_overrides_previous_values() {
        // Given a config with file-sourced values and override env vars set
        // (one test owns these env vars to keep the suite parallel-safe;
        // the calls are safe here: this single test is their only writer)
        unsafe {
            std::env::set_var("SANDEME_SHARED_PATHS", "/x, /y ,");
            std::env::set_var("SANDEME_PROXY_PORT", "7070");
        }
        let mut config = Config {
            shared_paths: vec!["/file".to_string()],
            proxy_port: 1234,
        };

        // When the environment is applied
        apply_env(&mut config);
        unsafe {
            std::env::remove_var("SANDEME_SHARED_PATHS");
            std::env::remove_var("SANDEME_PROXY_PORT");
        }

        // Then the environment wins and blank entries are dropped
        assert_eq!(config.shared_paths, vec!["/x", "/y"]);
        assert_eq!(config.proxy_port, 7070);
    }
}
