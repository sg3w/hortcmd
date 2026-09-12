// ============================================================
// Where hortcmd keeps its own files.
//
// Three modules used to spell this out identically — the connection
// profiles, the credential configuration and the accepted host keys all
// live in `<config>/hortcmd/`. One place now answers the question, which
// also gives the protocol harness (TICKET-038) somewhere to point: a test
// run that wrote its accepted host keys into the developer's real store
// would change the machine it was supposed to only observe.
// ============================================================

use crate::errors::AppResult;
use std::path::PathBuf;

/// The environment variable that redirects the whole directory.
///
/// Set by the integration harness, and usable for a portable install.
/// It is read on every call rather than cached: a test that changes it
/// between cases should see the change.
const OVERRIDE: &str = "HORTCMD_CONFIG_DIR";

/// `<config>/hortcmd`, or whatever `HORTCMD_CONFIG_DIR` names.
///
/// The directory is not created here — the callers that write already
/// create it, and the callers that only read must not.
pub fn dir() -> AppResult<PathBuf> {
    resolve(std::env::var_os(OVERRIDE))
}

/// The decision itself, with the environment passed in.
///
/// Tests exercise this rather than the variable: mutating the process
/// environment in one test changes where a test running beside it looks
/// for its files, which is a race nobody should have to debug twice.
fn resolve(override_value: Option<std::ffi::OsString>) -> AppResult<PathBuf> {
    if let Some(custom) = override_value {
        if !custom.is_empty() {
            return Ok(PathBuf::from(custom));
        }
    }
    Ok(dirs::config_dir()
        .ok_or_else(|| "Kein Konfigurationsverzeichnis gefunden".to_string())?
        .join("hortcmd"))
}

/// A file inside [`dir`].
pub fn path(file: &str) -> AppResult<PathBuf> {
    Ok(dir()?.join(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn an_override_replaces_the_platform_directory() {
        assert_eq!(
            resolve(Some(OsString::from("/tmp/hortcmd-test-config"))).unwrap(),
            PathBuf::from("/tmp/hortcmd-test-config")
        );
    }

    /// An empty value is no value. Honouring it would resolve the whole
    /// configuration to the process's current directory, which is how a
    /// stray `HORTCMD_CONFIG_DIR=` in a shell profile would silently
    /// hide every saved connection.
    #[test]
    fn an_empty_or_absent_override_falls_back() {
        for value in [None, Some(OsString::new())] {
            let resolved = resolve(value).unwrap();
            assert!(
                resolved.ends_with("hortcmd"),
                "unexpected fallback: {}",
                resolved.display()
            );
            assert!(resolved.is_absolute(), "the fallback must be absolute");
        }
    }

    /// The real entry points must agree with the resolver, so a future
    /// caller cannot quietly go around the override.
    #[test]
    fn path_sits_inside_dir() {
        assert_eq!(
            path("known_hosts").unwrap(),
            dir().unwrap().join("known_hosts")
        );
    }
}
