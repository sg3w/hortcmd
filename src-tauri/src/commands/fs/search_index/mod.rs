//! Explicit opt-in scopes. No indexing begins until a user approves a root.
mod crawler;
pub mod query;
mod storage;
#[cfg(test)]
mod tests;
mod types;
use super::transfer::recovery::types::RecoveryLocation;
use crate::{
    errors::{code, AppError, AppResult},
    vfs,
};
use notify::Watcher;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use types::*;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ROWS: usize = 50_000;
fn bad(detail: &str) -> AppError {
    AppError::new(code::VALIDATION_BAD_INPUT)
        .op("search-index")
        .detail(detail)
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub(super) fn glob(pattern: &str, sensitive: bool) -> AppResult<regex::Regex> {
    if pattern.len() > 1024 {
        return Err(bad("index pattern limit"));
    }
    regex::RegexBuilder::new(&super::glob::pattern(pattern))
        .case_insensitive(!sensitive)
        .size_limit(1024 * 1024)
        .build()
        .map_err(|_| bad("invalid index pattern"))
}
struct Scope {
    snapshot: Snapshot,
    secret: Option<storage::Secret>,
    dirty: Arc<AtomicBool>,
    _watcher: Option<notify::RecommendedWatcher>,
    pressure: bool,
}
fn worker() -> Arc<tokio::sync::Semaphore> {
    static WORKER: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
    WORKER
        .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
}
fn scopes() -> &'static Mutex<HashMap<String, Arc<Mutex<Scope>>>> {
    static SCOPES: OnceLock<Mutex<HashMap<String, Arc<Mutex<Scope>>>>> = OnceLock::new();
    SCOPES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn get(id: &str) -> AppResult<Arc<Mutex<Scope>>> {
    scopes()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(id)
        .cloned()
        .ok_or_else(|| bad("index locked or missing"))
}
fn location_path(location: &RecoveryLocation) -> &str {
    match location {
        RecoveryLocation::Local { path } | RecoveryLocation::Remote { path, .. } => path,
    }
}
fn prefix(location: &RecoveryLocation) -> String {
    match location {
        RecoveryLocation::Local { .. } => "0".into(),
        RecoveryLocation::Remote { profile, .. } => hex::encode(Sha256::digest(profile.as_bytes())),
    }
}
fn generation(location: &RecoveryLocation) -> AppResult<String> {
    let raw = location.resolve()?;
    if let RecoveryLocation::Local { path } = location {
        return Ok(format!(
            "{:?}",
            super::transfer::recovery::durability::object_identity(Path::new(path))?
        ));
    }
    let (provider, _) = vfs::resolve(&raw)?;
    vfs::identity::generation(&provider)
}
fn validate_policy(policy: &IndexPolicy, remote: bool) -> AppResult<()> {
    if policy.ignore.len() > 64 || policy.max_content_bytes > 1024 * 1024 {
        return Err(bad("index policy limit"));
    }
    for pattern in &policy.ignore {
        glob(pattern, false)?;
    }
    if remote && policy.content {
        return Err(bad(
            "remote content indexing requires no-follow and stable-read provider capabilities",
        ));
    }
    Ok(())
}
fn validate_snapshot(snapshot: &Snapshot) -> AppResult<()> {
    validate_policy(
        &snapshot.policy,
        matches!(snapshot.location, RecoveryLocation::Remote { .. }),
    )?;
    if snapshot.rows.len() > MAX_ROWS
        || snapshot.queue.len() > 5000
        || crawler::bytes(snapshot) > MAX_BYTES
    {
        return Err(bad("index bounds exceeded; rebuild"));
    }
    if !snapshot
        .id
        .starts_with(&format!("{}-", prefix(&snapshot.location)))
    {
        return Err(bad("index provider mismatch"));
    }
    if !Path::new(location_path(&snapshot.location)).is_absolute()
        || Path::new(location_path(&snapshot.location))
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(bad("invalid index root"));
    }
    for (path, row) in &snapshot.rows {
        if !crawler::relative(path)
            || row.name
                != Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
            || row.identity.len() > 4096
            || row.content.as_ref().is_some_and(|text| {
                !snapshot.policy.content
                    || row.directory
                    || text.len() > snapshot.policy.max_content_bytes as usize
                    || text.contains('\0')
            })
        {
            return Err(bad("index category or path mismatch"));
        }
    }
    for path in snapshot
        .rows
        .keys()
        .chain(snapshot.queue.iter())
        .filter(|path| !path.is_empty())
    {
        if !crawler::relative(path) || crawler::excluded(path, &snapshot.policy) {
            return Err(bad("index scope mismatch; rebuild"));
        }
    }
    Ok(())
}
impl Scope {
    fn new(snapshot: Snapshot, secret: Option<storage::Secret>) -> Self {
        let dirty = Arc::new(AtomicBool::new(false));
        let flag = dirty.clone();
        let policy = snapshot.policy.clone();
        let watch_root = location_path(&snapshot.location).to_owned();
        let watcher = if let RecoveryLocation::Local { path } = &snapshot.location {
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                if let Ok(event) = &event {
                    if matches!(event.kind, notify::EventKind::Access(_))
                        || (!event.paths.is_empty()
                            && event.paths.iter().all(|path| {
                                path.strip_prefix(&watch_root)
                                    .ok()
                                    .and_then(Path::to_str)
                                    .is_some_and(|path| crawler::excluded(path, &policy))
                            }))
                    {
                        return;
                    }
                }
                flag.store(true, Ordering::Relaxed);
            })
            .ok()
            .and_then(|mut watcher| {
                watcher
                    .watch(Path::new(path), notify::RecursiveMode::Recursive)
                    .ok()
                    .map(|_| watcher)
            })
        } else {
            None
        };
        Self {
            snapshot,
            secret,
            dirty,
            _watcher: watcher,
            pressure: false,
        }
    }
    fn stale(&self) -> bool {
        self.dirty.load(Ordering::Relaxed) || now().saturating_sub(self.snapshot.completed) >= 300
    }
    fn info(&self) -> IndexInfo {
        let s = &self.snapshot;
        let state = if self.pressure {
            "resource_pause"
        } else if s.limited {
            "limited"
        } else if s.paused {
            "paused"
        } else if !s.queue.is_empty() {
            "building"
        } else if self.stale() {
            "stale"
        } else {
            "snapshot"
        };
        IndexInfo {
            id: s.id.clone(),
            root: Some(location_path(&s.location).into()),
            policy: Some(s.policy.clone()),
            state: state.into(),
            entries: s.rows.len() as u32,
            bytes: crawler::bytes(s) as u32,
            pending: s.queue.len() as u32,
            unreadable: s.unreadable,
            persistent: self.secret.is_some(),
        }
    }
    fn save(&self) -> AppResult<()> {
        if let Some(secret) = &self.secret {
            storage::write(&self.snapshot, secret)?;
        }
        Ok(())
    }
    fn restart(&mut self) {
        self.snapshot.cycle += 1;
        self.snapshot.revision += 1;
        self.snapshot.reset_queue(VecDeque::from([String::new()]));
        self.snapshot.limited = false;
        self.snapshot.unreadable = 0;
        self.dirty.store(false, Ordering::Relaxed);
    }
}
#[tauri::command]
pub async fn index_create(
    root: String,
    policy: IndexPolicy,
    password: Option<String>,
) -> AppResult<IndexInfo> {
    tokio::task::spawn_blocking(move || {
        let mut location = RecoveryLocation::capture(&root)?;
        validate_policy(&policy, matches!(location, RecoveryLocation::Remote { .. }))?;
        match &mut location {
            RecoveryLocation::Local { path } => {
                #[cfg(not(target_os = "macos"))]
                {
                    return Err(bad(
                        "local indexing awaits native scope validation on this platform",
                    ));
                }
                if !Path::new(path).is_dir() {
                    return Err(bad("index root must be a directory"));
                }
                *path = std::fs::canonicalize(&*path)?
                    .to_str()
                    .ok_or_else(|| bad("non-Unicode root"))?
                    .to_owned();
            }
            RecoveryLocation::Remote { path, .. } => {
                if Path::new(path).components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                }) {
                    return Err(bad("remote index root must be normalized"));
                }
            }
        }
        if crawler::excluded(location_path(&location).trim_start_matches('/'), &policy) {
            return Err(bad("index root is excluded by policy"));
        }
        let generation = generation(&location)?;
        let secret = password.as_deref().map(storage::secret).transpose()?;
        let id = format!("{}-{}", prefix(&location), crate::net::unique_id());
        let snapshot = Snapshot {
            version: 1,
            id: id.clone(),
            location,
            policy,
            generation,
            weight: Default::default(),
            rows: BTreeMap::new(),
            queue: VecDeque::from([String::new()]),
            cycle: 1,
            revision: 1,
            completed: 0,
            paused: false,
            limited: false,
            unreadable: 0,
        };
        let mut all = scopes().lock().unwrap_or_else(|e| e.into_inner());
        if all.len() >= 8 {
            return Err(bad("at most eight unlocked index scopes"));
        }
        if secret.is_some() && storage::ids_in(&storage::directory()?)?.len() >= 64 {
            return Err(bad("persistent index scope limit"));
        }
        let scope = Scope::new(snapshot, secret);
        scope.save()?;
        let info = scope.info();
        all.insert(id, Arc::new(Mutex::new(scope)));
        Ok(info)
    })
    .await
    .map_err(|_| bad("index worker stopped"))?
}
#[tauri::command]
pub async fn index_list() -> AppResult<Vec<IndexInfo>> {
    tokio::task::spawn_blocking(|| {
        let opened: Vec<_> = scopes()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        let mut result: Vec<_> = opened
            .iter()
            .map(|scope| scope.lock().unwrap_or_else(|e| e.into_inner()).info())
            .collect();
        for id in storage::ids_in(&storage::directory()?)? {
            if !result.iter().any(|info| info.id == id) {
                result.push(IndexInfo {
                    id,
                    root: None,
                    policy: None,
                    state: "locked".into(),
                    entries: 0,
                    bytes: 0,
                    pending: 0,
                    unreadable: 0,
                    persistent: true,
                });
            }
        }
        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    })
    .await
    .map_err(|_| bad("index worker stopped"))?
}
#[tauri::command]
pub async fn index_unlock(id: String, password: String) -> AppResult<IndexInfo> {
    tokio::task::spawn_blocking(move || {
        let (mut snapshot, secret) = storage::read(&id, &password)?;
        if let RecoveryLocation::Remote { profile, .. } = &snapshot.location {
            if !crate::net::session::net_list_profiles()
                .iter()
                .any(|p| &p.id == profile)
            {
                storage::remove(&id)?;
                return Err(bad("index profile was deleted"));
            }
        }
        snapshot.paused = true; // Restart never silently resumes expensive work.
        let scope = Scope::new(snapshot, Some(secret));
        let info = scope.info();
        let mut all = scopes().lock().unwrap_or_else(|e| e.into_inner());
        if all.contains_key(&id) || all.len() >= 8 {
            return Err(bad("index already unlocked or scope limit"));
        }
        all.insert(id, Arc::new(Mutex::new(scope)));
        Ok(info)
    })
    .await
    .map_err(|_| bad("index worker stopped"))?
}
fn resource_pressure(allow_battery: bool) -> bool {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    if system.available_memory() < 256 * 1024 * 1024 {
        return true;
    }
    #[cfg(target_os = "macos")]
    if !allow_battery {
        return std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "batt"])
            .output()
            .map_or(true, |output| {
                !output.status.success()
                    || !String::from_utf8_lossy(&output.stdout).contains("AC Power")
            });
    }
    let _ = allow_battery;
    false
}
#[tauri::command]
pub async fn index_step(id: String, token: String) -> AppResult<IndexInfo> {
    let permit = worker()
        .try_acquire_owned()
        .map_err(|_| bad("index worker busy"))?;
    let cancel = super::cancel::register("index", &token);
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let scope = get(&id)?;
        let mut scope = scope.lock().unwrap_or_else(|e| e.into_inner());
        if scope.snapshot.paused || cancel.load(Ordering::Relaxed) {
            return Ok(scope.info());
        }
        scope.pressure = resource_pressure(scope.snapshot.policy.allow_battery);
        if scope.pressure {
            return Ok(scope.info());
        }
        let current = generation(&scope.snapshot.location)?;
        if current != scope.snapshot.generation {
            if matches!(scope.snapshot.location, RecoveryLocation::Local { .. }) {
                return Err(bad("index root identity changed; delete and approve again"));
            }
            scope.snapshot.generation = current;
            scope.restart();
        }
        if scope.snapshot.queue.is_empty() && scope.stale() {
            scope.restart();
        }
        let result = if scope.snapshot.queue.is_empty() {
            Ok(())
        } else {
            crawler::step(&mut scope.snapshot, &cancel)
        };
        scope.save()?;
        result?;
        Ok(scope.info())
    })
    .await
    .map_err(|_| bad("index worker stopped"));
    super::cancel::unregister("index", &token);
    result?
}
#[tauri::command]
pub fn index_cancel(token: String) {
    super::cancel::cancel("index", &token);
}
#[tauri::command]
pub async fn index_control(id: String, action: String) -> AppResult<()> {
    tokio::task::spawn_blocking(move || {
        if action == "delete" {
            let scope = scopes()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            if let Some(scope) = scope {
                let mut scope = scope.lock().unwrap_or_else(|e| e.into_inner());
                scope.snapshot.paused = true;
                scope.secret = None;
                scope.snapshot.clear_rows();
            }
            return storage::remove(&id);
        }
        let scope = get(&id)?;
        let mut scope = scope.lock().unwrap_or_else(|e| e.into_inner());
        match action.as_str() {
            "pause" => scope.snapshot.paused = true,
            "resume" => scope.snapshot.paused = false,
            "rebuild" => {
                scope.snapshot.clear_rows();
                scope.restart();
                scope.snapshot.paused = false;
            }
            _ => return Err(bad("unknown index action")),
        }
        scope.save()
    })
    .await
    .map_err(|_| bad("index worker stopped"))?
}
/// Erase unlocked and encrypted/locked scopes without needing their passphrase.
pub fn delete_profile(profile: &str) -> AppResult<()> {
    delete_profile_in(profile, &crate::config::path("search-index")?)
}
fn delete_profile_in(profile: &str, dir: &Path) -> AppResult<()> {
    let prefix = format!("{}-", hex::encode(Sha256::digest(profile.as_bytes())));
    let removed: Vec<_> = {
        let mut all = scopes().lock().unwrap_or_else(|e| e.into_inner());
        let ids: Vec<_> = all
            .keys()
            .filter(|id| id.starts_with(&prefix))
            .cloned()
            .collect();
        ids.into_iter().filter_map(|id| all.remove(&id)).collect()
    };
    for scope in removed {
        let mut scope = scope.lock().unwrap_or_else(|e| e.into_inner());
        scope.snapshot.paused = true;
        scope.snapshot.clear_rows();
        scope.secret = None;
    }
    if dir.exists() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if name.to_str().is_some_and(|name| {
                name.starts_with(&prefix) && (name.ends_with(".enc") || name.ends_with(".tmp"))
            }) {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}
