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
    commit, free_name, is_working_entry, measure, plan_for, stale_working_entries, target_in,
    working_name, Plan, BACKUP_PREFIX, STAGE_PREFIX,
};
use super::progress::{Prog, ProgressSink, SharedProg};
use crate::errors::{AppError, AppResult};
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
    files: HashMap<PathBuf, Vec<u8>>,
    dirs: HashSet<PathBuf>,
}

struct Fake {
    fs: Arc<Mutex<FakeFs>>,
    /// `rename` fails when the source is in here.
    fail_rename: Mutex<HashSet<PathBuf>>,
    /// `remove`/`remove_file` fails when the path is in here.
    fail_remove: Mutex<HashSet<PathBuf>>,
    replaces: bool,
}

impl Fake {
    fn new(replaces: bool) -> Arc<Fake> {
        Arc::new(Fake {
            fs: Arc::new(Mutex::new(FakeFs::default())),
            fail_rename: Mutex::new(HashSet::new()),
            fail_remove: Mutex::new(HashSet::new()),
            replaces,
        })
    }
    fn put(&self, path: &str, data: &str) {
        self.fs
            .lock()
            .unwrap()
            .files
            .insert(PathBuf::from(path), data.as_bytes().to_vec());
    }
    fn get(&self, path: &str) -> Option<String> {
        self.fs
            .lock()
            .unwrap()
            .files
            .get(Path::new(path))
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
    fn names_in(&self, dir: &str) -> Vec<String> {
        let fs = self.fs.lock().unwrap();
        fs.files
            .keys()
            .chain(fs.dirs.iter())
            .filter(|p| p.parent() == Some(Path::new(dir)))
            .filter_map(|p| p.file_name()?.to_str().map(str::to_string))
            .collect()
    }
}

struct FakeWriter {
    fs: Arc<Mutex<FakeFs>>,
    path: PathBuf,
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
        "fake".into()
    }
    fn list(&self, _path: &str) -> AppResult<crate::commands::fs::dir::DirListing> {
        Err(AppError::from("nicht unterstützt"))
    }
    fn stat(&self, path: &Path) -> io::Result<crate::vfs::Stat> {
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
    fn exists(&self, path: &Path) -> bool {
        let fs = self.fs.lock().unwrap();
        fs.files.contains_key(path) || fs.dirs.contains(path)
    }
    fn is_dir(&self, path: &Path) -> bool {
        self.fs.lock().unwrap().dirs.contains(path)
    }
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let fs = self.fs.lock().unwrap();
        Ok(fs
            .files
            .keys()
            .chain(fs.dirs.iter())
            .filter(|p| p.parent() == Some(path))
            .cloned()
            .collect())
    }
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        let fs = self.fs.lock().unwrap();
        let data = fs
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "nicht gefunden"))?;
        Ok(Box::new(std::io::Cursor::new(data)))
    }
    fn create_write(&self, path: &Path) -> io::Result<Box<dyn Write + Send>> {
        Ok(Box::new(FakeWriter {
            fs: Arc::clone(&self.fs),
            path: path.to_path_buf(),
            buf: Vec::new(),
        }))
    }
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.fs
            .lock()
            .unwrap()
            .files
            .insert(path.to_path_buf(), data.to_vec());
        Ok(())
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.fs.lock().unwrap().dirs.insert(path.to_path_buf());
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
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
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        if self.fail_remove.lock().unwrap().contains(path) {
            return Err(io::Error::other("remove abgelehnt (Test)"));
        }
        self.fs.lock().unwrap().files.remove(path);
        Ok(())
    }
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
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
    let plan = Plan::direct(PathBuf::from("/dst/report.txt"));
    assert!(!plan.is_staged());
    assert_eq!(plan.write_to, plan.target);
    assert!(!plan.replace);
}

#[test]
fn staged_mode_writes_next_to_the_target() {
    let fake = Fake::new(false);
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/report.txt"), true);
    assert!(plan.is_staged());
    assert_eq!(plan.write_to.parent(), plan.target.parent());
    let name = plan.write_to.file_name().unwrap().to_str().unwrap();
    assert!(is_working_entry(name), "not recognizable: {}", name);
    assert!(name.starts_with('.'), "staging entries must be hidden");
}

#[test]
fn working_names_are_unique_and_avoid_existing_entries() {
    let fake = Fake::new(false);
    let a = working_name(&*fake, Path::new("/dst"), STAGE_PREFIX);
    fake.put(a.to_str().unwrap(), "taken");
    let b = working_name(&*fake, Path::new("/dst"), STAGE_PREFIX);
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
    let stale = stale_working_entries(&*fake, Path::new("/dst"));
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
        Path::new("/dst/.stage"),
        Path::new("/dst/f.txt"),
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
        Path::new("/dst/.stage"),
        Path::new("/dst/f.txt"),
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
        Path::new("/dst/.stage"),
        Path::new("/dst/f.txt"),
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
        .insert(PathBuf::from("/dst/.stage"));

    let err = commit(
        &*fake,
        Path::new("/dst/.stage"),
        Path::new("/dst/f.txt"),
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
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), true);
    fake.put(plan.write_to.to_str().unwrap(), "half");

    let mut errors = Vec::new();
    let err = transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
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
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/new.txt"), false);
    fake.put(plan.write_to.to_str().unwrap(), "partial");

    let mut errors = Vec::new();
    let _ = transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/new.txt"),
        &plan,
        false,
        &mut errors,
        || Err(io::Error::new(ErrorKind::Interrupted, "abgebrochen")),
    );
    assert!(!fake.exists(Path::new("/dst/new.txt")));
    assert!(
        fake.names_in("/dst").is_empty(),
        "{:?}",
        fake.names_in("/dst")
    );
}

#[test]
fn the_final_name_appears_only_after_the_copy_is_complete() {
    let fake = Fake::new(false);
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/new.txt"), false);

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/new.txt"),
        &plan,
        false,
        &mut errors,
        || {
            // Mid-copy: the bytes exist, the final name does not.
            fake.put(plan.write_to.to_str().unwrap(), "all of it");
            assert!(!fake.exists(Path::new("/dst/new.txt")));
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
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), true);
    fake.fail_rename
        .lock()
        .unwrap()
        .insert(plan.write_to.clone());

    let mut errors = Vec::new();
    let err = transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
        &plan,
        false,
        &mut errors,
        || {
            fake.put(plan.write_to.to_str().unwrap(), "new");
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
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
        &plan,
        true,
        &mut errors,
        || {
            fake.put(plan.write_to.to_str().unwrap(), "payload");
            // Still the only committed copy at this point.
            assert!(fake.exists(Path::new("/src/f.txt")));
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("payload"));
    assert!(!fake.exists(Path::new("/src/f.txt")));
}

#[test]
fn a_failed_move_keeps_its_source() {
    let fake = Fake::new(false);
    fake.put("/src/f.txt", "payload");
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);

    let mut errors = Vec::new();
    let _ = transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
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
        .insert(PathBuf::from("/src/f.txt"));
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
        &plan,
        true,
        &mut errors,
        || {
            fake.put(plan.write_to.to_str().unwrap(), "payload");
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
    let plan = Plan::staged(&*fake, PathBuf::from("/dst/f.txt"), false);
    fake.put(plan.write_to.to_str().unwrap(), "half");
    fake.fail_remove
        .lock()
        .unwrap()
        .insert(plan.write_to.clone());

    let mut errors = Vec::new();
    let err = transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
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
    let plan = Plan::direct(PathBuf::from("/dst/f.txt"));

    let mut errors = Vec::new();
    transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/f.txt"),
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
    let plan = plan_for(&*fake, PathBuf::from("/dst/f.txt"), true, true);
    assert!(plan.is_staged());
    assert!(plan.replace);
    // Choosing the plan must not have touched anything yet.
    assert_eq!(fake.get("/dst/f.txt").as_deref(), Some("old"));
}

#[test]
fn direct_mode_never_stages() {
    let fake = Fake::new(false);
    let plan = plan_for(&*fake, PathBuf::from("/dst/f.txt"), false, false);
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
                PathBuf::from(format!("/dst/{}", name)),
                fake.exists(Path::new(&format!("/dst/{}", name))),
            );
            fake.put(plan.write_to.to_str().unwrap(), &format!("new {}", name));
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
    assert!(!fake.exists(Path::new("/dst/b.txt")));
    assert_eq!(fake.names_in("/dst"), vec!["a.txt".to_string()]);
}

#[test]
fn every_parallel_job_gets_its_own_staging_entry() {
    let fake = Fake::new(false);
    let plans = staged_jobs(&fake, &["a.txt", "b.txt", "c.txt"]);
    let mut names: Vec<_> = plans.iter().map(|p| p.write_to.clone()).collect();
    names.sort();
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

    commit(&local, &stage, &target, true).unwrap();
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
    commit(&local, &stage, &target, true).unwrap();
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
        free_name(&*fake, Path::new("/dst/report.txt")),
        PathBuf::from("/dst/report (2).txt")
    );
    fake.put("/dst/report (2).txt", "x");
    fake.put("/dst/report (3).txt", "x");
    assert_eq!(
        free_name(&*fake, Path::new("/dst/report.txt")),
        PathBuf::from("/dst/report (4).txt")
    );
}

#[test]
fn a_free_name_keeps_the_extension_and_handles_names_without_one() {
    let fake = Fake::new(false);
    fake.put("/dst/archive.tar.gz", "x");
    // Only the last suffix counts as the extension, as everywhere else.
    assert_eq!(
        free_name(&*fake, Path::new("/dst/archive.tar.gz")),
        PathBuf::from("/dst/archive.tar (2).gz")
    );
    fake.put("/dst/README", "x");
    assert_eq!(
        free_name(&*fake, Path::new("/dst/README")),
        PathBuf::from("/dst/README (2)")
    );
}

#[test]
fn the_target_keeps_the_sources_base_name() {
    assert_eq!(
        target_in(Path::new("/dst"), Path::new("/src/deep/report.txt")),
        Some(PathBuf::from("/dst/report.txt"))
    );
    // A path with no final component has no target.
    assert_eq!(target_in(Path::new("/dst"), Path::new("/")), None);
}

#[test]
fn measuring_a_tree_counts_every_file_and_byte() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "12345");
    fake.put("/src/sub/b.txt", "123");
    fake.put("/src/sub/c.txt", "1");
    fake.fs.lock().unwrap().dirs.insert(PathBuf::from("/src"));
    fake.fs
        .lock()
        .unwrap()
        .dirs
        .insert(PathBuf::from("/src/sub"));

    let (files, bytes) = measure(&*fake, Path::new("/src"));
    assert_eq!((files, bytes), (3, 9));
    // A single file measures itself.
    assert_eq!(measure(&*fake, Path::new("/src/a.txt")), (1, 5));
    // An unreadable entry counts as one empty file rather than
    // aborting the totals.
    assert_eq!(measure(&*fake, Path::new("/src/gone")), (1, 0));
}

// ---------- Characterization: sequential copy ----------

#[test]
fn a_sequential_copy_reproduces_the_tree_and_its_bytes() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    fake.put("/src/sub/b.txt", "welt");
    {
        let mut fs = fake.fs.lock().unwrap();
        fs.dirs.insert(PathBuf::from("/src"));
        fs.dirs.insert(PathBuf::from("/src/sub"));
    }

    let mut prog = silent_prog();
    copy_tree(
        &*fake,
        &*fake,
        Path::new("/src"),
        Path::new("/dst"),
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
        Path::new("/src/a.txt"),
        Path::new("/dst/a.txt"),
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
        Path::new("/src/a.txt"),
        Path::new("/dst/a.txt"),
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
        fs.dirs.insert(PathBuf::from("/src"));
        fs.dirs.insert(PathBuf::from("/src/sub"));
    }

    let mut tasks = Vec::new();
    flatten_tree(
        &*fake,
        &*fake,
        Path::new("/src"),
        Path::new("/dst"),
        7,
        &mut tasks,
    );
    assert_eq!(tasks.len(), 2);
    // The index identifies which top-level source a file came from,
    // which is what lets one failing job be discarded on its own.
    assert!(tasks.iter().all(|t| t.idx == 7));
    // The target folders exist before any worker writes into them.
    assert!(fake.is_dir(Path::new("/dst/sub")));
}

#[test]
fn a_cancelled_parallel_copy_writes_nothing() {
    let fake = Fake::new(false);
    fake.put("/src/a.txt", "hallo");
    let task = CopyTask {
        src: PathBuf::from("/src/a.txt"),
        dst: PathBuf::from("/dst/a.txt"),
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
    let plan = Plan::direct(PathBuf::from("/dst/a.txt"));
    let mut errors = Vec::new();

    let err = transfer_one_with(
        &*fake,
        &*fake,
        Path::new("/src/a.txt"),
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
    flatten_tree(&LocalProvider, &LocalProvider, &src, &dst, 0, &mut tasks);

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
    let hex: String = hash_file_sha256(&LocalProvider, &d.join("abc.bin"))
        .unwrap()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    assert_eq!(
        hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
