//! Tab-owned subscriptions. Native notifications carry no frontend path-matching
//! policy: the backend normalizes locations, coalesces changes and schedules reads.
use super::{location, mode, RefreshMode, Schedule};
use crate::errors::{code, AppError, AppResult};
use crate::vfs::{self, Scheme};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use ts_rs::TS;

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RefreshSubscription {
    pub id: String,
    pub generation: u32,
    pub path: String,
}

#[derive(Clone, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RefreshStatus {
    pub targets: Vec<RefreshSubscription>,
    pub mode: RefreshMode,
    pub connected: bool,
    pub error: Option<AppError>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RefreshWork {
    pub key: String,
    pub serial: u32,
    pub path: String,
    pub targets: Vec<RefreshSubscription>,
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RefreshBatch {
    pub status: Vec<RefreshStatus>,
    pub work: Vec<RefreshWork>,
}

#[derive(Default)]
struct Events {
    queue: VecDeque<notify::Result<notify::Event>>,
    overflow: bool,
}
struct Entry {
    location: location::Location,
    targets: Vec<RefreshSubscription>,
    schedule: Schedule,
    error: Option<AppError>,
    serial: u32,
    cancel: tokio::sync::watch::Sender<bool>,
}
#[derive(Default)]
struct Manager {
    entries: HashMap<String, Entry>,
    watcher: Option<RecommendedWatcher>,
    events: Arc<Mutex<Events>>,
    serial: u32,
}
fn state() -> &'static Mutex<Manager> {
    static STATE: OnceLock<Mutex<Manager>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(Manager::default()))
}
fn failure(detail: impl std::fmt::Display) -> AppError {
    AppError::new(code::FS_IO)
        .op("directory-refresh")
        .detail(detail)
}

impl Manager {
    fn sync(&mut self, subscriptions: Vec<RefreshSubscription>) -> AppResult<()> {
        if subscriptions.len() > 128
            || subscriptions
                .iter()
                .any(|s| s.id.len() > 128 || s.path.len() > 32768)
        {
            return Err(failure(
                "too many refresh subscriptions or an oversized address",
            ));
        }
        let mut ids = HashSet::new();
        let mut groups: HashMap<
            String,
            (
                location::Location,
                Vec<RefreshSubscription>,
                Option<AppError>,
            ),
        > = HashMap::new();
        for subscription in subscriptions {
            if !ids.insert(subscription.id.clone()) {
                return Err(failure("duplicate subscription id"));
            }
            let (location, error) = match location::resolve(&subscription.path) {
                Ok(location) => (location, None),
                Err(error) => (
                    location::Location {
                        key: format!("invalid:{}", subscription.path),
                        scheme: Scheme::Local,
                        native: None,
                    },
                    Some(error),
                ),
            };
            groups
                .entry(location.key.clone())
                .or_insert_with(|| (location, vec![], error))
                .1
                .push(subscription);
        }
        if groups.len() > 64 {
            return Err(failure("at most 64 locations may refresh automatically"));
        }
        let mut rebuild = false;
        self.entries.retain(|key, entry| {
            if groups.contains_key(key) && entry.schedule.mode != RefreshMode::Manual {
                return true;
            }
            if let (Some(watcher), Some(path)) = (&mut self.watcher, &entry.location.native) {
                if watcher.unwatch(path).is_err() {
                    rebuild = true;
                }
            }
            false
        });
        if rebuild {
            // An unwatch failure cannot leave a leaked handle behind. Rebuild
            // from the bounded set of remaining subscriptions and report any
            // new setup failure on the affected location.
            self.watcher = None;
            self.entries.clear();
        }
        for (key, (location, targets, error)) in groups {
            if let Some(entry) = self.entries.get_mut(&key) {
                entry.targets = targets;
                continue;
            }
            let mut entry = Entry {
                schedule: Schedule::new(mode(location.scheme, true), Instant::now()),
                location,
                targets,
                error,
                serial: 0,
                cancel: tokio::sync::watch::channel(false).0,
            };
            if let Some(path) = &entry.location.native {
                let setup = (|| {
                    if self.watcher.is_none() {
                        let events = self.events.clone();
                        self.watcher = Some(
                            notify::recommended_watcher(
                                move |event: notify::Result<notify::Event>| {
                                    if event.as_ref().is_ok_and(|e| {
                                        matches!(e.kind, notify::EventKind::Access(_))
                                    }) {
                                        return;
                                    }
                                    crate::commands::fs::cache::invalidate();
                                    let mut events =
                                        events.lock().unwrap_or_else(|e| e.into_inner());
                                    if events.queue.len() >= 128
                                        || event.as_ref().is_ok_and(|e| e.paths.len() > 128)
                                    {
                                        events.queue.clear();
                                        events.overflow = true;
                                    } else {
                                        events.queue.push_back(event);
                                    }
                                },
                            )
                            .map_err(failure)?,
                        );
                    }
                    self.watcher
                        .as_mut()
                        .unwrap()
                        .watch(path, RecursiveMode::NonRecursive)
                        .map_err(failure)
                })();
                if let Err(error) = setup {
                    entry.error = Some(error);
                    entry.schedule.mode = RefreshMode::Manual;
                }
            } else if entry.error.is_some() {
                entry.schedule.mode = RefreshMode::Manual;
            }
            self.entries.insert(key, entry);
        }
        if !self.entries.values().any(|e| e.location.native.is_some()) {
            self.watcher = None;
            *self.events.lock().unwrap_or_else(|e| e.into_inner()) = Events::default();
        }
        Ok(())
    }

    fn poll(&mut self, visible: bool, now: Instant) -> RefreshBatch {
        let events = std::mem::take(&mut *self.events.lock().unwrap_or_else(|e| e.into_inner()));
        if events.overflow || !events.queue.is_empty() {
            crate::commands::fs::cache::invalidate();
        }
        let mut status = Vec::new();
        let mut work = Vec::new();
        for (key, entry) in &mut self.entries {
            if let Some(root) = &entry.location.native {
                if events.overflow {
                    entry.schedule.changed(now);
                    entry.error = Some(failure("watch queue overflow; rereading directory"));
                }
                for event in &events.queue {
                    match event {
                        Ok(event)
                            if event.paths.is_empty()
                                || event.need_rescan()
                                || event.paths.iter().any(|p| location::matches(root, p)) =>
                        {
                            entry.schedule.changed(now)
                        }
                        Err(error) => {
                            entry.schedule.changed(now);
                            entry.error = Some(failure(error));
                        }
                        _ => {}
                    }
                }
            }
            let connected = entry.location.scheme == Scheme::Local
                || vfs::caps::for_address(&entry.targets[0].path).is_ok_and(|caps| caps.connected);
            entry.schedule.connection(connected, now);
            entry.schedule.visibility(visible, now);
            status.push(RefreshStatus {
                targets: entry.targets.clone(),
                mode: if connected {
                    entry.schedule.mode
                } else {
                    RefreshMode::Manual
                },
                connected,
                error: entry.error.clone(),
            });
            if entry.schedule.take_due(now) {
                self.serial = self.serial.wrapping_add(1);
                entry.serial = self.serial;
                work.push(RefreshWork {
                    key: key.clone(),
                    serial: entry.serial,
                    path: entry.targets[0].path.clone(),
                    targets: entry.targets.clone(),
                });
            }
        }
        RefreshBatch { status, work }
    }

    fn finish(&mut self, key: &str, serial: u32, error: Option<AppError>) {
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        if !entry.schedule.busy || entry.serial != serial {
            return;
        }
        entry.schedule.completed(error.is_none(), Instant::now());
        entry.error = error;
    }
}

pub fn subscribe(subscriptions: Vec<RefreshSubscription>) -> AppResult<()> {
    state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .sync(subscriptions)
}

pub fn poll(visible: bool) -> RefreshBatch {
    state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .poll(visible, Instant::now())
}

/// Removing the last subscription drops its sender and cancels queued reads.
/// A synchronous provider call already in progress drains under the four-worker
/// limit; it cannot publish a result into a closed tab.
pub async fn read(key: String, serial: u32) -> AppResult<crate::commands::fs::dir::DirListing> {
    let (path, mut cancel) = {
        let manager = state().lock().unwrap_or_else(|e| e.into_inner());
        let entry = manager
            .entries
            .get(&key)
            .filter(|entry| entry.schedule.busy && entry.serial == serial)
            .ok_or_else(|| AppError::new(code::CANCELLED))?;
        (entry.targets[0].path.clone(), entry.cancel.subscribe())
    };
    tokio::select! {
        _ = cancel.changed() => Err(AppError::new(code::CANCELLED)),
        result = crate::commands::fs::listings::read_fresh(path) => result,
    }
}

pub fn finish(key: String, serial: u32, error: Option<String>) {
    state()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .finish(&key, serial, error.map(failure));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(id: &str, path: &str) -> RefreshSubscription {
        RefreshSubscription {
            id: id.into(),
            generation: 3,
            path: path.into(),
        }
    }

    #[test]
    fn overflow_and_rename_invalidate_locations_and_stale_completions_are_ignored() {
        let now = Instant::now();
        let mut manager = Manager::default();
        for path in ["/one", "/two"] {
            manager.entries.insert(
                path.into(),
                Entry {
                    location: location::Location {
                        key: path.into(),
                        scheme: Scheme::Local,
                        native: Some(path.into()),
                    },
                    targets: vec![target(path, path)],
                    schedule: Schedule::new(RefreshMode::NativeWatch, now),
                    error: None,
                    serial: 0,
                    cancel: tokio::sync::watch::channel(false).0,
                },
            );
        }
        manager.events.lock().unwrap().overflow = true;
        assert!(manager.poll(true, now).work.is_empty());
        let batch = manager.poll(true, now + std::time::Duration::from_millis(250));
        assert_eq!(batch.work.len(), 2);
        assert!(batch.status.iter().all(|s| s.error.is_some()));
        for work in batch.work {
            manager.finish(&work.key, work.serial.wrapping_add(1), None);
            assert!(manager.entries[&work.key].schedule.busy);
            manager.finish(&work.key, work.serial, None);
            assert!(!manager.entries[&work.key].schedule.busy);
        }
        let rename = notify::Event::new(notify::EventKind::Modify(
            notify::event::ModifyKind::Name(notify::event::RenameMode::Both),
        ))
        .add_path("/one/old".into())
        .add_path("/two/new".into());
        manager.events.lock().unwrap().queue.push_back(Ok(rename));
        manager.poll(true, now + std::time::Duration::from_secs(1));
        assert_eq!(
            manager
                .poll(true, now + std::time::Duration::from_millis(1250))
                .work
                .len(),
            2
        );
    }

    #[test]
    fn remote_subscriptions_never_create_a_native_watcher() {
        let mut manager = Manager::default();
        manager
            .sync(vec![
                target("sftp", "sftp://unconnected/dir"),
                target("smb", "smb://unconnected/share"),
            ])
            .unwrap();
        assert!(manager.watcher.is_none());
        let batch = manager.poll(true, Instant::now() + std::time::Duration::from_secs(10));
        assert!(batch.work.is_empty());
        assert!(batch
            .status
            .iter()
            .all(|s| !s.connected && s.mode == RefreshMode::Manual));
        manager.sync(vec![]).unwrap();
        assert!(manager.entries.is_empty());
    }

    #[test]
    fn watch_setup_errors_are_structured_and_duplicate_ids_leave_state_intact() {
        let mut manager = Manager::default();
        manager
            .sync(vec![target(
                "missing",
                "/definitely-no-such-hortcmd-directory",
            )])
            .unwrap();
        let batch = manager.poll(true, Instant::now());
        assert!(batch.status[0].error.is_some());
        assert_eq!(batch.status[0].mode, RefreshMode::Manual);
        assert!(manager
            .sync(vec![target("duplicate", "/"), target("duplicate", "/")])
            .is_err());
        assert_eq!(manager.entries.len(), 1);
        assert_eq!(
            manager.entries.values().next().unwrap().targets[0].id,
            "missing"
        );
    }

    #[test]
    fn native_changes_are_shared_and_last_consumer_releases_the_watcher() {
        let path = std::env::temp_dir().join(format!(
            "hortcmd-native-watch-{}",
            crate::mcp::auth::random_hex(12).unwrap()
        ));
        std::fs::create_dir(&path).unwrap();
        let raw = path.to_str().unwrap();
        let mut manager = Manager::default();
        manager
            .sync(vec![target("left", raw), target("right", raw)])
            .unwrap();
        assert_eq!(manager.entries.len(), 1);
        assert!(manager.entries.values().next().unwrap().error.is_none());
        std::fs::write(path.join("created"), b"new entry").unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let work = loop {
            let batch = manager.poll(true, Instant::now());
            if !batch.work.is_empty() {
                break batch.work;
            }
            assert!(
                Instant::now() < deadline,
                "native notification did not arrive"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(work.len(), 1);
        assert_eq!(work[0].targets.len(), 2);
        manager.finish(&work[0].key, work[0].serial, None);
        manager.sync(vec![target("right", raw)]).unwrap();
        assert!(manager.watcher.is_some());
        let cancelled = manager.entries.values().next().unwrap().cancel.subscribe();
        manager.sync(vec![]).unwrap();
        assert!(cancelled.has_changed().is_err());
        assert!(manager.watcher.is_none());
        assert!(manager.entries.is_empty());
        std::fs::remove_dir_all(path).unwrap();
    }
}
