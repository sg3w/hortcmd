//! One cancellable supervisor per canonical store. No file or shell operation
//! is replayed: reconnect only replaces the provider used by future requests.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct MonitorSettings {
    pub enabled: bool,
    pub interval_seconds: u32,
    pub timeout_seconds: u32,
    pub auto_reconnect: bool,
    pub max_attempts: u32,
}
impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 30,
            timeout_seconds: 10,
            auto_reconnect: true,
            max_attempts: 3,
        }
    }
}
impl MonitorSettings {
    pub fn validate(&self) -> AppResult<()> {
        if !(5..=3600).contains(&self.interval_seconds)
            || !(1..=120).contains(&self.timeout_seconds)
            || !(1..=10).contains(&self.max_attempts)
        {
            return Err(
                AppError::new(crate::errors::code::VALIDATION_BAD_INPUT).op("connection-monitor")
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum MonitorState {
    Watching,
    Interrupted,
    Reconnecting,
    Failed,
    Unsupported,
}

pub(super) struct Control {
    pub profile_id: String,
    stop: AtomicBool,
    state: Mutex<MonitorState>,
}
impl Control {
    pub fn active(&self) -> bool {
        !self.stop.load(Ordering::SeqCst)
    }
    fn wait(&self, seconds: u64) -> bool {
        // Cancellation and suspend/resume never cause a burst of catch-up probes.
        let until = std::time::Instant::now() + Duration::from_secs(seconds);
        while self.active() && std::time::Instant::now() < until {
            std::thread::sleep(Duration::from_millis(100));
        }
        self.active()
    }
}
fn controls() -> &'static Mutex<HashMap<String, Arc<Control>>> {
    static CONTROLS: OnceLock<Mutex<HashMap<String, Arc<Control>>>> = OnceLock::new();
    CONTROLS.get_or_init(Default::default)
}
pub(super) fn stop(store: &str) {
    if let Some(control) = controls().lock().unwrap().remove(store) {
        control.stop.store(true, Ordering::SeqCst);
        lifecycle::cancel_for_profile(&control.profile_id);
    }
}
pub(super) fn stop_profile(id: &str) {
    let mut all = controls().lock().unwrap();
    all.retain(|_, control| {
        if control.profile_id != id {
            return true;
        }
        control.stop.store(true, Ordering::SeqCst);
        false
    });
    lifecycle::cancel_for_profile(id);
}
pub(super) fn state(profile: &Profile) -> Option<MonitorState> {
    controls()
        .lock()
        .unwrap()
        .get(&profile.store_id())
        .filter(|c| c.profile_id == profile.id && c.active())
        .map(|c| *c.state.lock().unwrap())
}
pub(super) fn start(app: AppHandle, profile: Profile) {
    let settings = profile.monitor.clone().unwrap_or_default();
    stop(&profile.store_id());
    if !settings.enabled {
        return;
    }
    let control = Arc::new(Control {
        profile_id: profile.id.clone(),
        stop: AtomicBool::new(false),
        state: Mutex::new(MonitorState::Watching),
    });
    controls()
        .lock()
        .unwrap()
        .insert(profile.store_id(), control.clone());
    let correlation = crate::diagnostics::current();
    std::thread::spawn(move || {
        let _scope = correlation.map(crate::diagnostics::Scope::enter);
        supervise(app, profile, settings, control);
    });
}
fn update(app: &AppHandle, control: &Control, state: MonitorState) {
    *control.state.lock().unwrap() = state;
    emit_states(app);
}
fn retryable(failure: Option<ConnFailure>) -> bool {
    // Unknown protocol/TLS/auth failures require the normal interactive flow.
    failure.is_some_and(|f| matches!(f.kind, FailureKind::Timeout | FailureKind::Unreachable))
}
fn backoff(attempt: u32) -> u64 {
    (2u64.pow(attempt.min(6))).min(60)
}

#[derive(Clone, Copy)]
enum Retry {
    Connected,
    Transient,
    Stop,
}

// Shared retry loop, with injectable effects and clock for deterministic tests.
fn reconnect(
    settings: &MonitorSettings,
    mut wait: impl FnMut(u64) -> bool,
    mut attempt: impl FnMut() -> Retry,
    mut state: impl FnMut(MonitorState),
) -> bool {
    state(MonitorState::Interrupted);
    if settings.auto_reconnect {
        for number in 0..settings.max_attempts {
            if !wait(backoff(number)) {
                return false;
            }
            state(MonitorState::Reconnecting);
            match attempt() {
                Retry::Connected => return true,
                Retry::Transient => (),
                Retry::Stop => break,
            }
        }
    }
    state(MonitorState::Failed);
    false
}

#[derive(Default)]
struct ProbeFailures(u32);
impl ProbeFailures {
    fn lost(
        &mut self,
        result: &std::io::Result<crate::vfs::Health>,
        transport_alive: bool,
    ) -> bool {
        use crate::vfs::Health;
        if !transport_alive {
            return true;
        }
        match result {
            Ok(Health::Alive) => self.0 = 0,
            Ok(Health::Busy | Health::Unsupported) => (),
            Err(_) => self.0 += 1,
        }
        self.0 >= 3
    }
}

fn supervise(app: AppHandle, profile: Profile, settings: MonitorSettings, control: Arc<Control>) {
    let store = profile.store_id();
    let mut failures = ProbeFailures::default();
    while control.wait(u64::from(settings.interval_seconds)) {
        let provider = registry().lock().unwrap().live.get(&store).cloned();
        if let Some(provider) = provider {
            let result =
                provider.health_check(Duration::from_secs(u64::from(settings.timeout_seconds)));
            if !control.active() {
                return;
            }
            if matches!(result, Ok(crate::vfs::Health::Unsupported)) {
                update(&app, &control, MonitorState::Unsupported);
            }
            if !failures.lost(&result, provider.transport_alive()) {
                continue;
            }
            report_provider_lost(&store, &provider, "Verbindung zum Server verloren");
        }
        if !control.active() {
            return;
        }
        let restored = reconnect(
            &settings,
            |seconds| control.wait(seconds),
            || {
                let outcome = connect_inner(app.clone(), profile.id.clone(), Some(control.clone()));
                if !control.active() {
                    return Retry::Stop;
                }
                match outcome {
                    Ok(ConnectOutcome::Connected(_)) => Retry::Connected,
                    Ok(ConnectOutcome::HostKey(_)) | Ok(ConnectOutcome::Pending(_)) => Retry::Stop,
                    Err(_) if retryable(lifecycle::snapshot(&profile.id).2) => Retry::Transient,
                    Err(_) => Retry::Stop,
                }
            },
            |state| {
                if control.active() {
                    update(&app, &control, state);
                }
            },
        );
        if !restored {
            return;
        }
        failures = ProbeFailures::default();
        update(&app, &control, MonitorState::Watching);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_profile_validation_preserves_monitor_and_live_provider() {
        let mut profile = Profile {
            id: crate::net::unique_id(),
            name: "monitored".into(),
            protocol: "sftp".into(),
            host: "localhost".into(),
            port: 22,
            user: "test".into(),
            auth: "password".into(),
            key_path: None,
            start_path: "/".into(),
            security: Default::default(),
            monitor: None,
        };
        let store = profile.store_id();
        let provider: Arc<dyn Provider> = Arc::new(crate::vfs::local::LocalProvider);
        let control = Arc::new(Control {
            profile_id: profile.id.clone(),
            stop: AtomicBool::new(false),
            state: Mutex::new(MonitorState::Watching),
        });
        controls()
            .lock()
            .unwrap()
            .insert(store.clone(), control.clone());
        {
            let mut reg = registry().lock().unwrap();
            reg.profiles.push(profile.clone());
            reg.live.insert(store.clone(), provider.clone());
        }
        profile.security.certificate = Some(crate::net::certificate::CertificateTrust {
            path: "/missing-certificate".into(),
            fingerprint: String::new(),
            profile_id: profile.id.clone(),
            endpoint: "invalid-approval".into(),
            is_ca: false,
        });
        let error = net_save_profile(profile.clone(), None).unwrap_err();
        assert!(error.is(crate::errors::code::NET_CERTIFICATE_CHANGED));
        assert!(control.active());
        assert_eq!(state(&profile), Some(MonitorState::Watching));
        {
            let mut reg = registry().lock().unwrap();
            assert!(Arc::ptr_eq(reg.live.get(&store).unwrap(), &provider));
            assert!(reg
                .profiles
                .iter()
                .find(|p| p.id == profile.id)
                .unwrap()
                .security
                .certificate
                .is_none());
            reg.live.remove(&store);
            reg.profiles.retain(|p| p.id != profile.id);
        }
        stop(&store);
    }

    #[test]
    fn legacy_defaults_and_limits() {
        let settings: MonitorSettings = serde_json::from_str("{}").unwrap();
        assert!(settings.enabled && settings.auto_reconnect);
        assert!(settings.validate().is_ok());
        let mut invalid = settings.clone();
        invalid.timeout_seconds = 0;
        assert!(invalid.validate().is_err());
        let saved = serde_json::to_string(&MonitorSettings {
            enabled: false,
            ..settings
        })
        .unwrap();
        assert!(
            !serde_json::from_str::<MonitorSettings>(&saved)
                .unwrap()
                .enabled
        );
    }
    #[test]
    fn bounded_backoff_and_security_failures_stop_retry() {
        assert_eq!((0..4).map(backoff).collect::<Vec<_>>(), vec![1, 2, 4, 8]);
        for kind in [
            FailureKind::Auth,
            FailureKind::HostKey,
            FailureKind::Keychain,
            FailureKind::Protocol,
            FailureKind::Cancelled,
        ] {
            assert!(!retryable(Some(ConnFailure::new(kind, "stop"))));
        }
        assert!(retryable(Some(ConnFailure::new(
            FailureKind::Unreachable,
            "offline"
        ))));
    }
}

#[cfg(test)]
mod supervisor_tests {
    use super::*;
    use crate::vfs::Health;
    use std::cell::Cell;
    #[test]
    fn every_protocol_uses_bounded_misses_and_backoff_without_replaying_work() {
        for _protocol in ["sftp", "ftp", "ftps", "ftps-implicit", "smb", "webdav"] {
            let mut failures = ProbeFailures::default();
            let timeout = || Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
            assert!(!failures.lost(&timeout(), true));
            assert!(!failures.lost(&Ok(Health::Busy), true));
            assert!(!failures.lost(&Ok(Health::Alive), true));
            assert!(!failures.lost(&timeout(), true));
            assert!(!failures.lost(&timeout(), true));
            assert!(failures.lost(&timeout(), true));
            let mut delays = vec![];
            let mut states = vec![];
            let mut attempts = 0;
            assert!(reconnect(
                &MonitorSettings::default(),
                |d| {
                    delays.push(d);
                    true
                },
                || {
                    attempts += 1;
                    if attempts == 2 {
                        Retry::Connected
                    } else {
                        Retry::Transient
                    }
                },
                |s| states.push(s)
            ));
            assert_eq!(delays, vec![1, 2]);
            assert_eq!(
                states,
                vec![
                    MonitorState::Interrupted,
                    MonitorState::Reconnecting,
                    MonitorState::Reconnecting
                ]
            );
            assert!(failures.lost(&Ok(Health::Alive), false));
        }
    }
    #[test]
    fn exhausted_disabled_cancelled_and_security_retries_stop() {
        for (enabled, stop, expected) in [(true, false, 3), (false, false, 0), (true, true, 1)] {
            let settings = MonitorSettings {
                auto_reconnect: enabled,
                ..Default::default()
            };
            let mut calls = 0;
            let mut states = vec![];
            assert!(!reconnect(
                &settings,
                |_| true,
                || {
                    calls += 1;
                    if stop {
                        Retry::Stop
                    } else {
                        Retry::Transient
                    }
                },
                |s| states.push(s)
            ));
            assert_eq!(calls, expected);
            assert_eq!(states.last(), Some(&MonitorState::Failed));
        }
        let cancelled = Cell::new(false);
        let mut calls = 0;
        assert!(!reconnect(
            &MonitorSettings::default(),
            |_| !cancelled.get(),
            || {
                calls += 1;
                cancelled.set(true);
                Retry::Transient
            },
            |_| {}
        ));
        assert_eq!(calls, 1);
    }
    #[test]
    fn stop_invalidates_delayed_supervisor_and_running_attempt() {
        let id = crate::net::unique_id();
        let control = Arc::new(Control {
            profile_id: id.clone(),
            stop: AtomicBool::new(false),
            state: Mutex::new(MonitorState::Watching),
        });
        controls()
            .lock()
            .unwrap()
            .insert(id.clone(), control.clone());
        let Start::Started(attempt) = lifecycle::begin(&id) else {
            panic!("unique profile");
        };
        stop(&id);
        assert!(!control.active());
        assert!(!control.wait(0));
        assert!(attempt.cancelled());
        let installed = Cell::new(false);
        assert!(!lifecycle::settle(&attempt, Ok(()), || installed.set(true)));
        assert!(!installed.get());
        lifecycle::forget(&id);
    }
}
