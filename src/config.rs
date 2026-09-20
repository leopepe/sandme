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
    let path = config_path(home);
    let file = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|source| SandmeError::ConfigRead {
            path: path.clone(),
            source,
        })?;
        toml::from_str::<Layer>(&content).map_err(|source| SandmeError::ConfigParse { source })?
    } else {
        Layer::default()
    };

    let file_merged = Config::default().merge(&file);
    // The shares before the environment is applied are the baseline a broadened
    // `SANDME_SHARED_PATHS`/`SANDME_READ_ONLY_PATHS` is judged against (FR-1003).
    let baseline_shared = file_merged.shared_paths.clone();
    let baseline_read_only = file_merged.read_only_paths.clone();
    let env = Layer::from_environment();
    let config = file_merged.merge(&env);

    let warnings = widening_warnings(&config, &env, &baseline_shared, &baseline_read_only);
    Ok(Loaded { config, warnings })
}

/// A partial configuration: every field `None` means "this layer said
/// nothing about this key". Produced once from the config file and once from
/// the `SANDME_*` environment, then merged onto [`Config::default`] in that
/// order so the environment wins (FR-009). `Some` is itself the provenance
/// bit — the fact `FileKeys`/`EnvKeys` used to track separately from the
/// value now travels with the value.
///
/// A missing key in an `Option<T>` field deserializes to `None` with no
/// `#[serde(default)]` needed, so nothing here duplicates a default the way
/// `Config`'s old `#[serde(default = "…")]` attributes did.
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
    /// Read the six `SANDME_*` variables from the process environment; the
    /// environment wins over the file (FR-009). Reuses the same parsing
    /// helpers (`env_bool`, `split_list`) the config file's layer implicitly
    /// shares through `toml`'s own string/bool/list handling.
    fn from_environment() -> Self {
        Self {
            shared_paths: env::var("SANDME_SHARED_PATHS").ok().map(|v| split_list(&v)),
            read_only_paths: env::var("SANDME_READ_ONLY_PATHS")
                .ok()
                .map(|v| split_list(&v)),
            proxy: env::var("SANDME_PROXY").ok().map(|v| env_bool(&v)),
            proxy_port: env::var("SANDME_PROXY_PORT")
                .ok()
                .and_then(|v| v.parse().ok()),
            gui_mode: env::var("SANDME_GUI_MODE").ok().map(|v| env_bool(&v)),
            allow_private_egress: env::var("SANDME_ALLOW_PRIVATE_EGRESS")
                .ok()
                .map(|v| env_bool(&v)),
        }
    }
}

impl Config {
    /// Apply a layer onto `self`: each field the layer sets (`Some`)
    /// overrides the current value; a field it says nothing about (`None`)
    /// leaves `self` unchanged. This is the only place precedence lives —
    /// `load_from` calls it once for the file layer, then once more for the
    /// environment layer, so the second call's `Some`s win (FR-009).
    fn merge(mut self, layer: &Layer) -> Self {
        if let Some(shared_paths) = &layer.shared_paths {
            self.shared_paths.clone_from(shared_paths);
        }
        if let Some(read_only_paths) = &layer.read_only_paths {
            self.read_only_paths.clone_from(read_only_paths);
        }
        if let Some(proxy) = layer.proxy {
            self.proxy = proxy;
        }
        if let Some(proxy_port) = layer.proxy_port {
            self.proxy_port = proxy_port;
        }
        if let Some(gui_mode) = layer.gui_mode {
            self.gui_mode = gui_mode;
        }
        if let Some(allow_private_egress) = layer.allow_private_egress {
            self.allow_private_egress = allow_private_egress;
        }
        self
    }
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
/// environment turned it on; `shared_paths`/`read_only_paths` warn only when the
/// environment broadened them beyond the file-or-default baseline (FR-1003,
/// FR-1705). A setting from the config file or left at its default never warns
/// (FR-1002): the environment merges last (FR-009), so `env` alone — whether it
/// set the key, and to what — is enough to decide; the file layer plays no part
/// in that decision and is not a parameter here.
fn widening_warnings(
    config: &Config,
    env: &Layer,
    baseline_shared: &[String],
    baseline_read_only: &[String],
) -> Vec<String> {
    let mut warnings = Vec::new();

    // `proxy` and `proxy_port` are deliberately not checked below. FR-1001 enumerates
    // `shared_paths`, `gui_mode` and `allow_private_egress`, and FR-1705 adds
    // `read_only_paths`, as the closed list of warned settings; `proxy` and `proxy_port`
    // are not on it. `proxy = false` narrows access — no proxy is started and the sandbox
    // grants no egress at all (see the doc comment on `Config::proxy`) — so warning on it
    // would be wrong, and `proxy_port` carries no security concern (SPEC-0010 Non-goals).
    // Every field of `Layer` is now an `Option`, so `env.proxy == Some(true)` type-checks;
    // this comment, not the type, is what keeps that check from being added.

    if config.allow_private_egress && env.allow_private_egress == Some(true) {
        warnings.push(opt_in_warning(
            "allow_private_egress = true",
            "SANDME_ALLOW_PRIVATE_EGRESS",
        ));
    }

    // The `config.gui_mode &&` conjunct does not block the falsey case
    // (`SANDME_GUI_MODE=0` over a file-set `gui_mode = true`) — `Some(false) != Some(true)`
    // does that on its own. The environment merges last (FR-009), so whenever
    // `env.gui_mode == Some(true)` holds, `config.gui_mode` is already `true`: the conjunct
    // is redundant given the merge order, not protective. It stays for readability — "the
    // effective value is the widening one, and the environment carried it" — so do not
    // read it as what blocks `SANDME_GUI_MODE=0`; dropping it would be equally safe today.
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

        // When the environment is applied
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

        // When the environment is applied
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

        // When the widening warnings are computed
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

            // When the environment is applied
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
    fn attributes_each_widening_warning_to_the_layer_that_set_it() {
        // A `WidenFn` sets a widened value for one warned key on a layer; a
        // `type` alias keeps the case table below from tripping
        // clippy::type_complexity.
        type WidenFn = fn(&mut Layer);
        // The layer that results from firing `set` only when `active`, so
        // each case below reads as one call instead of an inline branch.
        fn layer_for(set: WidenFn, active: bool) -> Layer {
            let mut layer = Layer::default();
            if active {
                set(&mut layer);
            }
            layer
        }

        // Given each of the four warned keys (FR-1001, FR-1705), and a way to
        // build a layer that widens that key
        let widen: [(&str, WidenFn); 4] = [
            ("shared_paths", |l| {
                l.shared_paths = Some(vec!["/widened".to_string()]);
            }),
            ("read_only_paths", |l| {
                l.read_only_paths = Some(vec!["/widened-ro".to_string()]);
            }),
            ("gui_mode", |l| l.gui_mode = Some(true)),
            ("allow_private_egress", |l| {
                l.allow_private_egress = Some(true);
            }),
        ];

        // crossed with the three states its provenance can be in: set by the
        // file only, set by the environment only, or set by neither (FR-1004)
        let states = [
            ("file-only", true, false, false),
            ("env-only", false, true, true),
            ("neither", false, false, false),
        ];

        let cases = widen.iter().flat_map(|&(key, set)| {
            states
                .iter()
                .map(move |&(state, fw, ew, warn)| (key, set, state, fw, ew, warn))
        });

        for (key, set, state, file_widened, env_widened, expect_warning) in cases {
            // When the file layer is merged first and the FR-1003 baseline is
            // taken off that intermediate value, exactly as load_from does,
            // then the environment layer is merged on top
            let file_merged = Config::default().merge(&layer_for(set, file_widened));
            let baseline_shared = file_merged.shared_paths.clone();
            let baseline_read_only = file_merged.read_only_paths.clone();
            let env = layer_for(set, env_widened);
            let config = file_merged.merge(&env);
            let warnings = widening_warnings(&config, &env, &baseline_shared, &baseline_read_only);

            // Then a warning names this key exactly when the environment —
            // not the file — is the layer that set it
            let warned = warnings.iter().any(|warning| warning.contains(key));
            assert_eq!(
                warned, expect_warning,
                "key={key} state={state} got={warnings:?}"
            );
        }
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

        // When the widening warnings are computed
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
    fn warns_when_the_environment_enables_gui_mode() {
        // Given gui_mode turned on by the environment alone
        let config = Config {
            gui_mode: true,
            ..Config::default()
        };
        let env = Layer {
            gui_mode: Some(true),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, &env, &config.shared_paths, &config.read_only_paths);

        // Then one names both the setting and the variable that carried it
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("gui_mode") && warning.contains("SANDME_GUI_MODE")),
            "got: {warnings:?}"
        );
    }

    #[test]
    fn stays_silent_when_the_environment_disables_gui_mode() {
        // Given SANDME_GUI_MODE=0 over a file-set gui_mode = true: the
        // environment is present (env_set) but its falsey value leaves the
        // effective config off (FR-1002's falsey-env clause)
        let config = Config {
            gui_mode: false,
            ..Config::default()
        };
        let env = Layer {
            gui_mode: Some(false),
            ..Layer::default()
        };

        // When the widening warnings are computed
        let warnings =
            widening_warnings(&config, &env, &config.shared_paths, &config.read_only_paths);

        // Then nothing is warned: the effective value is not the widening one
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

    #[test]
    #[serial_test::serial]
    fn stays_silent_when_the_environment_sets_proxy_or_proxy_port() {
        // Given an injected home with no config file, and both proxy settings
        // supplied by the environment. The `SANDME_*` variables are
        // process-wide; this test owns them (hence `#[serial]`). HOME is
        // injected by argument, not mutated.
        let dir = std::env::temp_dir().join("sandme-test-load-silent-proxy");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
            std::env::remove_var("SANDME_READ_ONLY_PATHS");
            std::env::remove_var("SANDME_GUI_MODE");
            std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS");
            std::env::set_var("SANDME_PROXY", "1");
            std::env::set_var("SANDME_PROXY_PORT", "9999");
        }

        // When the configuration is loaded for that home
        let loaded = load_from(Some(dir.as_path())).unwrap();

        // Then neither setting raises a warning: proxy narrows rather than
        // widens (src/config.rs:36-44) and proxy_port carries no security
        // concern (SPEC-0010 Non-goals) — neither carries a provenance bit
        // today.
        assert!(loaded.warnings.is_empty(), "got: {:?}", loaded.warnings);

        unsafe {
            std::env::remove_var("SANDME_PROXY");
            std::env::remove_var("SANDME_PROXY_PORT");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[serial_test::serial]
    fn load_warns_when_the_environment_broadens_shared_paths_beyond_the_file_baseline() {
        // Given a config file confining shared_paths to a project directory,
        // and the environment broadening it to "/". Nothing today exercises
        // the baseline load_from itself computes: the existing broadening
        // unit tests take baseline_shared/baseline_read_only in as explicit
        // arguments and never touch the line that decides what the baseline
        // *is*. The `SANDME_*` variables are process-wide; this test owns
        // them (hence `#[serial]`). HOME is injected by argument, not
        // mutated.
        let dir = std::env::temp_dir().join("sandme-test-load-warns-shared-paths-baseline");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".sandme")).unwrap();
        let project = dir.join("project");
        std::fs::write(
            dir.join(".sandme/config.toml"),
            format!("shared_paths = [{:?}]\n", project.to_string_lossy()),
        )
        .unwrap();
        unsafe {
            std::env::remove_var("SANDME_READ_ONLY_PATHS");
            std::env::remove_var("SANDME_GUI_MODE");
            std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS");
            std::env::remove_var("SANDME_PROXY_PORT");
            std::env::set_var("SANDME_SHARED_PATHS", "/");
        }

        // When the configuration is loaded for that home
        let loaded = load_from(Some(dir.as_path())).unwrap();

        // Then a warning names shared_paths as broadened by the environment
        // beyond the file-or-default baseline (FR-1003)
        assert!(
            loaded
                .warnings
                .iter()
                .any(|warning| warning.contains("shared_paths")
                    && warning.contains("SANDME_SHARED_PATHS")),
            "got: {:?}",
            loaded.warnings
        );

        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[serial_test::serial]
    fn load_stays_silent_when_the_environment_value_stays_within_the_file_baseline() {
        // Given the same file-confined baseline, but the environment value
        // stays inside it — a subdirectory, not a broader path. The
        // `SANDME_*` variables are process-wide; this test owns them (hence
        // `#[serial]`). HOME is injected by argument, not mutated.
        let dir = std::env::temp_dir().join("sandme-test-load-silent-shared-paths-baseline");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".sandme")).unwrap();
        let project = dir.join("project");
        let subdir = project.join("src");
        std::fs::write(
            dir.join(".sandme/config.toml"),
            format!("shared_paths = [{:?}]\n", project.to_string_lossy()),
        )
        .unwrap();
        unsafe {
            std::env::remove_var("SANDME_READ_ONLY_PATHS");
            std::env::remove_var("SANDME_GUI_MODE");
            std::env::remove_var("SANDME_ALLOW_PRIVATE_EGRESS");
            std::env::remove_var("SANDME_PROXY_PORT");
            std::env::set_var("SANDME_SHARED_PATHS", &subdir);
        }

        // When the configuration is loaded for that home
        let loaded = load_from(Some(dir.as_path())).unwrap();

        // Then nothing is warned: the environment narrowed rather than
        // broadened the file baseline (FR-1003)
        assert!(loaded.warnings.is_empty(), "got: {:?}", loaded.warnings);

        unsafe {
            std::env::remove_var("SANDME_SHARED_PATHS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
