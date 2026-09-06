//! Where a command word points on disk, resolved the way `execvp` resolves it.

use std::path::{Path, PathBuf};

use crate::error::SandmeError;

/// Find `program` on disk the way the child would, with symlinks resolved.
///
/// A bare command name is looked up on `PATH` — the child inherits sandme's
/// environment, so sandme's `PATH` is the child's. Canonicalizing the name
/// directly resolves it against sandme's working directory instead and finds
/// nothing (issue #13).
pub fn locate(program: &str) -> Option<PathBuf> {
    candidates(program)
        .into_iter()
        .find(|candidate| is_executable(candidate))
        .and_then(|candidate| std::fs::canonicalize(candidate).ok())
}

/// Why `program` could not be executed, or `None` if it looks executable.
///
/// Answers the question `sandbox-exec` refuses to answer. It reports every
/// `execvp` failure as one status and names the cause only in prose on the
/// child's stderr, so sandme works the cause out from the filesystem instead:
/// the child's streams are the child's, and parsing them would make sandme's
/// exit status depend on another tool's wording (NFR-201).
///
/// `None` covers both "this is runnable" and "this is unrunnable for a reason
/// sandme cannot see" — an unknown interpreter after `#!`, a binary for
/// another architecture. Those keep `sandbox-exec`'s own status: a wrong
/// answer is worse than an opaque one (FR-203, FR-204).
pub fn exec_failure(program: &str) -> Option<SandmeError> {
    let mut found = false;
    for candidate in candidates(program) {
        if is_executable(&candidate) {
            return None;
        }
        found |= candidate.exists();
    }

    let command = program.to_string();
    Some(if found {
        SandmeError::CommandNotExecutable { command }
    } else {
        SandmeError::CommandNotFound { command }
    })
}

/// The paths `execvp` would try for `program`, in the order it tries them.
///
/// A word containing a separator names a file directly. A bare word is looked
/// up in each `PATH` entry in turn; `execvp` keeps searching past an entry it
/// may not execute, so the first *executable* candidate wins, not the first
/// that exists.
fn candidates(program: &str) -> Vec<PathBuf> {
    if program.contains('/') {
        return vec![PathBuf::from(program)];
    }
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .collect()
}

/// Whether `path` is a file the current user could exec.
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locates_a_bare_name_on_path() {
        // `sh` is on PATH on every macOS host; the point is that a bare name
        // resolves at all, which canonicalizing against the CWD never did.
        assert!(locate("sh").is_some());
        assert_eq!(locate("sandme-no-such-command"), None);
    }

    #[test]
    fn reports_a_name_nothing_answers_to() {
        assert!(matches!(
            exec_failure("sandme-no-such-command"),
            Some(SandmeError::CommandNotFound { .. })
        ));
        assert!(matches!(
            exec_failure("/sandme-no-such-command"),
            Some(SandmeError::CommandNotFound { .. })
        ));
    }

    #[test]
    fn reports_a_path_that_is_not_executable() {
        // /etc/hosts is present on every macOS host and carries no execute bit.
        assert!(matches!(
            exec_failure("/etc/hosts"),
            Some(SandmeError::CommandNotExecutable { .. })
        ));
        // A directory is "found but not executable" too: that is what the
        // shell reports for it, and what execvp's EACCES means here.
        assert!(matches!(
            exec_failure("/etc"),
            Some(SandmeError::CommandNotExecutable { .. })
        ));
    }

    #[test]
    fn reports_no_failure_for_an_executable() {
        // Both spellings a user can write, and nothing but the filesystem is
        // consulted to answer either (NFR-201).
        assert!(exec_failure("sh").is_none());
        assert!(exec_failure("/bin/sh").is_none());
    }
}
