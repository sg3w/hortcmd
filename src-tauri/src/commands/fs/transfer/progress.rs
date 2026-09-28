// ============================================================
// Progress reporting (ticket 035).
//
// Two shapes of the same job: `Prog` for the sequential path, which
// owns its counters, and `SharedProg` for the parallel one, where
// several workers advance the same totals. Both throttle to one report
// per 40 ms, both block while paused, and both route through a
// `ProgressSink`.
//
// The sink is what makes the copy paths testable at all — an
// `AppHandle` cannot be built outside a running application, so without
// it the most valuable part of the engine could only be exercised
// end-to-end.
// ============================================================

use super::events::OpProgress;
use crate::vfs::Provider;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Where progress reports go.
///
/// In the running app this is the Tauri event bus. Behind a trait
/// because the copy paths are the part of the engine most worth
/// testing, and an `AppHandle` cannot be built outside a running
/// application — with a sink, a test drives a real copy and reads the
/// reports back (TICKET-035). Production behaviour is unchanged: the
/// blanket implementation below emits exactly the event it always did.
pub(crate) trait ProgressSink: Send + Sync {
    fn progress(&self, report: OpProgress);
}

impl ProgressSink for AppHandle {
    fn progress(&self, report: OpProgress) {
        let _ = self.emit("fs-progress", report);
    }
}

/// Per-item remarks a transfer collects while it runs: things that did not
/// stop the bytes from arriving but that the user is still told about — a
/// timestamp or mode the destination refused (ticket 385). Shared, because the
/// parallel workers fill the same list.
pub(crate) type Notes = Arc<Mutex<Vec<super::events::ItemError>>>;

/// Records one remark. A poisoned lock loses the remark rather than the
/// transfer: nothing here is worth failing a committed copy for.
pub(crate) fn note(notes: &Notes, item: &str, error: crate::errors::AppError) {
    if let Ok(mut notes) = notes.lock() {
        if notes.len() < 1000 {
            notes.push(super::events::ItemError::new(item, error));
        }
    }
}

/// Takes the collected remarks, leaving the list empty.
pub(crate) fn drain_notes(notes: &Notes) -> Vec<super::events::ItemError> {
    notes
        .lock()
        .map(|mut notes| std::mem::take(&mut *notes))
        .unwrap_or_default()
}

/// Holds the cumulative counters + cancel flag and sends throttled events.
/// Also used by the archive module (extract/pack).
pub(crate) struct Prog {
    pub(crate) sink: Arc<dyn ProgressSink>,
    pub(crate) id: String,
    pub(crate) op: String,
    pub(crate) files_total: u32,
    pub(crate) bytes_total: u64,
    pub(crate) files_done: u32,
    pub(crate) bytes_done: u64,
    pub(crate) last: Instant,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) pause: Arc<AtomicBool>,
    /// Remarks about items that were copied but not fully reproduced.
    pub(crate) notes: Notes,
}

impl Prog {
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn emit(&mut self, file_name: &str, file_done: u64, file_total: u64, force: bool) {
        self.emit_state(file_name, file_done, file_total, force, false);
    }

    fn emit_state(
        &mut self,
        file_name: &str,
        file_done: u64,
        file_total: u64,
        force: bool,
        paused: bool,
    ) {
        // Don't send more often than every 40 ms (exception: forced ticks).
        if !force && self.last.elapsed() < Duration::from_millis(40) {
            return;
        }
        self.last = Instant::now();
        self.sink.progress(OpProgress {
            id: self.id.clone(),
            op: self.op.clone(),
            file_name: file_name.to_string(),
            file_done,
            file_total,
            files_done: self.files_done,
            files_total: self.files_total,
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            paused,
        });
    }

    /// Blocks while the operation is paused (and not cancelled),
    /// keeping the UI informed via throttled "paused" ticks.
    /// Resets the throttle/rate reference after resuming.
    pub(crate) fn wait_if_paused(&mut self, file_name: &str, file_done: u64, file_total: u64) {
        if !self.pause.load(Ordering::Relaxed) {
            return;
        }
        while self.pause.load(Ordering::Relaxed) && !self.cancelled() {
            self.emit_state(file_name, file_done, file_total, true, true);
            std::thread::sleep(Duration::from_millis(150));
        }
        self.last = Instant::now();
    }
}

#[cfg(test)]
impl Prog {
    /// A progress object whose reports go nowhere. For tests that drive a real
    /// copy and care about its file-system effects, not about its events.
    pub(crate) fn silent(op: &str) -> Self {
        struct Discard;
        impl ProgressSink for Discard {
            fn progress(&self, _: OpProgress) {}
        }
        Self {
            sink: Arc::new(Discard),
            id: "test".into(),
            op: op.into(),
            files_total: 0,
            bytes_total: 0,
            files_done: 0,
            bytes_done: 0,
            last: Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            notes: Default::default(),
        }
    }
}

/// Thread-safe progress for the parallel copy path.
pub(crate) struct SharedProg {
    pub(crate) srcp: Arc<dyn Provider>,
    pub(crate) dstp: Arc<dyn Provider>,
    pub(crate) sink: Arc<dyn ProgressSink>,
    pub(crate) id: String,
    pub(crate) op: String,
    pub(crate) files_total: u32,
    pub(crate) bytes_total: u64,
    pub(crate) bytes_done: AtomicU64,
    pub(crate) files_done: AtomicU32,
    pub(crate) last: Mutex<Instant>,
    pub(crate) rate: Mutex<(Instant, u64)>, // (Fensterbeginn, Bytes im Fenster)
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) pause: Arc<AtomicBool>,
    /// Remarks about items that were copied but not fully reproduced.
    pub(crate) notes: Notes,
}

impl SharedProg {
    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn emit(
        &self,
        file_name: &str,
        file_done: u64,
        file_total: u64,
        force: bool,
        paused: bool,
    ) {
        {
            let mut last = self.last.lock().unwrap();
            if !force && last.elapsed() < Duration::from_millis(40) {
                return;
            }
            *last = Instant::now();
        }
        self.sink.progress(OpProgress {
            id: self.id.clone(),
            op: self.op.clone(),
            file_name: file_name.to_string(),
            file_done,
            file_total,
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            bytes_done: self.bytes_done.load(Ordering::Relaxed),
            bytes_total: self.bytes_total,
            paused,
        });
    }

    /// Blocks while paused (and not cancelled); afterwards resets
    /// the rate window so the throttle does not catch up.
    pub(crate) fn wait_if_paused(&self, name: &str, file_done: u64, file_total: u64) {
        if !self.pause.load(Ordering::Relaxed) {
            return;
        }
        while self.pause.load(Ordering::Relaxed) && !self.cancelled() {
            self.emit(name, file_done, file_total, true, true);
            std::thread::sleep(Duration::from_millis(150));
        }
        if let Ok(mut g) = self.rate.lock() {
            *g = (Instant::now(), 0);
        }
    }

    /// Global speed limit across all workers (limit = bytes/s).
    pub(crate) fn rate_limit(&self, n: usize, limit: u64) {
        if limit == 0 {
            return;
        }
        let sleep_secs;
        {
            let mut g = self.rate.lock().unwrap();
            g.1 += n as u64;
            let elapsed = g.0.elapsed().as_secs_f64();
            let allowed = limit as f64 * elapsed;
            sleep_secs = if g.1 as f64 > allowed {
                (g.1 as f64 - allowed) / limit as f64
            } else {
                0.0
            };
            if elapsed >= 1.0 {
                *g = (Instant::now(), 0);
            }
        }
        if sleep_secs > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(sleep_secs.min(1.0)));
        }
    }
}
