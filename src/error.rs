//! Every way a sandme invocation can fail, in one place.

use std::path::PathBuf;

use thiserror::Error;

/// All errors a sandme invocation can fail with.
///
/// Each variant is one cause the user can act on or that changes the message
/// they need; causes handled identically share a variant.
#[derive(Debug, Error)]
pub enum SandmeError {
    /// The configuration file exists but could not be read.
    #[error("config file {} could not be read: {source}", path.display())]
    ConfigRead {
        /// Path of the config file that failed.
        path: PathBuf,
        /// The underlying I/O failure.
        source: std::io::Error,
    },

    /// The configuration file is not valid TOML.
    #[error("config file could not be parsed: {source}")]
    ConfigParse {
        /// The underlying parse failure.
        source: toml::de::Error,
    },

    /// No command was given to run under the sandbox.
    #[error("no command given to run under the sandbox")]
    EmptyCommand,

    /// The command line could not be split into words (e.g. an unterminated
    /// quote).
    #[error("could not parse command: {0}")]
    CommandParse(String),

    /// The sandbox profile could not be written for `sandbox-exec`.
    #[error("sandbox profile could not be written: {0}")]
    ProfileWrite(std::io::Error),

    /// The sandboxed command could not be started.
    #[error("sandboxed command could not be started: {0}")]
    Execute(std::io::Error),

    /// The proxy could not take its port; the invocation fails rather than
    /// run a sandbox whose egress is unmediated.
    #[error("proxy could not listen on port {port}: {source}; is the port already in use?")]
    ProxyStartup {
        /// The port the proxy tried to bind.
        port: u16,
        /// The underlying bind failure.
        source: std::io::Error,
    },
}
