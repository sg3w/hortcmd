use super::local::History;
use std::fs;
use std::os::unix::fs::PermissionsExt;
struct Temp(std::path::PathBuf);
impl Temp {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Temp, std::sync::Arc<History>) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "hortcmd-undo-test-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    let dir = Temp(path.canonicalize().unwrap());
    let history = History::open(&dir.path().join("history")).unwrap();
    (dir, history)
}
#[test]
fn rename_survives_restart_and_refuses_changed_or_occupied_names() {
    let (dir, history) = fixture();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "canary-content-not-in-journal").unwrap();
    let pending = history
        .prepare("rename", a.to_str().unwrap(), b.to_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(history.preview().unwrap()[0].status, "pending");
    fs::rename(&a, &b).unwrap();
    pending.finish().unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    drop(history);
    let history = History::open(&dir.path().join("history")).unwrap();
    assert!(!fs::read_to_string(dir.path().join("history/history.json"))
        .unwrap()
        .contains("canary-content"));
    fs::write(&a, "external").unwrap();
    assert_eq!(history.preview().unwrap()[0].status, "collision");
    assert!(history.execute(&id).is_err());
    fs::remove_file(&a).unwrap();
    assert_eq!(history.execute(&id).unwrap()[0].status, "restored");
    assert_eq!(
        fs::read_to_string(&a).unwrap(),
        "canary-content-not-in-journal"
    );
}
#[test]
fn changed_copy_is_refused_and_matching_copy_is_retained_safely() {
    let (dir, history) = fixture();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "original").unwrap();
    let pending = history
        .prepare("copy", a.to_str().unwrap(), b.to_str().unwrap())
        .unwrap()
        .unwrap();
    fs::copy(&a, &b).unwrap();
    pending.finish().unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    fs::write(&b, "changed").unwrap();
    assert_eq!(history.preview().unwrap()[0].status, "changed");
    assert!(history.execute(&id).is_err());
    let c = dir.path().join("c");
    let pending = history
        .prepare("copy", a.to_str().unwrap(), c.to_str().unwrap())
        .unwrap()
        .unwrap();
    fs::copy(&a, &c).unwrap();
    pending.finish().unwrap();
    let result = history.execute(&history.preview().unwrap()[0].id).unwrap();
    assert_eq!(result[0].status, "restored");
    assert!(!c.exists());
    assert_eq!(
        fs::read_to_string(result[0].retained.as_ref().unwrap()).unwrap(),
        "original"
    );
}
#[test]
fn directory_and_permissions_inverse_and_irreversible_actions() {
    let (dir, history) = fixture();
    let p = dir.path().join("new");
    let pending = history
        .prepare("mkdir", p.to_str().unwrap(), p.to_str().unwrap())
        .unwrap()
        .unwrap();
    fs::create_dir(&p).unwrap();
    pending.finish().unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    history.execute(&id).unwrap();
    assert!(!p.exists());
    fs::write(&p, "x").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    let pending = history
        .prepare("permissions", p.to_str().unwrap(), p.to_str().unwrap())
        .unwrap()
        .unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
    pending.finish().unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    history.execute(&id).unwrap();
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for kind in ["delete", "trash", "pack"] {
        assert!(history
            .prepare(kind, p.to_str().unwrap(), p.to_str().unwrap())
            .unwrap()
            .is_none());
    }
    assert!(history
        .prepare("copy", p.to_str().unwrap(), p.to_str().unwrap())
        .unwrap()
        .is_none());
    assert!(history
        .prepare("rename", "sftp://secret@host/a", "sftp://secret@host/b")
        .unwrap()
        .is_none());
}

#[test]
fn grouped_swaps_restore_both_contents_and_decline_changed_member() {
    let (dir, history) = fixture();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let tmp = dir.path().join("tmp");
    fs::write(&a, "A").unwrap();
    fs::write(&b, "B").unwrap();
    let pa = history
        .prepare_group(
            "rename",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            Some("swap"),
            true,
        )
        .unwrap()
        .unwrap();
    let pb = history
        .prepare_group(
            "rename",
            b.to_str().unwrap(),
            a.to_str().unwrap(),
            Some("swap"),
            true,
        )
        .unwrap()
        .unwrap();
    fs::rename(&a, &tmp).unwrap();
    fs::rename(&b, &a).unwrap();
    fs::rename(&tmp, &b).unwrap();
    pa.finish().unwrap();
    pb.finish().unwrap();
    assert!(history
        .preview()
        .unwrap()
        .iter()
        .all(|r| r.status == "recorded"));
    history.seal("swap").unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    assert!(history
        .preview()
        .unwrap()
        .iter()
        .all(|r| r.status == "ready"));
    assert!(history
        .execute(&id)
        .unwrap()
        .iter()
        .all(|r| r.status == "restored"));
    assert_eq!(fs::read_to_string(&a).unwrap(), "A");
    assert_eq!(fs::read_to_string(&b).unwrap(), "B");
}

#[test]
fn metadata_owner_and_tags_restore_and_nonempty_directory_is_refused() {
    use std::os::unix::fs::MetadataExt;
    let (dir, history) = fixture();
    let p = dir.path().join("p");
    fs::write(&p, "x").unwrap();
    let pending = history
        .prepare("owner", p.to_str().unwrap(), p.to_str().unwrap())
        .unwrap()
        .unwrap();
    let original = fs::metadata(&p).unwrap();
    std::os::unix::fs::chown(&p, Some(original.uid()), Some(original.gid())).unwrap();
    pending.finish().unwrap();
    assert_eq!(
        history.execute(&history.preview().unwrap()[0].id).unwrap()[0].status,
        "restored"
    );
    let pending = history
        .prepare("tags", p.to_str().unwrap(), p.to_str().unwrap())
        .unwrap()
        .unwrap();
    xattr::set(
        &p,
        "com.apple.metadata:_kMDItemUserTags",
        b"prior-is-absent",
    )
    .unwrap();
    pending.finish().unwrap();
    assert_eq!(
        history.execute(&history.preview().unwrap()[0].id).unwrap()[0].status,
        "restored"
    );
    assert!(xattr::get(&p, "com.apple.metadata:_kMDItemUserTags")
        .unwrap()
        .is_none());
    let folder = dir.path().join("folder");
    let pending = history
        .prepare("mkdir", folder.to_str().unwrap(), folder.to_str().unwrap())
        .unwrap()
        .unwrap();
    fs::create_dir(&folder).unwrap();
    pending.finish().unwrap();
    fs::write(folder.join("new"), "retain").unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    assert!(history.execute(&id).is_err());
    assert_eq!(fs::read_to_string(folder.join("new")).unwrap(), "retain");
}

#[test]
fn corrupt_history_symlinks_and_replaced_objects_never_authorize_undo() {
    let (dir, history) = fixture();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "A").unwrap();
    let pending = history
        .prepare("move", a.to_str().unwrap(), b.to_str().unwrap())
        .unwrap()
        .unwrap();
    fs::rename(&a, &b).unwrap();
    pending.finish().unwrap();
    let id = history.preview().unwrap()[0].id.clone();
    fs::rename(&b, dir.path().join("keep")).unwrap();
    fs::write(&b, "A").unwrap();
    assert!(history.execute(&id).is_err());
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&b, &link).unwrap();
    assert!(history
        .prepare("rename", link.to_str().unwrap(), a.to_str().unwrap())
        .unwrap()
        .is_none());
    assert!(History::open(&dir.path().join("history")).is_err());
    drop(history);
    fs::write(dir.path().join("history/history.json"), b"corrupt").unwrap();
    assert!(History::open(&dir.path().join("history")).is_err());
    assert_eq!(fs::read_to_string(&b).unwrap(), "A");
}

#[test]
fn history_is_bounded_and_expired_or_interrupted_entries_have_no_authority() {
    let (dir, history) = fixture();
    for i in 0..102 {
        let p = dir.path().join(format!("d{i}"));
        let pending = history
            .prepare("mkdir", p.to_str().unwrap(), p.to_str().unwrap())
            .unwrap()
            .unwrap();
        fs::create_dir(&p).unwrap();
        pending.finish().unwrap();
    }
    assert_eq!(history.preview().unwrap().len(), 100);
    drop(history);
    let path = dir.path().join("history/history.json");
    let mut data: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    data["records"][99]["created"] = serde_json::json!(0);
    data["records"][98]["state"] = serde_json::json!("applying");
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    let history = History::open(&dir.path().join("history")).unwrap();
    let preview = history.preview().unwrap();
    assert_eq!(preview[0].status, "expired");
    assert!(history.execute(&preview[0].id).is_err());
    assert_eq!(preview[1].status, "interrupted");
    assert!(history.execute(&preview[1].id).is_err());
}

#[test]
fn assessment_is_read_only_and_irreversible_for_overwrite_or_remote_targets() {
    let (dir, history) = fixture();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::write(&a, "A").unwrap();
    assert!(super::local::classify(
        "rename",
        &[a.to_string_lossy().into_owned()],
        b.to_str().unwrap(),
        false
    )
    .unwrap());
    assert!(history.preview().unwrap().is_empty());
    assert!(!b.exists());
    fs::write(&b, "B").unwrap();
    assert!(!super::local::classify(
        "rename",
        &[a.to_string_lossy().into_owned()],
        b.to_str().unwrap(),
        false
    )
    .unwrap());
    assert!(!super::local::classify(
        "copy",
        &[a.to_string_lossy().into_owned()],
        "sftp://host/b",
        false
    )
    .unwrap());
}

#[test]
fn grouped_failure_reports_only_the_restored_members_and_retains_the_rest() {
    let (dir, history) = fixture();
    let parent = dir.path().join("original");
    fs::create_dir(&parent).unwrap();
    let a = parent.join("a");
    let b = dir.path().join("b");
    let x = dir.path().join("x");
    let y = dir.path().join("y");
    fs::write(&a, "A").unwrap();
    fs::write(&b, "B").unwrap();
    let pa = history
        .prepare_group(
            "move",
            a.to_str().unwrap(),
            x.to_str().unwrap(),
            Some("moves"),
            false,
        )
        .unwrap()
        .unwrap();
    let pb = history
        .prepare_group(
            "move",
            b.to_str().unwrap(),
            y.to_str().unwrap(),
            Some("moves"),
            false,
        )
        .unwrap()
        .unwrap();
    fs::rename(&a, &x).unwrap();
    fs::rename(&b, &y).unwrap();
    pa.finish().unwrap();
    pb.finish().unwrap();
    history.seal("moves").unwrap();
    fs::remove_dir(&parent).unwrap();
    let result = history.execute(&history.preview().unwrap()[0].id).unwrap();
    assert_eq!(result.iter().filter(|r| r.status == "restored").count(), 1);
    let failed = result.iter().find(|r| r.status == "failed").unwrap();
    assert_eq!(
        fs::read_to_string(failed.retained.as_ref().unwrap()).unwrap(),
        "A"
    );
    assert_eq!(fs::read_to_string(&b).unwrap(), "B");
    assert!(history.forget(&failed.id).is_err());
    fs::remove_file(failed.retained.as_ref().unwrap()).unwrap();
    assert_eq!(history.forget(&failed.id).unwrap().len(), 1);
}
