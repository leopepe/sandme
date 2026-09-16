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
/// Precedence: environment variables override config file values (FR-009).
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Filesystem paths the sandboxed command may access.
    #[serde(default = "default_shared_paths")]
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
    #[serde(default)]
    pub read_only_paths: Vec<String>,

    /// Whether to start the egress proxy and route the command through it.
    ///
    /// When `true` (the default), sandme starts the egress proxy, wires
    /// `HTTP(S)_PROXY` into the child, and grants the command egress to the
    /// proxy alone. When `false`, no proxy is started, no proxy environment is
    /// set, and the sandbox grants the command no network egress at all — a
    /// strictly more restrictive result. This narrows access, so it is not a
    /// widening setting and raises no provenance warning.
    #[serde(default = "default_proxy")]
    pub proxy: bool,

    /// Port for the HTTP proxy server.
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,

    /// Allow GUI applications to write to their state and scratch directories.
    ///
    /// When `true`, the sandbox grants write access to `~/Library` and to the
    /// per-user temporary directory (`$TMPDIR`), which GUI apps need for state,
    /// caches and scratch space (SPEC-0002 FR-101, SPEC-0007 FR-702). It does
    /// not open the world-shared `/private/tmp` or all of `/private/var/folders`.
    /// Defaults to `false` to preserve the strict security model.
    #[serde(default = "default_gui_mode")]
    pub gui_mode: bool,

    /// Relay to destinations on the host's own networks (SPEC-0003 FR-205).
    ///
    /// When `true`, the proxy forwards to loopback, RFC1918, link-local,
    /// unique-local and unspecified addresses, which a locally hosted service
    /// — a local model server, a dev API — needs. Defaults to `false`: those
    /// are exactly the destinations the sandbox profile denies the command
    /// directly, and relaying to them turns the proxy into a pivot (issue #15).
    #[serde(default = "default_allow_private_egress")]
    pub allow_private_egress: bool,
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

/// Every default has exactly one definition, used by both `Default` and serde.
///
/// Two definitions is how `shared_paths` came to have different defaults
/// depending on whether a config file existed: one path said the configured
/// default, the derived serde default for a `Vec` said empty, and `load`
/// replaces the whole struct when a file is present. One definition keeps them
/// in step.
///
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
    let mut config = Config::default();
    let mut file = FileKeys::default();

    let path = config_path(home);
    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|source| SandmeError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        // Parse into a table first to record which keys the file actually set:
        // serde fills the rest from defaults, so the deserialized struct alone
        // cannot tell a configured value from a defaulted one (FR-1004).
        let table: toml::Table =
            toml::from_str(&content).map_err(|source| SandmeError::ConfigParse { source })?;
        file = FileKeys::from_table(&table);
        config = table
            .try_into()
            .map_err(|source| SandmeError::ConfigParse { source })?;
    }

    // The shares before the environment is applied are the baseline a broadened
    // `SANDME_SHARED_PATHS`/`SANDME_READ_ONLY_PATHS` is judged against (FR-1003).
    let baseline_shared = config.shared_paths.clone();
    let baseline_read_only = config.read_only_paths.clone();
    let env = apply_env(&mut config);

    let warnings = widening_warnings(&config, file, env, &baseline_shared, &baseline_read_only);
    Ok(Loaded { config, warnings })
}

/// Apply environment-variable overrides; the environment wins (FR-009).
///
/// Returns which variables were present, so the caller can attribute each
/// setting's provenance (FR-1004) without re-reading the environment.
fn apply_env(config: &mut Config) -> EnvKeys {
    let mut env = EnvKeys::default();

    if let Ok(paths) = env::var("SANDME_SHARED_PATHS") {
        env.shared_paths = true;
        config.shared_paths = split_list(&paths);
    }

    if let Ok(paths) = env::var("SANDME_READ_ONLY_PATHS") {
        env.read_only_paths = true;
        config.read_only_paths = split_list(&paths);
    }

    if let Ok(proxy) = env::var("SANDME_PROXY") {
        config.proxy = env_bool(&proxy);
    }

    if let Ok(port) = env::var("SANDME_PROXY_PORT")
        && let Ok(port) = port.parse::<u16>()
    {
        config.proxy_port = port;
    }

    if let Ok(gui) = env::var("SANDME_GUI_MODE") {
        env.gui_mode = true;
        config.gui_mode = env_bool(&gui);
    }

    if let Ok(allow) = env::var("SANDME_ALLOW_PRIVATE_EGRESS") {
        env.allow_private_egress = true;
        config.allow_private_egress = env_bool(&allow);
    }

    env
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

/// Which widening settings a config file set, as opposed to leaving defaulted.
///
/// One `bool` per widening key: this is a flag set, not a struct that happens to
/// hold booleans, so `clippy::struct_excessive_bools` (which suggests grouping
/// unrelated flags into an enum/state type) does not apply — the fields ARE the
/// per-key provenance bits, and grouping them would obscure the one-to-one map.
#[allow(
    clippy::struct_excessive_bools,
    reason = "FR-1705: one provenance bit per widening key"
)]
#[derive(Debug, Default, Clone, Copy)]
struct FileKeys {
    shared_paths: bool,
    read_only_paths: bool,
    gui_mode: bool,
    allow_private_egress: bool,
}

impl FileKeys {
    /// A key present in the parsed table was set by the user's own config file.
    fn from_table(table: &toml::Table) -> Self {
        Self {
            shared_paths: table.contains_key("shared_paths"),
            read_only_paths: table.contains_key("read_only_paths"),
            gui_mode: table.contains_key("gui_mode"),
            allow_private_egress: table.contains_key("allow_private_egress"),
        }
    }
}

/// Which widening settings an environment variable set this run.
///
/// A per-key flag set, as with [`FileKeys`] — see its note on
/// `clippy::struct_excessive_bools`.
#[allow(
    clippy::struct_excessive_bools,
    reason = "FR-1705: one provenance bit per widening key"
)]
#[derive(Debug, Default, Clone, Copy)]
struct EnvKeys {
    shared_paths: bool,
    read_only_paths: bool,
    gui_mode: bool,
    allow_private_egress: bool,
}

/// Where a setting's effective value came from (FR-1004).
///
/// Only [`Provenance::Environment`] is a concern for issue #30. The sandboxed
/// command cannot write `~/.sandme/config.toml` — the sandbox denies it — but
/// under the default home share it can plant `export SANDME_…` in a shell rc,
/// so the *next* run starts pre-widened with nothing warning the user. Recording
/// the source lets a widening setting be called out when it came from the
/// environment, and stay silent when the user set it in their own config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Provenance {
    /// Built-in default; neither the file nor the environment set it.
    Default,
    /// Set in `~/.sandme/config.toml`, which the sandbox denies the child.
    ConfigFile,
    /// Set by a `SANDME_*` variable, which the child can plant in a shell rc.
    Environment,
}

/// Resolve a setting's provenance. The environment overrides the file (FR-009),
/// so its presence wins the attribution.
fn provenance(file_set: bool, env_set: bool) -> Provenance {
    if env_set {
        Provenance::Environment
    } else if file_set {
        Provenance::ConfigFile
    } else {
        Provenance::Default
    }
}

/// The issue that motivates the warnings, cited in each so a reader can find it.
const ISSUE_REF: &str = "issue #30";

/// Warn about each security-widening setting the environment supplied (FR-1001).
///
/// A boolean opt-in (`gui_mode`, `allow_private_egress`) warns only when the
/// environment turned it on; `shared_paths` warns only when the environment
/// broadened it beyond the file-or-default baseline (FR-1003). A setting from
/// the config file or left at its default never warns (FR-1002).
fn widening_warnings(
    config: &Config,
    file: FileKeys,
    env: EnvKeys,
    baseline_shared: &[String],
    baseline_read_only: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();

    if config.allow_private_egress
        && provenance(file.allow_private_egress, env.allow_private_egress)
            == Provenance::Environment
    {
        warnings.push(opt_in_warning(
            "allow_private_egress = true",
            "SANDME_ALLOW_PRIVATE_EGRESS",
        ));
    }

    if config.gui_mode && provenance(file.gui_mode, env.gui_mode) == Provenance::Environment {
        warnings.push(opt_in_warning("gui_mode = true", "SANDME_GUI_MODE"));
    }

    if provenance(file.shared_paths, env.shared_paths) == Provenance::Environment
        && is_broadened(&config.shared_paths, baseline_shared)
    {
        warnings.push(shared_paths_warning());
    }

    if provenance(file.read_only_paths, env.read_only_paths) == Provenance::Environment
        && is_broadened(&config.read_only_paths, baseline_read_only)
    {
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
        let mut config = Config {
            shared_paths: vec!["/file".to_string()],
            read_only_paths: Vec::new(),
            proxy: true,
            proxy_port: 1234,
            gui_mode: false,
            allow_private_egress: false,
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
        let mut config = Config {
            proxy: true,
            read_only_paths: vec!["/file-only".to_string()],
            ..Config::default()
        };

        // When the environment is applied
        apply_env(&mut config);
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
            let mut config = Config::default();
            unsafe { std::env::set_var("SANDME_PROXY", value) }
            apply_env(&mut config);
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
        let env = EnvKeys {
            read_only_paths: true,
            ..EnvKeys::default()
        };

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, FileKeys::default(), env, &config.shared_paths, &[]);

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
        let file = FileKeys {
            read_only_paths: true,
            ..FileKeys::default()
        };

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, file, EnvKeys::default(), &config.shared_paths, &[]);

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
            let mut config = Config::default();
            unsafe { std::env::set_var("SANDME_ALLOW_PRIVATE_EGRESS", value) }

            // When the environment is applied
            apply_env(&mut config);

            // Then the proxy is allowed to reach the host's own networks
            assert!(config.allow_private_egress, "{value} should enable it");
        }

        // And anything else leaves the safe default in place: the setting
        // re-opens the pivot in issue #15, so it takes an explicit yes
        for value in ["0", "false", "yes", ""] {
            let mut config = Config::default();
            unsafe { std::env::set_var("SANDME_ALLOW_PRIVATE_EGRESS", value) }
            apply_env(&mut config);
            assert!(!config.allow_private_egress, "{value} should not enable it");
        }
        unsafe { std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS") }
    }

    #[test]
    fn provenance_attributes_the_environment_over_the_file() {
        // Given a setting present in both the file and the environment
        // When its provenance is resolved
        // Then the environment wins, because it overrides the file (FR-009)
        assert_eq!(provenance(true, true), Provenance::Environment);
        assert_eq!(provenance(false, true), Provenance::Environment);
    }

    #[test]
    fn provenance_attributes_the_config_file_when_the_environment_is_absent() {
        // Given a setting the file sets and the environment does not
        // When its provenance is resolved
        // Then it is attributed to the config file
        assert_eq!(provenance(true, false), Provenance::ConfigFile);
    }

    #[test]
    fn provenance_attributes_the_default_when_neither_sets_it() {
        // Given a setting neither the file nor the environment sets
        // When its provenance is resolved
        // Then it is attributed to the built-in default
        assert_eq!(provenance(false, false), Provenance::Default);
    }

    #[test]
    fn warns_when_the_environment_enables_private_egress() {
        // Given allow_private_egress turned on by the environment alone
        let config = Config {
            allow_private_egress: true,
            ..Config::default()
        };
        let env = EnvKeys {
            allow_private_egress: true,
            ..EnvKeys::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(
            &config,
            FileKeys::default(),
            env,
            &config.shared_paths,
            &config.read_only_paths,
        );

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
        let file = FileKeys {
            allow_private_egress: true,
            ..FileKeys::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(
            &config,
            file,
            EnvKeys::default(),
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
        let env = EnvKeys {
            allow_private_egress: true,
            ..EnvKeys::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(
            &config,
            FileKeys::default(),
            env,
            &config.shared_paths,
            &config.read_only_paths,
        );

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
        let env = EnvKeys {
            shared_paths: true,
            ..EnvKeys::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(&config, FileKeys::default(), env, &baseline, &[]);

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
        let env = EnvKeys {
            shared_paths: true,
            ..EnvKeys::default()
        };

        // When the widening warnings are computed
        let warnings = widening_warnings(&config, FileKeys::default(), env, &baseline, &[]);

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
