//! Cooperative, handle-relative DFS. No file content reads and no background task.
use super::types::*;
use cap_std::fs::{Dir, Metadata, ReadDir};
use std::{
    collections::{BTreeSet, HashSet},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
const PATH_LIMIT: usize = 4096;
static RESERVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct Reservation(usize);
impl Reservation {
    fn new(bytes: usize) -> Result<Self, String> {
        RESERVED
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                old.checked_add(bytes).filter(|n| *n <= 128 * 1024 * 1024)
            })
            .map_err(|_| "Storage scan memory budget is busy; close another analyzer")?;
        Ok(Self(bytes))
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        RESERVED.fetch_sub(self.0, Ordering::SeqCst);
    }
}

const STEP_ENTRIES: usize = 256;
const BATCH_NODES: usize = 64;

#[cfg(unix)]
fn identity(meta: &Metadata) -> Option<(u64, u64)> {
    use cap_std::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}
#[cfg(not(unix))]
fn identity(_: &Metadata) -> Option<(u64, u64)> {
    None
}
#[cfg(unix)]
fn allocated(meta: &Metadata) -> Option<u64> {
    use cap_std::fs::MetadataExt;
    meta.blocks().checked_mul(512)
}
#[cfg(not(unix))]
fn allocated(_: &Metadata) -> Option<u64> {
    None
}
fn link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use cap_std::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    meta.file_type().is_symlink()
}
fn open_directory(entry: &cap_std::fs::DirEntry) -> io::Result<Dir> {
    use cap_std::fs::{OpenOptions, OpenOptionsExt};
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    #[cfg(windows)]
    options.custom_flags(0x02000000 | 0x00200000); // BACKUP_SEMANTICS | OPEN_REPARSE_POINT
    let file = entry.open_with(&options)?;
    let metadata = file.metadata()?;
    if link(&metadata) || !metadata.is_dir() {
        return Err(io::Error::other("entry is no longer a real directory"));
    }
    Ok(Dir::from_std_file(file.into_std()))
}
fn node(id: u32, parent: Option<u32>, path: &Path, kind: &str, depth: u16) -> UsageNode {
    UsageNode {
        id,
        parent,
        location: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned(),
        kind: kind.into(),
        depth,
        direct_bytes: 0,
        recursive_bytes: 0,
        allocated_bytes: cfg!(unix).then_some(0),
        deduplicated_bytes: cfg!(unix).then_some(0),
        files: 0,
        folders: 0,
        unreadable: 0,
        omitted_entries: 0,
        omitted_bytes: 0,
        incomplete: false,
        incomplete_below_depth: false,
    }
}
struct Frame {
    dir: Dir,
    entries: ReadDir,
    path: PathBuf,
    owner: u32,
    depth: u16,
    meta: Metadata,
}
pub struct Scan {
    _reservation: Reservation,
    pub options: UsageOptions,
    nodes: Vec<UsageNode>,
    dirty: BTreeSet<u32>,
    frames: Vec<Frame>,
    identities: HashSet<(u64, u64)>,
    started: Instant,
    pub status: String,
    scanned: u32,
    issues: UsageIssues,
    current: String,
    volume: Option<u64>,
    cache_epoch: u64,
}
impl Scan {
    pub fn new(mut options: UsageOptions) -> Result<Self, String> {
        options.validate()?;
        let reservation = Reservation::new(
            options.node_limit as usize * 8768 + (usize::from(options.max_depth) + 1) * 73728,
        )?;
        use crate::vfs::{
            caps::{self, Capability},
            location::VfsLocation,
            Scheme,
        };
        let location = VfsLocation::parse(&options.root).map_err(|e| e.to_string())?;
        caps::require(&location.uri(), Capability::RecursiveSize).map_err(|_| {
            "This provider does not support bounded recursive storage scans".to_owned()
        })?;
        caps::require(&location.uri(), Capability::Stat).map_err(|e| e.to_string())?;
        if location.scheme() != Scheme::Local {
            return Err("Provider has no bounded no-follow scan adapter".into());
        }
        let requested = PathBuf::from(location.path());
        if !requested.is_absolute() {
            return Err("Scan root must be absolute".into());
        }
        let metadata =
            std::fs::symlink_metadata(&requested).map_err(|_| "Cannot inspect scan root")?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("Scan roots cannot be reparse points".into());
            }
        }
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Select a real directory, not a link".into());
        }
        let path = requested
            .canonicalize()
            .map_err(|_| "Cannot resolve scan root")?;
        options.root = path.to_string_lossy().into_owned();
        options.validate()?;
        let dir = Dir::open_ambient_dir(&path, cap_std::ambient_authority())
            .map_err(|_| "Cannot open scan root")?;
        let meta = dir.dir_metadata().map_err(|_| "Cannot inspect open root")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if identity(&meta) != Some((metadata.dev(), metadata.ino())) {
                return Err("Scan root changed while opening".into());
            }
        }
        let volume = identity(&meta).map(|i| i.0);
        let entries = dir.entries().map_err(|_| "Cannot enumerate scan root")?;
        let mut root = node(0, None, &path, "directory", 0);
        let mut frames = vec![Frame {
            dir,
            entries,
            path,
            owner: 0,
            depth: 0,
            meta,
        }];
        let status = if options.max_depth == 0 {
            root.incomplete = true;
            root.incomplete_below_depth = true;
            frames.clear();
            "depth-limited"
        } else {
            "running"
        };
        let mut nodes = Vec::with_capacity(options.node_limit as usize);
        nodes.push(root);
        Ok(Self {
            _reservation: reservation,
            options,
            nodes,
            dirty: BTreeSet::from([0]),
            frames,
            identities: HashSet::new(),
            started: Instant::now(),
            status: status.into(),
            scanned: 0,
            issues: UsageIssues::default(),
            current: String::new(),
            volume,
            cache_epoch: crate::commands::fs::cache::listings()
                .lock()
                .map(|c| c.ticket())
                .unwrap_or(0),
        })
    }
    fn mark(&mut self, mut id: u32, depth: bool) {
        loop {
            let n = &mut self.nodes[id as usize];
            n.incomplete = true;
            if depth {
                n.incomplete_below_depth = true;
            }
            self.dirty.insert(id);
            match n.parent {
                Some(parent) => id = parent,
                None => break,
            }
        }
    }
    fn add(
        &mut self,
        mut id: u32,
        bytes: u64,
        disk: Option<u64>,
        unique: Option<u64>,
        file: bool,
        folder: bool,
    ) {
        loop {
            let n = &mut self.nodes[id as usize];
            match n.recursive_bytes.checked_add(bytes) {
                Some(v) => n.recursive_bytes = v,
                None => {
                    n.incomplete = true;
                    self.issues.overflow = self.issues.overflow.saturating_add(1);
                }
            }
            n.allocated_bytes = n
                .allocated_bytes
                .zip(disk)
                .and_then(|(a, b)| a.checked_add(b));
            n.deduplicated_bytes = n
                .deduplicated_bytes
                .zip(unique)
                .and_then(|(a, b)| a.checked_add(b));
            n.files = n.files.saturating_add(u64::from(file));
            n.folders = n.folders.saturating_add(u64::from(folder));
            self.dirty.insert(id);
            match n.parent {
                Some(parent) => id = parent,
                None => break,
            }
        }
    }
    fn issue(&mut self, id: u32, error: &io::Error) {
        match error.kind() {
            io::ErrorKind::PermissionDenied => self.issues.unreadable += 1,
            io::ErrorKind::NotFound => self.issues.disappeared += 1,
            _ => self.issues.provider_errors += 1,
        }
        self.nodes[id as usize].unreadable += 1;
        self.mark(id, false);
    }
    pub fn stop(&mut self, status: &str) {
        self.status = status.into();
        self.frames.clear();
        self.identities.clear();
        self.mark(0, false);
    }
    pub fn step(
        &mut self,
        cancel: &AtomicBool,
        authorize: &dyn Fn() -> Result<(), String>,
    ) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            self.stop("cancelled");
            return Ok(());
        }
        if self.started.elapsed().as_secs() >= u64::from(self.options.seconds)
            && !self.frames.is_empty()
        {
            self.stop("time-limited");
        }
        if self.status != "running" {
            return Ok(());
        }
        let tick = Instant::now();
        for _ in 0..STEP_ENTRIES {
            if cancel.load(Ordering::Relaxed) {
                self.stop("cancelled");
                break;
            }
            authorize()?;
            if self.scanned >= self.options.item_limit {
                self.stop("item-limited");
                break;
            }
            if tick.elapsed().as_millis() >= 25 {
                break;
            }
            if self.started.elapsed().as_secs() >= u64::from(self.options.seconds) {
                self.stop("time-limited");
                break;
            }
            let Some(frame) = self.frames.last_mut() else {
                self.status = "complete".into();
                break;
            };
            let owner = frame.owner;
            let depth = frame.depth + 1;
            let entry = match frame.entries.next() {
                Some(Ok(e)) => e,
                Some(Err(e)) => {
                    self.issue(owner, &e);
                    continue;
                }
                None => {
                    let frame = self.frames.pop().unwrap();
                    if frame
                        .dir
                        .dir_metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        != frame.meta.modified().ok()
                    {
                        self.issues.changed += 1;
                        self.mark(owner, false);
                    }
                    continue;
                }
            };
            self.scanned += 1;
            let name = entry.file_name();
            if !self.options.include_hidden && name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = frame.path.join(&name);
            if path.as_os_str().len() > PATH_LIMIT || name.to_str().is_none() {
                self.issues.unsupported += 1;
                self.mark(owner, false);
                continue;
            }
            self.current = path.to_string_lossy().into_owned();
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(e) => {
                    self.issue(owner, &e);
                    continue;
                }
            };
            let is_link = link(&meta);
            let directory = meta.is_dir() && !is_link;
            if !directory && !is_link && !meta.is_file() {
                self.issues.unsupported += 1;
                self.mark(owner, false);
                continue;
            }
            let bytes = if directory { 0 } else { meta.len() };
            let disk = if directory {
                cfg!(unix).then_some(0)
            } else {
                allocated(&meta)
            };
            let unique = if directory {
                cfg!(unix).then_some(0)
            } else if let Some(key) = identity(&meta) {
                if self.identities.contains(&key) {
                    Some(0)
                } else if self.identities.len() < self.options.node_limit as usize {
                    self.identities.insert(key);
                    Some(bytes)
                } else {
                    None
                }
            } else {
                None
            };
            let retained = self.nodes.len() < self.options.node_limit as usize;
            let id = if retained {
                let id = self.nodes.len() as u32;
                let mut n = node(
                    id,
                    Some(owner),
                    &path,
                    if is_link {
                        "link"
                    } else if directory {
                        "directory"
                    } else {
                        "file"
                    },
                    depth,
                );
                n.direct_bytes = bytes;
                self.nodes.push(n);
                id
            } else {
                let n = &mut self.nodes[owner as usize];
                n.omitted_entries += 1;
                n.omitted_bytes = n.omitted_bytes.saturating_add(bytes);
                owner
            };
            self.add(id, bytes, disk, unique, !directory, directory);
            if !directory {
                continue;
            }
            if depth >= self.options.max_depth {
                self.mark(id, true);
                continue;
            }
            if self.options.stay_on_volume && identity(&meta).map(|i| i.0) != self.volume {
                self.issues.unsupported += 1;
                self.mark(id, false);
                continue;
            }
            if identity(&meta)
                .is_some_and(|key| self.frames.iter().any(|f| identity(&f.meta) == Some(key)))
            {
                self.issues.cyclic += 1;
                self.mark(id, false);
                continue;
            }
            // The opened handle must still identify the enumerated directory.
            // DirEntry::open_dir is relative to its open parent, not an ambient path.
            match open_directory(&entry).and_then(|dir| {
                let actual = dir.dir_metadata()?;
                if identity(&actual) != identity(&meta) {
                    return Err(io::Error::other("directory changed"));
                }
                let entries = dir.entries()?;
                Ok((dir, entries, actual))
            }) {
                Ok((dir, entries, meta)) => self.frames.push(Frame {
                    dir,
                    entries,
                    path,
                    owner: id,
                    depth,
                    meta,
                }),
                Err(e) => self.issue(id, &e),
            }
        }
        Ok(())
    }
    pub fn batch(&mut self) -> UsageBatch {
        let ids = self
            .dirty
            .iter()
            .take(BATCH_NODES)
            .copied()
            .collect::<Vec<_>>();
        let nodes = ids
            .iter()
            .map(|id| self.nodes[*id as usize].clone())
            .collect();
        for id in ids {
            self.dirty.remove(&id);
        }
        let stale = self.started.elapsed().as_secs() > 60
            || crate::commands::fs::cache::listings()
                .lock()
                .map(|c| c.ticket() != self.cache_epoch)
                .unwrap_or(true);
        UsageBatch {
            nodes,
            pending: !self.dirty.is_empty(),
            status: self.status.clone(),
            elapsed_ms: self.started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32,
            scanned: self.scanned,
            current_path: self.current.clone(),
            issues: self.issues.clone(),
            stale,
            options: self.options.clone(),
            retained_nodes: self.nodes.len() as u32,
            // Conservative budget includes path storage, identity tables and OS
            // iterator buffers/handles at the maximum active depth, not RSS.
            estimated_memory_bytes: (self.nodes.capacity() * 8704
                + self.frames.len() * 73728
                + self.identities.len() * 64) as u32,
            memory_budget_bytes: self.options.node_limit * 8768
                + (u32::from(self.options.max_depth) + 1) * 73728,
        }
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;
    #[test]
    fn million_accounting_updates_are_bounded_and_overflow_is_partial() {
        let options = UsageOptions {
            root: std::env::temp_dir().to_string_lossy().into(),
            size_mode: Default::default(),
            max_depth: 32,
            item_limit: 1_000_000,
            node_limit: 16,
            seconds: 30,
            include_hidden: true,
            stay_on_volume: true,
        };
        let mut scan = Scan::new(options).unwrap();
        for _ in 0..1_000_000 {
            scan.add(0, 3, Some(0), None, true, false);
        }
        let batch = scan.batch();
        assert_eq!(batch.nodes[0].recursive_bytes, 3_000_000);
        assert_eq!(batch.nodes[0].files, 1_000_000);
        assert_eq!(batch.retained_nodes, 1);
        assert!(batch.estimated_memory_bytes <= batch.memory_budget_bytes);
        scan.add(0, u64::MAX, Some(u64::MAX), None, true, false);
        assert!(scan.nodes[0].incomplete);
        assert!(scan.issues.overflow > 0);
        scan.started = Instant::now() - std::time::Duration::from_secs(31);
        scan.step(&AtomicBool::new(false), &|| Ok(())).unwrap();
        assert_eq!(scan.status, "time-limited");
        scan.issue(0, &io::Error::from(io::ErrorKind::PermissionDenied));
        scan.issue(0, &io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(scan.issues.unreadable, 1);
        assert_eq!(scan.issues.disappeared, 1);
    }
}
