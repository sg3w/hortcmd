//! Provider refresh policy and bounded scheduling, independent of a UI runtime.
//! The native watcher and subscription commands use this clock state rather than
//! putting filesystem policy in the frontend.
pub mod location;
pub mod subscriptions;

use crate::vfs::Scheme;
use serde::Serialize;
use std::time::{Duration, Instant};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum RefreshMode {
    NativeWatch,
    AdaptivePolling,
    Manual,
}

pub fn mode(scheme: Scheme, connected: bool) -> RefreshMode {
    if !connected {
        return RefreshMode::Manual;
    }
    match scheme {
        Scheme::Local => RefreshMode::NativeWatch,
        Scheme::Sftp | Scheme::Ftp | Scheme::Smb | Scheme::Webdav => RefreshMode::AdaptivePolling,
    }
}

const DEBOUNCE: Duration = Duration::from_millis(250);
const MAX_BURST: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

pub struct Schedule {
    pub mode: RefreshMode,
    pub busy: bool,
    due: Instant,
    first_change: Option<Instant>,
    failures: u32,
    connected: bool,
    visible: bool,
}

impl Schedule {
    pub fn new(mode: RefreshMode, now: Instant) -> Self {
        Self {
            mode,
            busy: false,
            due: now + POLL,
            first_change: None,
            failures: 0,
            connected: true,
            visible: true,
        }
    }

    /// A storm may postpone the debounce only up to one second from its first
    /// change. Changes during a read remain pending for a subsequent refresh.
    pub fn changed(&mut self, now: Instant) {
        let first = *self.first_change.get_or_insert(now);
        self.due = (now + DEBOUNCE).min(first + MAX_BURST);
    }

    pub fn visibility(&mut self, visible: bool, now: Instant) {
        if visible && !self.visible {
            self.due = now;
            self.first_change = Some(now);
        }
        self.visible = visible;
    }

    pub fn connection(&mut self, connected: bool, now: Instant) {
        if connected && !self.connected {
            self.failures = 0;
            self.due = now;
            self.first_change = Some(now);
        }
        self.connected = connected;
    }

    pub fn take_due(&mut self, now: Instant) -> bool {
        if self.busy || !self.visible || !self.connected || self.mode == RefreshMode::Manual {
            return false;
        }
        if self.mode == RefreshMode::NativeWatch && self.first_change.is_none() {
            return false;
        }
        if now < self.due {
            return false;
        }
        self.busy = true;
        self.first_change = None;
        true
    }

    pub fn completed(&mut self, success: bool, now: Instant) {
        self.busy = false;
        self.failures = if success {
            0
        } else {
            (self.failures + 1).min(4)
        };
        if self.first_change.is_some() {
            return;
        }
        let delay = POLL.saturating_mul(1 << self.failures).min(MAX_BACKOFF);
        self.due = now + delay;
        if !success {
            // A native watch can still be installed while the listing fails.
            // Retry with backoff even if no second filesystem event arrives.
            self.first_change = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_remote_scheme_polls_only_when_connected() {
        assert_eq!(mode(Scheme::Local, true), RefreshMode::NativeWatch);
        for scheme in [Scheme::Sftp, Scheme::Ftp, Scheme::Smb, Scheme::Webdav] {
            assert_eq!(mode(scheme, true), RefreshMode::AdaptivePolling);
            assert_eq!(mode(scheme, false), RefreshMode::Manual);
        }
    }

    #[test]
    fn bursts_coalesce_but_cannot_starve_a_refresh() {
        let start = Instant::now();
        let mut schedule = Schedule::new(RefreshMode::NativeWatch, start);
        assert!(!schedule.take_due(start + Duration::from_secs(30)));
        for ms in (0..1000).step_by(100) {
            schedule.changed(start + Duration::from_millis(ms));
            assert!(!schedule.take_due(start + Duration::from_millis(ms)));
        }
        assert!(schedule.take_due(start + MAX_BURST));
        assert!(!schedule.take_due(start + MAX_BURST));
    }

    #[test]
    fn a_change_during_a_read_causes_a_second_refresh() {
        let start = Instant::now();
        let mut schedule = Schedule::new(RefreshMode::NativeWatch, start);
        schedule.changed(start);
        assert!(schedule.take_due(start + DEBOUNCE));
        schedule.changed(start + DEBOUNCE);
        schedule.completed(true, start + DEBOUNCE);
        assert!(!schedule.take_due(start + DEBOUNCE));
        assert!(schedule.take_due(start + DEBOUNCE * 2));
    }

    #[test]
    fn polling_suspends_when_hidden_or_disconnected_and_resumes_immediately() {
        let start = Instant::now();
        let mut schedule = Schedule::new(RefreshMode::AdaptivePolling, start);
        schedule.visibility(false, start);
        assert!(!schedule.take_due(start + POLL));
        schedule.visibility(true, start + POLL);
        assert!(schedule.take_due(start + POLL));
        schedule.completed(true, start + POLL);
        schedule.connection(false, start + POLL);
        assert!(!schedule.take_due(start + POLL * 3));
        schedule.connection(true, start + POLL * 3);
        assert!(schedule.take_due(start + POLL * 3));
    }

    #[test]
    fn errors_back_off_to_one_minute_and_success_resets_the_delay() {
        let start = Instant::now();
        let mut now = start + POLL;
        let mut schedule = Schedule::new(RefreshMode::AdaptivePolling, start);
        for delay in [10, 20, 40, 60, 60] {
            assert!(schedule.take_due(now));
            schedule.completed(false, now);
            now += Duration::from_secs(delay);
            assert!(!schedule.take_due(now - Duration::from_millis(1)));
        }
        assert!(schedule.take_due(now));
        schedule.completed(true, now);
        assert!(schedule.take_due(now + POLL));
    }
}
