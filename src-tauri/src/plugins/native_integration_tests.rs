//! Real helper-process tests. No native GUI or installed plugin is required.
#![cfg(unix)]
use hortcmd_lib::plugins::{
    broker::{Instance, PluginBroker, PluginInstance},
    manifest::PluginManifest,
    native_host,
    protocol::PluginEvent,
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Fixture {
    fn new(abi: u32) -> Self {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).unwrap();
        let root = std::env::temp_dir().join(format!("hortcmd-native-test-{}", hex::encode(bytes)));
        std::fs::create_dir(&root).unwrap();
        let library = root.join(if cfg!(target_os = "macos") {
            "plugin.dylib"
        } else {
            "plugin.so"
        });
        let status = Command::new("cc")
            .args([
                "-shared",
                "-fPIC",
                &format!("-DABI_VERSION={abi}"),
                "-I../docs/plugins",
                "src/plugins/fixtures/native_plugin.c",
                "-o",
            ])
            .arg(&library)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .unwrap();
        assert!(status.success());
        Self(library)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}
fn instance() -> (Arc<PluginBroker>, Arc<Mutex<Instance>>) {
    let manifest: PluginManifest = serde_json::from_value(serde_json::json!({
        "id":"org.test.native", "name":"Native", "version":"1.0.0", "description":"Test", "author":"Test", "apiVersion":1,
        "runtime":"native", "entry":{"macos":"plugin.dylib", "linux":"plugin.so"}, "icon":"🌈", "permissions":[]
    })).unwrap();
    let state = Arc::new(Mutex::new(Instance {
        binding: Default::default(),
        info: PluginInstance {
            id: "test".into(),
            plugin_id: manifest.id.clone(),
            view_id: "native".into(),
            title: "Native".into(),
            window_label: "main".into(),
            ready: false,
            closed: false,
        },
        manifest,
        root: PathBuf::new(),
        entry: String::new(),
        permissions: vec![],
        events: VecDeque::new(),
        created: Instant::now(),
    }));
    let broker = Arc::new(PluginBroker::default());
    broker
        .instances
        .lock()
        .unwrap()
        .insert("test".into(), state.clone());
    (broker, state)
}
async fn ready(state: &Arc<Mutex<Instance>>) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while !state.lock().unwrap().info.ready {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn actual_library_callbacks_denial_and_cooperative_shutdown() {
    let fixture = Fixture::new(1);
    let (broker, state) = instance();
    let child = native_host::spawn(
        std::path::Path::new(env!("CARGO_BIN_EXE_hortcmd")),
        &fixture.0,
    )
    .unwrap();
    let denied = Arc::new(Mutex::new(false));
    let seen = denied.clone();
    let monitor = state.clone();
    let task = tokio::spawn(native_host::supervise(child, state, move |request| {
        let result = broker.control("test", "main", request);
        if result.is_err() {
            *seen.lock().unwrap() = true;
        }
        result
    }));
    ready(&monitor).await;
    monitor.lock().unwrap().push(PluginEvent::Command {
        command: "test".into(),
    });
    tokio::time::timeout(Duration::from_secs(15), async {
        while !*denied.lock().unwrap() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    monitor.lock().unwrap().info.closed = true;
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
#[tokio::test]
async fn bad_abi_and_native_crash_never_crash_the_parent() {
    for abi in [2, 1] {
        let fixture = Fixture::new(abi);
        let (broker, state) = instance();
        let monitor = state.clone();
        let child = native_host::spawn(
            std::path::Path::new(env!("CARGO_BIN_EXE_hortcmd")),
            &fixture.0,
        )
        .unwrap();
        let task = tokio::spawn(native_host::supervise(child, state, move |request| {
            broker.control("test", "main", request)
        }));
        if abi == 1 {
            ready(&monitor).await;
            monitor.lock().unwrap().push(PluginEvent::Command {
                command: "crash".into(),
            });
        }
        assert!(tokio::time::timeout(Duration::from_secs(15), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert!(monitor.lock().unwrap().info.closed);
    }
}

#[tokio::test]
async fn closing_a_hung_handler_terminates_and_reaps_the_helper() {
    let fixture = Fixture::new(1);
    let (broker, state) = instance();
    let monitor = state.clone();
    let child = native_host::spawn(
        std::path::Path::new(env!("CARGO_BIN_EXE_hortcmd")),
        &fixture.0,
    )
    .unwrap();
    let task = tokio::spawn(native_host::supervise(child, state, move |request| {
        broker.control("test", "main", request)
    }));
    ready(&monitor).await;
    monitor.lock().unwrap().push(PluginEvent::Command {
        command: "hang".into(),
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    monitor.lock().unwrap().info.closed = true;
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
