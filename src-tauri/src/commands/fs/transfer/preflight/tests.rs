use super::*;
use std::path::{Path, PathBuf};
#[test]
fn checked_peak_space_counts_backups_without_allocating_them_twice() {
    let estimate = space(&RecoveryKind::Copy, 100, 40, 2, true, true).unwrap();
    assert_eq!(estimate.additional_bytes, 100);
    assert_eq!(estimate.staging_bytes, 100);
    assert_eq!(estimate.backup_bytes, 40);
    assert_eq!(estimate.peak_footprint_bytes, 140);
    assert_eq!(estimate.verification_read_bytes, 200);
    assert!(space(&RecoveryKind::Copy, u64::MAX, 1, 1, true, false).is_err());
}

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Temp, String, String) {
    let root = std::env::temp_dir().join(format!("hortcmd-preflight-{}", crate::net::unique_id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir(root.join("dst")).unwrap();
    let root = root.canonicalize().unwrap();
    let source = root.join("src").to_string_lossy().into_owned();
    let dest = root.join("dst").to_string_lossy().into_owned();
    (Temp(root), source, dest)
}
#[test]
fn dry_run_detects_overlap_duplicates_nested_sources_and_preserves_destination() {
    let (root, src, dst) = fixture();
    std::fs::write(Path::new(&src).join("a"), b"hello").unwrap();
    let cancel = AtomicBool::new(false);
    let settings = RecoverySettings::default();
    let plan = build(
        RecoveryKind::Copy,
        std::slice::from_ref(&src),
        &dst,
        settings.clone(),
        &cancel,
    )
    .unwrap();
    plan.ensure_executable().unwrap();
    assert_eq!(plan.mappings[0].files, 1);
    assert_eq!(plan.space.additional_bytes, 5);
    assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 0);
    assert!(!root.0.join("history").exists());
    let self_copy = build(
        RecoveryKind::Copy,
        std::slice::from_ref(&src),
        &src,
        settings.clone(),
        &cancel,
    )
    .unwrap();
    assert!(self_copy.ensure_executable().is_err());
    let duplicate = build(
        RecoveryKind::Copy,
        &[src.clone(), src.clone()],
        &dst,
        settings.clone(),
        &cancel,
    )
    .unwrap();
    assert!(duplicate.issues.iter().any(|i| i.code == "duplicateTarget"));
    let nested = build(
        RecoveryKind::Copy,
        &[src.clone(), format!("{src}/a")],
        &dst,
        settings,
        &cancel,
    )
    .unwrap();
    assert!(nested.issues.iter().any(|i| i.code == "nestedSources"));
    assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 0);
}
#[test]
fn changed_source_and_replaced_destination_invalidate_approved_plan() {
    let (root, src, dst) = fixture();
    let file = format!("{src}/a");
    std::fs::write(&file, "a").unwrap();
    let settings = RecoverySettings::default();
    let cancel = AtomicBool::new(false);
    let before = build(
        RecoveryKind::Copy,
        std::slice::from_ref(&file),
        &dst,
        settings.clone(),
        &cancel,
    )
    .unwrap();
    std::fs::write(&file, "changed").unwrap();
    let after = build(
        RecoveryKind::Copy,
        std::slice::from_ref(&file),
        &dst,
        settings,
        &cancel,
    )
    .unwrap();
    assert!(after.matches_review(&before.digest).is_err());
    std::fs::rename(&dst, root.0.join("retained-destination")).unwrap();
    std::fs::create_dir(&dst).unwrap();
    assert!(revalidate_destination(
        &after,
        &vfs::local::LocalProvider,
        &crate::vfs::StreamPath::from(Path::new(&dst)),
        0
    )
    .is_err());
}
#[test]
fn native_move_requires_no_second_copy_and_cancellation_never_mutates() {
    let (_root, src, dst) = fixture();
    let file = format!("{src}/a");
    std::fs::write(&file, "12345").unwrap();
    let plan = build(
        RecoveryKind::Move,
        std::slice::from_ref(&file),
        &dst,
        RecoverySettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(plan.mappings[0].native_move);
    assert_eq!(plan.space.additional_bytes, 0);
    assert!(build(
        RecoveryKind::Copy,
        &[file],
        &dst,
        RecoverySettings::default(),
        &AtomicBool::new(true)
    )
    .is_err());
    assert_eq!(std::fs::read_dir(&dst).unwrap().count(), 0);
}
#[test]
fn walker_enforces_names_objects_cycles_and_missing_sources() {
    let (_root, src, dst) = fixture();
    let file = Path::new(&src).join("too-long");
    std::fs::write(&file, "12345").unwrap();
    let cancel = AtomicBool::new(false);
    let provider = vfs::local::LocalProvider;
    let limits = vfs::limits::ProviderLimits {
        maximum_name_bytes: Some(3),
        ..Default::default()
    };
    assert!(walk::Walker::new(&cancel)
        .measure(
            &provider,
            &crate::vfs::StreamPath::from(&file),
            &crate::vfs::StreamPath::from(&Path::new(&dst).join("too-long")),
            &limits
        )
        .is_err());
    let limits = vfs::limits::ProviderLimits {
        maximum_object_bytes: Some(3),
        ..Default::default()
    };
    assert!(walk::Walker::new(&cancel)
        .measure(
            &provider,
            &crate::vfs::StreamPath::from(&file),
            &crate::vfs::StreamPath::from(&Path::new(&dst).join("a")),
            &limits
        )
        .is_err());
    assert!(measure(
        &provider,
        &crate::vfs::StreamPath::from(&Path::new(&src).join("missing"))
    )
    .is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&src, Path::new(&src).join("cycle")).unwrap();
        assert!(walk::Walker::new(&cancel)
            .measure(
                &provider,
                &crate::vfs::StreamPath::from(Path::new(&src)),
                &crate::vfs::StreamPath::from(&Path::new(&dst).join("src")),
                &Default::default()
            )
            .is_err());
    }
}

#[derive(Default)]
struct Capacity {
    synthetic: Option<Arc<std::sync::atomic::AtomicU64>>,
    stop: Option<Arc<AtomicBool>>,
    store: Option<String>,
    free: Option<u64>,
    quota: Option<u64>,
}
impl Provider for Capacity {
    fn store_id(&self) -> String {
        self.store.clone().unwrap_or_else(|| "local".into())
    }
    fn list(&self, p: &str) -> AppResult<crate::commands::fs::dir::DirListing> {
        vfs::local::LocalProvider.list(p)
    }
    fn stat(&self, p: &crate::vfs::StreamPath) -> std::io::Result<vfs::Stat> {
        if self.synthetic.is_some() {
            return Ok(vfs::Stat {
                is_dir: p == &crate::vfs::StreamPath::from("/synthetic"),
                len: 1,
            });
        }
        vfs::local::LocalProvider.stat(p)
    }
    fn exists(&self, p: &crate::vfs::StreamPath) -> bool {
        p.native().unwrap().exists()
    }
    fn is_dir(&self, p: &crate::vfs::StreamPath) -> bool {
        p.native().unwrap().is_dir()
    }
    fn read_dir(&self, p: &crate::vfs::StreamPath) -> std::io::Result<Vec<crate::vfs::StreamPath>> {
        vfs::local::LocalProvider.read_dir(p)
    }
    fn visit_dir(
        &self,
        path: &crate::vfs::StreamPath,
        limit: usize,
        visit: &mut dyn FnMut(crate::vfs::StreamPath) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        if let Some(count) = &self.synthetic {
            for i in 0..1_000_000 {
                if i >= limit {
                    return Err(std::io::Error::other("preflight directory limit"));
                }
                count.fetch_add(1, Ordering::Relaxed);
                if i == 100 {
                    if let Some(stop) = &self.stop {
                        stop.store(true, Ordering::Relaxed);
                    }
                }
                visit(path.join(format!("f-{i}"))?)?;
            }
            Ok(())
        } else {
            vfs::local::LocalProvider.visit_dir(path, limit, visit)
        }
    }
    fn open_read(
        &self,
        p: &crate::vfs::StreamPath,
    ) -> std::io::Result<Box<dyn std::io::Read + Send>> {
        vfs::local::LocalProvider.open_read(p)
    }
    fn create_write(
        &self,
        _: &crate::vfs::StreamPath,
    ) -> std::io::Result<Box<dyn std::io::Write + Send>> {
        panic!("preflight must not write")
    }
    fn write(&self, _: &crate::vfs::StreamPath, _: &[u8]) -> std::io::Result<()> {
        panic!("preflight must not write")
    }
    fn create_dir_all(&self, _: &crate::vfs::StreamPath) -> std::io::Result<()> {
        panic!("preflight must not create")
    }
    fn rename(
        &self,
        _: &crate::vfs::StreamPath,
        _: &crate::vfs::StreamPath,
    ) -> std::io::Result<()> {
        panic!("preflight must not rename")
    }
    fn remove_file(&self, _: &crate::vfs::StreamPath) -> std::io::Result<()> {
        panic!("preflight must not remove")
    }
    fn remove_dir_all(&self, _: &crate::vfs::StreamPath) -> std::io::Result<()> {
        panic!("preflight must not remove")
    }
    fn limits(&self, _: &crate::vfs::StreamPath) -> std::io::Result<vfs::limits::ProviderLimits> {
        Ok(vfs::limits::ProviderLimits {
            available_bytes: self.free,
            quota_remaining_bytes: self.quota,
            staging: true,
            ..Default::default()
        })
    }
}
#[test]
fn live_capacity_uses_the_stricter_known_limit_and_keeps_unknown_explicit() {
    let (_root, src, dst) = fixture();
    let file = format!("{src}/a");
    std::fs::write(&file, "12345").unwrap();
    let plan = build(
        RecoveryKind::Copy,
        &[file],
        &dst,
        RecoverySettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    for (free, quota, allowed) in [
        (Some(100), Some(4), false),
        (Some(4), Some(100), false),
        (None, None, true),
        (Some(100), None, true),
    ] {
        let provider = Capacity {
            free,
            quota,
            ..Default::default()
        };
        assert_eq!(
            revalidate_destination(
                &plan,
                &provider,
                &crate::vfs::StreamPath::from(Path::new(&dst)),
                5
            )
            .is_ok(),
            allowed
        );
    }
}

#[test]
fn million_entry_source_is_streamed_to_a_bound_and_cancellation_stops_work() {
    use std::sync::atomic::AtomicU64;
    for cancelled in [false, true] {
        let count = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let provider = Capacity {
            synthetic: Some(count.clone()),
            stop: cancelled.then(|| cancel.clone()),
            ..Default::default()
        };
        assert!(walk::Walker::new(&cancel)
            .measure(
                &provider,
                &crate::vfs::StreamPath::from(Path::new("/synthetic")),
                &crate::vfs::StreamPath::from(Path::new("/out")),
                &Default::default()
            )
            .is_err());
        assert!(count.load(Ordering::Relaxed) <= if cancelled { 101 } else { 50_000 });
    }
}
#[test]
fn a_new_provider_for_the_same_store_has_a_new_generation() {
    let first: Arc<dyn Provider> = Arc::new(Capacity {
        store: Some("test:generation".into()),
        ..Default::default()
    });
    let old = vfs::identity::generation(&first).unwrap();
    assert_eq!(old, vfs::identity::generation(&first).unwrap());
    drop(first);
    let next: Arc<dyn Provider> = Arc::new(Capacity {
        store: Some("test:generation".into()),
        ..Default::default()
    });
    assert_ne!(old, vfs::identity::generation(&next).unwrap());
}

#[test]
fn publication_refuses_a_destination_changed_after_collision_review() {
    let (_root, src, dst) = fixture();
    let source = format!("{src}/a");
    let target = Path::new(&dst).join("a");
    std::fs::write(&source, "new").unwrap();
    std::fs::write(&target, "old").unwrap();
    let reviewed = build(
        RecoveryKind::Copy,
        &[source],
        &dst,
        RecoverySettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let provider = vfs::local::LocalProvider;
    let mut plan = super::super::plan::Plan::staged(
        &provider,
        crate::vfs::StreamPath::from(target.clone()),
        true,
    )
    .unwrap();
    plan.preflight = Some(Arc::new(reviewed));
    plan.observe_target(&provider).unwrap();
    std::fs::write(plan.write_to.native().unwrap(), "new").unwrap();
    std::fs::write(&target, "external writer").unwrap();
    assert!(super::super::plan::publish(&provider, &plan).is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "external writer");
    assert_eq!(
        std::fs::read_to_string(plan.write_to.native().unwrap()).unwrap(),
        "new"
    );
}
#[test]
fn case_equivalent_destinations_and_declared_reserved_names_are_rejected() {
    let (root, src, dst) = fixture();
    let other = root.0.join("other");
    std::fs::create_dir(&other).unwrap();
    let a = Path::new(&src).join("A");
    let b = other.join("a");
    std::fs::write(&a, "A").unwrap();
    std::fs::write(&b, "B").unwrap();
    let plan = build(
        RecoveryKind::Copy,
        &[
            a.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
        ],
        &dst,
        RecoverySettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(plan.issues.iter().any(|i| i.code == "duplicateTarget"));
    let limits = vfs::limits::ProviderLimits {
        reserved_names: Some(vec!["reserved".into()]),
        ..Default::default()
    };
    assert!(walk::check_name(
        &crate::vfs::StreamPath::from(Path::new("/dst/RESERVED")),
        &limits
    )
    .is_err());
    assert_eq!(walk::folded("É"), walk::folded("e\u{301}"));
}
