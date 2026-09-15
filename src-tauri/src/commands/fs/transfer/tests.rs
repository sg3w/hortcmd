// ============================================================
// Characterization tests for the transfer engine (tickets 029/035).
//
// Written against the behaviour the engine had *before* it was split
// into modules, and expected to pass unchanged afterwards — that is
// what makes the split verifiable rather than hopeful.
//
// The `Fake` provider below is the point: it can fail a rename or a
// remove on demand, which is how the "the old destination survives"
// rules are tested without corrupting a real file system. The
// `Recorder` sink does the same for progress, so the copy loops can run
// with no application around them.
// ============================================================

use super::copy::{
    copy_file_prog, copy_tree, hash_file_sha256, resolve_buf, transfer_one_with, CopyTuning,
};
use super::events::OpProgress;
use super::parallel::{commit_finished_jobs, flatten_tree, parallel_copy_file, CopyTask};
use super::plan::{
    commit, free_name, is_working_entry, plan_for, stale_working_entries, target_in, working_name,
    Plan, BACKUP_PREFIX, STAGE_PREFIX,
};
use super::progress::{Prog, ProgressSink, SharedProg};
use crate::errors::AppResult;
use crate::vfs::local::LocalProvider;
use crate::vfs::Provider;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ---------- An in-memory provider with injectable failures ----------

#[derive(Default)]
struct FakeFs {
    files: HashMap<crate::vfs::StreamPath, Vec<u8>>,
    dirs: HashSet<crate::vfs::StreamPath>,
}

struct Fake {
    store: String,
    fs: Arc<Mutex<FakeFs>>,
    /// `rename` fails when the source is in here.
    fail_rename: Mutex<HashSet<crate::vfs::StreamPath>>,
    /// `remove`/`remove_file` fails when the path is in here.
    fail_remove: Mutex<HashSet<crate::vfs::StreamPath>>,
    replaces: bool,
}

impl Fake {
    fn new(replaces: bool) -> Arc<Fake> {
        Arc::new(Fake {
            store: "fake".into(),
            fs: Arc::new(Mutex::new(FakeFs::default())),
            fail_rename: Mutex::new(HashSet::new()),
            fail_remove: Mutex::new(HashSet::new()),
            replaces,
        })
    }
    fn path(&self, raw: &str) -> crate::vfs::StreamPath {
        if self.store.contains(':') {
            crate::vfs::StreamPath::remote(raw).unwrap()
        } else {
            crate::vfs::StreamPath::from(raw)
        }
    }
    fn put(&self, path: &str, data: &str) {
        self.fs
            .lock()
            .unwrap()
            .files
            .insert(self.path(path), data.as_bytes().to_vec());
    }
    fn get(&self, path: &str) -> Option<String> {
        self.fs
            .lock()
            .unwrap()
            .files
            .get(&self.path(path))
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
    fn names_in(&self, dir: &str) -> Vec<String> {
        let fs = self.fs.lock().unwrap();
        fs.files
            .keys()
            .chain(fs.dirs.iter())
            .filter(|p| p.parent().as_ref() == Some(&self.path(dir)))
            .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
            .collect()
    }
}

struct FakeWriter {
    fs: Arc<Mutex<FakeFs>>,
    path: crate::vfs::StreamPath,
    buf: Vec<u8>,
}

impl Write for FakeWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.fs
            .lock()
            .unwrap()
            .files
            .insert(self.path.clone(), self.buf.clone());
        Ok(())
    }
}

impl Drop for FakeWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

impl Provider for Fake {
    fn store_id(&self) -> String {
        self.store.clone()
    }
    fn list(&self, raw: &str) -> AppResult<crate::commands::fs::dir::DirListing> {
        let path = self.path(raw);
        let fs = self.fs.lock().unwrap();
        if !fs.dirs.contains(&path) {
            return Err(io::Error::from(ErrorKind::NotFound).into());
        }
        let entries = fs
            .files
            .keys()
            .chain(fs.dirs.iter())
            .filter(|p| p.parent().as_ref() == Some(&path))
            .map(|p| crate::commands::fs::dir::DirEntry {
                name: p.file_name().unwrap().to_string_lossy().into_owned(),
                is_dir: fs.dirs.contains(p),
                is_symlink: false,
                size: fs.files.get(p).map_or(0, |b| b.len() as u64),
                modified: 0,
                hidden: false,
                readonly: false,
                executable: false,
                mode: None,
            })
            .collect();
        Ok(crate::commands::fs::dir::DirListing {
            path: raw.into(),
            entries,
        })
    }
    fn stat(&self, path: &crate::vfs::StreamPath) -> io::Result<crate::vfs::Stat> {
        let fs = self.fs.lock().unwrap();
        if fs.dirs.contains(path) {
            return Ok(crate::vfs::Stat {
                is_dir: true,
                len: 0,
            });
        }
        fs.files
            .get(path)
            .map(|b| crate::vfs::Stat {
                is_dir: false,
                len: b.len() as u64,
            })
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "nicht gefunden"))
    }
    fn exists(&self, path: &crate::vfs::StreamPath) -> bool {
        let fs = self.fs.lock().unwrap();
        fs.files.contains_key(path) || fs.dirs.contains(path)
    }
    fn is_dir(&self, path: &crate::vfs::StreamPath) -> bool {
        self.fs.lock().unwrap().dirs.contains(path)
    }
    fn read_dir(&self, path: &crate::vfs::StreamPath) -> io::Result<Vec<crate::vfs::StreamPath>> {
        let fs = self.fs.lock().unwrap();
        Ok(fs
            .files
            .keys()
            .chain(fs.dirs.iter())
            .filter(|p| p.parent().as_ref() == Some(path))
            .cloned()
            .collect())
    }
    fn open_read(&self, path: &crate::vfs::StreamPath) -> io::Result<Box<dyn Read + Send>> {
        let fs = self.fs.lock().unwrap();
        let data = fs
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "nicht gefunden"))?;
        Ok(Box::new(std::io::Cursor::new(data)))
    }
    fn create_write(&self, path: &crate::vfs::StreamPath) -> io::Result<Box<dyn Write + Send>> {
        Ok(Box::new(FakeWriter {
            fs: Arc::clone(&self.fs),
            path: path.to_path_buf(),
            buf: Vec::new(),
        }))
    }
    fn write(&self, path: &crate::vfs::StreamPath, data: &[u8]) -> io::Result<()> {
        self.fs
            .lock()
            .unwrap()
            .files
            .insert(path.to_path_buf(), data.to_vec());
        Ok(())
    }
    fn create_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        self.fs.lock().unwrap().dirs.insert(path.to_path_buf());
        Ok(())
    }
    fn rename(&self, from: &crate::vfs::StreamPath, to: &crate::vfs::StreamPath) -> io::Result<()> {
        if self.fail_rename.lock().unwrap().contains(from) {
            return Err(io::Error::other("rename abgelehnt (Test)"));
        }
        let mut fs = self.fs.lock().unwrap();
        if let Some(data) = fs.files.remove(from) {
            fs.files.insert(to.to_path_buf(), data);
            return Ok(());
        }
        if fs.dirs.remove(from) {
            fs.dirs.insert(to.to_path_buf());
            return Ok(());
        }
        Err(io::Error::new(ErrorKind::NotFound, "nicht gefunden"))
    }
    fn remove_file(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        if self.fail_remove.lock().unwrap().contains(path) {
            return Err(io::Error::other("remove abgelehnt (Test)"));
        }
        self.fs.lock().unwrap().files.remove(path);
        Ok(())
    }
    fn remove_dir_all(&self, path: &crate::vfs::StreamPath) -> io::Result<()> {
        if self.fail_remove.lock().unwrap().contains(path) {
            return Err(io::Error::other("remove abgelehnt (Test)"));
        }
        self.fs.lock().unwrap().dirs.remove(path);
        Ok(())
    }
    fn supports_staging(&self) -> bool {
        true
    }
    fn rename_replaces(&self) -> bool {
        self.replaces
    }
}

// ---------- Helpers ----------

static N: AtomicU64 = AtomicU64::new(0);

fn fresh_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rc-safe-{}-{}",
        std::process::id(),
        N.fetch_add(1, AtomicOrdering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fail() -> io::Error {
    io::Error::other("Kopieren fehlgeschlagen (Test)")
}

// ---------- Plan / naming ----------

#[test]
fn direct_mode_writes_straight_to_the_target() {
    let plan = Plan::direct(crate::vfs::StreamPath::from(PathBuf::from(
        "/dst/report.txt",
    )));
    assert!(!plan.is_staged());
    assert_eq!(plan.write_to, plan.target);
    assert!(!plan.replace);
}

#[test]
fn staged_mode_writes_next_to_the_target() {
    let fake = Fake::new(false);
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/report.txt")),
        true,
    )
    .unwrap();
    assert!(plan.is_staged());
    assert_eq!(plan.write_to.parent(), plan.target.parent());
    let name = plan.write_to.file_name().unwrap().to_str().unwrap();
    assert!(is_working_entry(name), "not recognizable: {}", name);
    assert!(name.starts_with('.'), "staging entries must be hidden");
}

#[test]
fn working_names_are_unique_and_avoid_existing_entries() {
    let fake = Fake::new(false);
    let a = working_name(
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/dst")),
        STAGE_PREFIX,
    )
    .unwrap();
    fake.put(a.native().unwrap().to_str().unwrap(), "taken");
    let b = working_name(
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/dst")),
        STAGE_PREFIX,
    )
    .unwrap();
    assert_ne!(a, b);
    assert!(!fake.exists(&b));
}

#[test]
fn only_our_own_prefixes_count_as_working_entries() {
    assert!(is_working_entry(".hortcmd-stage-1-2"));
    assert!(is_working_entry(".hortcmd-backup-1-2"));
    assert!(!is_working_entry("report.txt"));
    assert!(!is_working_entry(".hidden"));
    assert!(!is_working_entry(".hortcmd-notes"));
}

#[test]
fn leftovers_are_reported_but_never_touched() {
    let fake = Fake::new(false);
    fake.put("/dst/.hortcmd-stage-9-1", "half a copy");
    fake.put("/dst/keep.txt", "user data");
    let stale = stale_working_entries(&*fake, &crate::vfs::StreamPath::from(Path::new("/dst")));
    assert_eq!(stale, vec![".hortcmd-stage-9-1".to_string()]);
    // Reporting is all it does.
    assert_eq!(
        fake.get("/dst/.hortcmd-stage-9-1").as_deref(),
        Some("half a copy")
    );
}

// ---------- Commit ----------

#[test]
fn a_new_destination_is_committed_with_one_rename() {
    let fake = Fake::new(false);
    fake.put("/dst/.stage", "new");
    commit(
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/dst/.stage")),
        &crate::vfs::StreamPath::from(Path::new("/dst/f.txt")),
        false,
    )
    .unwrap();
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("new"));
    assert!(fake.get("/dst/.stage").is_none());
}

#[test]
fn an_atomic_replace_is_used_where_the_provider_has_one() {
    let fake = Fake::new(true);
    fake.put("/dst/f.txt", "old");
    fake.put("/dst/.stage", "new");
    commit(
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/dst/.stage")),
        &crate::vfs::StreamPath::from(Path::new("/dst/f.txt")),
        true,
    )
    .unwrap();
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("new"));
    // No backup was needed, so none is left behind.
    assert!(fake
        .names_in("/dst")
        .iter()
        .all(|n| !n.starts_with(BACKUP_PREFIX)));
}

#[test]
fn without_an_atomic_replace_the_old_destination_is_kept_until_the_end() {
    let fake = Fake::new(false);
    fake.put("/dst/f.txt", "old");
    fake.put("/dst/.stage", "new");
    commit(
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/dst/.stage")),
        &crate::vfs::StreamPath::from(Path::new("/dst/f.txt")),
        true,
    )
    .unwrap();
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("new"));
    // The backup is cleared only after the new name is in place.
    assert_eq!(fake.names_in("/dst"), vec!["f.txt".to_string()]);
}

#[test]
fn a_failed_commit_puts_the_old_destination_back() {
    let fake = Fake::new(false);
    fake.put("/dst/f.txt", "old");
    fake.put("/dst/.stage", "new");
    fake.fail_rename
        .lock()
        .unwrap()
        .insert(crate::vfs::StreamPath::from(PathBuf::from("/dst/.stage")));

    let err = commit(
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/dst/.stage")),
        &crate::vfs::StreamPath::from(Path::new("/dst/f.txt")),
        true,
    )
    .unwrap_err();
    assert!(err.to_string().contains("abgelehnt"));
    // Byte-for-byte the same file as before, under its own name.
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
    assert_eq!(fake.get("/dst/.stage").as_deref(), Some("new"));
    assert_eq!(
        fake.names_in("/dst").len(),
        2,
        "the rollback must leave no backup behind: {:?}",
        fake.names_in("/dst")
    );
}

// ---------- Ordering: copy, commit, source removal ----------

#[test]
fn a_failed_copy_leaves_the_old_destination_untouched() {
    let fake = Fake::new(false);
    fake.put("/dst/f.txt", "old");
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        true,
    )
    .unwrap();
    fake.put(plan.write_to.native().unwrap().to_str().unwrap(), "half");

    let mut errors = Vec::new();
    let err = transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        false,
        &mut errors,
        || Err(fail()),
    )
    .unwrap_err();

    assert!(err.to_string().contains("Test"));
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
    assert!(!fake.exists(&plan.write_to), "staging entry must be gone");
    assert!(errors.is_empty(), "clean cleanup reports nothing");
}

#[test]
fn a_cancelled_new_copy_leaves_no_file_under_the_final_name() {
    let fake = Fake::new(false);
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/new.txt")),
        false,
    )
    .unwrap();
    fake.put(plan.write_to.native().unwrap().to_str().unwrap(), "partial");

    let mut errors = Vec::new();
    let _ = transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/new.txt")),
        &plan,
        false,
        &mut errors,
        || Err(io::Error::new(ErrorKind::Interrupted, "abgebrochen")),
    );
    assert!(!fake.exists(&crate::vfs::StreamPath::from(Path::new("/dst/new.txt"))));
    assert!(
        fake.names_in("/dst").is_empty(),
        "{:?}",
        fake.names_in("/dst")
    );
}

#[test]
fn the_final_name_appears_only_after_the_copy_is_complete() {
    let fake = Fake::new(false);
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/new.txt")),
        false,
    )
    .unwrap();

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/new.txt")),
        &plan,
        false,
        &mut errors,
        || {
            // Mid-copy: the bytes exist, the final name does not.
            fake.put(
                plan.write_to.native().unwrap().to_str().unwrap(),
                "all of it",
            );
            assert!(!fake.exists(&crate::vfs::StreamPath::from(Path::new("/dst/new.txt"))));
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(fake.get("/dst/new.txt").as_deref(), Some("all of it"));
    assert!(errors.is_empty());
}

#[test]
fn a_failed_commit_reports_and_keeps_the_old_destination() {
    let fake = Fake::new(false);
    fake.put("/dst/f.txt", "old");
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        true,
    )
    .unwrap();
    fake.fail_rename
        .lock()
        .unwrap()
        .insert(plan.write_to.clone());

    let mut errors = Vec::new();
    let err = transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        false,
        &mut errors,
        || {
            fake.put(plan.write_to.native().unwrap().to_str().unwrap(), "new");
            Ok(())
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("abgelehnt"));
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
    assert!(!fake.exists(&plan.write_to));
}

#[test]
fn a_move_removes_its_source_only_after_the_commit() {
    let fake = Fake::new(false);
    fake.put("/src/f.txt", "payload");
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        false,
    )
    .unwrap();

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        true,
        &mut errors,
        || {
            fake.put(plan.write_to.native().unwrap().to_str().unwrap(), "payload");
            // Still the only committed copy at this point.
            assert!(fake.exists(&crate::vfs::StreamPath::from(Path::new("/src/f.txt"))));
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("payload"));
    assert!(!fake.exists(&crate::vfs::StreamPath::from(Path::new("/src/f.txt"))));
}

#[test]
fn a_failed_move_keeps_its_source() {
    let fake = Fake::new(false);
    fake.put("/src/f.txt", "payload");
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        false,
    )
    .unwrap();

    let mut errors = Vec::new();
    let _ = transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        true,
        &mut errors,
        || Err(fail()),
    );
    assert_eq!(fake.get("/src/f.txt").as_deref(), Some("payload"));
}

#[test]
fn an_undeletable_move_source_is_reported_and_both_copies_stay() {
    let fake = Fake::new(false);
    fake.put("/src/f.txt", "payload");
    fake.fail_remove
        .lock()
        .unwrap()
        .insert(crate::vfs::StreamPath::from(PathBuf::from("/src/f.txt")));
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        false,
    )
    .unwrap();

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        true,
        &mut errors,
        || {
            fake.put(plan.write_to.native().unwrap().to_str().unwrap(), "payload");
            Ok(())
        },
    )
    .unwrap();

    // The committed destination is never rolled back to "repair" this.
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("payload"));
    assert_eq!(fake.get("/src/f.txt").as_deref(), Some("payload"));
    assert_eq!(errors.len(), 1);
    // Both copies survive, and the failure names the source it could
    // not remove.
    assert_eq!(errors[0].item, "/src/f.txt");
    assert!(
        errors[0].error.op.as_deref() == Some("move"),
        "{:?}",
        errors[0]
    );
}

#[test]
fn a_failed_cleanup_is_reported_on_its_own() {
    let fake = Fake::new(false);
    let plan = Plan::staged(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        false,
    )
    .unwrap();
    fake.put(plan.write_to.native().unwrap().to_str().unwrap(), "half");
    fake.fail_remove
        .lock()
        .unwrap()
        .insert(plan.write_to.clone());

    let mut errors = Vec::new();
    let err = transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        false,
        &mut errors,
        || Err(fail()),
    )
    .unwrap_err();

    assert!(err.to_string().contains("Kopieren"), "{}", err);
    assert_eq!(errors.len(), 1);
    // The cleanup problem is reported on its own, naming the staging
    // entry it could not remove — never mistaken for the failure that
    // caused it.
    assert!(
        errors[0].item.contains(".hortcmd-stage-"),
        "{:?}",
        errors[0]
    );
}

#[test]
fn direct_mode_keeps_writing_to_the_final_name() {
    let fake = Fake::new(false);
    let plan = Plan::direct(crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")));

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/f.txt")),
        &plan,
        false,
        &mut errors,
        || {
            fake.put("/dst/f.txt", "written directly");
            Ok(())
        },
    )
    .unwrap();

    // No staging entry is created and none has to be cleaned up.
    assert_eq!(fake.names_in("/dst"), vec!["f.txt".to_string()]);
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("written directly"));
}

// ---------- Plan selection ----------

#[test]
fn safe_mode_stages_an_overwrite_instead_of_deleting_first() {
    let fake = Fake::new(false);
    fake.put("/dst/f.txt", "old");
    let plan = plan_for(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        true,
        true,
    )
    .unwrap();
    assert!(plan.is_staged());
    assert!(plan.replace);
    // Choosing the plan must not have touched anything yet.
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
}

#[test]
fn direct_mode_never_stages() {
    let fake = Fake::new(false);
    let plan = plan_for(
        &*fake,
        crate::vfs::StreamPath::from(PathBuf::from("/dst/f.txt")),
        false,
        false,
    )
    .unwrap();
    assert!(!plan.is_staged());
    assert!(!plan.replace);
}

// ---------- Parallel commit phase ----------

/// Builds `n` staged jobs whose bytes are already in place, as the
/// worker pool would leave them.
fn staged_jobs(fake: &Fake, names: &[&str]) -> Vec<Plan> {
    names
        .iter()
        .map(|name| {
            let plan = Plan::staged(
                fake,
                crate::vfs::StreamPath::from(PathBuf::from(format!("/dst/{}", name))),
                fake.exists(&crate::vfs::StreamPath::from(Path::new(&format!(
                    "/dst/{}",
                    name
                )))),
            )
            .unwrap();
            fake.put(
                plan.write_to.native().unwrap().to_str().unwrap(),
                &format!("new {}", name),
            );
            plan
        })
        .collect()
}

#[test]
fn one_failed_worker_does_not_stop_the_others_from_committing() {
    let fake = Fake::new(false);
    fake.put("/dst/b.txt", "old b");
    let plans = staged_jobs(&fake, &["a.txt", "b.txt", "c.txt"]);
    let refs: Vec<&Plan> = plans.iter().collect();

    let mut errors = Vec::new();
    // The middle worker failed.
    let ok = commit_finished_jobs(&*fake, &refs, &[true, false, true], false, &mut errors);

    assert_eq!(ok, 2);
    assert_eq!(fake.get("/dst/a.txt").as_deref(), Some("new a.txt"));
    assert_eq!(fake.get("/dst/c.txt").as_deref(), Some("new c.txt"));
    // The failed job neither published nor disturbed what was there.
    assert_eq!(fake.get("/dst/b.txt").as_deref(), Some("old b"));
    assert!(!fake.exists(&plans[1].write_to));
    assert!(errors.is_empty());
}

#[test]
fn a_cancelled_parallel_run_commits_nothing() {
    let fake = Fake::new(false);
    fake.put("/dst/a.txt", "old a");
    let plans = staged_jobs(&fake, &["a.txt", "b.txt"]);
    let refs: Vec<&Plan> = plans.iter().collect();

    let mut errors = Vec::new();
    let ok = commit_finished_jobs(&*fake, &refs, &[true, true], true, &mut errors);

    assert_eq!(ok, 0);
    assert_eq!(fake.get("/dst/a.txt").as_deref(), Some("old a"));
    assert!(!fake.exists(&crate::vfs::StreamPath::from(Path::new("/dst/b.txt"))));
    assert_eq!(fake.names_in("/dst"), vec!["a.txt".to_string()]);
}

#[test]
fn every_parallel_job_gets_its_own_staging_entry() {
    let fake = Fake::new(false);
    let plans = staged_jobs(&fake, &["a.txt", "b.txt", "c.txt"]);
    let mut names: Vec<_> = plans.iter().map(|p| p.write_to.clone()).collect();
    names.sort_by_key(|p| p.to_string_lossy().into_owned());
    names.dedup();
    assert_eq!(names.len(), 3, "workers must not share a staging name");
}

// ---------- The local provider's own commit behaviour ----------

#[test]
fn the_local_provider_commits_and_replaces() {
    let dir = fresh_dir();
    let local = crate::vfs::local::LocalProvider;
    assert!(local.supports_staging());

    let target = dir.join("report.txt");
    std::fs::write(&target, "old").unwrap();
    let stage = dir.join(".hortcmd-stage-test");
    std::fs::write(&stage, "new").unwrap();

    commit(
        &local,
        &crate::vfs::StreamPath::from(&stage),
        &crate::vfs::StreamPath::from(&target),
        true,
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    assert!(!stage.exists());
    let left: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec!["report.txt".to_string()]);
}

#[test]
fn the_local_provider_commits_a_directory_over_an_existing_one() {
    let dir = fresh_dir();
    let local = crate::vfs::local::LocalProvider;

    let target = dir.join("tree");
    std::fs::create_dir_all(target.join("sub")).unwrap();
    std::fs::write(target.join("sub/old.txt"), "old").unwrap();

    let stage = dir.join(".hortcmd-stage-dir");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("new.txt"), "new").unwrap();

    // A populated directory target cannot use a plain rename — the
    // backup route has to carry it.
    commit(
        &local,
        &crate::vfs::StreamPath::from(&stage),
        &crate::vfs::StreamPath::from(&target),
        true,
    )
    .unwrap();
    assert!(target.join("new.txt").exists());
    assert!(!target.join("sub").exists());
    assert!(!stage.exists());
}

// ---------- Characterization: naming, measuring, collisions ----------
//
// These pin the behaviour the extraction in TICKET-035 must preserve.
// They were written *before* the split and are expected to pass
// unchanged afterwards — that is what makes them characterization
// tests rather than tests of the new shape.

#[test]
fn a_free_name_counts_up_past_every_taken_one() {
    let fake = Fake::new(false);
    fake.put("/dst/report.txt", "x");
    assert_eq!(
        free_name(
            &*fake,
            &crate::vfs::StreamPath::from(Path::new("/dst/report.txt"))
        )
        .unwrap(),
        crate::vfs::StreamPath::from(PathBuf::from("/dst/report (2).txt"))
    );
    fake.put("/dst/report (2).txt", "x");
    fake.put("/dst/report (3).txt", "x");
    assert_eq!(
        free_name(
            &*fake,
            &crate::vfs::StreamPath::from(Path::new("/dst/report.txt"))
        )
        .unwrap(),
        crate::vfs::StreamPath::from(PathBuf::from("/dst/report (4).txt"))
    );
}

#[test]
fn a_free_name_keeps_the_extension_and_handles_names_without_one() {
    let fake = Fake::new(false);
    fake.put("/dst/archive.tar.gz", "x");
    // Only the last suffix counts as the extension, as everywhere else.
    assert_eq!(
        free_name(
            &*fake,
            &crate::vfs::StreamPath::from(Path::new("/dst/archive.tar.gz"))
        )
        .unwrap(),
        crate::vfs::StreamPath::from(PathBuf::from("/dst/archive.tar (2).gz"))
    );
    fake.put("/dst/README", "x");
    assert_eq!(
        free_name(
            &*fake,
            &crate::vfs::StreamPath::from(Path::new("/dst/README"))
        )
        .unwrap(),
        crate::vfs::StreamPath::from(PathBuf::from("/dst/README (2)"))
    );
}

#[test]
fn the_target_keeps_the_sources_base_name() {
    assert_eq!(
        target_in(
            &crate::vfs::StreamPath::from(Path::new("/dst")),
            &crate::vfs::StreamPath::from(Path::new("/src/deep/report.txt"))
        ),
        Some(crate::vfs::StreamPath::from("/dst/report.txt"))
    );
    // A path with no final component has no target.
    assert_eq!(
        target_in(
            &crate::vfs::StreamPath::from(Path::new("/dst")),
            &crate::vfs::StreamPath::from(Path::new("/"))
        ),
        None
    );
}

// ---------- Characterization: sequential copy ----------

#[test]
fn a_sequential_copy_reproduces_the_tree_and_its_bytes() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    fake.put("/src/sub/b.txt", "welt");
    {
        let mut fs = fake.fs.lock().unwrap();
        fs.dirs
            .insert(crate::vfs::StreamPath::from(PathBuf::from("/src")));
        fs.dirs
            .insert(crate::vfs::StreamPath::from(PathBuf::from("/src/sub")));
    }

    let mut prog = silent_prog();
    copy_tree(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src")),
        &crate::vfs::StreamPath::from(Path::new("/dst")),
        &mut prog,
        plain_tuning(),
    )
    .unwrap();

    assert_eq!(fake.get("/dst/a.txt").as_deref(), Some("hallo"));
    assert_eq!(fake.get("/dst/sub/b.txt").as_deref(), Some("welt"));
    // The source is untouched — this is a copy, not a move.
    assert_eq!(fake.get("/src/a.txt").as_deref(), Some("hallo"));
}

#[test]
fn a_cancelled_sequential_copy_stops_and_removes_the_partial_file() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    let cancel = Arc::new(AtomicBool::new(true)); // cancelled before it starts
    let mut prog = prog_with_cancel(cancel);

    let err = copy_file_prog(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/a.txt")),
        &crate::vfs::StreamPath::from(Path::new("/dst/a.txt")),
        &mut prog,
        plain_tuning(),
    )
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Interrupted);
    assert_eq!(fake.get("/dst/a.txt"), None, "no partial file may survive");
}

#[test]
fn verification_compares_the_checksums_of_source_and_target() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    let mut prog = silent_prog();
    let tune = CopyTuning {
        limit: 0,
        verify: true,
        buf_size: 4096,
    };
    copy_file_prog(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/a.txt")),
        &crate::vfs::StreamPath::from(Path::new("/dst/a.txt")),
        &mut prog,
        tune,
    )
    .unwrap();
    assert_eq!(fake.get("/dst/a.txt").as_deref(), Some("hallo"));
}

// ---------- Characterization: parallel copy ----------

#[test]
fn flattening_assigns_every_file_to_its_top_level_source() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "1");
    fake.put("/src/sub/b.txt", "22");
    {
        let mut fs = fake.fs.lock().unwrap();
        fs.dirs
            .insert(crate::vfs::StreamPath::from(PathBuf::from("/src")));
        fs.dirs
            .insert(crate::vfs::StreamPath::from(PathBuf::from("/src/sub")));
    }

    let mut tasks = Vec::new();
    flatten_tree(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src")),
        &crate::vfs::StreamPath::from(Path::new("/dst")),
        7,
        &mut tasks,
    )
    .unwrap();
    assert_eq!(tasks.len(), 2);
    // The index identifies which top-level source a file came from,
    // which is what lets one failing job be discarded on its own.
    assert!(tasks.iter().all(|t| t.idx == 7));
    // The target folders exist before any worker writes into them.
    assert!(fake.is_dir(&crate::vfs::StreamPath::from(Path::new("/dst/sub"))));
}

#[test]
fn a_cancelled_parallel_copy_writes_nothing() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    let task = CopyTask {
        src: crate::vfs::StreamPath::from(PathBuf::from("/src/a.txt")),
        dst: crate::vfs::StreamPath::from(PathBuf::from("/dst/a.txt")),
        idx: 0,
    };
    let sp = shared_prog(fake.clone(), Arc::new(AtomicBool::new(true)));
    let err = parallel_copy_file(&task, &sp, plain_tuning()).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Interrupted);
    assert_eq!(fake.get("/dst/a.txt"), None);
}

// ---------- Characterization: move semantics ----------

#[test]
fn a_move_that_fails_to_copy_keeps_its_source() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    let plan = Plan::direct(crate::vfs::StreamPath::from(PathBuf::from("/dst/a.txt")));
    let mut errors = Vec::new();

    let err = transfer_one_with(
        &*fake,
        &*fake,
        &crate::vfs::StreamPath::from(Path::new("/src/a.txt")),
        &plan,
        true, // remove_source
        &mut errors,
        || Err(fail()),
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Other);
    // The source is the only copy left — losing it would lose the file.
    assert_eq!(fake.get("/src/a.txt").as_deref(), Some("hallo"));
}

// ---------- Test-only helpers ----------

fn plain_tuning() -> CopyTuning {
    CopyTuning {
        limit: 0,
        verify: false,
        buf_size: 4096,
    }
}

fn silent_prog() -> Prog {
    prog_with_cancel(Arc::new(AtomicBool::new(false)))
}

/// Records every progress report instead of emitting it.
#[derive(Default)]
struct Recorder {
    reports: Mutex<Vec<OpProgress>>,
}

impl ProgressSink for Recorder {
    fn progress(&self, report: OpProgress) {
        self.reports.lock().unwrap().push(report);
    }
}

/// A `Prog` whose reports go nowhere but a recorder — the tests care
/// about the file-system effects, not the events.
fn prog_with_cancel(cancel: Arc<AtomicBool>) -> Prog {
    Prog {
        sink: Arc::new(Recorder::default()),
        id: "test".into(),
        op: "copy".into(),
        files_total: 1,
        bytes_total: 0,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel,
        pause: Arc::new(AtomicBool::new(false)),
    }
}

fn shared_prog(fake: Arc<Fake>, cancel: Arc<AtomicBool>) -> SharedProg {
    SharedProg {
        srcp: fake.clone(),
        dstp: fake,
        sink: Arc::new(Recorder::default()),
        id: "test".into(),
        op: "copy".into(),
        files_total: 1,
        bytes_total: 0,
        bytes_done: AtomicU64::new(0),
        files_done: AtomicU32::new(0),
        last: Mutex::new(Instant::now()),
        rate: Mutex::new((Instant::now(), 0)),
        cancel,
        pause: Arc::new(AtomicBool::new(false)),
    }
}

// ---------- Moved here with their subjects (TICKET-035) ----------

/// A fresh temporary directory on the real file system — these three
/// exercise the local provider, not the fake.
fn tmp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rc-transfer-{}-{}",
        std::process::id(),
        N.fetch_add(1, AtomicOrdering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
}

#[test]
fn resolve_buf_clamps() {
    assert_eq!(resolve_buf(0), 256 * 1024); // Standard
    assert_eq!(resolve_buf(1), 4 * 1024); // untere Grenze
    assert_eq!(resolve_buf(512), 512 * 1024);
    assert_eq!(resolve_buf(1_000_000), 16 * 1024 * 1024); // obere Grenze
}

#[test]
fn flatten_tree_collects_files_and_makes_dirs() {
    // Source tree with a subfolder; flatten must create the target folders
    // and yield one task per file (not for directories).
    let root = tmp_dir();
    let src = root.join("src");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("a.txt"), "A").unwrap();
    fs::write(src.join("sub/b.txt"), "B").unwrap();
    let dst = root.join("dst");

    let mut tasks: Vec<CopyTask> = Vec::new();
    flatten_tree(
        &LocalProvider,
        &LocalProvider,
        &crate::vfs::StreamPath::from(&src),
        &crate::vfs::StreamPath::from(&dst),
        0,
        &mut tasks,
    )
    .unwrap();

    assert_eq!(tasks.len(), 2, "zwei Dateien erwartet");
    assert!(dst.join("sub").is_dir(), "Zielordner angelegt");
    let names: Vec<String> = tasks
        .iter()
        .map(|t| t.dst.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"a.txt".to_string()));
    assert!(names.contains(&"b.txt".to_string()));
    assert!(tasks.iter().all(|t| t.idx == 0));
}

#[test]
fn hash_file_matches_known_vector() {
    // SHA-256("abc") is a known reference value – the basis of the
    // copy verification (source vs. target).
    let d = tmp_dir();
    write(&d, "abc.bin", "abc");
    let hex: String = hash_file_sha256(
        &LocalProvider,
        &crate::vfs::StreamPath::from(&d.join("abc.bin")),
    )
    .unwrap()
    .iter()
    .map(|b| format!("{:02x}", b))
    .collect();
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn failed_parallel_directory_preparation_does_not_schedule_file_writes() {
    let root = tmp_dir();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("a.txt"), b"source").unwrap();
    let blocked = root.join("blocked");
    fs::write(&blocked, b"existing destination").unwrap();
    let mut tasks = Vec::new();
    let result = flatten_tree(
        &LocalProvider,
        &LocalProvider,
        &crate::vfs::StreamPath::from(&source),
        &crate::vfs::StreamPath::from(&blocked.join("sub")),
        0,
        &mut tasks,
    );
    assert!(result.is_err());
    assert!(
        tasks.is_empty(),
        "unusable destination must not reach workers"
    );
    assert_eq!(fs::read(&blocked).unwrap(), b"existing destination");
    assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"source");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn canonical_uri_resolution_writes_the_exact_decoded_provider_object() {
    use crate::vfs::{
        self,
        location::{ProviderPath, VfsLocation},
    };
    for scheme in ["sftp", "ftp", "smb", "webdav"] {
        let host = format!("canonical-fixture-{scheme}");
        let location = VfsLocation::parse(&format!("{scheme}://u@{host}/")).unwrap();
        let mut fake = Fake::new(true);
        Arc::get_mut(&mut fake).unwrap().store = location.store_identity().0;
        crate::net::session::register_live(fake.clone());
        for name in ["%20", "a?b#c", "é", "e\u{301}", "a@b"] {
            let path = ProviderPath::from_raw("/safe").unwrap().join(name).unwrap();
            let uri = format!("{scheme}://u@{}{}", host.to_uppercase(), path.uri());
            let (provider, resolved) = vfs::resolve(&uri).unwrap();
            assert_eq!(resolved.to_string_lossy(), format!("/safe/{name}"));
            provider.write(&resolved, b"exact object").unwrap();
            assert_eq!(
                fake.get(&resolved.to_string_lossy()).as_deref(),
                Some("exact object")
            );
        }
        crate::net::session::unregister_live(&fake.store_id());
    }
}

fn remote_fixture() -> (Arc<Fake>, crate::net::session::TestConnection, String) {
    let host = format!("component-fixture-{}", crate::net::unique_id());
    let profile: crate::net::session::Profile = serde_json::from_value(serde_json::json!({
        "id":host, "name":"component fixture", "protocol":"webdav", "host":host,
        "port":443, "user":"u", "auth":"anonymous", "key_path":null, "start_path":"/"
    }))
    .unwrap();
    let prefix = format!("webdav://{}/", profile.connection_key());
    let mut fake = Fake::new(false);
    Arc::get_mut(&mut fake).unwrap().store = format!("webdav:{}", profile.connection_key());
    for dir in ["/", "/src", "/src//", "/src//folder", "/dst", "/dst//"] {
        fake.create_dir_all(&fake.path(dir)).unwrap();
    }
    let connection = crate::net::session::register_test_connection(profile, fake.clone());
    (fake, connection, prefix.trim_end_matches('/').to_owned())
}

#[test]
fn repeated_components_survive_planning_publication_rollback_and_journal_reopen() {
    use super::recovery::{
        self,
        types::{Intent, RecoveryKind, RecoverySettings, RecoveryState},
        Journal,
    };
    for failure in ["none", "publish", "rollback"] {
        let (fake, _connection, prefix) = remote_fixture();
        fake.put("/src//folder/file.txt", "new payload");
        fake.put("/dst//file.txt", "old payload");
        fake.put("/dst/file.txt", "different resource");
        let source_uri = format!("{prefix}/src//folder/file.txt");
        let destination_uri = format!("{prefix}/dst//");
        let source = crate::vfs::resolve(&source_uri).unwrap().1;
        let target = fake.path("/dst//file.txt");
        let review = super::preflight::build(
            RecoveryKind::Copy,
            std::slice::from_ref(&source_uri),
            &destination_uri,
            RecoverySettings::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        review.ensure_executable().unwrap();
        assert_eq!(review.mappings[0].source.stream_path().unwrap(), source);
        assert_eq!(review.mappings[0].target.stream_path().unwrap(), target);
        super::preflight::revalidate_source(
            &review.mappings[0],
            &*fake,
            &review.limits,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            crate::vfs::resolve(&review.mappings[0].target.resolve().unwrap())
                .unwrap()
                .1,
            target
        );
        let tree = super::preflight::build(
            RecoveryKind::Copy,
            &[format!("{prefix}/src//folder")],
            &destination_uri,
            RecoverySettings::default(),
            &AtomicBool::new(false),
        )
        .unwrap();
        tree.ensure_executable().unwrap();
        assert_eq!(tree.mappings[0].files, 1);

        let root = fresh_dir();
        let journal = Journal::open(&root).unwrap();
        journal
            .enqueue(
                "job",
                Intent::transfer(
                    RecoveryKind::Copy,
                    &[source_uri],
                    &destination_uri,
                    RecoverySettings::default(),
                )
                .unwrap(),
            )
            .unwrap();
        let handle = journal.begin("job").unwrap();
        let mut plan = Plan::staged(&*fake, target.clone(), true).unwrap();
        plan.track(&handle, &source, &*fake).unwrap();
        let stage = plan.write_to.clone();
        let backup = plan.backup.clone().unwrap();
        assert_eq!(stage.parent(), Some(fake.path("/dst//")));
        assert_eq!(backup.parent(), Some(fake.path("/dst//")));
        // Aliases produced by the old host-Path adapter must never be touched,
        // even by startup recovery's existence probes.
        let stage_alias = format!("/dst/{}", stage.file_name().unwrap().to_str().unwrap());
        let backup_alias = format!("/dst/{}", backup.file_name().unwrap().to_str().unwrap());
        fake.put(&stage_alias, "unrelated stage");
        fake.put(&backup_alias, "unrelated backup");
        if failure != "none" {
            fake.fail_rename.lock().unwrap().insert(stage.clone());
        }
        if failure == "rollback" {
            fake.fail_rename.lock().unwrap().insert(backup.clone());
        }
        let mut errors = Vec::new();
        let result = super::copy::transfer_one(
            &*fake,
            &*fake,
            &source,
            &plan,
            &mut silent_prog(),
            CopyTuning {
                limit: 0,
                verify: true,
                buf_size: 4096,
            },
            false,
            &mut errors,
        );
        assert_eq!(result.is_ok(), failure == "none");
        assert_eq!(
            fake.get("/dst/file.txt").as_deref(),
            Some("different resource")
        );
        assert_eq!(fake.get(&stage_alias).as_deref(), Some("unrelated stage"));
        assert_eq!(fake.get(&backup_alias).as_deref(), Some("unrelated backup"));
        assert_eq!(
            fake.get("/src//folder/file.txt").as_deref(),
            Some("new payload")
        );
        if failure == "rollback" {
            assert!(!fake.exists(&target));
            assert_eq!(
                fake.get(&backup.to_string_lossy()).as_deref(),
                Some("old payload")
            );
        } else {
            assert_eq!(
                fake.get("/dst//file.txt").as_deref(),
                Some(if failure == "none" {
                    "new payload"
                } else {
                    "old payload"
                })
            );
        }
        drop(plan);
        drop(handle);
        drop(journal);
        let reopened = Journal::open(&root).unwrap();
        let record = reopened.records().unwrap().remove(0);
        let item = &record.items[0];
        assert_eq!(
            crate::vfs::resolve(&item.target.resolve().unwrap())
                .unwrap()
                .1,
            target
        );
        assert_eq!(
            crate::vfs::resolve(&item.staging.as_ref().unwrap().resolve().unwrap())
                .unwrap()
                .1,
            stage
        );
        assert_eq!(
            crate::vfs::resolve(&item.backup.as_ref().unwrap().resolve().unwrap())
                .unwrap()
                .1,
            backup
        );
        let view = recovery::reconcile::entry(&reopened, &record);
        assert_eq!(
            view.state == RecoveryState::CleanupRequired,
            failure != "none"
        );
        // Remote recovery remains conservative: no native fingerprint or guessed deletion.
        assert!(recovery::reconcile::cleanup(&reopened, &record).is_err());
        assert_eq!(fake.get(&backup_alias).as_deref(), Some("unrelated backup"));
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn mcp_containment_and_descendant_checks_distinguish_empty_components() {
    let (fake, _connection, prefix) = remote_fixture();
    fake.put("/dst//file.txt", "inside");
    fake.put("/dst/file.txt", "outside");
    let root = format!("{prefix}/dst//");
    let inside = format!("{prefix}/dst//file.txt");
    let outside = format!("{prefix}/dst/file.txt");
    let config = crate::mcp::McpConfig {
        roots: vec![root.clone()],
        enabled: true,
        allow_write: true,
        ..Default::default()
    };
    crate::mcp::scope::require_in_scope(&config.roots, &inside).unwrap();
    assert!(crate::mcp::scope::require_in_scope(&config.roots, &outside).is_err());
    let (provider, path) =
        crate::mcp::scoped_provider::ScopedProvider::resolve(&root, &config).unwrap();
    assert_eq!(
        provider.read_dir(&path).unwrap(),
        vec![fake.path("/dst//file.txt")]
    );
    assert!(provider
        .rename(&fake.path("/dst//file.txt"), &fake.path("/dst/file.txt"))
        .is_err());
    assert_eq!(fake.get("/dst/file.txt").as_deref(), Some("outside"));
}

#[test]
fn cancelled_remote_copy_preserves_the_exact_target_and_normalized_alias() {
    let (fake, _connection, _) = remote_fixture();
    fake.put("/src//file.txt", "source");
    fake.put("/dst//file.txt", "old target");
    fake.put("/dst/file.txt", "unrelated target");
    let plan = Plan::staged(&*fake, fake.path("/dst//file.txt"), true).unwrap();
    let alias = format!(
        "/dst/{}",
        plan.write_to.file_name().unwrap().to_str().unwrap()
    );
    fake.put(&alias, "unrelated stage");
    let mut progress = silent_prog();
    progress
        .cancel
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let error = super::copy::transfer_one(
        &*fake,
        &*fake,
        &fake.path("/src//file.txt"),
        &plan,
        &mut progress,
        CopyTuning {
            limit: 0,
            verify: true,
            buf_size: 4096,
        },
        false,
        &mut Vec::new(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(!fake.exists(&plan.write_to));
    assert_eq!(fake.get("/src//file.txt").as_deref(), Some("source"));
    assert_eq!(fake.get("/dst//file.txt").as_deref(), Some("old target"));
    assert_eq!(
        fake.get("/dst/file.txt").as_deref(),
        Some("unrelated target")
    );
    assert_eq!(fake.get(&alias).as_deref(), Some("unrelated stage"));
}

#[test]
fn rename_refuses_other_stores_before_access_and_keeps_same_store_no_overwrite() {
    use crate::commands::fs::file::rename_entry;
    let (first, _first_session, a) = remote_fixture();
    let (second, _second_session, b) = remote_fixture();
    first.put("/src/file", "first");
    second.put("/src/file", "second");
    for (from, to) in [
        (format!("{a}/src/file"), format!("{b}/src/file")),
        (
            format!("{a}/src/file"),
            "/absent-local-rename-target".into(),
        ),
        (
            "/absent-local-rename-source".into(),
            format!("{a}/src/file"),
        ),
    ] {
        let error = rename_entry(from, to).unwrap_err();
        assert_eq!(&*error.code, crate::errors::code::VALIDATION_BAD_INPUT);
    }
    assert_eq!(first.get("/src/file").as_deref(), Some("first"));
    assert_eq!(second.get("/src/file").as_deref(), Some("second"));
    first.put("/src/taken", "keep");
    assert!(rename_entry(format!("{a}/src/file"), format!("{a}/src/taken")).is_err());
    assert_eq!(first.get("/src/taken").as_deref(), Some("keep"));
    rename_entry(format!("{a}/src/file"), format!("{a}/src/new")).unwrap();
    assert_eq!(first.get("/src/new").as_deref(), Some("first"));
    assert!(first.get("/src/file").is_none());
}
