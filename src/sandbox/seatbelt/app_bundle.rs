//! Redirection of a macOS app-bundle CLI wrapper to the bundle's own executable.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::executable;

/// What sandme should run in place of the program the user named.
///
/// The three cases are told apart because they need different treatment:
/// the first is the common path and stays silent, the second rewrites the
/// program, and the third is a bundle sandme recognised but could not read
/// through — a case the user has to hear about (see [`main_executable`]).
enum Resolution {
    /// Run the program exactly as the user wrote it.
    AsWritten,
    /// Run this bundle executable instead of the wrapper.
    MainExecutable(PathBuf),
    /// The program lives in an app bundle whose executable could not be found.
    Unreadable {
        /// The `.app` directory the program lives in.
        bundle: PathBuf,
        /// Why the bundle's executable could not be determined.
        reason: &'static str,
    },
}

/// The executable to run in place of `program`, or `None` to run it as written.
///
/// macOS ships IDEs as app bundles with a small CLI wrapper on `PATH`
/// (`/usr/local/bin/zed` → `Zed.app/Contents/MacOS/cli`) that asks
/// `LaunchServices` to open the app. `LaunchServices` is denied inside the
/// sandbox, so the wrapper fails with the app's own `cannot start app bundle`;
/// running the bundle's main executable directly works. This translates the
/// one into the other.
///
/// A bundle sandme recognises but cannot read through is reported on stderr
/// (`docs/guidelines/architecture/posix.md` §3) rather than passed along
/// silently: otherwise the user sees only the app's failure, with no sign
/// that sandme looked at all.
pub fn main_executable(program: &str) -> Option<String> {
    match resolve(program) {
        Resolution::AsWritten => None,
        Resolution::MainExecutable(path) => Some(path.to_string_lossy().into_owned()),
        Resolution::Unreadable { bundle, reason } => {
            eprintln!(
                "sandme: {program} is the CLI wrapper of the app bundle {}, but {reason}; \
                 running it unchanged, which macOS may refuse inside the sandbox",
                bundle.display()
            );
            None
        }
    }
}

/// Decide what `program` should become.
fn resolve(program: &str) -> Resolution {
    let Some(located) = executable::locate(program) else {
        return Resolution::AsWritten;
    };
    let Some(bundle) = bundle_root(&located) else {
        return Resolution::AsWritten;
    };

    let Ok(plist) = std::fs::read_to_string(bundle.join("Contents/Info.plist")) else {
        return unreadable(bundle, "its Contents/Info.plist could not be read");
    };
    let Some(name) = executable_name(&plist) else {
        return unreadable(bundle, "its Info.plist names no usable CFBundleExecutable");
    };

    let main = bundle.join("Contents/MacOS").join(name);
    if !main.is_file() {
        return unreadable(bundle, "the executable its Info.plist names does not exist");
    }
    if main == located {
        // Already the bundle's own executable: nothing to redirect.
        return Resolution::AsWritten;
    }
    Resolution::MainExecutable(main)
}

/// Name the bundle sandme could not read through.
fn unreadable(bundle: &Path, reason: &'static str) -> Resolution {
    Resolution::Unreadable {
        bundle: bundle.to_path_buf(),
        reason,
    }
}

/// The `.app` directory `path` sits inside, if it sits inside one.
fn bundle_root(path: &Path) -> Option<&Path> {
    let contents = path
        .ancestors()
        .find(|ancestor| ancestor.file_name() == Some(OsStr::new("Contents")))?;
    let bundle = contents.parent()?;
    (bundle.extension() == Some(OsStr::new("app"))).then_some(bundle)
}

/// The `CFBundleExecutable` value of an `Info.plist`, as a bare file name.
///
/// A plist is XML, but sandme needs exactly one string from it, and the key is
/// written the same way by every bundle macOS builds. A name containing a
/// separator is rejected: the plist belongs to whatever the user is launching,
/// and a `../` in it would send sandme outside the bundle it just identified.
///
/// Only the XML `Info.plist` form is parsed; a binary `Info.plist` is not
/// decoded, and no `plist` dependency is added to decode one: a plist is read
/// only for an on-`PATH` wrapper that canonicalizes into a `.app` bundle, and
/// every wrapper observed to do so ships an XML plist, so the binary form is
/// anticipation rather than evidence. A bundle that ships one falls to
/// [`Resolution::Unreadable`] — sandme warns and runs the wrapper unchanged
/// (SPEC-0014/FR-1403).
fn executable_name(plist: &str) -> Option<&str> {
    let after_key = plist.split_once("<key>CFBundleExecutable</key>")?.1;
    let value = after_key.split_once("<string>")?.1;
    let name = value.split_once("</string>")?.0.trim();
    (!name.is_empty() && !name.contains('/')).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plist_naming(executable: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <plist version=\"1.0\"><dict>\n\
             \t<key>CFBundleName</key>\n\t<string>Fake</string>\n\
             \t<key>CFBundleExecutable</key>\n\t<string>{executable}</string>\n\
             </dict></plist>\n"
        )
    }

    #[test]
    fn reads_the_executable_name_from_a_plist() {
        assert_eq!(executable_name(&plist_naming("zed")), Some("zed"));
    }

    #[test]
    fn ignores_a_plist_without_the_key() {
        let plist = "<plist><dict><key>CFBundleName</key><string>Fake</string></dict></plist>";
        assert_eq!(executable_name(plist), None);
    }

    #[test]
    fn ignores_an_empty_executable_name() {
        assert_eq!(executable_name(&plist_naming("  ")), None);
    }

    #[test]
    fn ignores_an_executable_name_that_escapes_the_bundle() {
        assert_eq!(executable_name(&plist_naming("../../../bin/sh")), None);
    }

    #[test]
    fn finds_the_bundle_a_wrapper_lives_in() {
        let wrapper = Path::new("/Applications/Zed.app/Contents/MacOS/cli");
        assert_eq!(
            bundle_root(wrapper),
            Some(Path::new("/Applications/Zed.app"))
        );
    }

    #[test]
    fn finds_no_bundle_outside_one() {
        assert_eq!(bundle_root(Path::new("/usr/bin/vim")), None);
        assert_eq!(bundle_root(Path::new("/opt/Contents/MacOS/x")), None);
    }
}
