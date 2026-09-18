//! Native libraries are loaded only by a child copy of the application binary.
use super::{
    broker::{Instance, PluginBroker, PluginInstance},
    manifest::{package_file, valid_id, PluginRuntime},
    native_abi::{ABI_VERSION, MESSAGE_LIMIT},
    protocol::PluginRequest,
    registry::read_manifest,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Manager;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

async fn read_line(reader: &mut BufReader<tokio::process::ChildStdout>) -> Result<Value, String> {
    let mut bytes = Vec::new();
    reader
        .take((MESSAGE_LIMIT + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await
        .map_err(|e| e.to_string())?;
    if bytes.len() > MESSAGE_LIMIT || bytes.last() != Some(&b'\n') {
        return Err("Native helper exited or exceeded its message limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
async fn send(writer: &mut tokio::process::ChildStdin, value: Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if bytes.len() >= MESSAGE_LIMIT {
        return Err("Native reply exceeds message limit".into());
    }
    bytes.push(b'\n');
    tokio::time::timeout(Duration::from_secs(5), writer.write_all(&bytes))
        .await
        .map_err(|_| "Native helper is not reading")?
        .map_err(|e| e.to_string())
}
pub fn spawn(executable: &Path, library: &Path) -> Result<tokio::process::Child, String> {
    let mut command = tokio::process::Command::new(executable);
    command
        .arg("--hortcmd-plugin-helper")
        .arg(library)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    command.spawn().map_err(|e| e.to_string())
}

/// One event at a time makes synchronous C callbacks unambiguous. A stalled
/// initialization or event is terminated after ten seconds. Pipes are bounded.
pub async fn supervise(
    mut child: tokio::process::Child,
    instance: Arc<Mutex<Instance>>,
    dispatch: impl Fn(PluginRequest) -> Result<Value, String>,
) -> Result<(), String> {
    let mut reader = BufReader::new(child.stdout.take().ok_or("Missing native stdout")?);
    let mut writer = child.stdin.take().ok_or("Missing native stdin")?;
    let mut idle = false;
    let mut initialized = false;
    let mut deadline = Instant::now() + Duration::from_secs(10);
    // Keep the read future alive across timer ticks: read_until is not otherwise
    // cancellation-safe with a buffer owned by a temporary future.
    let result = async {
        loop {
            if instance.lock().map_err(|_| "Native state unavailable")?.info.closed {
                let _ = send(&mut writer, json!({"type":"close"})).await;
                return Ok(());
            }
            if !idle && Instant::now() > deadline { return Err("Native helper timed out".into()); }
            let read = read_line(&mut reader); tokio::pin!(read);
            let message = loop {
                tokio::select! {
                    message = &mut read => break message?,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        let (closed, event) = {
                            let mut state = instance.lock().map_err(|_| "Native state unavailable")?;
                            (state.info.closed, if idle { state.events.pop_front() } else { None })
                        };
                        if closed { let _ = send(&mut writer, json!({"type":"close"})).await; return Ok(()); }
                        if !idle && Instant::now() > deadline { return Err("Native helper timed out".into()); }
                        if let Some(event) = event {
                            send(&mut writer, json!({"type":"event", "event":event})).await?;
                            idle = false; deadline = Instant::now() + Duration::from_secs(10);
                        }
                    }
                }
            };
            match message["type"].as_str() {
                Some("request") => {
                    let request = serde_json::from_value(message["request"].clone()).map_err(|e| format!("Invalid native request: {e}"))?;
                    let response = match dispatch(request) { Ok(value) => json!({"value":value}), Err(error) => json!({"error":error}) };
                    send(&mut writer, response).await?;
                }
                Some("ready") if !initialized && message["abiVersion"] == ABI_VERSION => {
                    initialized = true; idle = true;
                    instance.lock().map_err(|_| "Native state unavailable")?.info.ready = true;
                }
                Some("idle") if initialized && !idle => idle = true,
                Some("error") => return Err(message["error"].as_str().unwrap_or("Native helper failed").to_string()),
                _ => return Err("Invalid native handshake or event response".into()),
            }
        }
    }.await;
    if let Ok(mut state) = instance.lock() {
        state.info.closed = true;
    }
    // Give cooperative shutdown a short grace period, then always reap the child.
    if tokio::time::timeout(Duration::from_millis(300), child.wait())
        .await
        .is_err()
    {
        #[cfg(unix)]
        if let Some(id) = child.id() {
            // SAFETY: spawn created a new process group with this child's PID.
            unsafe {
                libc::kill(-(id as i32), libc::SIGKILL);
            }
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    result
}

pub fn start(app: &tauri::AppHandle, root: &Path, plugin: &str) -> Result<PluginInstance, String> {
    if !valid_id(plugin) {
        return Err("Invalid plugin id".into());
    }
    let root = root.join(plugin);
    if std::fs::symlink_metadata(&root)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Package links are forbidden".into());
    }
    let manifest = read_manifest(&root)?;
    if manifest.id != plugin || manifest.runtime != PluginRuntime::Native {
        return Err("Not a native plugin".into());
    }
    let broker = app.state::<PluginBroker>();
    let permissions = broker
        .grants
        .lock()
        .map_err(|_| "Plugin grants unavailable")?
        .get(plugin)
        .ok_or("Plugin is not active")?
        .clone();
    let mut instances = broker
        .instances
        .lock()
        .map_err(|_| "Plugin registry unavailable")?;
    for state in instances.values() {
        let state = state.lock().map_err(|_| "Plugin instance unavailable")?;
        if state.info.plugin_id == plugin && !state.info.closed {
            return Ok(state.info.clone());
        }
    }
    instances.retain(|_, s| s.lock().is_ok_and(|s| !s.info.closed));
    if instances.len() >= 32 {
        return Err("Too many plugin instances".into());
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    let id = hex::encode(bytes);
    let info = PluginInstance {
        id: id.clone(),
        plugin_id: plugin.into(),
        view_id: "native".into(),
        title: manifest.name.clone(),
        window_label: "main".into(),
        ready: false,
        closed: false,
    };
    let entry = manifest
        .entry
        .current()
        .ok_or("Unsupported platform")?
        .to_string();
    let library = package_file(&root, &entry)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let child = spawn(
        &std::env::current_exe().map_err(|e| e.to_string())?,
        &library,
    )?;
    if let Some(pid) = child.id() {
        broker
            .native_processes
            .lock()
            .map_err(|_| "Native process registry unavailable")?
            .insert(id.clone(), pid);
    }
    let mut state = Instance {
        binding: Default::default(),
        info: info.clone(),
        manifest,
        root,
        entry,
        permissions,
        events: VecDeque::new(),
        created: Instant::now(),
    };
    let workspace = broker
        .workspace
        .lock()
        .map_err(|_| "Plugin context unavailable")?
        .clone();
    super::context::initialize(&mut state, workspace.as_ref());
    let instance = Arc::new(Mutex::new(state));
    instances.insert(id.clone(), instance.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let callback_app = app.clone();
        let callback_id = id.clone();
        let result = supervise(child, instance, move |request| {
            callback_app
                .state::<PluginBroker>()
                .control(&callback_id, "main", request)
        })
        .await;
        if let Err(error) = result {
            eprintln!("Native plugin helper stopped: {error}");
        }
        if let Ok(mut processes) = app.state::<PluginBroker>().native_processes.lock() {
            processes.remove(&id);
        }
        app.state::<PluginBroker>().close(&id);
    });
    Ok(info)
}

/// Called synchronously before application exit, including when a native handler
/// is stuck and cannot service its control pipe.
pub fn shutdown(broker: &PluginBroker) {
    if let Ok(mut processes) = broker.native_processes.lock() {
        for (_, pid) in std::mem::take(&mut *processes) {
            #[cfg(unix)]
            // SAFETY: each PID belongs to a helper launched in its own group.
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill.exe")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
}
