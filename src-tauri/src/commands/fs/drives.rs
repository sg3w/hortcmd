//! Mounted-volume metadata only: no directory walks or data reads.
use super::dir::{drive_snapshot, Drive};
use std::time::{Duration, Instant};
use tauri::Emitter;

const POLL: Duration = Duration::from_secs(2);
const CAPACITY_INTERVAL: Duration = Duration::from_secs(10);
const CAPACITY_DELTA: u64 = 1024 * 1024;

fn changed(previous: &[Drive], current: &[Drive], elapsed: Duration) -> bool {
    previous.len() != current.len()
        || previous.iter().zip(current).any(|(a, b)| {
            a.mount != b.mount
                || a.name != b.name
                || a.total != b.total
                || (elapsed >= CAPACITY_INTERVAL && a.free.abs_diff(b.free) >= CAPACITY_DELTA)
        })
}

fn snapshot() -> Option<Vec<Drive>> {
    match std::panic::catch_unwind(drive_snapshot) {
        Ok(Some(drives)) => Some(drives),
        _ => {
            eprintln!("[hortcmd] Volume metadata unavailable; retaining last snapshot");
            None
        }
    }
}

#[derive(Default)]
struct Sampler {
    #[cfg(target_os = "macos")]
    topology: Vec<String>,
    #[cfg(target_os = "macos")]
    next_capacity: Option<Instant>,
}
impl Sampler {
    fn poll(&mut self) -> Option<Vec<Drive>> {
        #[cfg(target_os = "macos")]
        let topology = super::drives_native::mounts()
            .or_else(|| {
                eprintln!("[hortcmd] Mount table unavailable; retaining last snapshot");
                None
            })?
            .into_iter()
            .map(|mount| mount.drive.mount)
            .collect::<Vec<_>>();
        #[cfg(target_os = "macos")]
        if topology == self.topology && self.next_capacity.is_some_and(|next| Instant::now() < next)
        {
            return None;
        }
        let drives = snapshot()?;
        #[cfg(target_os = "macos")]
        {
            self.topology = topology;
            self.next_capacity = Some(Instant::now() + CAPACITY_INTERVAL);
        }
        Some(drives)
    }
}

pub fn start(app: tauri::AppHandle) {
    if let Err(error) = std::thread::Builder::new()
        .name("volume-watch".into())
        .spawn(move || {
            let mut sampler = Sampler::default();
            let mut previous = Vec::new();
            let mut emitted = Instant::now();
            loop {
                if let Some(drives) = sampler.poll() {
                    if changed(&previous, &drives, emitted.elapsed())
                        && app.emit("drives-changed", &drives).is_ok()
                    {
                        previous = drives;
                        emitted = Instant::now();
                    }
                }
                std::thread::sleep(POLL);
            }
        })
    {
        eprintln!("[hortcmd] Volume watcher could not start: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn drive(mount: &str, free: u64) -> Drive {
        Drive {
            name: mount.into(),
            mount: mount.into(),
            total: 100_000_000,
            free,
        }
    }
    #[test]
    fn topology_is_immediate_capacity_is_bounded_and_idle_is_silent() {
        let old = vec![drive("/", 50_000_000)];
        for seconds in (0..=120).step_by(2) {
            assert!(!changed(&old, &old, Duration::from_secs(seconds)));
            assert!(!changed(
                &old,
                &[drive("/", 49_999_000)],
                Duration::from_secs(seconds)
            ));
        }
        let inserted = vec![old[0].clone(), drive("/Volumes/card", 10_000_000)];
        assert!(changed(&old, &inserted, Duration::ZERO));
        assert!(changed(&inserted, &old, Duration::ZERO));
        assert!(!changed(
            &old,
            &[drive("/", 40_000_000)],
            Duration::from_secs(2)
        ));
        assert!(changed(
            &old,
            &[drive("/", 40_000_000)],
            Duration::from_secs(10)
        ));
    }
    /// Run explicitly while mounting/unmounting an owned test image.
    #[test]
    #[ignore]
    fn native_volume_probe() {
        let mut sampler = Sampler::default();
        let mut previous = sampler.poll().expect("initial volume snapshot");
        let mut emitted = Instant::now();
        let started = Instant::now();
        let mut spent = Duration::ZERO;
        for _ in 0..40 {
            std::thread::sleep(POLL);
            let at = Instant::now();
            if let Some(current) = sampler.poll() {
                if changed(&previous, &current, emitted.elapsed()) {
                    eprintln!(
                        "VOLUME CHANGE {}s: {:?}",
                        started.elapsed().as_secs(),
                        current
                            .iter()
                            .map(|d| (&d.mount, d.free))
                            .collect::<Vec<_>>()
                    );
                    previous = current;
                    emitted = Instant::now();
                }
            }
            spent += at.elapsed();
        }
        eprintln!("40 metadata polls took {:?}", spent);
    }
}
