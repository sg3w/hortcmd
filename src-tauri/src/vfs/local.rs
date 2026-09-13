// ============================================================
// Local file-system provider.
//
// Thin, idiomatic wrappers over `std::fs` implementing the VFS
// `Provider` trait. This is where the previous `commands/fs/*`
// local logic now lives; the behavior is deliberately identical to
// before so local paths keep working unchanged.
// ============================================================

use super::{Provider, Stat};
use crate::commands::fs::attrs;
use crate::commands::fs::dir::{DirEntry, DirListing};
use crate::errors::{AppError, AppResult};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Provider for the local file system (`local` / `file://`).
pub struct LocalProvider;

impl Provider for LocalProvider {
    fn store_id(&self) -> String {
        "local".to_string()
    }

    /// Reads the contents of a directory (canonicalize + per-entry attrs).
    ///
    /// The path is canonicalised, so entering a symlinked folder shows the
    /// **resolved** target path and `..` leads to the target's real parent
    /// rather than back through the link (TICKET-018). This is the deliberate
    /// choice: it matches how the rest of the app resolves paths and makes a
    /// symlink cycle (`a/link -> a`) impossible to walk into indefinitely,
    /// because every hop lands on the same canonical path.
    fn list(&self, path: &str) -> AppResult<DirListing> {
        let p = PathBuf::from(path);
        let canonical = fs::canonicalize(&p).unwrap_or(p);

        let read = fs::read_dir(&canonical)
            .map_err(|e| AppError::from(format!("{}: {}", canonical.display(), e)))?;

        let mut entries = Vec::new();
        for item in read.flatten() {
            let meta = match item.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let name = item.file_name().to_string_lossy().into_owned();
            // `DirEntry::metadata()` is an `lstat`: for a symlink it describes
            // the link itself, so `is_dir` would be false even for a link to a
            // folder and `len` would be the byte length of the target string
            // (TICKET-018). Resolve the target once — and only for symlinks, so
            // plain entries cost no additional syscall.
            let is_symlink = meta.file_type().is_symlink();
            let target = if is_symlink {
                fs::metadata(item.path()).ok()
            } else {
                None
            };
            // What the entry *behaves* like comes from the target; a broken
            // link has none, so it falls back to the link's own metadata and
            // stays listed as a plain 0-byte file instead of vanishing.
            let effective = target.as_ref().unwrap_or(&meta);
            let is_dir = effective.is_dir();
            // Identity stays link-level: `is_symlink`, the hidden flag and the
            // mode bits describe the link, so the list keeps showing it as one
            // (`l` in the permissions column, symlink colour rule).
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::MetadataExt;
                Some(meta.mode())
            };
            #[cfg(not(unix))]
            let mode = None;
            entries.push(DirEntry {
                hidden: attrs::is_hidden(&meta, &name),
                // Writing to or running a symlink hits the target, so these
                // two are read from the effective metadata.
                readonly: attrs::is_readonly(effective),
                executable: attrs::is_executable(effective, &name),
                name,
                is_dir,
                is_symlink,
                size: if is_dir || (is_symlink && target.is_none()) {
                    0
                } else {
                    effective.len()
                },
                modified: effective
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
        fs::read_dir(path)?.map(|e| e.map(|d| d.path())).collect()
    }

    fn visit_dir(
        &self,
        path: &Path,
        limit: usize,
        visit: &mut dyn FnMut(PathBuf) -> io::Result<()>,
    ) -> io::Result<()> {
        for (index, entry) in fs::read_dir(path)?.enumerate() {
            if index >= limit {
                return Err(io::Error::other("preflight directory limit"));
            }
            visit(entry?.path())?;
        }
        Ok(())
    }

    fn limits(&self, path: &Path) -> io::Result<super::limits::ProviderLimits> {
        let mut limits = super::limits::ProviderLimits {
            available_bytes: self.free_space(path),
            staging: true,
            reserved_names: Some(vec![".".into(), "..".into()]),
            forbidden_characters: Some("/\0".into()),
            ..Default::default()
        };
        #[cfg(target_os = "macos")]
        {
            use std::ffi::CString;
            use std::os::unix::ffi::OsStrExt;
            unsafe extern "C" {
                fn access(path: *const std::ffi::c_char, mode: i32) -> i32;
                fn pathconf(path: *const std::ffi::c_char, name: i32) -> std::ffi::c_long;
            }
            let path = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
            // Darwin SDK sys/unistd.h: W_OK=2, _PC_NAME_MAX=4, _PC_PATH_MAX=5.
            // SAFETY: path is NUL-terminated and outlives all three read-only calls.
            unsafe {
                limits.writable = Some(access(path.as_ptr(), 2) == 0);
                limits.maximum_name_bytes = u32::try_from(pathconf(path.as_ptr(), 4)).ok();
                limits.maximum_path_bytes = u32::try_from(pathconf(path.as_ptr(), 5)).ok();
            }
        }
        Ok(limits)
    }

    fn planning_identity(&self, path: &Path) -> io::Result<Option<String>> {
        crate::commands::fs::transfer::recovery::durability::rename_identity(path)
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

    // ----- Failure-safe transfers (ticket 029) -----

    fn supports_staging(&self) -> bool {
        true
    }

    /// `std::fs::rename` replaces an existing file atomically on both
    /// Unix and Windows — the documented primitive the ticket asks for.
    fn rename_replaces(&self) -> bool {
        true
    }

    /// Reported by the same mount lookup the volume bar uses.
    fn free_space(&self, path: &Path) -> Option<u64> {
        crate::commands::fs::volume::free_space_of(path)
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

    /// TICKET-018: a symlink must be classified by its target, while still
    /// being reported as a link.
    #[cfg(unix)]
    #[test]
    fn symlinks_are_classified_by_their_target() {
        use std::os::unix::fs::symlink;

        let p = LocalProvider;
        let d = tmp_dir();
        p.create_dir_all(&d.join("real")).unwrap();
        p.write(&d.join("file.txt"), b"abc").unwrap();
        // Target names are deliberately shorter/longer than the real sizes,
        // so a leaked `lstat` length would not accidentally match.
        symlink("real", d.join("link_to_dir")).unwrap();
        symlink("file.txt", d.join("link_to_file")).unwrap();
        symlink("nowhere", d.join("broken_link")).unwrap();
        // A cycle: the link points back at the directory containing it.
        symlink(&d, d.join("cycle")).unwrap();

        let listing = p.list(&d.to_string_lossy()).unwrap();
        let get = |n: &str| {
            listing
                .entries
                .iter()
                .find(|e| e.name == n)
                .unwrap_or_else(|| panic!("{} missing from the listing", n))
        };

        let to_dir = get("link_to_dir");
        assert!(to_dir.is_dir, "a link to a folder behaves like a folder");
        assert!(to_dir.is_symlink, "and must still be visible as a link");
        assert_eq!(to_dir.size, 0, "never the target string's length");
        assert!(!to_dir.executable);

        let to_file = get("link_to_file");
        assert!(!to_file.is_dir);
        assert!(to_file.is_symlink);
        assert_eq!(to_file.size, 3, "the target's size, not the link's");

        // Dangling links stay listed rather than vanishing or erroring.
        let broken = get("broken_link");
        assert!(!broken.is_dir);
        assert!(broken.is_symlink);
        assert_eq!(broken.size, 0);

        // A cycle resolves to the directory itself — listed, not hung.
        let cycle = get("cycle");
        assert!(cycle.is_dir);
        assert!(cycle.is_symlink);
        assert_eq!(cycle.size, 0);

        // Plain entries are untouched by the resolution path.
        assert!(!get("file.txt").is_symlink);
        assert_eq!(get("file.txt").size, 3);
        assert!(get("real").is_dir);
        assert!(!get("real").is_symlink);
    }
}
