use super::*;
use std::{fs, path::PathBuf};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let mut b = [0; 16];
        getrandom::fill(&mut b).unwrap();
        let p = std::env::temp_dir().join(format!("hortcmd-usage-{}", hex::encode(b)));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn options(t: &Temp) -> UsageOptions {
    UsageOptions {
        root: t.0.to_string_lossy().into(),
        size_mode: Default::default(),
        max_depth: 32,
        item_limit: 100000,
        node_limit: 1000,
        seconds: 30,
        include_hidden: true,
        stay_on_volume: true,
    }
}
fn finish(scan: &mut Scan) -> types::UsageBatch {
    for _ in 0..20000 {
        scan.step(&AtomicBool::new(false), &|| Ok(())).unwrap();
        let batch = scan.batch();
        if batch.status != "running" && !batch.pending {
            return batch;
        }
    }
    panic!("scan failed to finish")
}
#[test]
fn depth_limits_and_iterative_traversal() {
    let t = Temp::new();
    let mut p = t.0.clone();
    for _ in 0..260 {
        p = p.join("d");
        fs::create_dir(&p).unwrap();
        fs::write(p.join("f"), b"123").unwrap();
    }
    for depth in [0, 1, 32, 128, 256] {
        let mut o = options(&t);
        o.max_depth = depth;
        let mut scan = Scan::new(o).unwrap();
        let final_batch = finish(&mut scan);
        assert!(final_batch.scanned <= u32::from(depth) * 2 + 1);
        assert!(final_batch.estimated_memory_bytes <= final_batch.memory_budget_bytes);
    }
    let mut o = options(&t);
    o.max_depth = 257;
    assert!(Scan::new(o).is_err());
}
#[test]
fn bounded_nodes_aggregate_and_cancellation_never_claims_completion() {
    let t = Temp::new();
    for i in 0..2000 {
        fs::write(t.0.join(i.to_string()), b"1234").unwrap();
    }
    let mut o = options(&t);
    o.node_limit = 16;
    let mut scan = Scan::new(o).unwrap();
    let mut root = None;
    loop {
        scan.step(&AtomicBool::new(false), &|| Ok(())).unwrap();
        let b = scan.batch();
        assert!(b.nodes.len() <= 64);
        for n in b.nodes {
            if n.id == 0 {
                root = Some(n);
            }
        }
        if b.status != "running" && !b.pending {
            break;
        }
    }
    let root = root.unwrap();
    assert_eq!(root.recursive_bytes, 8000);
    assert_eq!(root.files, 2000);
    assert_eq!(root.omitted_entries, 1985);
    let mut scan = Scan::new(options(&t)).unwrap();
    scan.step(&AtomicBool::new(true), &|| Ok(())).unwrap();
    let b = scan.batch();
    assert_eq!(b.status, "cancelled");
    assert!(b.nodes[0].incomplete);
}
#[cfg(unix)]
#[test]
fn links_hardlinks_and_hidden_policy() {
    let t = Temp::new();
    fs::write(t.0.join("a"), b"abc").unwrap();
    fs::hard_link(t.0.join("a"), t.0.join("b")).unwrap();
    std::os::unix::fs::symlink(".", t.0.join("loop")).unwrap();
    fs::write(t.0.join(".hidden"), b"secret").unwrap();
    let mut o = options(&t);
    o.include_hidden = false;
    let mut scan = Scan::new(o).unwrap();
    let mut nodes = Vec::new();
    loop {
        scan.step(&AtomicBool::new(false), &|| Ok(())).unwrap();
        let b = scan.batch();
        nodes.extend(b.nodes);
        if b.status != "running" && !b.pending {
            break;
        }
    }
    let root = nodes.iter().rev().find(|n| n.id == 0).unwrap();
    assert_eq!(root.recursive_bytes, 7);
    assert_eq!(root.deduplicated_bytes, Some(4));
    assert_eq!(root.files, 3);
    assert!(nodes.iter().any(|n| n.kind == "link"));
    assert!(!nodes.iter().any(|n| n.name == ".hidden"));
}
#[test]
fn malformed_limits_and_provider_rejections_are_explicit() {
    let t = Temp::new();
    for value in [0, 1_000_001] {
        let mut o = options(&t);
        o.item_limit = value;
        assert!(Scan::new(o).is_err());
    }
    let mut raw = serde_json::to_value(options(&t)).unwrap();
    raw["maxDepth"] = serde_json::json!(-1);
    assert!(serde_json::from_value::<UsageOptions>(raw).is_err());
    for scheme in ["sftp", "ftp", "ftps", "smb", "webdav"] {
        let mut o = options(&t);
        o.root = format!("{scheme}://example.test/folder");
        assert!(Scan::new(o).is_err());
    }
}

#[test]
fn installed_package_lifecycle_grants_pause_revocation_and_reactivation() {
    use crate::plugins::{
        context::PluginWorkspace,
        js_host, management,
        protocol::{PluginContext, PluginSide},
    };
    let t = Temp::new();
    let root = t.0.join("plugins");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins/storage-usage");
    let broker = PluginBroker::default();
    let id = management::install(&broker, &root, &source, None).unwrap();
    let record = management::records(&broker, &root).unwrap().remove(0);
    assert!(!record.active);
    management::activate(&broker, &root, &id, record.fingerprint.as_ref().unwrap()).unwrap();
    let context = PluginContext {
        path: t.0.to_string_lossy().into(),
        folder_path: t.0.to_string_lossy().into(),
        file_path: None,
        selection: vec![],
        side: PluginSide::Left,
        copy_limit: 0,
        copy_operation: None,
    };
    *broker.workspace.lock().unwrap() = Some(PluginWorkspace {
        active: PluginSide::Left,
        left: context.clone(),
        right: context,
    });
    let info = js_host::create(&broker, &root, &id, None, true, PluginSide::Left, None).unwrap();
    let request = PluginRequest::UsageStart {
        options: options(&t),
    };
    assert!(execute(&broker, &info.id, "plugin-impostor", &request).is_err());
    execute(&broker, &info.id, "main", &request).unwrap();
    assert_eq!(
        execute(
            &broker,
            &info.id,
            "main",
            &PluginRequest::UsageControl {
                action: "pause".into()
            }
        )
        .unwrap()["status"],
        "paused"
    );
    let paused = execute(&broker, &info.id, "main", &PluginRequest::UsageStep).unwrap();
    assert_eq!(paused["scanned"], 0);
    execute(
        &broker,
        &info.id,
        "main",
        &PluginRequest::UsageControl {
            action: "resume".into(),
        },
    )
    .unwrap();
    let state = broker.instance(&info.id, "main").unwrap();
    state.lock().unwrap().permissions.clear();
    assert!(execute(&broker, &info.id, "main", &PluginRequest::UsageStep).is_err());
    broker.close(&info.id);
    assert!(state.lock().unwrap().usage.scan.lock().unwrap().is_none());
    management::deactivate(&broker, &root, &id, false).unwrap();
    assert!(js_host::create(&broker, &root, &id, None, true, PluginSide::Left, None).is_err());
    let record = management::records(&broker, &root).unwrap().remove(0);
    management::activate(&broker, &root, &id, record.fingerprint.as_ref().unwrap()).unwrap();
    let info = js_host::create(&broker, &root, &id, None, true, PluginSide::Left, None).unwrap();
    execute(&broker, &info.id, "main", &request).unwrap();
    management::deactivate(&broker, &root, &id, true).unwrap();
    assert!(execute(&broker, &info.id, "main", &PluginRequest::UsageStep).is_err());
    assert!(!root.join(id).exists());
}

#[test]
fn item_limits_and_failed_authorization_remain_partial() {
    let t = Temp::new();
    for i in 0..20 {
        fs::write(t.0.join(i.to_string()), b"a").unwrap();
    }
    let mut o = options(&t);
    o.item_limit = 2;
    let mut scan = Scan::new(o).unwrap();
    let b = finish(&mut scan);
    assert_eq!(b.status, "item-limited");
    assert_eq!(b.scanned, 2);
    let mut scan = Scan::new(options(&t)).unwrap();
    assert!(scan
        .step(&AtomicBool::new(false), &|| Err("revoked".into()))
        .is_err());
    let handle = Handle::default();
    *handle.scan.lock().unwrap() = Some(scan);
    handle.close();
    assert!(handle.scan.lock().unwrap().is_none());
}
