// ============================================================
// Cancel tokens for long-running background tasks (folder size,
// search). The frontend passes a token when starting a task and uses
// the same token to cancel it.
//
// A cancel can arrive before the task registers (dialog closed right
// away) – the flag is then stored in advance and reused, so the run
// starts already cancelled and the cancel cannot be lost.
// ============================================================

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

type CancelMap = Mutex<HashMap<String, Arc<AtomicBool>>>;
static CANCELS: OnceLock<CancelMap> = OnceLock::new();

fn cancels() -> &'static CancelMap {
    CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Tokens of different task kinds live in the same map, so they are
/// namespaced to keep them apart.
fn key(kind: &str, token: &str) -> String {
    format!("{}:{}", kind, token)
}

/// Flag of a task; created if the token is not known yet.
pub fn register(kind: &str, token: &str) -> Arc<AtomicBool> {
    match cancels().lock() {
        Ok(mut map) => map
            .entry(key(kind, token))
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone(),
        // A poisoned registry must not kill the task – it then simply
        // cannot be cancelled any more.
        Err(_) => Arc::new(AtomicBool::new(false)),
    }
}

pub fn unregister(kind: &str, token: &str) {
    if let Ok(mut map) = cancels().lock() {
        map.remove(&key(kind, token));
    }
}

/// Sets the cancel flag. Also works before the task has registered.
pub fn cancel(kind: &str, token: &str) {
    if let Ok(mut map) = cancels().lock() {
        map.entry(key(kind, token))
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
