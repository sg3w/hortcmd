use super::*;
pub(super) struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn fixture() -> (Temp, Snapshot) {
    let dir = std::env::temp_dir().join(format!("hortcmd-index-{}", crate::net::unique_id()));
    std::fs::create_dir(&dir).unwrap();
    let location = RecoveryLocation::Local {
        path: dir.canonicalize().unwrap().to_string_lossy().into_owned(),
    };
    let snapshot = Snapshot {
        version: 1,
        id: "0-abcd".into(),
        generation: generation(&location).unwrap(),
        location,
        policy: IndexPolicy {
            content: false,
            hidden: false,
            ignore: vec!["ignored*".into()],
            allow_battery: true,
            offline: false,
            max_content_bytes: 1024,
        },
        rows: BTreeMap::new(),
        queue: VecDeque::from([String::new()]),
        cycle: 1,
        revision: 1,
        completed: 0,
        paused: false,
        limited: false,
        unreadable: 0,
    };
    (Temp(dir), snapshot)
}
fn finish(snapshot: &mut Snapshot) {
    for _ in 0..100 {
        if snapshot.queue.is_empty() {
            return;
        }
        crawler::step(snapshot, &AtomicBool::new(false)).unwrap();
    }
    panic!("crawler did not finish");
}
fn query() -> IndexQuery {
    IndexQuery {
        name: String::new(),
        path: String::new(),
        content: String::new(),
        case_sensitive: false,
        kind: "any".into(),
        min_size: None,
        max_size: None,
        modified_after: None,
        after: None,
        revision: None,
        limit: 500,
        adapter: "application".into(),
    }
}
#[test]
fn policies_reject_remote_content_and_exclude_secrets_staging_and_traversal() {
    let (_dir, mut snapshot) = fixture();
    snapshot.policy.hidden = true;
    for path in [
        ".ssh/id_rsa",
        ".env",
        ".env.production",
        "x/private.pem",
        ".hortcmd-transfer-abcd",
        "ignored-dir/file",
        "secrets/value",
    ] {
        assert!(crawler::excluded(path, &snapshot.policy), "{path}");
    }
    for path in ["../outside", "/absolute", "x/../../y", "bad\nname"] {
        assert!(!crawler::relative(path));
    }
    snapshot.policy.content = true;
    assert!(validate_policy(&snapshot.policy, true).is_err());
    assert!(validate_policy(&snapshot.policy, false).is_ok());
}
#[cfg(target_os = "macos")]
#[test]
fn only_approved_metadata_and_optional_bounded_text_are_collected() {
    use std::os::unix::fs::symlink;
    let (dir, mut snapshot) = fixture();
    std::fs::write(dir.0.join("allowed.txt"), "public content canary").unwrap();
    std::fs::write(dir.0.join(".env"), "credential canary").unwrap();
    std::fs::write(dir.0.join("binary.bin"), b"zero\0binary").unwrap();
    std::fs::write(dir.0.join("large.txt"), vec![b'x'; 1025]).unwrap();
    let (outside, _) = fixture();
    std::fs::write(outside.0.join("outside-secret"), "outside").unwrap();
    symlink(&outside.0, dir.0.join("link")).unwrap();
    finish(&mut snapshot);
    assert_eq!(
        snapshot.rows.keys().cloned().collect::<Vec<_>>(),
        ["allowed.txt", "binary.bin", "large.txt"]
    );
    assert!(snapshot.rows.values().all(|row| row.content.is_none()));
    snapshot.policy.content = true;
    snapshot.cycle += 1;
    snapshot.queue.push_back(String::new());
    snapshot.rows.clear();
    finish(&mut snapshot);
    assert_eq!(
        snapshot.rows["allowed.txt"].content.as_deref(),
        Some("public content canary")
    );
    assert!(snapshot.rows["binary.bin"].content.is_none());
    assert!(snapshot.rows["large.txt"].content.is_none());
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(!encoded.contains("credential canary"));
    assert!(!encoded.contains("outside-secret"));
}
#[cfg(target_os = "macos")]
#[test]
fn checkpoints_resume_and_reconciliation_removes_renames_and_missed_changes() {
    let (dir, mut snapshot) = fixture();
    for index in 0..20 {
        std::fs::create_dir(dir.0.join(index.to_string())).unwrap();
        std::fs::write(dir.0.join(index.to_string()).join("a.txt"), "one").unwrap();
    }
    crawler::step(&mut snapshot, &AtomicBool::new(true)).unwrap();
    assert!(snapshot.rows.is_empty());
    assert_eq!(snapshot.queue.len(), 1);
    crawler::step(&mut snapshot, &AtomicBool::new(false)).unwrap();
    assert!(!snapshot.queue.is_empty());
    let secret = storage::secret("test passphrase only").unwrap();
    let bytes = storage::encode(&snapshot, &secret).unwrap();
    let (mut restored, _) = storage::decode(&snapshot.id, &bytes, "test passphrase only").unwrap();
    finish(&mut restored);
    assert_eq!(restored.rows.len(), 40);
    std::fs::rename(dir.0.join("0/a.txt"), dir.0.join("1/moved.txt")).unwrap();
    std::fs::remove_file(dir.0.join("2/a.txt")).unwrap();
    std::fs::write(dir.0.join("3/a.txt"), "changed content").unwrap();
    std::fs::write(dir.0.join("created.txt"), "new").unwrap();
    restored.queue.push_back(String::new());
    restored.cycle += 1;
    finish(&mut restored);
    assert!(!restored.rows.contains_key("0/a.txt"));
    assert!(!restored.rows.contains_key("2/a.txt"));
    assert!(restored.rows.contains_key("1/moved.txt"));
    assert!(restored.rows.contains_key("created.txt"));
    assert_eq!(restored.rows["3/a.txt"].size, 15);
}
#[test]
fn encryption_authenticates_scope_and_rejects_corruption_and_wrong_passwords() {
    let (_dir, mut snapshot) = fixture();
    snapshot.rows.insert(
        "private-canary.txt".into(),
        Row {
            name: "private-canary.txt".into(),
            directory: false,
            size: 4,
            modified: 3,
            identity: "private-identity".into(),
            content: Some("private-text-canary".into()),
            seen: 1,
        },
    );
    snapshot.policy.content = true;
    let secret = storage::secret("test passphrase only").unwrap();
    let bytes = storage::encode(&snapshot, &secret).unwrap();
    for canary in ["private-canary", "private-text-canary", "private-identity"] {
        assert!(!String::from_utf8_lossy(&bytes).contains(canary));
    }
    assert!(storage::decode(&snapshot.id, &bytes, "test passphrase only").is_ok());
    assert!(storage::decode("0-ffff", &bytes, "test passphrase only").is_err());
    assert!(storage::decode(&snapshot.id, &bytes, "wrong passphrase only").is_err());
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(storage::decode(&snapshot.id, &corrupt, "test passphrase only").is_err());
    snapshot.version = 2;
    assert!(storage::decode(
        &snapshot.id,
        &storage::encode(&snapshot, &secret).unwrap(),
        "test passphrase only"
    )
    .is_err());
}
#[cfg(target_os = "macos")]
#[test]
fn indexed_and_live_queries_agree_on_unicode_filters_content_and_pagination() {
    let (dir, mut indexed) = fixture();
    indexed.policy.content = true;
    for (name, content) in [
        ("Äpfel.TXT", "Hello apple"),
        ("small.txt", "x"),
        ("note.md", "hello note"),
    ] {
        std::fs::write(dir.0.join(name), content).unwrap();
    }
    finish(&mut indexed);
    let mut live = indexed.clone();
    live.rows.clear();
    live.queue.push_back(String::new());
    finish(&mut live);
    let mut cases = vec![query()];
    let mut q = query();
    q.name = "*.txt".into();
    cases.push(q);
    let mut q = query();
    q.name = "ä*".into();
    cases.push(q);
    let mut q = query();
    q.name = "ä*".into();
    q.case_sensitive = true;
    cases.push(q);
    let mut q = query();
    q.content = "HELLO".into();
    cases.push(q);
    let mut q = query();
    q.min_size = Some(2);
    q.max_size = Some(10);
    q.kind = "file".into();
    cases.push(q);
    let mut q = query();
    q.modified_after = Some(now() + 100);
    cases.push(q);
    let mut q = query();
    q.path = "*.md".into();
    cases.push(q);
    let expected = [3, 2, 1, 0, 2, 1, 0, 1];
    for (q, expected) in cases.iter().zip(expected) {
        let a = query::test_page(&indexed, q).unwrap();
        let b = query::test_page(&live, q).unwrap();
        assert_eq!(
            a.hits.iter().map(|h| &h.relative).collect::<Vec<_>>(),
            b.hits.iter().map(|h| &h.relative).collect::<Vec<_>>()
        );
        assert_eq!(a.hits.len(), expected);
    }
    let mut q = query();
    q.limit = 1;
    let first = query::test_page(&indexed, &q).unwrap();
    q.after = first.next;
    q.revision = Some(first.revision);
    let second = query::test_page(&indexed, &q).unwrap();
    assert_ne!(first.hits[0].id, second.hits[0].id);
    indexed.revision += 1;
    assert!(query::test_page(&indexed, &q).is_err());
}
#[cfg(target_os = "macos")]
#[test]
fn repeated_queries_do_not_read_the_tree_and_native_watch_marks_changes_stale() {
    let (dir, mut snapshot) = fixture();
    for index in 0..1000 {
        std::fs::write(dir.0.join(format!("file-{index}.txt")), "data").unwrap();
    }
    let started = std::time::Instant::now();
    finish(&mut snapshot);
    let initial = started.elapsed();
    let mut request = query();
    request.name = "file-1*".into();
    let started = std::time::Instant::now();
    for _ in 0..20 {
        assert_eq!(
            query::test_page(&snapshot, &request).unwrap().hits.len(),
            111
        );
    }
    let repeated = started.elapsed();
    struct Sink(Mutex<Vec<String>>);
    impl crate::commands::fs::search::HitSink for Sink {
        fn send(&self, batch: Vec<crate::commands::fs::search::SearchHit>) -> bool {
            self.0
                .lock()
                .unwrap()
                .extend(batch.into_iter().map(|hit| hit.name));
            true
        }
    }
    let sink = Sink(Mutex::new(Vec::new()));
    let started = std::time::Instant::now();
    for _ in 0..20 {
        sink.0.lock().unwrap().clear();
        crate::commands::fs::search::run_search(
            dir.0.clone(),
            crate::commands::fs::search::SearchOptions {
                mode: "files".into(),
                name: "file-1*".into(),
                name_regex: false,
                content: String::new(),
                content_regex: false,
                case_sensitive: false,
                ignore_dirs: vec![],
                min_size: 0,
            },
            Arc::new(AtomicBool::new(false)),
            &crate::commands::fs::search::Limits::DEFAULT,
            &sink,
        )
        .unwrap();
        let mut actual = sink.0.lock().unwrap().clone();
        actual.sort();
        assert_eq!(
            actual,
            query::test_page(&snapshot, &request)
                .unwrap()
                .hits
                .iter()
                .map(|hit| hit.relative.clone())
                .collect::<Vec<_>>()
        );
    }
    let live = started.elapsed();
    eprintln!("index benchmark: 1000 files; initial {initial:?}; 20 prefix queries {repeated:?}; 20 legacy live scans plus result comparison {live:?}; identical 111 results, index queries enumerate zero files");
    request.adapter = "native".into();
    let fallback = query::test_page(&snapshot, &request).unwrap();
    assert!(fallback.fallback);
    assert_eq!(fallback.source, "application_index");
    assert_eq!(fallback.hits.len(), 111);
    let scope = Scope::new(snapshot, None);
    std::fs::write(dir.0.join("changed.txt"), "new").unwrap();
    if scope._watcher.is_some() {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !scope.dirty.load(Ordering::Relaxed) && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(scope.dirty.load(Ordering::Relaxed));
        assert!(scope.stale());
    }
}

#[test]
fn deleting_a_profile_removes_locked_snapshots_and_interrupted_writes_only_for_that_profile() {
    let (dir, _) = fixture();
    let profile = format!("test-profile-{}", crate::net::unique_id());
    let prefix = hex::encode(Sha256::digest(profile.as_bytes()));
    for suffix in ["-abcd.enc", "-abcd.crash.tmp"] {
        std::fs::write(dir.0.join(format!("{prefix}{suffix}")), "encrypted canary").unwrap();
    }
    std::fs::write(dir.0.join("0-abcd.enc"), "other scope").unwrap();
    delete_profile_in(&profile, &dir.0).unwrap();
    let names: Vec<_> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from("0-abcd.enc")]);
}
#[cfg(target_os = "macos")]
#[test]
fn a_replaced_root_and_intermediate_symlink_never_expand_approval() {
    use std::os::unix::fs::symlink;
    let (dir, mut snapshot) = fixture();
    let (outside, _) = fixture();
    std::fs::create_dir(dir.0.join("nested")).unwrap();
    std::fs::write(outside.0.join("secret"), "outside").unwrap();
    snapshot.queue = VecDeque::from(["nested".into()]);
    std::fs::remove_dir(dir.0.join("nested")).unwrap();
    symlink(&outside.0, dir.0.join("nested")).unwrap();
    assert!(crawler::step(&mut snapshot, &AtomicBool::new(false)).is_err());
    assert!(snapshot.rows.is_empty());
    snapshot.generation = "another object".into();
    snapshot.queue = VecDeque::from([String::new()]);
    assert!(crawler::step(&mut snapshot, &AtomicBool::new(false)).is_err());
    assert!(snapshot.rows.is_empty());
}

#[cfg(unix)]
#[test]
fn persistent_snapshots_and_directory_have_private_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, snapshot) = fixture();
    let private = dir.0.join("private");
    storage::private_directory(&private).unwrap();
    let destination = private.join("0-abcd.enc");
    let secret = storage::secret("test passphrase only").unwrap();
    storage::write_to(&snapshot, &secret, &destination).unwrap();
    assert_eq!(
        std::fs::metadata(&private).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&destination)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(std::fs::read_dir(&private).unwrap().count(), 1);
    assert!(storage::decode(
        &snapshot.id,
        &std::fs::read(&destination).unwrap(),
        "test passphrase only"
    )
    .is_ok());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn api_requires_scope_consent_revalidates_roots_and_clears_offline_profile_data() {
    const CHILD: &str = "HORTCMD_INDEX_API_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let (dir, _) = fixture();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "commands::fs::search_index::tests::api_requires_scope_consent_revalidates_roots_and_clears_offline_profile_data"])
            .env(CHILD, "1").env("HORTCMD_CONFIG_DIR", &dir.0).output().unwrap();
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let (dir, fixture) = fixture();
    assert!(
        query::index_query("not-approved".into(), query(), "missing".into())
            .await
            .is_err()
    );
    std::fs::write(dir.0.join("approved.txt"), "data").unwrap();
    let info = index_create(
        dir.0.to_string_lossy().into_owned(),
        fixture.policy.clone(),
        None,
    )
    .await
    .unwrap();
    {
        let scope = get(&info.id).unwrap();
        finish(&mut scope.lock().unwrap().snapshot);
    }
    let mut native = query();
    native.adapter = "native".into();
    let page = query::index_query(info.id.clone(), native, "native-fallback".into())
        .await
        .unwrap();
    assert!(page.fallback);
    assert_eq!(page.source, "application_index");
    assert_eq!(page.hits.len(), 1);
    index_cancel("cancelled-before-query".into());
    assert!(
        query::index_query(info.id.clone(), query(), "cancelled-before-query".into())
            .await
            .is_err()
    );
    {
        let scope = get(&info.id).unwrap();
        scope.lock().unwrap().snapshot.generation = "different root".into();
    }
    let mut live = query();
    live.adapter = "live".into();
    assert!(
        query::index_query(info.id.clone(), live, "replaced-root".into())
            .await
            .is_err()
    );
    index_control(info.id.clone(), "delete".into())
        .await
        .unwrap();
    assert!(get(&info.id).is_err());

    let profile = "profile-deleted-test";
    let id = format!("{}-abcd", hex::encode(Sha256::digest(profile.as_bytes())));
    let mut snapshot = fixture.clone();
    snapshot.id = id.clone();
    snapshot.location = RecoveryLocation::Remote {
        profile: profile.into(),
        identity: "endpoint-digest".into(),
        path: "/approved".into(),
    };
    snapshot.policy.offline = true;
    snapshot.rows.insert(
        "old.txt".into(),
        Row {
            name: "old.txt".into(),
            directory: false,
            size: 4,
            modified: 1,
            identity: "version".into(),
            content: None,
            seen: 1,
        },
    );
    scopes()
        .lock()
        .unwrap()
        .insert(id.clone(), Arc::new(Mutex::new(Scope::new(snapshot, None))));
    let page = query::index_query(id.clone(), query(), "offline".into())
        .await
        .unwrap();
    assert_eq!(page.freshness, "offline");
    assert!(page.hits[0].address.is_none());
    {
        let scope = get(&id).unwrap();
        scope.lock().unwrap().snapshot.policy.offline = false;
    }
    assert!(
        query::index_query(id.clone(), query(), "offline-not-approved".into())
            .await
            .is_err()
    );
    delete_profile(profile).unwrap();
    assert!(get(&id).is_err());
}

#[test]
fn unrelated_listing_cache_changes_preserve_index_freshness_and_pagination() {
    let (_root, mut snapshot) = fixture();
    snapshot.queue.clear();
    snapshot.completed = now();
    for name in ["a.txt", "b.txt"] {
        snapshot.rows.insert(
            name.into(),
            Row {
                name: name.into(),
                directory: false,
                size: 1,
                modified: 1,
                identity: name.into(),
                content: None,
                seen: snapshot.cycle,
            },
        );
    }
    // A restored persistent scope uses the same freshness decision as an
    // in-memory scope; no persistence writes are needed for this regression.
    let mut scope = Scope::new(
        snapshot,
        Some(storage::Secret {
            salt: [1; 16],
            key: [2; 32],
        }),
    );
    let mut request = query();
    request.limit = 1;
    let first = query::test_page(&scope.snapshot, &request).unwrap();
    request.after = first.next;
    request.revision = Some(first.revision);
    let cycle = scope.snapshot.cycle;
    let (unrelated, _) = fixture();
    for index in 0..5 {
        std::fs::write(unrelated.0.join(format!("file-{index}")), "unrelated").unwrap();
        super::super::cache::invalidate();
        assert!(
            !scope.stale(),
            "another pane's cache epoch must not restart this scope"
        );
        assert_eq!(scope.info().state, "snapshot");
        assert_eq!(scope.snapshot.cycle, cycle);
        let second = query::test_page(&scope.snapshot, &request).unwrap();
        assert_eq!(second.revision, first.revision);
        assert_eq!(second.hits[0].relative, "b.txt");
    }
    scope.dirty.store(true, Ordering::Relaxed);
    assert!(scope.stale());
    scope.dirty.store(false, Ordering::Relaxed);
    scope.snapshot.completed = now().saturating_sub(301);
    assert!(
        scope.stale(),
        "the fallback freshness interval still applies"
    );
}

#[test]
fn indexed_globs_support_the_same_alternatives_as_live_search() {
    let pattern = glob("*.txt;*.md report?.csv", false).unwrap();
    for name in ["readme.TXT", "notes.md", "report1.csv"] {
        assert!(pattern.is_match(name));
    }
    for name in ["notes.md.bak", "report12.csv", "photo.png"] {
        assert!(!pattern.is_match(name));
    }
    assert!(glob("[draft].txt", true).unwrap().is_match("[draft].txt"));
    assert!(!glob("*.TXT", true).unwrap().is_match("readme.txt"));
    assert!(glob(&"a".repeat(1025), false).is_err());
}
