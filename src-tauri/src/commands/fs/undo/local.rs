//! macOS local adapter. Remote providers have no stable no-replace undo contract.
use super::{unavailable, UndoEntry};
use crate::commands::fs::process_lock::ProcessLock;
use crate::commands::fs::transfer::recovery::durability;
use crate::errors::AppResult;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use xattr::FileExt as XattrFileExt;

const LIMIT: usize = 100;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_AGE: u64 = 7 * 24 * 3600;
const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn conflict() -> io::Error {
    io::Error::other("undo state changed or destination occupied")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Snapshot {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    modified: (i64, i64),
    digest: String,
    tags: Option<Vec<u8>>,
}
fn snapshot(path: &Path) -> io::Result<Snapshot> {
    let before = fs::symlink_metadata(path)?;
    if before.file_type().is_symlink() {
        return Err(conflict());
    }
    let digest = durability::fingerprint(path, false)?;
    let file = File::open(path)?;
    let after = file.metadata()?;
    if before.ino() != after.ino()
        || before.dev() != after.dev()
        || before.mtime() != after.mtime()
        || before.mtime_nsec() != after.mtime_nsec()
    {
        return Err(conflict());
    }
    let tags = file.get_xattr(TAGS)?;
    if tags.as_ref().is_some_and(|v| v.len() > 64 * 1024) {
        return Err(conflict());
    }
    Ok(Snapshot {
        device: after.dev(),
        inode: after.ino(),
        mode: after.mode(),
        uid: after.uid(),
        gid: after.gid(),
        modified: (after.mtime(), after.mtime_nsec()),
        digest,
        tags,
    })
}

fn eligibility(
    kind: &str,
    source: &str,
    target: &str,
    allow_target: bool,
) -> AppResult<Option<Option<Snapshot>>> {
    if !matches!(
        kind,
        "rename" | "move" | "copy" | "mkdir" | "permissions" | "owner" | "tags"
    ) {
        return Ok(None);
    }
    let (source, target) = (Path::new(source), Path::new(target));
    if !source.is_absolute() || !target.is_absolute() {
        return Ok(None);
    }
    let metadata = matches!(kind, "permissions" | "owner" | "tags");
    if !metadata && !allow_target && fs::symlink_metadata(target).is_ok() {
        return Ok(None);
    }
    let Some(parent) = target.parent() else {
        return Ok(None);
    };
    let Ok(canonical_parent) = parent.canonicalize() else {
        return Ok(None);
    };
    if !parent.is_dir() || canonical_parent != parent {
        return Ok(None);
    }
    if matches!(kind, "rename" | "move") && !durability::same_device(source, parent) {
        return Ok(None);
    }
    let before = if matches!(kind, "copy" | "mkdir") {
        None
    } else {
        match snapshot(source) {
            Ok(s) => Some(s),
            Err(_) => return Ok(None),
        }
    };
    Ok(Some(before))
}

#[derive(Clone, Serialize, Deserialize)]
struct UndoPlan {
    id: String,
    group: Option<String>,
    kind: String,
    source: PathBuf,
    target: PathBuf,
    created: u64,
    before: Option<Snapshot>,
    after: Option<Snapshot>,
    state: String,
    retained: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
struct Data {
    version: u32,
    records: Vec<UndoPlan>,
}
pub(super) struct History {
    root: PathBuf,
    _lock: ProcessLock,
    records: Mutex<Vec<UndoPlan>>,
}
impl History {
    pub(super) fn open(root: &Path) -> AppResult<Arc<Self>> {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)?;
        if fs::symlink_metadata(root)?.file_type().is_symlink() {
            return Err(unavailable());
        }
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(root.join("lock"))?;
        let lock = ProcessLock::acquire(lock)?;
        let path = root.join("history.json");
        let records = if path.try_exists()? {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Err(unavailable());
            }
            let mut bytes = Vec::new();
            File::open(path)?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(unavailable());
            }
            let data: Data = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
            if data.version != 1 || data.records.len() > LIMIT {
                return Err(unavailable());
            }
            data.records
                .into_iter()
                .map(|mut record| {
                    if matches!(record.state.as_str(), "pending" | "recorded" | "applying") {
                        record.state = "interrupted".into();
                    }
                    record
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Arc::new(Self {
            root: root.to_owned(),
            _lock: lock,
            records: Mutex::new(records),
        }))
    }
    fn save(&self, records: &[UndoPlan]) -> AppResult<()> {
        let bytes = serde_json::to_vec(&Data {
            version: 1,
            records: records.to_vec(),
        })
        .map_err(|_| unavailable())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(unavailable());
        }
        let tmp = self.root.join("history.tmp");
        // Never follow an externally substituted temporary file.
        match fs::remove_file(&tmp) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(tmp, self.root.join("history.json"))?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
    pub(super) fn prepare(
        self: &Arc<Self>,
        kind: &str,
        source: &str,
        target: &str,
    ) -> AppResult<Option<Pending>> {
        self.prepare_group(kind, source, target, None, false)
    }
    pub(super) fn prepare_group(
        self: &Arc<Self>,
        kind: &str,
        source: &str,
        target: &str,
        group: Option<&str>,
        allow_target: bool,
    ) -> AppResult<Option<Pending>> {
        let Some(before) = eligibility(kind, source, target, allow_target)? else {
            return Ok(None);
        };
        let (source, target) = (Path::new(source), Path::new(target));
        let mut records = self.records.lock().map_err(|_| unavailable())?;
        // Never forget a retained file or an interrupted inverse operation.
        records.retain(|r| r.retained.is_some() || now().saturating_sub(r.created) <= MAX_AGE);
        if records.len() >= LIMIT {
            if let Some(index) = records.iter().position(|r| {
                r.retained.is_none() && matches!(r.state.as_str(), "ready" | "restored" | "failed")
            }) {
                records.remove(index);
            } else {
                return Err(unavailable());
            }
        }
        let id = format!(
            "{}-{}",
            now(),
            records
                .iter()
                .filter_map(|r| r.id.rsplit('-').next()?.parse::<u64>().ok())
                .max()
                .unwrap_or(0)
                + 1
        );
        records.push(UndoPlan {
            id: id.clone(),
            group: group.map(str::to_owned),
            kind: kind.into(),
            source: source.to_owned(),
            target: target.to_owned(),
            created: now(),
            before,
            after: None,
            state: "pending".into(),
            retained: None,
        });
        self.save(&records)?;
        Ok(Some(Pending {
            history: self.clone(),
            id,
        }))
    }
    fn view(record: &UndoPlan, records: &[UndoPlan]) -> UndoEntry {
        let status = if record.state == "ready" {
            if now().saturating_sub(record.created) > MAX_AGE {
                "expired"
            } else if record
                .after
                .as_ref()
                .is_none_or(|s| snapshot(&record.target).as_ref().ok() != Some(s))
            {
                "changed"
            } else if matches!(record.kind.as_str(), "rename" | "move")
                && fs::symlink_metadata(&record.source).is_ok()
                && snapshot(&record.source).ok().as_ref() != record.after.as_ref()
                && !records.iter().any(|other| {
                    record.group.is_some()
                        && other.group == record.group
                        && other.target == record.source
                        && other.state == "ready"
                })
            {
                "collision"
            } else {
                "ready"
            }
        } else {
            &record.state
        };
        UndoEntry {
            id: record.id.clone(),
            group: record.group.clone(),
            kind: record.kind.clone(),
            path: record.target.to_string_lossy().into_owned(),
            target: matches!(record.kind.as_str(), "rename" | "move")
                .then(|| record.source.to_string_lossy().into_owned()),
            status: status.into(),
            retained: record
                .retained
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
        }
    }
    pub(super) fn forget(&self, id: &str) -> AppResult<Vec<UndoEntry>> {
        let mut records = self.records.lock().map_err(|_| unavailable())?;
        let index = records
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(unavailable)?;
        let record = &records[index];
        if matches!(record.state.as_str(), "pending" | "recorded" | "applying")
            || record
                .retained
                .as_ref()
                .is_some_and(|p| fs::symlink_metadata(p).is_ok())
        {
            return Err(unavailable());
        }
        records.remove(index);
        self.save(&records)?;
        Ok(records
            .iter()
            .rev()
            .map(|r| Self::view(r, &records))
            .collect())
    }
    pub(super) fn seal(&self, group: &str) -> AppResult<()> {
        let mut records = self.records.lock().map_err(|_| unavailable())?;
        for record in records
            .iter_mut()
            .filter(|r| r.group.as_deref() == Some(group) && r.state == "recorded")
        {
            record.state = "ready".into();
        }
        self.save(&records)
    }
    pub(super) fn preview(&self) -> AppResult<Vec<UndoEntry>> {
        let records = self.records.lock().map_err(|_| unavailable())?;
        Ok(records
            .iter()
            .rev()
            .map(|r| Self::view(r, &records))
            .collect())
    }
    pub(super) fn execute(&self, id: &str) -> AppResult<Vec<UndoEntry>> {
        let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
        let mut records = self.records.lock().map_err(|_| unavailable())?;
        let index = records
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(unavailable)?;
        let group = records[index].group.clone();
        let indices: Vec<usize> = records
            .iter()
            .enumerate()
            .filter(|(i, r)| {
                *i == index || (group.is_some() && r.group == group && r.state == "ready")
            })
            .map(|(i, _)| i)
            .collect();
        if indices
            .iter()
            .any(|&i| Self::view(&records[i], &records).status != "ready")
        {
            return Err(unavailable());
        }
        let grouped_rename = (indices.len() > 1
            || (records[index].source != records[index].target
                && fs::symlink_metadata(&records[index].source).is_ok()))
            && indices
                .iter()
                .all(|&i| matches!(records[i].kind.as_str(), "rename" | "move"));
        // Persist every inverse staging destination before touching a user path.
        for &i in &indices {
            records[i].state = "applying".into();
            if records[i].kind == "copy" || grouped_rename {
                records[i].retained = Some(
                    records[i]
                        .target
                        .parent()
                        .ok_or_else(unavailable)?
                        .join(format!(".hortcmd-undo-{}", records[i].id)),
                );
            }
        }
        self.save(&records)?;
        if grouped_rename {
            let mut staged = Vec::new();
            for &i in &indices {
                let r = &records[i];
                let held = r.retained.as_ref().ok_or_else(unavailable)?;
                if snapshot(&r.target).ok().as_ref() != r.after.as_ref()
                    || rename_exclusive(&r.target, held).is_err()
                {
                    break;
                }
                staged.push(i);
                if snapshot(held).ok().as_ref() != r.after.as_ref() {
                    break;
                }
            }
            let complete = staged.len() == indices.len()
                && staged.iter().all(|&i| {
                    snapshot(records[i].retained.as_ref().unwrap())
                        .ok()
                        .as_ref()
                        == records[i].after.as_ref()
                });
            for &i in &indices {
                if !staged.contains(&i) {
                    records[i].retained = None;
                    records[i].state = "failed".into();
                    continue;
                }
                let held = records[i].retained.clone().ok_or_else(unavailable)?;
                let dest = if complete {
                    &records[i].source
                } else {
                    &records[i].target
                };
                let restored = rename_exclusive(&held, dest)
                    .and_then(|()| durability::sync_parent(dest))
                    .is_ok();
                if restored {
                    records[i].retained = None;
                }
                records[i].state = if complete && restored {
                    "restored"
                } else {
                    "failed"
                }
                .into();
                self.save(&records)?;
            }
        } else {
            for &i in &indices {
                let result = inverse(&mut records[i]);
                records[i].state = if result.is_ok() { "restored" } else { "failed" }.into();
                self.save(&records)?;
            }
        }
        self.save(&records)?;
        Ok(records
            .iter()
            .rev()
            .map(|r| Self::view(r, &records))
            .collect())
    }
}

pub(super) struct Pending {
    history: Arc<History>,
    id: String,
}
impl Pending {
    pub(super) fn finish(self) -> AppResult<()> {
        let mut records = self.history.records.lock().map_err(|_| unavailable())?;
        let record = records
            .iter_mut()
            .find(|r| r.id == self.id)
            .ok_or_else(unavailable)?;
        let after = snapshot(&record.target)?;
        if let Some(before) = &record.before {
            if before.device != after.device
                || before.inode != after.inode
                || before.digest != after.digest
                || before.modified != after.modified
            {
                return Err(unavailable());
            }
        }
        record.after = Some(after);
        record.state = if record.group.is_some() {
            "recorded"
        } else {
            "ready"
        }
        .into();
        self.history.save(&records)
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Ok(mut records) = self.history.records.lock() {
            if let Some(record) = records
                .iter_mut()
                .find(|r| r.id == self.id && r.state == "pending")
            {
                record.state = "failed".into();
                let _ = self.history.save(&records);
            }
        }
    }
}

// Darwin's atomic exclusive rename refuses an existing destination, including
// dangling symlinks. Unlike exists()+rename(), it cannot clobber a racing writer.
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renamex_np(
            from: *const std::ffi::c_char,
            to: *const std::ffi::c_char,
            flags: u32,
        ) -> i32;
    }
    let from = CString::new(from.as_os_str().as_bytes()).map_err(|_| conflict())?;
    let to = CString::new(to.as_os_str().as_bytes()).map_err(|_| conflict())?;
    // SAFETY: both paths are NUL-terminated and alive for the call; RENAME_EXCL=4.
    if unsafe { renamex_np(from.as_ptr(), to.as_ptr(), 4) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn inverse(record: &mut UndoPlan) -> io::Result<()> {
    let _cache_mutation = crate::commands::fs::cache::Mutation::begin();
    let expected = record.after.as_ref().ok_or_else(conflict)?;
    if snapshot(&record.target)? != *expected {
        return Err(conflict());
    }
    match record.kind.as_str() {
        "rename" | "move" => {
            if record
                .source
                .parent()
                .ok_or_else(conflict)?
                .canonicalize()?
                != record.source.parent().ok_or_else(conflict)?
            {
                return Err(conflict());
            }
            rename_exclusive(&record.target, &record.source)?;
            if snapshot(&record.source)? != *expected {
                if rename_exclusive(&record.source, &record.target).is_err() {
                    record.retained = Some(record.source.clone());
                }
                return Err(conflict());
            }
            durability::sync_parent(&record.source)?;
            durability::sync_parent(&record.target)?;
        }
        "mkdir" => {
            fs::remove_dir(&record.target)?;
            durability::sync_parent(&record.target)?;
        }
        "copy" => {
            // Retain data in a sibling quarantine instead of unlinking it: another
            // process may still hold a writable descriptor after our verification.
            let held = record.retained.clone().ok_or_else(conflict)?;
            rename_exclusive(&record.target, &held)?;
            record.retained = Some(held.clone());
            durability::sync_parent(&held)?;
            if snapshot(&held)? != *expected {
                if rename_exclusive(&held, &record.target).is_ok() {
                    record.retained = None;
                }
                return Err(conflict());
            }
        }
        "permissions" | "owner" | "tags" => {
            let before = record.before.as_ref().ok_or_else(conflict)?;
            let file = File::open(&record.target)?;
            let meta = file.metadata()?;
            if meta.dev() != expected.device || meta.ino() != expected.inode {
                return Err(conflict());
            }
            match record.kind.as_str() {
                "permissions" => file.set_permissions(fs::Permissions::from_mode(before.mode))?,
                "owner" => {
                    use std::os::fd::AsRawFd;
                    unsafe extern "C" {
                        fn fchown(fd: i32, owner: u32, group: u32) -> i32;
                    }
                    // SAFETY: the descriptor remains owned by file throughout.
                    if unsafe { fchown(file.as_raw_fd(), before.uid, before.gid) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    file.set_permissions(fs::Permissions::from_mode(before.mode))?;
                }
                _ => {
                    if let Some(tags) = &before.tags {
                        file.set_xattr(TAGS, tags)?;
                    } else if file.get_xattr(TAGS)?.is_some() {
                        file.remove_xattr(TAGS)?;
                    }
                }
            }
            file.sync_all()?;
        }
        _ => return Err(conflict()),
    }
    Ok(())
}
fn history() -> AppResult<Arc<History>> {
    static HISTORY: OnceLock<Mutex<Option<Arc<History>>>> = OnceLock::new();
    let mut history = HISTORY
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| unavailable())?;
    if let Some(value) = &*history {
        return Ok(value.clone());
    }
    let value = History::open(&crate::config::path("undo-v1")?)?;
    *history = Some(value.clone());
    Ok(value)
}
#[cfg(not(test))]
pub(super) fn prepare(kind: &str, source: &str, target: &str) -> AppResult<Option<Pending>> {
    history()?.prepare(kind, source, target)
}
pub(super) fn preview() -> AppResult<Vec<UndoEntry>> {
    history()?.preview()
}
pub(super) fn execute(id: &str) -> AppResult<Vec<UndoEntry>> {
    history()?.execute(id)
}

#[cfg(not(test))]
pub(super) fn prepare_group(
    kind: &str,
    source: &str,
    target: &str,
    group: &str,
    allow_target: bool,
) -> AppResult<Option<Pending>> {
    history()?.prepare_group(kind, source, target, Some(group), allow_target)
}

#[cfg(not(test))]
pub(super) fn seal(group: &str) -> AppResult<()> {
    history()?.seal(group)
}

pub(super) fn classify(
    kind: &str,
    sources: &[String],
    target: &str,
    directory: bool,
) -> AppResult<bool> {
    if sources.is_empty() || sources.len() > LIMIT {
        return Ok(false);
    }
    for source in sources {
        let target = if directory {
            let Some(name) = Path::new(source).file_name() else {
                return Ok(false);
            };
            Path::new(target).join(name)
        } else {
            PathBuf::from(target)
        };
        if eligibility(kind, source, &target.to_string_lossy(), false)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn forget(id: &str) -> AppResult<Vec<UndoEntry>> {
    history()?.forget(id)
}
