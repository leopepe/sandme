//! Every way a sandme invocation can fail, in one place.

use std::path::PathBuf;

use thiserror::Error;

/// All errors a sandme invocation can fail with.
///
/// Each variant is one cause the user can act on or that changes the message
/// they need; causes handled identically share a variant. The exit status each
/// one becomes is decided in one place, `main::failure_code` (SPEC-0004).
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

    /// Nothing of the name the user gave could be found to execute.
    ///
    /// Separate from [`Self::CommandNotExecutable`] because the two are
    /// separate statuses to the caller — `127` and `126` — and separate
    /// mistakes to the user: a typo against a permission.
    #[error("{command}: command not found")]
    CommandNotFound {
        /// The command word as the user wrote it.
        command: String,
    },

    /// A file of that name was found, but it is not something to execute.
    #[error("{command}: found but not executable")]
    CommandNotExecutable {
        /// The command word as the user wrote it.
        command: String,
    },

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

    /// The SSH tunnel (sandme run as ssh's `ProxyCommand`) found no proxy to
    /// use: `HTTP_PROXY` is absent or not in the form sandme writes. This is a
    /// sandme fault, not the user's — the child is always given the variable.
    #[error("SSH tunnel: no usable HTTP_PROXY in the environment")]
    TunnelProxyUnset,

    /// The SSH tunnel could not reach sandme's proxy on loopback.
    #[error("SSH tunnel: could not reach the proxy at {proxy}: {source}")]
    TunnelUnreachable {
        /// The proxy address the tunnel tried to reach.
        proxy: std::net::SocketAddr,
        /// The underlying I/O failure.
        source: std::io::Error,
    },

    /// sandme's proxy declined the `CONNECT` for the SSH destination — a `403`
    /// means the host is on a range the proxy will not relay to (SPEC-0003
    /// FR-201), which HTTPS egress would hit the same way.
    #[error("SSH tunnel: proxy declined CONNECT to {host}:{port} (HTTP {status})")]
    TunnelRefused {
        /// The SSH host ssh asked to reach.
        host: String,
        /// The SSH port ssh asked to reach.
        port: String,
        /// The HTTP status the proxy answered the `CONNECT` with.
        status: String,
    },

    /// The running kernel cannot enforce the Landlock sandbox this invocation
    /// needs (Landlock absent or disabled, or its ABI below the v4 floor that
    /// outbound TCP-port restriction requires). sandme refuses to run rather
    /// than run the command unconfined or with egress unrestricted — the Linux
    /// counterpart of Seatbelt's all-or-nothing guarantee (SPEC-0016).
    #[error("this kernel cannot enforce the sandbox: {reason}; refusing to run the command")]
    // Constructed only by the Linux backend; on other targets it is matched in
    // `main::failure_code` but never built, which dead-code analysis counts as
    // unconstructed. Allow that off Linux.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    SandboxNotEnforced {
        /// Why enforcement is impossible, phrased for the user.
        reason: String,
    },

    /// A `shared_paths` entry names `/`, a protected pseudo-filesystem
    /// (`/proc`, `/sys`), or a lexical ancestor of one. Under Landlock's
    /// allow-only, recursive grant that would re-admit `/proc/<pid>/environ`
    /// (issue #31) with no deny primitive to carve it back out, so sandme
    /// refuses to run rather than grant the leak back (SPEC-0016, D5).
    #[error(
        "shared path {path:?} would grant a protected system tree (/proc or /sys); \
         narrow it to the directory you actually need; refusing to run"
    )]
    // Constructed only by the Linux backend's shared-path guard; on other
    // targets `plan::guard_shared_path` has no non-test caller, so a non-test
    // build never builds this variant, which dead-code analysis counts as
    // unconstructed. Allow that off Linux.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    SharedPathTooBroad {
        /// The offending `shared_paths` entry, as configured.
        path: String,
    },

    /// A path bound for the sandbox profile carries a character that could
    /// break out of its SBPL string literal; the invocation fails rather than
    /// emit a profile an attacker could have shaped.
    #[error(
        "path {path:?} contains a character that cannot be placed in the sandbox profile \
         (a quote, backslash, or control character); refusing to run"
    )]
    UnsafeProfilePath {
        /// The offending path, as it would have been written into the profile.
        path: String,
    },
}
