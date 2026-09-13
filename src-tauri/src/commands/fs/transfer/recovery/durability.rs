//! Bounded local durability/verification. Remote durability remains explicitly unknown.
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_NODES: usize = 100_000;
const MAX_DEPTH: usize = 128;
fn changed() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "transfer tree changed or exceeds recovery limits",
    )
}

/// Sync data and child directory entries before their top-level name is published.
/// Returns a content digest, so recovery can check a commit rather than guess from existence.
pub fn fingerprint(root: &Path, sync: bool) -> io::Result<String> {
    fingerprint_cancellable(root, sync, &AtomicBool::new(false))
}

pub fn fingerprint_cancellable(root: &Path, sync: bool, cancel: &AtomicBool) -> io::Result<String> {
    let mut hash = Sha256::new();
    let mut nodes = 0;
    visit(root, "", 0, sync, &mut nodes, &mut hash, cancel)?;
    Ok(hex::encode(hash.finalize()))
}

fn visit(
    path: &Path,
    relative: &str,
    depth: usize,
    sync: bool,
    nodes: &mut usize,
    hash: &mut Sha256,
    cancel: &AtomicBool,
) -> io::Result<()> {
    check_cancel(cancel)?;
    *nodes += 1;
    if *nodes > MAX_NODES || depth > MAX_DEPTH {
        return Err(changed());
    }
    let before = fs::symlink_metadata(path)?;
    if before.file_type().is_symlink() || (!before.is_dir() && !before.is_file()) {
        return Err(changed());
    }
    hash.update((relative.len() as u64).to_le_bytes());
    hash.update(relative.as_bytes());
    hash.update([u8::from(before.is_dir())]);
    if before.is_dir() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            if entries.len() + *nodes >= MAX_NODES {
                return Err(changed());
            }
            entries.push(entry?);
        }
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().into_string().map_err(|_| changed())?;
            visit(
                &entry.path(),
                &format!("{relative}/{name}"),
                depth + 1,
                sync,
                nodes,
                hash,
                cancel,
            )?;
        }
    } else {
        hash.update(before.len().to_le_bytes());
        let mut file = File::open(path)?;
        let mut buffer = [0u8; 256 * 1024];
        let mut read = 0u64;
        loop {
            check_cancel(cancel)?;
            let bytes = file.read(&mut buffer)?;
            if bytes == 0 {
                break;
            }
            read = read.checked_add(bytes as u64).ok_or_else(changed)?;
            if read > before.len() {
                return Err(changed());
            }
            hash.update(&buffer[..bytes]);
        }
        if read != before.len() {
            return Err(changed());
        }
        if sync {
            file.sync_all()?;
        }
    }
    let after = fs::symlink_metadata(path)?;
    if after.file_type() != before.file_type()
        || after.len() != before.len()
        || after.modified()? != before.modified()?
    {
        return Err(changed());
    }
    if sync && before.is_dir() {
        File::open(path)?.sync_all()?;
    }
    Ok(())
}

pub fn sync_parent(path: &Path) -> io::Result<()> {
    File::open(path.parent().ok_or_else(changed)?)?.sync_all()
}

/// Same-store rename keeps the object's identity; avoid reading an entire tree
/// merely to move its directory entry. Platforms without stable identity decline
/// verification instead of inventing proof.
pub fn rename_identity(path: &Path) -> io::Result<Option<String>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(path)?;
        Ok(Some(format!(
            "identity:{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec()
        )))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(None)
    }
}

pub fn matches(path: &Path, expected: &str) -> io::Result<bool> {
    if expected.starts_with("identity:") {
        Ok(rename_identity(path)?.as_deref() == Some(expected))
    } else {
        Ok(fingerprint(path, false)? == expected)
    }
}

pub fn same_device(source: &Path, destination_dir: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::symlink_metadata(source), fs::metadata(destination_dir)) {
            (Ok(source), Ok(destination)) => source.dev() == destination.dev(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (source, destination_dir);
        false
    }
}

/// An ownership check for a private staging directory; content changes must not
/// invalidate its identity while the transfer is writing inside it.
pub fn object_identity(path: &Path) -> io::Result<Option<String>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = fs::symlink_metadata(path)?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(changed());
        }
        Ok(Some(format!("{}:{}", meta.dev(), meta.ino())))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(None)
    }
}

fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "transfer cancelled",
        ))
    } else {
        Ok(())
    }
}
