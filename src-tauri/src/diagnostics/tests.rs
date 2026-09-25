use super::*;
use std::fs;
fn record() -> DiagnosticRecord {
    DiagnosticRecord {
        at: storage::now(),
        level: Level::Info,
        component: Component::Backend,
        event: Event::Started,
        correlation: new_id(),
        code: None,
        identifier: None,
    }
}
struct Temp(storage::Store);
impl Temp {
    fn new() -> Self {
        Self(storage::Store {
            root: std::env::temp_dir().join(format!("hortcmd-diagnostics-{}", new_id())),
        })
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0.root);
    }
}
#[test]
fn every_content_channel_is_rejected_and_identifiers_are_consistent() {
    let t = Temp::new();
    for secret in [
        "password=swordfish",
        "Authorization: Bearer SECRET",
        "sftp://alice:pass@host/file?token=canary",
        "-----BEGIN PRIVATE KEY-----secret",
        "terminal input/output",
        "clipboard content",
        "file contents",
        "päss/%73ecret/文件",
        "nested {error: credential}",
    ] {
        let mut r = record();
        r.identifier = Some(pseudonym(&[7; 32], secret));
        r.code = Some(secret.into());
        t.0.append(r).unwrap();
        let persisted = fs::read_to_string(t.0.root.join("events-0.jsonl")).unwrap();
        assert!(!persisted.contains(secret));
        let mut value = serde_json::to_value(record()).unwrap();
        value["payload"] = secret.into();
        assert!(serde_json::from_value::<DiagnosticRecord>(value).is_err());
        assert_eq!(pseudonym(&[7; 32], secret), pseudonym(&[7; 32], secret));
        assert_ne!(pseudonym(&[7; 32], secret), pseudonym(&[8; 32], secret));
    }
    assert!(t.0.records().iter().all(|r| r.code.is_none()));
}
#[test]
fn rotation_age_corruption_and_delete_are_bounded() {
    let t = Temp::new();
    t.0.prepare().unwrap();
    for i in 0..storage::FILES {
        fs::write(
            t.0.root.join(format!("events-{i}.jsonl")),
            vec![b' '; storage::FILE_BYTES as usize],
        )
        .unwrap();
    }
    t.0.append(record()).unwrap();
    assert_eq!(fs::read_dir(&t.0.root).unwrap().count(), storage::FILES);
    let total: u64 = fs::read_dir(&t.0.root)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    assert!(total <= storage::FILE_BYTES * storage::FILES as u64);
    assert_eq!(t.0.records().len(), 1);
    let mut r = record();
    r.at -= storage::AGE + 1.0;
    t.0.append(r).unwrap();
    assert_eq!(t.0.records().len(), 1);
    let file = fs::File::open(t.0.root.join("events-0.jsonl")).unwrap();
    file.set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    t.0.cleanup().unwrap();
    assert!(!t.0.root.join("events-0.jsonl").exists());
    t.0.clear().unwrap();
    assert!(t.0.records().is_empty());
}
#[test]
fn failed_storage_and_concurrent_writers_do_not_affect_operations() {
    let t = Temp::new();
    fs::write(&t.0.root, b"blocked").unwrap();
    assert!(t.0.append(record()).is_err());
    fs::remove_file(&t.0.root).unwrap();
    let store = std::sync::Arc::new(Mutex::new(storage::Store {
        root: t.0.root.clone(),
    }));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                for _ in 0..50 {
                    store.lock().unwrap().append(record()).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(store.lock().unwrap().records().len(), 400);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(t.0.root.join("events-0.jsonl"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
#[test]
fn errors_and_workers_keep_the_request_correlation_without_leaking_between_scopes() {
    assert!(current().is_none());
    let id = new_id();
    {
        let _scope = ipc_scope("copy_entries", Some(&id)).unwrap();
        let error = crate::errors::AppError::new(crate::errors::code::FS_NOT_FOUND);
        assert_eq!(&*error.id, id);
        let inherited = current().unwrap();
        let worker = std::thread::spawn(move || {
            let _scope = Scope::enter(inherited.clone());
            for event in [
                Event::Started,
                Event::Retried,
                Event::Cancelled,
                Event::Failed,
                Event::Finished,
            ] {
                emit(Component::Provider, event, Level::Info, None, None);
                assert_eq!(current(), Some(inherited.clone()));
            }
        });
        worker.join().unwrap();
    }
    assert!(current().is_none());
    for cmd in [
        "terminal_write",
        "terminal_read",
        "plugin_poll",
        "read_text_file",
        "clipboard_set",
    ] {
        assert!(ipc_scope(cmd, Some(&id)).is_none());
    }
}
#[test]
fn bundle_schema_is_explicit_opt_in_and_never_overwrites() {
    let t = Temp::new();
    t.0.prepare().unwrap();
    let mut status = diagnostics_status();
    status.location.clear();
    let bundle = make_bundle(status, vec![record()], false);
    assert_eq!(bundle["schema"], 1);
    assert_eq!(bundle["metadata"]["location"], "");
    assert_eq!(bundle["manifest"]["includesLogLocation"], false);
    assert_eq!(
        bundle["manifest"]["categories"].as_array().unwrap().len(),
        3
    );
    let mut status = diagnostics_status();
    status.location = "/Users/example/diagnostics".into();
    let opted = make_bundle(status, vec![], true);
    assert_eq!(opted["manifest"]["includesLogLocation"], true);
    assert_eq!(opted["metadata"]["location"], "/Users/example/diagnostics");
    let target = t.0.root.join("bundle.json");
    write_bundle(&target, &bundle).unwrap();
    assert!(write_bundle(&target, &opted).is_err());
}
