//! Durable intent and per-item commit boundaries (TICKET-044).
pub mod commands;
pub mod durability;
mod location;
pub mod reconcile;
mod storage;
pub mod types;

use crate::errors::{code, AppError, AppResult};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use storage::{Storage, MAX_ITEMS, MAX_RECORDS};
use types::{Intent, Item, Phase, Record};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn invalid() -> AppError {
    AppError::new(code::VALIDATION_BAD_INPUT).op("transfer-journal")
}

struct State {
    records: BTreeMap<String, Record>,
    active: HashSet<String>,
    next: u64,
}
pub struct Journal {
    storage: Storage,
    state: Mutex<State>,
}

impl Journal {
    fn persist(&self, record: &Record) -> AppResult<()> {
        if serde_json::to_vec(record).map_err(|_| invalid())?.len() > storage::MAX_RECORD_BYTES {
            return Err(AppError::new(code::TRANSFER_JOURNAL_LIMIT));
        }
        self.storage.save(record).map_err(|error| {
            AppError::new(code::TRANSFER_JOURNAL_UNAVAILABLE)
                .op("transfer-journal")
                .detail(error)
        })
    }

    pub fn open(path: &Path) -> AppResult<Arc<Self>> {
        let unavailable = |error: std::io::Error| {
            AppError::new(code::TRANSFER_JOURNAL_UNAVAILABLE)
                .op("transfer-journal")
                .detail(error)
        };
        let storage = Storage::open(path).map_err(unavailable)?;
        let records = storage.load().map_err(unavailable)?;
        let next = records
            .iter()
            .map(|r| r.order)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(invalid)?;
        Ok(Arc::new(Self {
            storage,
            state: Mutex::new(State {
                records: records.into_iter().map(|r| (r.id.clone(), r)).collect(),
                active: HashSet::new(),
                next,
            }),
        }))
    }

    /// Must succeed before the frontend calls an operation queued or any writer starts.
    pub fn enqueue(&self, id: &str, intent: Intent) -> AppResult<()> {
        self.enqueue_retry(id, intent, None)
    }

    fn enqueue_retry(&self, id: &str, intent: Intent, retry_of: Option<String>) -> AppResult<()> {
        if id.is_empty()
            || id.len() > 128
            || id.chars().any(char::is_control)
            || intent.sources.is_empty()
            || intent.sources.len() > MAX_ITEMS
            || intent.names.len() > MAX_ITEMS
        {
            return Err(invalid());
        }
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if let Some(existing) = state.records.get(id) {
            return if existing.intent == intent && !existing.terminal {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        if retry_of.as_ref().is_some_and(|parent| {
            state.active.contains(parent)
                || state
                    .records
                    .values()
                    .any(|r| r.retry_of.as_ref() == Some(parent))
        }) {
            return Err(invalid());
        }
        // Only acknowledged terminal history expires automatically. Interrupted
        // records and filesystem artifacts are never evicted to make room.
        let mut completed: Vec<_> = state
            .records
            .values()
            .filter(|r| r.terminal && r.acknowledged)
            .map(|r| (r.updated, r.id.clone()))
            .collect();
        completed.sort();
        let excess = completed.len().saturating_sub(100);
        for (index, (updated, id)) in completed.into_iter().enumerate() {
            if index < excess || now().saturating_sub(updated) > 30 * 24 * 3600 {
                self.storage.remove(&id)?;
                state.records.remove(&id);
            }
        }
        if state.records.len() >= MAX_RECORDS {
            return Err(AppError::new(code::TRANSFER_JOURNAL_LIMIT));
        }
        let timestamp = now();
        let record = Record {
            version: 1,
            id: id.into(),
            intent,
            preflight: None,
            order: state.next,
            created: timestamp,
            updated: timestamp,
            started: false,
            terminal: false,
            acknowledged: false,
            retry_of,
            reason: None,
            items: Vec::new(),
        };
        self.persist(&record)?;
        state.next = state.next.checked_add(1).ok_or_else(invalid)?;
        let parent = record.retry_of.clone();
        state.records.insert(id.into(), record);
        if let Some(parent) = parent {
            if let Some(mut old) = state.records.get(&parent).cloned() {
                old.terminal = true;
                old.acknowledged = true;
                old.reason = Some("replaced".into());
                self.persist(&old)?;
                state.records.insert(parent, old);
            }
        }
        Ok(())
    }

    pub fn attach_preflight(
        &self,
        id: &str,
        plan: super::preflight::types::OperationPlan,
    ) -> AppResult<()> {
        self.update(id, |record| {
            if record.started || record.terminal {
                return Err(invalid());
            }
            record.preflight = Some(plan);
            Ok(())
        })
    }

    pub fn begin(self: &Arc<Self>, id: &str) -> AppResult<Handle> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if state.active.contains(id) {
            return Err(invalid());
        }
        let mut record = state.records.get(id).cloned().ok_or_else(invalid)?;
        if record.started || record.terminal {
            return Err(invalid());
        }
        record.started = true;
        record.updated = now();
        self.persist(&record)?;
        state.records.insert(id.into(), record);
        state.active.insert(id.into());
        Ok(Handle {
            journal: self.clone(),
            id: id.into(),
        })
    }

    fn update(&self, id: &str, update: impl FnOnce(&mut Record) -> AppResult<()>) -> AppResult<()> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        let mut record = state.records.get(id).cloned().ok_or_else(invalid)?;
        update(&mut record)?;
        record.updated = now();
        self.persist(&record)?;
        state.records.insert(id.into(), record);
        Ok(())
    }

    pub fn records(&self) -> AppResult<Vec<Record>> {
        let state = self.state.lock().map_err(|_| invalid())?;
        let mut records: Vec<_> = state.records.values().cloned().collect();
        records.sort_by_key(|r| r.order);
        let replaced: HashSet<_> = records.iter().filter_map(|r| r.retry_of.clone()).collect();
        records.retain(|r| !replaced.contains(&r.id));
        Ok(records)
    }

    pub fn active(&self, id: &str) -> bool {
        self.state
            .lock()
            .map(|s| s.active.contains(id))
            .unwrap_or(true)
    }

    pub fn acknowledge(&self, id: &str) -> AppResult<()> {
        let record = self
            .records()?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(invalid)?;
        let state = reconcile::entry(self, &record).state;
        if state != types::RecoveryState::Completed
            && !(record.terminal
                && matches!(
                    state,
                    types::RecoveryState::Retryable | types::RecoveryState::Resumable
                ))
        {
            return Err(invalid());
        }
        self.update(id, |r| {
            r.terminal = true;
            r.acknowledged = true;
            Ok(())
        })
    }

    /// Explicitly discards metadata only. It never removes a staging/backup/source.
    pub fn discard(&self, id: &str) -> AppResult<()> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if state.active.contains(id) {
            return Err(invalid());
        }
        self.storage.remove(id)?;
        state.records.remove(id);
        Ok(())
    }
}

#[derive(Clone)]
pub struct Handle {
    journal: Arc<Journal>,
    id: String,
}
#[derive(Clone)]
pub struct Step {
    handle: Handle,
    index: usize,
}

impl Handle {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn track(&self, item: Item) -> AppResult<Step> {
        let mut index = 0;
        self.journal.update(&self.id, |r| {
            if r.items.len() >= MAX_ITEMS || r.terminal {
                return Err(AppError::new(code::TRANSFER_JOURNAL_LIMIT));
            }
            index = r.items.len();
            r.items.push(item);
            Ok(())
        })?;
        fault("planned");
        Ok(Step {
            handle: self.clone(),
            index,
        })
    }
    pub fn finish(&self, reason: Option<&str>) -> AppResult<()> {
        crate::commands::fs::undo::seal(&self.id)?;
        self.journal.update(&self.id, |r| {
            r.terminal = true;
            r.reason = reason.map(str::to_owned);
            Ok(())
        })?;
        self.journal
            .state
            .lock()
            .map_err(|_| invalid())?
            .active
            .remove(&self.id);
        Ok(())
    }
    pub fn preflight(&self) -> AppResult<Option<super::preflight::types::OperationPlan>> {
        let state = self.journal.state.lock().map_err(|_| invalid())?;
        Ok(state
            .records
            .get(&self.id)
            .ok_or_else(invalid)?
            .preflight
            .clone())
    }
    pub fn intent(&self) -> AppResult<Intent> {
        let state = self.journal.state.lock().map_err(|_| invalid())?;
        Ok(state
            .records
            .get(&self.id)
            .ok_or_else(invalid)?
            .intent
            .clone())
    }
}

impl Step {
    /// Normal cancellation/failure can discard an unpublished private stage while
    /// its owner is still known. Once publication begins, only reconciliation may
    /// decide what can be removed.
    pub fn cleanup_unpublished(&self) -> AppResult<()> {
        let item = {
            let state = self.handle.journal.state.lock().map_err(|_| invalid())?;
            state
                .records
                .get(&self.handle.id)
                .and_then(|r| r.items.get(self.index))
                .cloned()
                .ok_or_else(invalid)?
        };
        if !matches!(
            item.phase,
            Phase::Planned | Phase::Writing | Phase::Prepared
        ) {
            return Ok(());
        }
        let Some(owner) = item.owner else {
            return Ok(());
        };
        let types::RecoveryLocation::Local { path } = owner.location else {
            return Ok(());
        };
        let path = Path::new(&path);
        if !path.try_exists()? {
            return Ok(());
        }
        if durability::object_identity(path)?.as_deref() != Some(&owner.identity) {
            return Err(invalid());
        }
        if let Some(types::RecoveryLocation::Local { path: backup }) = &item.backup {
            if Path::new(backup).try_exists()? {
                return Err(invalid());
            }
        }
        std::fs::remove_dir_all(path)?;
        durability::sync_parent(path)?;
        self.transition(Phase::Skipped)
    }

    pub fn own(&self, location: types::RecoveryLocation) -> AppResult<()> {
        let types::RecoveryLocation::Local { path } = &location else {
            return Err(invalid());
        };
        let identity = durability::object_identity(Path::new(path))?.ok_or_else(invalid)?;
        self.handle.journal.update(&self.handle.id, |r| {
            r.items.get_mut(self.index).ok_or_else(invalid)?.owner =
                Some(types::OwnedArtifact { location, identity });
            Ok(())
        })
    }

    pub fn transition(&self, phase: Phase) -> AppResult<()> {
        if phase == Phase::Committed {
            fault("before_commit_record");
        }
        self.handle.journal.update(&self.handle.id, |r| {
            let item = r.items.get_mut(self.index).ok_or_else(invalid)?;
            item.phase = phase;
            Ok(())
        })?;
        match phase {
            Phase::Writing => fault("writing"),
            Phase::Committed => fault("committed"),
            Phase::SourceRemoved => fault("source_removed"),
            _ => {}
        }
        Ok(())
    }
    pub fn prepared(&self, digest: Option<String>, durable: bool) -> AppResult<()> {
        self.handle.journal.update(&self.handle.id, |r| {
            let item = r.items.get_mut(self.index).ok_or_else(invalid)?;
            item.phase = Phase::Prepared;
            item.digest = digest;
            item.durable = durable;
            Ok(())
        })?;
        fault("prepared");
        Ok(())
    }
}

pub fn journal() -> AppResult<Arc<Journal>> {
    static JOURNAL: OnceLock<Mutex<Option<Arc<Journal>>>> = OnceLock::new();
    let mut value = JOURNAL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| invalid())?;
    if let Some(journal) = &*value {
        return Ok(journal.clone());
    }
    let journal = Journal::open(&crate::config::path("transfers-v1")?)?;
    *value = Some(journal.clone());
    Ok(journal)
}

/// Ends in-memory ownership even when a worker unwinds; the persisted state then
/// remains interrupted instead of being mislabeled successful.
pub struct Run(pub Handle);
impl Drop for Run {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.journal.state.lock() {
            state.active.remove(&self.0.id);
        }
    }
}

impl Intent {
    pub fn transfer(
        kind: types::RecoveryKind,
        sources: &[String],
        destination: &str,
        settings: types::RecoverySettings,
    ) -> AppResult<Self> {
        Ok(Self {
            kind,
            sources: sources
                .iter()
                .map(|s| types::RecoveryLocation::capture(s))
                .collect::<AppResult<_>>()?,
            destination: types::RecoveryLocation::capture(destination)?,
            settings,
            base: String::new(),
            names: Vec::new(),
        })
    }
}

pub fn begin(id: &str, intent: Intent) -> AppResult<Run> {
    let journal = journal()?;
    journal.enqueue(id, intent)?;
    Ok(Run(journal.begin(id)?))
}

/// Fault points are compiled only into the Rust test binary and enabled only by
/// subprocess tests. A production app cannot be terminated through an environment flag.
pub(crate) fn fault(_point: &str) {
    #[cfg(test)]
    if std::env::var("HORTCMD_RECOVERY_TEST_CHILD").as_deref() == Ok("1")
        && std::env::var("HORTCMD_RECOVERY_TEST_POINT").as_deref() == Ok(_point)
    {
        std::process::exit(86);
    }
}

#[cfg(test)]
mod tests;

/// This iteration is enabled on macOS. Windows/Linux adapters and native
/// durability checks are explicitly deferred; their existing transfer paths remain.
pub fn supported() -> bool {
    cfg!(target_os = "macos")
}
pub fn optional_begin(
    id: &str,
    intent: impl FnOnce() -> AppResult<Intent>,
) -> AppResult<Option<Run>> {
    if supported() {
        begin(id, intent()?).map(Some)
    } else {
        Ok(None)
    }
}
