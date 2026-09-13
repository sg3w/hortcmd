//! One namespace for subscriptions and native notifications. Remote addresses
//! are parsed but never handed to std::fs or notify.
use crate::errors::AppResult;
use crate::vfs::{self, Scheme};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Location {
    pub key: String,
    pub scheme: Scheme,
    pub native: Option<PathBuf>,
}

pub fn resolve(raw: &str) -> AppResult<Location> {
    let address = vfs::parse_address(raw)?;
    if address.scheme == Scheme::Local {
        let path = std::fs::canonicalize(&address.path)?;
        if !path.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "watch location is not a directory",
            )
            .into());
        }
        return Ok(Location {
            key: path.to_string_lossy().into_owned(),
            scheme: address.scheme,
            native: Some(path),
        });
    }
    // Preserve remote path case and dot components: server-side symlinks mean
    // lexical normalization could silently select a different directory.
    Ok(Location {
        key: format!(
            "{}:{}\0{}",
            address.scheme.as_str(),
            address.connection_key().unwrap_or_default(),
            address.path
        ),
        scheme: address.scheme,
        native: None,
    })
}

fn canonical_event(path: &Path) -> PathBuf {
    // Normalize the parent, not the entry: following a changed symlink would
    // turn an in-directory notification into an unrelated target path.
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => std::fs::canonicalize(parent)
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
    }
}

pub fn matches(root: &Path, event_path: &Path) -> bool {
    let path = canonical_event(event_path);
    path == root || path.parent() == Some(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_schemes_never_resolve_to_native_watches() {
        for raw in [
            "sftp://host/目录",
            "ftp://host/目录",
            "smb://host/share/目录",
            "webdav://host/目录",
        ] {
            assert!(resolve(raw).unwrap().native.is_none());
        }
        assert_ne!(
            resolve("sftp://host/Case").unwrap().key,
            resolve("sftp://host/case").unwrap().key
        );
        assert_eq!(
            resolve("sftp://host/dir").unwrap().key,
            resolve("sftp://host:22/dir").unwrap().key
        );
    }

    #[test]
    fn local_aliases_unicode_deleted_children_and_siblings_match_correctly() {
        let base = std::env::temp_dir().join(format!(
            "hortcmd-refresh-{}",
            crate::mcp::auth::random_hex(12).unwrap()
        ));
        std::fs::create_dir(&base).unwrap();
        let root = base.join("目录");
        std::fs::create_dir(&root).unwrap();
        let location = resolve(root.to_str().unwrap()).unwrap();
        let canonical = location.native.unwrap();
        assert!(matches(&canonical, &root.join("deleted-file")));
        assert!(matches(&canonical, &root));
        assert!(!matches(&canonical, &base.join("目录-other/deleted")));
        assert!(!matches(&canonical, &root.join("nested/file")));
        let file_url = format!("file://{}", root.display());
        assert_eq!(resolve(&file_url).unwrap().key, location.key);
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(&base, &link).unwrap();
            assert!(matches(&canonical, &link));
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_root_and_unc_paths_keep_their_namespace() {
        assert!(matches(Path::new(r"C:\"), Path::new(r"C:\deleted")));
        assert!(matches(
            Path::new(r"\\server\share"),
            Path::new(r"\\server\share\deleted")
        ));
        assert!(!matches(
            Path::new(r"\\server\share"),
            Path::new(r"\\server\other\deleted")
        ));
    }
}
