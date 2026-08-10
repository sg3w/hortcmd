// ============================================================
// Local file-system provider.
//
// Thin, idiomatic wrappers over `std::fs` implementing the VFS
// `Provider` trait. This is where the previous `commands/fs/*`
// local logic now lives; the behavior is deliberately identical to
// before so local paths keep working unchanged.
// ============================================================

use super::{Provider, Scheme, Stat};
use crate::commands::fs::attrs;
use crate::commands::fs::dir::{DirEntry, DirListing};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Provider for the local file system (`local` / `file://`).
pub struct LocalProvider;

impl Provider for LocalProvider {
    fn scheme(&self) -> Scheme {
        Scheme::Local
    }

    /// Reads the contents of a directory. Mirrors the former
    /// `list_dir` command verbatim (canonicalize + per-entry attrs).
    fn list(&self, path: &str) -> Result<DirListing, String> {
        let p = PathBuf::from(path);
        let canonical = fs::canonicalize(&p).unwrap_or(p);

        let read =
            fs::read_dir(&canonical).map_err(|e| format!("{}: {}", canonical.display(), e))?;

        let mut entries = Vec::new();
        for item in read.flatten() {
            let meta = match item.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let name = item.file_name().to_string_lossy().into_owned();
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::MetadataExt;
                Some(meta.mode())
            };
            #[cfg(not(unix))]
            let mode = None;
            entries.push(DirEntry {
                hidden: attrs::is_hidden(&meta, &name),
                readonly: attrs::is_readonly(&meta),
                executable: attrs::is_executable(&meta, &name),
                name,
                is_dir: meta.is_dir(),
                is_symlink: meta.file_type().is_symlink(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                mode,
            });
        }

        Ok(DirListing {
            path: canonical.to_string_lossy().into_owned(),
            entries,
        })
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        let meta = fs::metadata(path)?;
        Ok(Stat {
            is_dir: meta.is_dir(),
            len: meta.len(),
        })
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(path)?
            .map(|e| e.map(|d| d.path()))
            .collect()
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(fs::File::open(path)?))
    }

    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        Ok(Box::new(fs::File::create(path)?))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        fs::write(path, data)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rc-vfstest-{}-{}", t, n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_write_read_remove() {
        let p = LocalProvider;
        let d = tmp_dir();
        let f = d.join("a.txt");

        p.write(&f, b"hello").unwrap();
        assert!(p.exists(&f));
        assert_eq!(p.stat(&f).unwrap().len, 5);
        assert!(!p.stat(&f).unwrap().is_dir);

        let mut buf = Vec::new();
        p.open_read(&f).unwrap().read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"hello");

        p.remove(&f).unwrap();
        assert!(!p.exists(&f));
    }

    #[test]
    fn mkdir_rename_readdir() {
        let p = LocalProvider;
        let d = tmp_dir();
        let sub = d.join("sub");
        p.create_dir_all(&sub).unwrap();
        assert!(p.is_dir(&sub));

        p.write(&sub.join("x"), b"1").unwrap();
        let renamed = d.join("sub2");
        p.rename(&sub, &renamed).unwrap();
        assert!(!p.exists(&sub));

        let kids = p.read_dir(&renamed).unwrap();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].file_name().unwrap(), "x");

        p.remove(&renamed).unwrap();
        assert!(!p.exists(&renamed));
    }

    #[test]
    fn list_returns_entries() {
        let p = LocalProvider;
        let d = tmp_dir();
        p.write(&d.join("file.txt"), b"abc").unwrap();
        p.create_dir_all(&d.join("dir")).unwrap();

        let listing = p.list(&d.to_string_lossy()).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"file.txt"));
        assert!(names.contains(&"dir"));
        let file = listing
            .entries
            .iter()
            .find(|e| e.name == "file.txt")
            .unwrap();
        assert_eq!(file.size, 3);
        assert!(!file.is_dir);
    }
}
