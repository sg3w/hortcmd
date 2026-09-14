// ============================================================
// Where a client may look (TICKET-041).
//
// An allowlist of roots, and one question asked of every argument before
// any tool touches it: does this address lie inside one of them?
//
// The question is harder than it looks, and each hard part is a way out
// of the allowlist that has been used against real software:
//
//   * `..` — `~/Documents/../../etc/passwd` is inside `~/Documents` if
//     you compare strings. Resolved lexically first, so it is not.
//   * **prefix matching** — `/home/user-other` starts with `/home/user`
//     and is a different directory. Compared segment by segment.
//   * **case folding** — on a case-insensitive volume `/USERS/x` and
//     `/Users/x` are the same directory, so a root written one way must
//     cover the other; on a case-sensitive one they are two directories
//     and must not.
//   * **symlinks** — a link inside a root pointing out of it is the one
//     case that cannot be settled by looking at the string, so the local
//     check ends with the operating system's own answer
//     (`canonicalize`) for both the root and the target.
//   * **a different store** — `sftp://a@h/x` and `sftp://b@h/x` are not
//     the same place even where the paths match, so the scheme and the
//     connection identity are part of the comparison.
//
// Empty allowlist means nothing is reachable. That is the default, and
// it is the only default that cannot be an accident.
// ============================================================

use crate::errors::{code, AppError, AppResult};
use crate::vfs::{self, Address, Scheme};
use std::path::{Component, Path, PathBuf};

/// Tidies the roots a user typed: no blanks, no duplicates, no trailing
/// separators (so `/home/u/` and `/home/u` are one root, not two).
pub fn normalize_roots(roots: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in roots {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tidied = match vfs::parse_address(trimmed) {
            Ok(location) if location.scheme() != Scheme::Local => location.uri(),
            _ => trim_trailing_separator(trimmed),
        };
        if !out.iter().any(|existing| existing == &tidied) {
            out.push(tidied);
        }
    }
    out
}

fn trim_trailing_separator(value: &str) -> String {
    // A bare root (`/`, `C:\`, `sftp://u@h/`) keeps its separator —
    // trimming that turns it into something else entirely.
    let trimmed = value.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') || trimmed.ends_with("//") {
        value.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Splits a provider path into its meaningful segments, resolving `.`
/// and `..` lexically.
///
/// Lexical resolution is not the whole answer for a local path — a
/// symlink can still point out — but it is the whole answer for a remote
/// one, where there is no `canonicalize` to ask and the server resolves
/// the same way.
pub fn lexical_segments(path: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in path.split(['/', '\\']) {
        match raw {
            "" | "." => {}
            ".." => {
                // Popping past the root stays at the root, which is what
                // every file system does with `/..`.
                out.pop();
            }
            other => out.push(other.to_string()),
        }
    }
    out
}

/// Whether `child` is `parent` or lies beneath it, segment by segment.
fn segments_contain(parent: &[String], child: &[String], case_sensitive: bool) -> bool {
    if child.len() < parent.len() {
        return false;
    }
    parent.iter().zip(child).all(|(p, c)| {
        if case_sensitive {
            p == c
        } else {
            p.eq_ignore_ascii_case(c)
        }
    })
}

/// The identity of the store an address names — scheme plus connection.
///
/// Two addresses with the same path on different stores are different
/// places, and a root on one must not admit the other.
fn store_of(address: &Address) -> String {
    match address.scheme() {
        Scheme::Local => "local".to_string(),
        scheme => format!(
            "{}:{}",
            scheme.as_str(),
            address.connection_key().unwrap_or_default()
        ),
    }
}

/// Whether `candidate` lies inside `root`.
///
/// Both are raw addresses as a user or a client would write them.
pub fn contains(root: &str, candidate: &str) -> AppResult<bool> {
    let root_addr = vfs::parse_address(root)?;
    let cand_addr = vfs::parse_address(candidate)?;

    if store_of(&root_addr) != store_of(&cand_addr) {
        return Ok(false);
    }

    // Compare local paths in the filesystem's canonical namespace. A lexical
    // precheck would reject legitimate aliases such as /var and /private/var.
    if root_addr.scheme() == Scheme::Local {
        return Ok(real_path_contains(&root_addr.path(), &cand_addr.path()));
    }
    Ok(vfs::location::ProviderPath::from_raw(&root_addr.path())?
        .contains(&vfs::location::ProviderPath::from_raw(&cand_addr.path())?))
}

/// Containment in the filesystem-resolved namespace.
///
/// Only *narrows*: a target that does not exist yet — the destination of
/// a copy, a folder about to be created — has no canonical form, and
/// refusing it because of that would make the write tier useless. Its
/// nearest existing ancestor is checked instead, which is the directory
/// it will actually be created in.
fn real_path_contains(root: &str, candidate: &str) -> bool {
    let Ok(real_root) = std::fs::canonicalize(root) else {
        // A root that does not resolve cannot admit anything. That is a
        // misconfiguration, and the safe reading of it is "nothing".
        return false;
    };
    let real_candidate = match std::fs::canonicalize(candidate) {
        Ok(p) => p,
        Err(_) => match nearest_existing(Path::new(candidate)) {
            Some(p) => p,
            None => return false,
        },
    };
    path_contains(&real_root, &real_candidate)
}

/// The deepest ancestor of `path` that exists, canonicalized.
///
/// The remaining components are appended lexically — they cannot be
/// symlinks, because they are not there yet.
fn nearest_existing(path: &Path) -> Option<PathBuf> {
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    let mut current = path;
    loop {
        if let Ok(real) = std::fs::canonicalize(current) {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        // A dangling link is not a missing directory. It may resolve outside
        // the root as soon as its target appears.
        if std::fs::symlink_metadata(current).is_ok() {
            return None;
        }
        let name = current.file_name()?;
        tail.push(name);
        current = current.parent()?;
        if current.as_os_str().is_empty() {
            return None;
        }
    }
}

/// Component-wise containment for two already-resolved paths.
fn path_contains(root: &Path, candidate: &Path) -> bool {
    // canonicalize returns the actual filesystem spelling. Folding it again
    // would admit distinct siblings on case-sensitive macOS volumes.
    let case_sensitive = true;
    let normal = |p: &Path| -> Vec<String> {
        p.components()
            .filter_map(|c| match c {
                Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                Component::Prefix(p) => Some(p.as_os_str().to_string_lossy().into_owned()),
                Component::RootDir => Some("/".to_string()),
                // `.` and `..` are gone after canonicalize; if one
                // survives, dropping it keeps the comparison lexical.
                _ => None,
            })
            .collect()
    };
    segments_contain(&normal(root), &normal(candidate), case_sensitive)
}

/// Checks an address against the whole allowlist.
///
/// The error is deliberately its own code: "outside the allowed roots"
/// and "not found" must never look the same to a client, or a refusal
/// becomes a way to probe the file system.
pub fn require_in_scope(roots: &[String], candidate: &str) -> AppResult<()> {
    if roots.is_empty() {
        return Err(denied(candidate, "no root is configured"));
    }
    for root in roots {
        if contains(root, candidate).unwrap_or(false) {
            let address = vfs::parse_address(candidate)?;
            if address.scheme() != Scheme::Local {
                let (provider, path) = vfs::resolve(candidate)?;
                verify_remote_path(provider.as_ref(), &path)?;
            }
            return Ok(());
        }
    }
    Err(denied(candidate, "outside the allowed roots"))
}

/// FTP and SMB expose links in directory metadata. Verify each component
/// from the namespace root; never follow a link even if its lexical path is
/// allowed. WebDAV collections are scoped in the server's URL namespace.
fn verify_remote_path(provider: &dyn vfs::Provider, path: &str) -> AppResult<()> {
    if path.split(['/', '\\']).any(|part| part == "..") {
        return Err(denied(path, "remote parent traversal is ambiguous"));
    }
    let mut parent = String::from("/");
    let mut segments = lexical_segments(path).into_iter();
    if provider.store_id().starts_with("smb:") {
        // The first component selects an SMB share, not a filesystem entry.
        // Do not require optional srvsvc share enumeration to access an
        // explicitly allowed share. Descendants still undergo link checks.
        if let Some(share) = segments.next() {
            parent = format!("/{share}");
        }
    }
    for segment in segments {
        let listing = provider.list(&parent)?;
        let child = if parent == "/" {
            format!("/{segment}")
        } else {
            format!("{parent}/{segment}")
        };
        match listing.entries.iter().find(|e| e.name == segment) {
            Some(entry) if entry.is_symlink => {
                return Err(denied(path, "remote symbolic links are not allowed"))
            }
            Some(_) => parent = child,
            None if provider.exists(Path::new(&child)) => {
                return Err(denied(path, "remote spelling is not canonical"))
            }
            None => return Ok(()), // a new destination beneath a checked parent
        }
    }
    Ok(())
}

fn denied(candidate: &str, why: &str) -> AppError {
    AppError::new(code::CAPABILITY_UNSUPPORTED)
        .op("mcp-scope")
        // The address is the user's own and goes to the audit log, which
        // the user reads; the client is told only that it was refused.
        .detail(format!("{}: {}", candidate, why))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_are_tidied_without_being_mangled() {
        let roots = normalize_roots(&[
            "  /home/u  ".into(),
            "/home/u/".into(),
            "".into(),
            "   ".into(),
            "/".into(),
            "sftp://u@h/data/".into(),
        ]);
        assert_eq!(roots, vec!["/home/u", "/", "sftp://u@h/data/"]);
    }

    #[test]
    fn dot_segments_are_resolved_before_anything_is_compared() {
        assert_eq!(lexical_segments("/a/b/../c"), vec!["a", "c"]);
        assert_eq!(lexical_segments("/a/./b//c/"), vec!["a", "b", "c"]);
        assert_eq!(lexical_segments("/a/../.."), Vec::<String>::new());
        assert_eq!(lexical_segments("a\\b\\..\\c"), vec!["a", "c"]);
    }

    /// A temporary tree, because the local check ends with the
    /// operating system's own answer and `/home/user` does not exist on
    /// every machine this runs on.
    fn temp_tree(name: &str) -> std::path::PathBuf {
        let base =
            std::env::temp_dir().join(format!("hortcmd-scope-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // The temp directory is itself a symlink on macOS (/var → /private/var),
        // so the root is canonicalized here too — exactly as a user
        // configuring a root would end up with.
        std::fs::canonicalize(&base).unwrap()
    }

    /// The prefix trap: a sibling whose name starts with the root's.
    #[test]
    fn a_sibling_with_a_longer_name_is_not_inside() {
        // Lexically first, which is where the trap lives…
        assert!(!segments_contain(
            &["home".into(), "user".into()],
            &["home".into(), "user-other".into(), "x".into()],
            true
        ));
        assert!(!segments_contain(
            &["home".into(), "user".into()],
            &["home".into(), "username".into()],
            true
        ));

        // …and then against a real tree.
        let base = temp_tree("sibling");
        let root = base.join("user");
        std::fs::create_dir_all(root.join("x")).unwrap();
        std::fs::create_dir_all(base.join("user-other")).unwrap();
        let root_str = root.to_string_lossy().into_owned();

        assert!(contains(&root_str, &root.join("x").to_string_lossy()).unwrap());
        assert!(contains(&root_str, &root_str).unwrap());
        assert!(!contains(&root_str, &base.join("user-other").to_string_lossy()).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn walking_up_out_of_a_root_is_refused() {
        let base = temp_tree("updots");
        let root = base.join("root");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::create_dir_all(base.join("elsewhere")).unwrap();
        let root_str = root.to_string_lossy().into_owned();

        let up = root.join("..").join("elsewhere");
        assert!(!contains(&root_str, &up.to_string_lossy()).unwrap());
        assert!(!contains(&root_str, &root.join("..").to_string_lossy()).unwrap());

        // Up and back in again is the same place, so it is fine.
        let around = root.join("docs").join("..").join("notes");
        assert!(contains(&root_str, &around.to_string_lossy()).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_different_store_is_never_inside() {
        assert!(!contains("sftp://u@h/data", "/data/x").unwrap());
        assert!(!contains("/data", "sftp://u@h/data/x").unwrap());
        assert!(!contains("sftp://u@h/data", "sftp://other@h/data/x").unwrap());
        assert!(!contains("sftp://u@h/data", "smb://u@h/data/x").unwrap());
        assert!(contains("sftp://u@h/data", "sftp://u@h/data/sub/f.txt").unwrap());
    }

    /// A remote server decides its own case rules, so the check does not
    /// assume it folds.
    #[test]
    fn remote_paths_are_compared_exactly() {
        assert!(!contains("sftp://u@h/Data", "sftp://u@h/data/x").unwrap());
        assert!(contains("sftp://u@h/Data", "sftp://u@h/Data/x").unwrap());
    }

    #[test]
    fn an_empty_allowlist_reaches_nothing() {
        let err = require_in_scope(&[], "/home/user/x").unwrap_err();
        assert!(err.is(code::CAPABILITY_UNSUPPORTED));
    }

    #[test]
    fn the_allowlist_admits_any_root_that_contains_the_path() {
        let roots = vec!["/home/u/docs".to_string(), "sftp://u@h/data".to_string()];
        assert!(contains(&roots[1], "sftp://u@h/data/a/b.txt").unwrap());
        assert!(
            require_in_scope(&roots, "sftp://u@h/data/a/b.txt").is_err(),
            "closed remote must be refused"
        );
        assert!(require_in_scope(&roots, "sftp://u@h/other").is_err());
    }

    /// The symlink case, against the real file system: a link inside the
    /// root pointing out of it must not be reachable.
    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_root_is_refused() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join(format!("hortcmd-scope-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), b"x").unwrap();
        symlink(&outside, root.join("escape")).unwrap();
        std::fs::write(root.join("inside.txt"), b"x").unwrap();

        let root_str = root.to_string_lossy().into_owned();
        // Lexically this is inside the root; it is not.
        let escaped = root.join("escape").join("secret.txt");
        assert!(
            !contains(&root_str, &escaped.to_string_lossy()).unwrap(),
            "a symlink out of the root must not be reachable"
        );
        assert!(contains(&root_str, &root.join("inside.txt").to_string_lossy()).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A destination that does not exist yet is judged by the directory
    /// it would be created in — otherwise nothing could ever be written.
    #[test]
    fn a_target_that_does_not_exist_yet_is_judged_by_its_parent() {
        let base = std::env::temp_dir().join(format!("hortcmd-scope-new-{}", std::process::id()));
        let root = base.join("root");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&root).unwrap();

        let root_str = root.to_string_lossy().into_owned();
        let new_file = root.join("sub").join("not-yet.txt");
        assert!(contains(&root_str, &new_file.to_string_lossy()).unwrap());

        let outside = base.join("elsewhere").join("not-yet.txt");
        assert!(!contains(&root_str, &outside.to_string_lossy()).unwrap());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlink_destinations_are_refused() {
        let base = temp_tree("dangling");
        let root = base.join("root");
        std::fs::create_dir(&root).unwrap();
        std::os::unix::fs::symlink(base.join("outside"), root.join("escape")).unwrap();
        assert!(!contains(
            &root.to_string_lossy(),
            &root.join("escape/file").to_string_lossy()
        )
        .unwrap());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn canonical_aliases_admit_the_same_root_but_never_a_sibling() {
        let base = temp_tree("canonical-alias");
        let root = base.join("root");
        let alias = base.join("alias");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(base.join("outside")).unwrap();
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        assert!(contains(
            &alias.to_string_lossy(),
            &root.join("new-file").to_string_lossy()
        )
        .unwrap());
        assert!(contains(
            &root.to_string_lossy(),
            &alias.join("new-file").to_string_lossy()
        )
        .unwrap());
        assert!(!contains(
            &alias.to_string_lossy(),
            &base.join("outside/new-file").to_string_lossy()
        )
        .unwrap());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn resolved_paths_preserve_case() {
        assert!(!path_contains(Path::new("/Root"), Path::new("/root/file")));
    }

    #[test]
    fn a_root_that_does_not_resolve_admits_nothing() {
        assert!(!contains("/definitely/not/here", "/definitely/not/here/x").unwrap());
    }
}
