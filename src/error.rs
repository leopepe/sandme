/// All errors for the sandme CLI.
#[derive(Debug, thiserror::Error)]
pub enum SandmeError {
    #[error("failed to parse config: {0}")]
    Config(String),

    #[error("sandbox execution failed: {0}")]
    Sandbox(#[from] std::io::Error),

    #[error("command not found: {0}")]
    CommandNotFound(String),

    #[error("proxy failed to start")]
    ProxyStartup,
}
