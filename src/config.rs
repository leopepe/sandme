//! Configuration from `~/.sandme/config.toml`, overridden by environment variables.
//!
//! This file exceeds the 400-line limit in `docs/guidelines/code/simplicity.md`
//! §2 and takes that guideline's escape hatch (SPEC-0010). The overage is tests:
//! the module proper is one cohesive purpose — load the configuration — and its
//! unit tests sit at its foot, where `docs/guidelines/code/consistency.md` §4
//! requires them. Splitting either out is the worse alternative the guideline
//! names.

use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::SandmeError;

/// sandme configuration loaded from config file and environment variables.
///
/// Built by merging two [`Layer`]s onto the defaults — the config file first,
/// then the environment, so the environment wins (FR-009). `Config` itself is
/// not deserialised: every default has exactly one definition, in [`Default`].
#[derive(Debug, Clone)]
pub struct Config {
    /// Filesystem paths the sandboxed command may access.
    pub shared_paths: Vec<String>,

    /// Filesystem paths the sandboxed command may read and execute, but not
    /// write.
    ///
    /// Widens reads without widening writes: a toolchain prefix (a Homebrew
    /// directory, a language runtime) can be named here so its binaries run and
    /// its files read, while writes stay confined to `shared_paths`. `~/`
    /// expands to the home directory and symlinks are resolved, exactly as for
    /// `shared_paths`. Defaults to empty — no toolchain directory is baked into
    /// the base set (SPEC-0002 minimal-grant thesis).
    pub read_only_paths: Vec<String>,

    /// Whether to start the egress proxy and route the command through it.
    ///
    /// When `true` (the default), sandme starts the egress proxy, wires
    /// `HTTP(S)_PROXY` into the child, and grants the command egress to the
    /// proxy alone. When `false`, no proxy is started, no proxy environment is
    /// set, and the sandbox grants the command no network egress at all — a
    /// strictly more restrictive result. This narrows access, so it is not a
    /// widening setting and raises no provenance warning.
    pub proxy: bool,

    /// Port for the HTTP proxy server.
    pub proxy_port: u16,

    /// Allow GUI applications to write to their state and scratch directories.
    ///
    /// When `true`, the sandbox grants write access to `~/Library` and to the
    /// per-user temporary directory (`$TMPDIR`), which GUI apps need for state,
    /// caches and scratch space (SPEC-0002 FR-101, SPEC-0007 FR-702). It does
    /// not open the world-shared `/private/tmp` or all of `/private/var/folders`.
    /// Defaults to `false` to preserve the strict security model.
    pub gui_mode: bool,

    /// Relay to destinations on the host's own networks (SPEC-0003 FR-205).
    ///
    /// When `true`, the proxy forwards to loopback, RFC1918, link-local,
    /// unique-local and unspecified addresses, which a locally hosted service
    /// — a local model server, a dev API — needs. Defaults to `false`: those
    /// are exactly the destinations the sandbox profile denies the command
    /// directly, and relaying to them turns the proxy into a pivot (issue #15).
    pub allow_private_egress: bool,
}

/// One source's partial view of the configuration: the config file, or the
/// environment. A field that is `None` means that source said nothing about
/// the setting, so `Some` *is* the provenance bit — which is why there is no
/// separate `Provenance` type and no second pass over the parsed file to see
/// which keys it carried.
#[derive(Debug, Default, Deserialize)]
struct Layer {
    shared_paths: Option<Vec<String>>,
    read_only_paths: Option<Vec<String>>,
    proxy: Option<bool>,
    proxy_port: Option<u16>,
    gui_mode: Option<bool>,
    allow_private_egress: Option<bool>,
}

impl Layer {
    /// Read the `SANDME_*` overrides into a layer (FR-008).
    ///
    /// A variable that is absent, or a `SANDME_PROXY_PORT` that is not a
    /// `u16`, leaves its field `None` — the layer says nothing and the value
    /// underneath stands.
    fn from_environment() -> Self {
        Self {
            shared_paths: env::var("SANDME_SHARED_PATHS")
                .ok()
                .map(|paths| split_list(&paths)),
            read_only_paths: env::var("SANDME_READ_ONLY_PATHS")
                .ok()
                .map(|paths| split_list(&paths)),
            proxy: env::var("SANDME_PROXY").ok().map(|on| env_bool(&on)),
            proxy_port: env::var("SANDME_PROXY_PORT")
                .ok()
                .and_then(|port| port.parse::<u16>().ok()),
            gui_mode: env::var("SANDME_GUI_MODE").ok().map(|on| env_bool(&on)),
            allow_private_egress: env::var("SANDME_ALLOW_PRIVATE_EGRESS")
                .ok()
                .map(|on| env_bool(&on)),
        }
    }
}

impl Config {
    /// Overlay what `layer` says onto this configuration, leaving the rest as
    /// it was. Applied file-first then environment, this is FR-009's
    /// precedence.
    fn merge(mut self, layer: &Layer) -> Self {
        if let Some(paths) = &layer.shared_paths {
            self.shared_paths.clone_from(paths);
        }
        if let Some(paths) = &layer.read_only_paths {
            self.read_only_paths.clone_from(paths);
        }
        if let Some(proxy) = layer.proxy {
            self.proxy = proxy;
        }
        if let Some(port) = layer.proxy_port {
            self.proxy_port = port;
        }
        if let Some(gui_mode) = layer.gui_mode {
            self.gui_mode = gui_mode;
        }
        if let Some(allow) = layer.allow_private_egress {
            self.allow_private_egress = allow;
        }
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shared_paths: default_shared_paths(),
            read_only_paths: Vec::new(),
            proxy: default_proxy(),
            proxy_port: default_proxy_port(),
            gui_mode: default_gui_mode(),
            allow_private_egress: default_allow_private_egress(),
        }
    }
}

/// The default is the current working directory, not the home directory
/// (SPEC-0007 FR-701). A fresh install confines the sandboxed command to the
/// directory the user invoked it from, so `~/.ssh`, `~/.aws` and shell history
/// are not shared out of the box; anyone wanting the old whole-home behaviour
/// sets `shared_paths = ["~/"]` explicitly. `current_dir` fails only when the
/// working directory has been removed or is unreadable — pathological for an
/// interactive CLI, and with no safe path to substitute the default is then no
/// share at all, leaving the command under the base sandbox until `shared_paths`
/// names something.
fn default_shared_paths() -> Vec<String> {
    env::current_dir()
        .map(|dir| vec![dir.to_string_lossy().into_owned()])
        .unwrap_or_default()
}

/// The default proxy port is `0`: the OS chooses a free ephemeral port at bind
/// time (SPEC-0014/FR-1404), so concurrent `sandme` runs never collide on a
/// fixed port. The bound port is read back from the listener and pinned into
/// the sandbox grant, so the command still reaches the proxy. A caller who
/// needs a fixed, predictable port sets `proxy_port` explicitly.
fn default_proxy_port() -> u16 {
    0
}

fn default_proxy() -> bool {
    true
}

fn default_gui_mode() -> bool {
    false
}

fn default_allow_private_egress() -> bool {
    false
}

/// Resolve the config file path (~/.sandme/config.toml) from an injected
/// `home`, the home-parameterized config-path helper.
///
/// Derives the path from `home` alone, falling back to a relative path when no
/// home is available. Reads no process environment: [`load`] resolves `$HOME`
/// once at the adapter boundary and passes it in, and tests drive it with a
/// temp directory instead of mutating `$HOME`.
fn config_path(home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from("./config.toml");
    };
    home.join(".sandme").join("config.toml")
}

/// A loaded configuration and any warnings raised while sourcing it.
///
/// The warnings are returned rather than printed so `load` does no I/O of its
/// own and stays unit-testable; `main` writes them to stderr (issue #30).
pub struct Loaded {
    /// The resolved configuration.
    pub config: Config,
    /// One line per security-widening setting the environment supplied, each
    /// without the `sandme: ` prefix `main` prepends. Empty in the common case.
    pub warnings: Vec<String>,
}

/// Load configuration from file and environment (FR-007, FR-008, FR-009).
///
/// 1. Start with defaults.
/// 2. Load the config file (`~/.sandme/config.toml`) if it exists; a file
///    that does not parse is reported, not silently replaced.
/// 3. Override with environment variables (`SANDME_SHARED_PATHS`, `SANDME_PROXY_PORT`).
/// 4. Warn about any security-widening setting the environment — not the config
///    file — supplied (FR-1001, issue #30).
pub fn load() -> Result<Loaded, SandmeError> {
    load_from(env::var("HOME").ok().map(PathBuf::from).as_deref())
}

/// Load configuration for an injected `home`, the pure-of-`$HOME` core of
/// [`load`]. The config-file path is derived from `home` alone (via
/// [`config_path`]); tests supply a temp directory here rather than
/// mutating the process `$HOME`. The `SANDME_*` environment overrides are still
/// read from the process environment — injecting those is out of scope.
fn load_from(home: Option<&Path>) -> Result<Loaded, SandmeError> {
    let mut file = Layer::default();

    let path = config_path(home);
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|source| SandmeError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        file = toml::from_str(&content).map_err(|source| SandmeError::ConfigParse { source })?;
    }

    let config = Config::default().merge(&file);
    // The shares before the environment is applied are the baseline a broadened
    // `SANDME_SHARED_PATHS`/`SANDME_READ_ONLY_PATHS` is judged against (FR-1003).
    let baseline_shared = config.shared_paths.clone();
    let baseline_read_only = config.read_only_paths.clone();

    let env = Layer::from_environment();
    let config = config.merge(&env);

    let warnings = widening_warnings(&config, &env, &baseline_shared, &baseline_read_only);
    Ok(Loaded { config, warnings })
}

/// Split a comma-separated environment list into trimmed, non-empty entries.
///
/// Shared by `SANDME_SHARED_PATHS` and `SANDME_READ_ONLY_PATHS`, which both take
/// a comma-separated path list: each entry is trimmed and blank entries (a
/// trailing comma, `a,,b`) are dropped, so the two parse identically.
fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// The house boolean convention for a `SANDME_*` flag: `1` or `true`
/// (case-insensitive) is on, anything else is off.
///
/// Shared by `SANDME_PROXY`, `SANDME_GUI_MODE` and `SANDME_ALLOW_PRIVATE_EGRESS`
/// so the three read the same spellings.
fn env_bool(s: &str) -> bool {
    s == "1" || s.to_lowercase() == "true"
}

/// The issue that motivates the warnings, cited in each so a reader can find it.
const ISSUE_REF: &str = "issue #30";

/// Warn about each security-widening setting the environment supplied (FR-1001).
///
/// A boolean opt-in (`gui_mode`, `allow_private_egress`) warns only when the
/// environment turned it on; `shared_paths` warns only when the environment
/// broadened it beyond the file-or-default baseline (FR-1003). A setting from
/// the config file or left at its default never warns (FR-1002).
///
/// The environment layer alone decides attribution: it is applied last, so a
/// key it carries is the key that took effect whatever the file said (FR-009,
/// FR-1004). `proxy` is absent here deliberately — `proxy = false` narrows
/// access rather than widening it, so it warrants no warning.
fn widening_warnings(
    config: &Config,
    env: &Layer,
    baseline_shared: &[String],
    baseline_read_only: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();

    if config.allow_private_egress && env.allow_private_egress == Some(true) {
        warnings.push(opt_in_warning(
            "allow_private_egress = true",
            "SANDME_ALLOW_PRIVATE_EGRESS",
        ));
    }

    if config.gui_mode && env.gui_mode == Some(true) {
        warnings.push(opt_in_warning("gui_mode = true", "SANDME_GUI_MODE"));
    }

    if env.shared_paths.is_some() && is_broadened(&config.shared_paths, baseline_shared) {
        warnings.push(shared_paths_warning());
    }

    if env.read_only_paths.is_some() && is_broadened(&config.read_only_paths, baseline_read_only) {
        warnings.push(read_only_paths_warning());
    }

    warnings
}

/// The line for a boolean opt-in the environment enabled.
fn opt_in_warning(setting: &str, env_var: &str) -> String {
    format!(
        "warning: {setting} came from the environment ({env_var}), not your config file; a \
         sandboxed command can plant this in a shell rc to pre-widen your next run ({ISSUE_REF})"
    )
}

/// The line for a `shared_paths` the environment broadened.
fn shared_paths_warning() -> String {
    format!(
        "warning: shared_paths was broadened by the environment (SANDME_SHARED_PATHS) beyond your \
         config file or the default; a sandboxed command can plant this in a shell rc to pre-widen \
         your next run ({ISSUE_REF})"
    )
}

/// The line for a `read_only_paths` the environment broadened.
fn read_only_paths_warning() -> String {
    format!(
        "warning: read_only_paths was broadened by the environment (SANDME_READ_ONLY_PATHS) beyond \
         your config file or the default; a sandboxed command can plant this in a shell rc to \
         pre-widen your next run ({ISSUE_REF})"
    )
}

/// True when `candidate` grants access beyond `baseline` — some candidate path
/// lies outside every baseline path (FR-1003).
///
/// The comparison is purely textual: `config.rs` does not touch the filesystem
/// (docs/guidelines/code/simplicity.md §3), so a path spelled through a symlink
/// (`/var` vs `/private/var`) or with an unexpanded `~` is compared as written.
/// The check therefore errs toward warning, never toward silence.
fn is_broadened(candidate: &[String], baseline: &[String]) -> bool {
    candidate
        .iter()
        .any(|path| !baseline.iter().any(|root| is_within(path, root)))
}

/// True when `path` is `root` itself or a descendant of it.
fn is_within(path: &str, root: &str) -> bool {
    let path = normalize(path);
    let root = normalize(root);
    if root == "/" {
        return path.starts_with('/');
    }
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Drop a trailing slash so `~/` and `~` compare equal, keeping root as `/`.
fn normalize(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() { "/" } else { trimmed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_share_is_the_current_directory() {
        // Given the process's working directory
        let cwd = env::current_dir().unwrap();

        // When the default shared paths are computed
        // Then the working directory is the one and only default share
        // (SPEC-0007 FR-701), not the home directory it used to be
        assert_eq!(
            default_shared_paths(),
            vec![cwd.to_string_lossy().into_owned()]
        );
    }

    #[test]
    #[serial_test::serial]
    fn keeps_defaults_for_absent_settings() {
        // Given an injected home directory with no config file
        let dir = std::env::temp_dir().join("sandme-test-keeps-defaults");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // The `SANDME_*` variables are process-wide; this test owns them (hence
        // `#[serial]`). HOME is injected by argument, not mutated.
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_PROXY_PORT");
        }

        // When the configuration is loaded for that home
        let config = load_from(Some(dir.as_path())).unwrap().config;

        // Then the defaults are returned — for shared_paths, the documented
        // default (the working directory) rather than serde's empty Vec, which
        // is the bug this test guards against
        assert_eq!(config.shared_paths, default_shared_paths());
        assert!(!config.shared_paths.is_empty());
        // The default proxy port is now 0 — the OS-chosen ephemeral port that
        // makes parallel runs collision-free (SPEC-0017/FR-1702); it was 8787 before.
        assert_eq!(config.proxy_port, 0);
        // The proxy is on and no read-only paths are granted by default.
        assert!(config.proxy);
        assert!(config.read_only_paths.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[serial_test::serial]
    fn keeps_defaults_for_keys_a_config_file_omits() {
        // Given a config file that sets one key and leaves the rest out
        let dir = std::env::temp_dir().join("sandme-test-partial-config-file");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".sandme")).unwrap();
        std::fs::write(dir.join(".sandme/config.toml"), "proxy_port = 9000\n").unwrap();
        // The `SANDME_*` variables are process-wide; this test owns them (hence
        // `#[serial]`). HOME is injected by argument, not mutated.
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_PROXY_PORT");
        }

        // When the configuration is loaded for that home
        let config = load_from(Some(dir.as_path())).unwrap().config;

        // Then the key the file sets is honoured, and the omitted keys keep
        // the documented defaults rather than serde's empty ones. Before this
        // was fixed, shared_paths came back empty and the sandboxed command
        // silently lost all filesystem access.
        assert_eq!(config.proxy_port, 9000);
        assert_eq!(config.shared_paths, default_shared_paths());
        assert!(!config.shared_paths.is_empty());
        assert!(!config.gui_mode);
        assert!(!config.allow_private_egress);

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
        let config = Config {
            shared_paths: vec!["/file".to_string()],
            read_only_paths: Vec::new(),
            proxy: true,
            proxy_port: 1234,
            gui_mode: false,
            allow_private_egress: false,
        };

        // When the environment layer is merged on top
        let config = config.merge(&Layer::from_environment());
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_PROXY_PORT");
        }

        // Then the environment wins and blank entries are dropped
        assert_eq!(config.shared_paths, vec!["/x", "/y"]);
        assert_eq!(config.proxy_port, 7070);
    }

    #[test]
    #[serial_test::serial]
    fn disables_the_proxy_when_the_environment_asks_for_it() {
        // Given a config with the proxy on and read-only paths in the file, and
        // the two new environment overrides set (one test owns these variables
        // to keep the suite parallel-safe; this single test is their only
        // writer). SANDME_PROXY=0 disables by the house boolean convention.
        unsafe {
            std::env::set_var("SANDME_PROXY", "0");
            std::env::set_var("SANDME_READ_ONLY_PATHS", "/opt/toolchain, /usr/local ,");
        }
        let config = Config {
            proxy: true,
            read_only_paths: vec!["/file-only".to_string()],
            ..Config::default()
        };

        // When the environment layer is merged on top
        let config = config.merge(&Layer::from_environment());
        unsafe {
            std::env::remove_var("SANDME_PROXY");
            std::env::remove_var("SANDME_READ_ONLY_PATHS");
        }

        // Then SANDME_PROXY=0 turned the proxy off (FR-009), and the
        // environment's read-only paths won over the file's, blanks dropped
        assert!(!config.proxy);
        assert_eq!(config.read_only_paths, vec!["/opt/toolchain", "/usr/local"]);
    }

    #[test]
    #[serial_test::serial]
    fn enables_the_proxy_for_a_true_ish_spelling() {
        // Given the house boolean convention: `1`/`true` (case-insensitive) on,
        // anything else off (one test owns this variable; sole writer)
        for (value, expected) in [
            ("1", true),
            ("true", true),
            ("TRUE", true),
            ("0", false),
            ("false", false),
            ("no", false),
            ("", false),
        ] {
            unsafe { std::env::set_var("SANDME_PROXY", value) }
            let config = Config::default().merge(&Layer::from_environment());
            assert_eq!(config.proxy, expected, "SANDME_PROXY={value:?}");
        }
        unsafe { std::env::remove_var("SANDME_PROXY") }
    }

    #[test]
    fn warns_when_the_environment_broadens_read_only_paths() {
        // Given an empty baseline (the default) and an env value naming a path,
        // sourced from the environment
        let config = Config {
            read_only_paths: vec!["/opt/toolchain".to_string()],
            ..Config::default()
        };
        let env = Layer {
            read_only_paths: Some(config.read_only_paths.clone()),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(&config, &env, &config.shared_paths, &[]);

        // Then one names read_only_paths as broadened by the environment (FR-1003)
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("read_only_paths")
                    && warning.contains("SANDME_READ_ONLY_PATHS")),
            "got: {warnings:?}"
        );
    }

    #[test]
    fn stays_silent_when_the_config_file_sets_read_only_paths() {
        // Given read-only paths sourced from the config file, not the environment
        let config = Config {
            read_only_paths: vec!["/opt/toolchain".to_string()],
            ..Config::default()
        };
        // When the widening warnings are computed, with the environment layer
        // empty — an absent field is the "not from the environment" bit
        let warnings = widening_warnings(&config, &Layer::default(), &config.shared_paths, &[]);

        // Then nothing is warned: the user set it in a file the child cannot
        // write (FR-1002)
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    #[serial_test::serial]
    fn opens_private_egress_only_when_the_environment_asks_for_it() {
        // Given a default config and the opt-out set to each accepted spelling
        // (one test owns this variable to keep the suite parallel-safe;
        // the calls are safe here: this single test is its only writer)
        for value in ["1", "true", "TRUE"] {
            unsafe { std::env::set_var("SANDME_ALLOW_PRIVATE_EGRESS", value) }

            // When the environment layer is merged on top
            let config = Config::default().merge(&Layer::from_environment());

            // Then the proxy is allowed to reach the host's own networks
            assert!(config.allow_private_egress, "{value} should enable it");
        }

        // And anything else leaves the safe default in place: the setting
        // re-opens the pivot in issue #15, so it takes an explicit yes
        for value in ["0", "false", "yes", ""] {
            unsafe { std::env::set_var("SANDME_ALLOW_PRIVATE_EGRESS", value) }
            let config = Config::default().merge(&Layer::from_environment());
            assert!(!config.allow_private_egress, "{value} should not enable it");
        }
        unsafe { std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS") }
    }

    #[test]
    fn attributes_a_setting_to_the_environment_even_when_the_config_file_set_it_too() {
        // Given a widening setting both the config file and the environment
        // turned on
        let file = Layer {
            allow_private_egress: Some(true),
            ..Layer::default()
        };
        let env = Layer {
            allow_private_egress: Some(true),
            ..Layer::default()
        };
        let config = Config::default().merge(&file).merge(&env);

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, &env, &config.shared_paths, &config.read_only_paths);

        // Then the environment carries the attribution, because it is merged
        // last and so supplied the value that took effect (FR-009, FR-1004)
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("SANDME_ALLOW_PRIVATE_EGRESS")),
            "got: {warnings:?}"
        );
    }

    #[test]
    fn warns_when_the_environment_enables_private_egress() {
        // Given allow_private_egress turned on by the environment alone
        let config = Config {
            allow_private_egress: true,
            ..Config::default()
        };
        let env = Layer {
            allow_private_egress: Some(true),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, &env, &config.shared_paths, &config.read_only_paths);

        // Then one names both the setting and the variable that carried it
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("allow_private_egress")
                    && warning.contains("SANDME_ALLOW_PRIVATE_EGRESS")),
            "got: {warnings:?}"
        );
    }

    #[test]
    fn stays_silent_when_the_config_file_enables_private_egress() {
        // Given the same setting on, but sourced from the config file
        let config = Config {
            allow_private_egress: true,
            ..Config::default()
        };
        // When the widening warnings are computed, with the environment layer
        // empty — an absent field is the "not from the environment" bit
        let warnings = widening_warnings(
            &config,
            &Layer::default(),
            &config.shared_paths,
            &config.read_only_paths,
        );

        // Then nothing is warned: the user set it in a file the child cannot
        // write (FR-1002)
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn stays_silent_when_the_environment_disables_private_egress() {
        // Given SANDME_ALLOW_PRIVATE_EGRESS present but set to a falsey value, so
        // the environment sourced it yet it is not a widening
        let config = Config::default();
        let env = Layer {
            allow_private_egress: Some(false),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, &env, &config.shared_paths, &config.read_only_paths);

        // Then nothing is warned: only the widening value warrants it
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn warns_when_the_environment_broadens_shared_paths() {
        // Given a baseline confined to a project and an env value reaching "/"
        let baseline = vec!["/home/user/project".to_string()];
        let config = Config {
            shared_paths: vec!["/".to_string()],
            ..Config::default()
        };
        let env = Layer {
            shared_paths: Some(config.shared_paths.clone()),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(&config, &env, &baseline, &[]);

        // Then one names shared_paths as broadened by the environment (FR-1003)
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("shared_paths")
                    && warning.contains("SANDME_SHARED_PATHS")),
            "got: {warnings:?}"
        );
    }

    #[test]
    fn stays_silent_when_env_shared_paths_stay_within_the_baseline() {
        // Given an env value that only narrows the share to a subdirectory
        let baseline = vec!["/home/user".to_string()];
        let config = Config {
            shared_paths: vec!["/home/user/project".to_string()],
            ..Config::default()
        };
        let env = Layer {
            shared_paths: Some(config.shared_paths.clone()),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(&config, &env, &baseline, &[]);

        // Then nothing is warned: a narrower share is not a widening (FR-1003)
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    #[serial_test::serial]
    fn load_warns_when_the_environment_enables_private_egress() {
        // Given an injected home with no config file and the setting on in the
        // environment. The `SANDME_*` variables are process-wide; this test owns
        // them (hence `#[serial]`). HOME is injected by argument, not mutated.
        let dir = std::env::temp_dir().join("sandme-test-load-warns-egress");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::set_var("SANDME_ALLOW_PRIVATE_EGRESS", "1");
        }

        // When the configuration is loaded for that home
        let loaded = load_from(Some(dir.as_path())).unwrap();

        // Then the setting takes effect (the warning informs, it does not veto)
        // and a warning names it and the variable that carried it (FR-1001)
        assert!(loaded.config.allow_private_egress);
        assert!(
            loaded
                .warnings
                .iter()
                .any(|warning| warning.contains("allow_private_egress")
                    && warning.contains("SANDME_ALLOW_PRIVATE_EGRESS")),
            "got: {:?}",
            loaded.warnings
        );

        unsafe {
            std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[serial_test::serial]
    fn load_stays_silent_when_the_config_file_enables_private_egress() {
        // Given a config file that enables the setting and no env override. The
        // `SANDME_*` variables are process-wide; this test owns them (hence
        // `#[serial]`). HOME is injected by argument, not mutated.
        let dir = std::env::temp_dir().join("sandme-test-load-silent-egress");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".sandme")).unwrap();
        std::fs::write(
            dir.join(".sandme/config.toml"),
            "allow_private_egress = true\n",
        )
        .unwrap();
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS");
        }

        // When the configuration is loaded for that home
        let loaded = load_from(Some(dir.as_path())).unwrap();

        // Then the setting is on but nothing is warned: the user set it in a
        // file the sandbox denies the child (FR-1002)
        assert!(loaded.config.allow_private_egress);
        assert!(loaded.warnings.is_empty(), "got: {:?}", loaded.warnings);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
