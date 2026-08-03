use std::env;
use std::path::PathBuf;

use serde::Deserialize;

/// sandme configuration loaded from config file and environment variables.
///
/// Precedence: environment variables override config file values.
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

/// Load configuration from file and environment.
///
/// 1. Start with defaults.
/// 2. Load config file (~/.sandme/config.toml) if it exists.
/// 3. Override with environment variables (SANDEME_SHARED_PATHS, SANDEME_PROXY_PORT).
pub fn load() -> Result<Config, ConfigError> {
    let mut config = Config::default();

    // Load from config file
    let path = config_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| ConfigError::Read { path: path.clone(), source: e })?;
        let file_config: toml::Value = toml::from_str(&content)
            .map_err(|e| ConfigError::Parse { source: e })?;

        // Apply file values
        if let Some(obj) = file_config.as_table() {
            if let Some(paths) = obj.get("shared_paths")
                && let Some(arr) = paths.as_array() {
                    config.shared_paths = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            if let Some(port) = obj.get("proxy_port")
                && let Some(p) = port.as_integer() {
                    config.proxy_port = p as u16;
                }
        }
    }

    // Override with environment variables
    load_env(&mut config);

    Ok(config)
}

fn load_env(config: &mut Config) {
    // SANDEME_SHARED_PATHS: comma-separated list of paths
    if let Ok(paths_str) = env::var("SANDEME_SHARED_PATHS") {
        config.shared_paths = paths_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    // SANDEME_PROXY_PORT
    if let Ok(port_str) = env::var("SANDEME_PROXY_PORT")
        && let Ok(p) = port_str.parse::<u16>() {
            config.proxy_port = p;
        }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file: {path}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse config file: {source}")]
    Parse {
        #[from]
        source: toml::de::Error,
    },
}
