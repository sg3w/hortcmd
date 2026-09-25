// ============================================================
// Cancel tokens for long-running background tasks (folder size,
// search). The frontend passes a token when starting a task and uses
// the same token to cancel it.
//
// A cancel can arrive before the task registers (dialog closed right
// away) – the flag is then stored in advance and reused, so the run
// starts already cancelled. The most recent 1,024 pre-arms are kept;
// active tasks are never evicted.
// ============================================================

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const MAX_PENDING: usize = 1024;

#[derive(Default)]
struct CancelRegistry {
    active: HashMap<String, Arc<AtomicBool>>,
    // Unknown tokens may be early cancels or late requests for completed work.
    // Bound these independently so pressure never evicts an active task.
    pending: VecDeque<String>,
}
impl CancelRegistry {
    fn register(&mut self, key: String) -> Arc<AtomicBool> {
        if let Some(flag) = self.active.get(&key) {
            return flag.clone();
        }
        let cancelled = self.take_pending(&key);
        let flag = Arc::new(AtomicBool::new(cancelled));
        self.active.insert(key, flag.clone());
        flag
    }

    fn take_pending(&mut self, key: &str) -> bool {
        if let Some(position) = self.pending.iter().position(|entry| entry == key) {
            self.pending.remove(position);
            true
        } else {
            false
        }
    }

    fn unregister(&mut self, key: &str) {
        self.active.remove(key);
        self.take_pending(key);
    }

    fn cancel(&mut self, key: String) {
        if let Some(flag) = self.active.get(&key) {
            flag.store(true, Ordering::Relaxed);
            return;
        }
        self.take_pending(&key);
        if self.pending.len() == MAX_PENDING {
            self.pending.pop_front();
        }
        self.pending.push_back(key);
    }
}

static CANCELS: OnceLock<Mutex<CancelRegistry>> = OnceLock::new();
fn cancels() -> &'static Mutex<CancelRegistry> {
    CANCELS.get_or_init(Default::default)
}

/// Tokens of different task kinds live in the same map, so they are
/// namespaced to keep them apart.
fn key(kind: &str, token: &str) -> String {
    format!("{}:{}", kind, token)
}

/// Flag of a task; created if the token is not known yet.
pub fn register(kind: &str, token: &str) -> Arc<AtomicBool> {
    match cancels().lock() {
        Ok(mut map) => map.register(key(kind, token)),
        // A poisoned registry must not kill the task – it then simply
        // cannot be cancelled any more.
        Err(_) => Arc::new(AtomicBool::new(false)),
    }
}

pub fn unregister(kind: &str, token: &str) {
    if let Ok(mut map) = cancels().lock() {
        map.unregister(&key(kind, token));
    }
}

/// Sets the cancel flag. Also works before the task has registered.
pub fn cancel(kind: &str, token: &str) {
    if let Ok(mut map) = cancels().lock() {
        map.cancel(key(kind, token));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_cancels_are_bounded_without_evicting_active_tasks_or_recent_prearms() {
        let mut registry = CancelRegistry::default();
        let active = registry.register("active".into());
        for i in 0..MAX_PENDING * 10 {
            let key = format!("completed-{i}");
            let flag = registry.register(key.clone());
            registry.unregister(&key);
            drop(flag);
            registry.cancel(key);
        }
        assert_eq!(registry.active.len(), 1);
        assert_eq!(registry.pending.len(), MAX_PENDING);
        registry.cancel("active".into());
        assert!(active.load(Ordering::Relaxed));
        assert!(Arc::ptr_eq(&active, &registry.register("active".into())));
        let recent = format!("completed-{}", MAX_PENDING * 10 - 1);
        registry.cancel(recent.clone());
        assert_eq!(registry.pending.len(), MAX_PENDING);
        assert!(registry.register(recent.clone()).load(Ordering::Relaxed));
        registry.unregister(&recent);
        assert!(!registry.register(recent).load(Ordering::Relaxed));
        assert!(!registry
            .register("completed-0".into())
            .load(Ordering::Relaxed));
    }

    #[test]
    fn cancel_sets_the_flag() {
        let flag = register("test-a", "t-1");
        assert!(!flag.load(Ordering::Relaxed));
        cancel("test-a", "t-1");
        assert!(flag.load(Ordering::Relaxed));
        unregister("test-a", "t-1");
        // Unknown tokens must not panic.
        cancel("test-a", "t-1");
        unregister("test-a", "t-1");
    }

    #[test]
    fn cancel_before_start_is_not_lost() {
        cancel("test-b", "t-2");
        let flag = register("test-b", "t-2");
        assert!(flag.load(Ordering::Relaxed));
        unregister("test-b", "t-2");
    }

    #[test]
    fn kinds_do_not_collide() {
        let a = register("test-c", "same");
        let b = register("test-d", "same");
        cancel("test-c", "same");
        assert!(a.load(Ordering::Relaxed));
        assert!(!b.load(Ordering::Relaxed));
        unregister("test-c", "same");
        unregister("test-d", "same");
    }
}
