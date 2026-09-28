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
    capacity_checks: Option<Arc<std::sync::atomic::AtomicU32>>,
    readonly: bool,
    no_staging: bool,
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
    fn limits(
        &self,
        _: &crate::vfs::StreamPath,
        check_capacity: bool,
    ) -> std::io::Result<vfs::limits::ProviderLimits> {
        if check_capacity {
            if let Some(count) = &self.capacity_checks {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(vfs::limits::ProviderLimits {
            writable: Some(!self.readonly),
            available_bytes: if check_capacity { self.free } else { None },
            quota_remaining_bytes: if check_capacity { self.quota } else { None },
            staging: !self.no_staging,
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

#[test]
fn disabled_capacity_skips_probes_but_preserves_other_guards() {
    let (_root, src, dst) = fixture();
    let source = format!("{src}/a");
    std::fs::write(&source, "12345").unwrap();
    for kind in [RecoveryKind::Copy, RecoveryKind::Move] {
        for enabled in [false, true] {
            let settings = RecoverySettings {
                check_space: Some(enabled),
                ..Default::default()
            };
            let plan = build(
                kind.clone(),
                std::slice::from_ref(&source),
                &dst,
                settings.clone(),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(
                enabled
                    || !plan
                        .issues
                        .iter()
                        .any(|i| i.code == "capacityUnknown" || i.code == "noSpace")
            );
            assert!(enabled || plan.limits.available().is_none());
            let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
            let mut provider = Capacity {
                capacity_checks: Some(calls.clone()),
                free: Some(0),
                quota: Some(0),
                ..Default::default()
            };
            // This guard is used before writing and again at publication.
            for _ in 0..2 {
                assert_eq!(
                    revalidate_destination(&plan, &provider, &dst.as_str().into(), 5).is_ok(),
                    !enabled
                );
            }
            assert_eq!(calls.load(Ordering::Relaxed), if enabled { 2 } else { 0 });
            provider.readonly = true;
            assert!(revalidate_destination(&plan, &provider, &dst.as_str().into(), 5).is_err());
            provider.readonly = false;
            provider.no_staging = true;
            assert!(revalidate_destination(&plan, &provider, &dst.as_str().into(), 5).is_err());
            let self_copy = build(
                kind.clone(),
                std::slice::from_ref(&source),
                &src,
                settings,
                &AtomicBool::new(false),
            )
            .unwrap();
            assert!(self_copy.ensure_executable().is_err());
        }
    }
}

#[test]
fn old_transfer_settings_keep_space_checks_enabled() {
    let old = r#"{"limit":0,"verify":false,"bufKb":256,"threads":1,"safe":true}"#;
    let settings: RecoverySettings = serde_json::from_str(old).unwrap();
    assert!(settings.checks_space());
    let disabled = RecoverySettings {
        check_space: Some(false),
        ..settings
    };
    assert!(
        !serde_json::from_str::<RecoverySettings>(&serde_json::to_string(&disabled).unwrap())
            .unwrap()
            .checks_space()
    );
}

/// The advisory has to mean something: a plain local plan preserves timestamps
/// and modes, so it no longer fires there, while a store that cannot say —
/// every remote adapter, which takes the trait's default limits — still does
/// (ticket 385).
#[test]
fn metadata_advisory_fires_only_where_fidelity_is_unknown() {
    let (_root, src, dst) = fixture();
    std::fs::write(Path::new(&src).join("a"), b"hello").unwrap();
    let plan = build(
        RecoveryKind::Copy,
        std::slice::from_ref(&src),
        &dst,
        RecoverySettings::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        !plan.issues.iter().any(|i| i.code == "metadataUnknown"),
        "{:?}",
        plan.issues
    );

    let path = crate::vfs::StreamPath::from(Path::new(&dst));
    assert_eq!(
        vfs::local::LocalProvider
            .limits(&path, false)
            .unwrap()
            .preserves_metadata,
        Some(true)
    );
    // A provider that implements only the required operations keeps the
    // unknown answer, so its plans keep the advisory.
    assert_eq!(
        Capacity {
            store: Some("sftp:test".into()),
            ..Default::default()
        }
        .limits(&path, false)
        .unwrap()
        .preserves_metadata,
        None
    );
}
