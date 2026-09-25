use super::{
    bad,
    types::{Row, Snapshot},
    MAX_BYTES, MAX_ROWS,
};
use crate::commands::fs::transfer::recovery::types::RecoveryLocation;
use crate::{errors::AppResult, vfs};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::Read,
    path::{Component, Path},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub fn relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !Path::new(path).is_absolute()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
pub fn excluded(path: &str, policy: &super::types::IndexPolicy) -> bool {
    path.split('/').any(|name| {
        let lower = name.to_lowercase();
        (!policy.hidden && name.starts_with('.'))
            || lower.starts_with(".hortcmd")
            || [
                ".git",
                ".ssh",
                ".aws",
                ".gnupg",
                ".azure",
                ".kube",
                ".env",
                "credentials",
                "secrets",
                "keychains",
                "search-index",
            ]
            .contains(&lower.as_str())
            || lower.starts_with(".env.")
            || [".pem", ".key", ".kdbx", ".p12", ".pfx"]
                .iter()
                .any(|suffix| lower.ends_with(suffix))
            || policy
                .ignore
                .iter()
                .any(|pattern| super::glob(pattern, false).is_ok_and(|re| re.is_match(name)))
    })
}
pub fn bytes(snapshot: &Snapshot) -> usize {
    snapshot.bytes()
}

fn content(mut file: impl Read, maximum: u32, cancel: &AtomicBool) -> AppResult<Option<String>> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(bad("index cancelled"));
        }
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > maximum as usize || buffer[..n].contains(&0) {
            return Ok(None);
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    Ok(String::from_utf8(bytes).ok())
}
#[cfg(target_os = "macos")]
fn local_directory(root: &Path, relative: &str, expected: &str) -> AppResult<cap_std::fs::Dir> {
    use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
    use std::os::unix::fs::OpenOptionsExt as StdOpenOptionsExt;
    let root_file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x20000000 | 0x100000) // Darwin O_NOFOLLOW_ANY | O_DIRECTORY
        .open(root)?;
    use std::os::unix::fs::MetadataExt;
    let meta = root_file.metadata()?;
    if format!("{:?}", Some(format!("{}:{}", meta.dev(), meta.ino()))) != expected {
        return Err(bad("index root changed during open"));
    }
    let mut dir = Dir::from_std_file(root_file);
    for component in Path::new(relative).components() {
        let Component::Normal(name) = component else {
            return Err(bad("invalid scoped path"));
        };
        let mut options = OpenOptions::new();
        options.read(true).custom_flags(0x100 | 0x100000); // Darwin O_NOFOLLOW | O_DIRECTORY
        dir = Dir::from_std_file(dir.open_with(name, &options)?.into_std());
    }
    Ok(dir)
}
struct Item {
    path: String,
    row: Row,
}
#[cfg(target_os = "macos")]
fn local(
    snapshot: &Snapshot,
    root: &Path,
    folder: &str,
    cancel: &AtomicBool,
) -> AppResult<Vec<Item>> {
    use cap_std::fs::MetadataExt;
    use cap_std::fs::{OpenOptions, OpenOptionsExt};
    use std::os::unix::fs::MetadataExt as StdMetadataExt;
    let root_meta = std::fs::symlink_metadata(root)?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err(bad("index root changed"));
    }
    if format!(
        "{:?}",
        Some(format!("{}:{}", root_meta.dev(), root_meta.ino()))
    ) != snapshot.generation
    {
        return Err(bad("index root identity changed"));
    }
    let dir = local_directory(root, folder, &snapshot.generation)?;
    if dir.dir_metadata()?.dev() != root_meta.dev() {
        return Err(bad("index mount boundary changed"));
    }
    let mut rows = Vec::new();
    let mut weight = 0;
    for (count, entry) in dir.entries()?.enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(bad("index cancelled"));
        }
        if count >= 50_000 {
            return Err(bad("index directory limit"));
        }
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| bad("non-Unicode index path"))?;
        let path = if folder.is_empty() {
            name.clone()
        } else {
            format!("{folder}/{name}")
        };
        if !relative(&path) || excluded(&path, &snapshot.policy) {
            continue;
        }
        let meta = dir.symlink_metadata(&name)?;
        if meta.file_type().is_symlink() || (!meta.is_dir() && !meta.is_file()) {
            continue;
        }
        // Read native hidden/system flags through a no-follow metadata handle.
        let mut flags_options = OpenOptions::new();
        flags_options.read(true).custom_flags(0x100 | 0x8000); // O_NOFOLLOW | O_EVTONLY
        let native = dir
            .open_with(&name, &flags_options)?
            .into_std()
            .metadata()?;
        use std::os::macos::fs::MetadataExt as MacMetadataExt;
        if native.dev() != meta.dev() || native.ino() != meta.ino() {
            return Err(bad("index entry changed"));
        }
        if native.st_flags() & 0x80000 != 0
            || (!snapshot.policy.hidden && native.st_flags() & 0x8000 != 0)
        {
            continue;
        }
        let std_meta = meta.clone();
        if std_meta.dev() != root_meta.dev() {
            continue;
        }
        let modified = std_meta
            .modified()?
            .into_std()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let identity = format!(
            "{}:{}:{}:{}:{}:{}",
            std_meta.dev(),
            std_meta.ino(),
            meta.len(),
            modified.as_nanos(),
            std_meta.ctime_nsec(),
            std_meta.ctime()
        );
        let text = if snapshot.policy.content
            && meta.is_file()
            && meta.nlink() == 1
            && meta.len() <= u64::from(snapshot.policy.max_content_bytes)
        {
            if let Some(old) = snapshot
                .rows
                .get(&path)
                .filter(|row| row.identity == identity)
            {
                old.content.clone()
            } else {
                let mut options = OpenOptions::new();
                options.read(true).custom_flags(0x100);
                let file = dir.open_with(&name, &options)?;
                let opened = file.metadata()?;
                if opened.dev() != std_meta.dev() || opened.ino() != std_meta.ino() {
                    return Err(bad("file changed during index read"));
                }
                let result = content(file.try_clone()?, snapshot.policy.max_content_bytes, cancel)?;
                let after = file.metadata()?;
                if after.len() != std_meta.len()
                    || after.modified()? != std_meta.modified()?
                    || after.ctime() != std_meta.ctime()
                    || after.ctime_nsec() != std_meta.ctime_nsec()
                {
                    return Err(bad("file changed during index read"));
                }
                result
            }
        } else {
            None
        };
        weight +=
            path.len() + name.len() + identity.len() + text.as_ref().map_or(0, String::len) + 256;
        rows.push(Item {
            path,
            row: Row {
                name,
                directory: meta.is_dir(),
                size: meta.len(),
                modified: modified.as_secs(),
                identity,
                content: text,
                seen: snapshot.cycle,
            },
        });
        if weight > MAX_BYTES / 2 {
            return Err(bad("index directory content limit"));
        }
    }
    Ok(rows)
}
#[cfg(not(target_os = "macos"))]
fn local(_: &Snapshot, _: &Path, _: &str, _: &AtomicBool) -> AppResult<Vec<Item>> {
    Err(bad(
        "local indexing awaits native scope validation on this platform",
    ))
}
fn remote(
    snapshot: &Snapshot,
    address: &str,
    folder: &str,
    cancel: &AtomicBool,
) -> AppResult<Vec<Item>> {
    let (provider, root) = vfs::resolve(address)?;
    let current = vfs::identity::generation(&provider)?;
    if current != snapshot.generation {
        return Err(bad("connection changed; rebuild index"));
    }
    let directory = if folder.is_empty() {
        root
    } else {
        folder
            .split('/')
            .try_fold(root, |path, part| path.join(part))?
    };
    let listing = provider.list_cancellable(&directory.to_string_lossy(), cancel)?;
    if listing.entries.len() > 50_000 {
        return Err(bad("index directory limit"));
    }
    let mut rows = Vec::new();
    for entry in listing.entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(bad("index cancelled"));
        }
        if entry.name.is_empty()
            || entry.name.contains('/')
            || entry.name.contains('\\')
            || entry.name == "."
            || entry.name == ".."
        {
            return Err(bad("unsafe provider entry"));
        }
        let path = if folder.is_empty() {
            entry.name.clone()
        } else {
            format!("{folder}/{}", entry.name)
        };
        if !relative(&path)
            || excluded(&path, &snapshot.policy)
            || entry.is_symlink
            || (entry.hidden && !snapshot.policy.hidden)
        {
            continue;
        }
        // The current VFS cannot prove no-follow, mount identity or stable content
        // reads remotely. Remote scopes are metadata-only, explicitly approved.
        rows.push(Item {
            path,
            row: Row {
                name: entry.name,
                directory: entry.is_dir,
                size: entry.size,
                modified: entry.modified,
                identity: hex::encode(Sha256::digest(format!("{}:{}", entry.size, entry.modified))),
                content: None,
                seen: snapshot.cycle,
            },
        });
    }
    let (after, _) = vfs::resolve(address)?;
    if !std::sync::Arc::ptr_eq(&provider, &after) {
        return Err(bad("connection changed during indexing"));
    }
    Ok(rows)
}
pub fn step(snapshot: &mut Snapshot, cancel: &AtomicBool) -> AppResult<()> {
    let address = snapshot.location.resolve()?;
    let started = Instant::now();
    for _ in 0..8 {
        if cancel.load(Ordering::Relaxed) || started.elapsed().as_millis() >= 200 {
            break;
        }
        let Some(folder) = snapshot.queue.front().cloned() else {
            break;
        };
        let result = match &snapshot.location {
            RecoveryLocation::Local { path } => local(snapshot, Path::new(path), &folder, cancel),
            RecoveryLocation::Remote { .. } => remote(snapshot, &address, &folder, cancel),
        };
        let rows = match result {
            Ok(rows) => rows,
            Err(error) => {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                snapshot.unreadable = snapshot.unreadable.saturating_add(1);
                snapshot.paused = true;
                return Err(error);
            }
        };
        snapshot.revision += 1;
        if !insert_rows(snapshot, rows, cancel) {
            return Ok(());
        }
        snapshot.pop_folder();
    }
    if snapshot.queue.is_empty() && !snapshot.limited {
        snapshot.retain_current_rows();
        snapshot.completed = super::now();
    }
    Ok(())
}

// Keep the current folder queued until every row has been inserted. A cancelled
// batch can therefore be retried without losing its remaining children.
fn insert_rows(
    snapshot: &mut Snapshot,
    rows: impl IntoIterator<Item = Item>,
    cancel: &AtomicBool,
) -> bool {
    let mut queued: HashSet<String> = snapshot.queue.iter().cloned().collect();
    for item in rows {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        if snapshot.rows.len() >= MAX_ROWS
            || snapshot.queue.len() >= 5000
            || item.path.split('/').count() > 64
            || snapshot.bytes() >= MAX_BYTES / 2
        {
            snapshot.limited = true;
            snapshot.paused = true;
            return false;
        }
        if item.row.directory && queued.insert(item.path.clone()) {
            snapshot.push_folder(item.path.clone());
        }
        snapshot.insert_row(item.path, item.row);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_insertion_cancels_mid_batch_and_resumes_without_duplicate_queue_entries() {
        let (_dir, mut snapshot) = super::super::tests::fixture();
        for i in 0..3000 {
            snapshot.push_folder(format!("pending-{i}"));
        }
        let cancel = AtomicBool::new(false);
        let items = || {
            (0..50_000).map(|i| Item {
                path: format!("child-{i}"),
                row: Row {
                    name: format!("child-{i}"),
                    directory: true,
                    size: 0,
                    modified: 0,
                    identity: String::new(),
                    content: None,
                    seen: 1,
                },
            })
        };
        let mut consumed = 0;
        let rows = items().inspect(|_| {
            consumed += 1;
            if consumed == 1001 {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(!insert_rows(&mut snapshot, rows, &cancel));
        assert_eq!(consumed, 1001);
        assert_eq!(snapshot.bytes(), snapshot.recomputed_bytes());
        assert_eq!(snapshot.rows.len(), 1000);
        assert_eq!(snapshot.queue.len(), 4001);
        assert_eq!(snapshot.queue.front().unwrap(), "");
        assert!(!snapshot.limited);
        assert_eq!(snapshot.completed, 0);
        cancel.store(false, Ordering::Relaxed);
        assert!(insert_rows(&mut snapshot, items().take(1100), &cancel));
        assert_eq!(snapshot.bytes(), snapshot.recomputed_bytes());
        assert_eq!(snapshot.rows.len(), 1100);
        assert_eq!(snapshot.queue.len(), 4101);
    }
}
