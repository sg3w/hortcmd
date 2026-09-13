use super::types::*;
use super::*;
use crate::commands::fs::transfer::{
    copy::{self, CopyTuning},
    events::OpProgress,
    plan::Plan,
    progress::{Prog, ProgressSink},
};
use crate::vfs::local::LocalProvider;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("hortcmd-recovery-{}", crate::net::unique_id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn local(path: &Path) -> RecoveryLocation {
    RecoveryLocation::Local {
        path: path.to_string_lossy().into_owned(),
    }
}
fn intent(root: &Path, kind: RecoveryKind) -> Intent {
    Intent {
        kind,
        sources: vec![local(&root.join("input/result.txt"))],
        destination: local(&root.join("output")),
        settings: RecoverySettings {
            verify: true,
            threads: 4,
            limit: 0,
            buf_kb: 4,
            safe: true,
        },
        base: String::new(),
        names: Vec::new(),
    }
}
struct Quiet;
impl ProgressSink for Quiet {
    fn progress(&self, _: OpProgress) {}
}
fn progress() -> Prog {
    Prog {
        sink: Arc::new(Quiet),
        id: "job".into(),
        op: "move".into(),
        files_total: 1,
        bytes_total: 7,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
    }
}

#[test]
fn queue_order_settings_and_process_lock_survive_reopen() {
    let temp = Temp::new();
    let root = temp.0.join("journal");
    let journal = Journal::open(&root).unwrap();
    journal
        .enqueue("second-lexically", intent(&temp.0, RecoveryKind::Copy))
        .unwrap();
    journal
        .enqueue("a-first-lexically", intent(&temp.0, RecoveryKind::Move))
        .unwrap();
    assert!(
        Journal::open(&root).is_err(),
        "a second process must not mutate the same journal"
    );
    drop(journal);
    let reopened = Journal::open(&root).unwrap();
    let records = reopened.records().unwrap();
    assert_eq!(
        records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["second-lexically", "a-first-lexically"]
    );
    assert_eq!(records[0].intent.settings.threads, 4);
    assert!(records[0].intent.settings.verify);
    assert_eq!(
        reconcile::entry(&reopened, &records[0]).state,
        RecoveryState::Queued
    );
}

#[test]
fn corrupt_future_or_oversized_records_fail_closed_without_replacing_evidence() {
    for bytes in [
        b"{truncated".to_vec(),
        b"{\"version\":900}".to_vec(),
        vec![0; storage::MAX_RECORD_BYTES + 1],
    ] {
        let temp = Temp::new();
        let journal = Journal::open(&temp.0).unwrap();
        drop(journal);
        fs::write(temp.0.join("broken.json"), &bytes).unwrap();
        assert!(Journal::open(&temp.0).is_err());
        assert_eq!(fs::read(temp.0.join("broken.json")).unwrap(), bytes);
    }
}

#[test]
fn identity_and_secret_bearing_addresses_cannot_enter_remote_intent() {
    use crate::net::session::Profile;
    let profile = Profile {
        id: "profile-id".into(),
        name: "Example".into(),
        protocol: "sftp".into(),
        host: "example.test".into(),
        port: 22,
        user: "alice".into(),
        auth: "password".into(),
        key_path: None,
        start_path: "/".into(),
    };
    let location = RecoveryLocation::capture_with(
        "sftp://alice@example.test:22/data",
        std::slice::from_ref(&profile),
    )
    .unwrap();
    let serialized = serde_json::to_string(&location).unwrap();
    assert!(serialized.contains("profile-id"));
    assert!(!serialized.contains("alice"));
    assert!(!serialized.contains("example.test"));
    for raw in [
        "sftp://alice:SEEDED_PASSWORD@example.test:22/data",
        "sftp://alice@example.test:22/data?token=SEEDED_TOKEN",
        "sftp://alice@example.test:22/data#SEEDED_FRAGMENT",
        "https://example.test/signed?token=SEEDED_TOKEN",
    ] {
        assert!(RecoveryLocation::capture_with(raw, std::slice::from_ref(&profile)).is_err());
    }
    assert!(
        serde_json::from_str::<Intent>(r#"{"kind":"copy","password":"SEEDED_PASSWORD"}"#).is_err()
    );
}

#[test]
fn changed_or_replaced_staging_and_backups_are_never_cleaned_on_a_guess() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("input")).unwrap();
    fs::create_dir(temp.0.join("output")).unwrap();
    fs::write(temp.0.join("input/result.txt"), b"new").unwrap();
    fs::write(temp.0.join("output/result.txt"), b"old").unwrap();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    journal
        .enqueue("job", intent(&temp.0, RecoveryKind::Copy))
        .unwrap();
    let handle = journal.begin("job").unwrap();
    let mut plan = Plan::staged(&LocalProvider, temp.0.join("output/result.txt"), true);
    plan.track(&handle, &temp.0.join("input/result.txt"), &LocalProvider)
        .unwrap();
    let stage = plan.owned_root.clone().unwrap();
    drop(Run(handle));
    let record = journal.records().unwrap().remove(0);
    let displaced = temp.0.join("displaced");
    fs::rename(&stage, &displaced).unwrap();
    fs::create_dir(&stage).unwrap();
    fs::write(stage.join("unrelated"), b"keep").unwrap();
    assert!(reconcile::cleanup(&journal, &record).is_err());
    assert_eq!(fs::read(stage.join("unrelated")).unwrap(), b"keep");
    fs::remove_dir_all(&stage).unwrap();
    fs::rename(&displaced, &stage).unwrap();
    fs::rename(
        temp.0.join("output/result.txt"),
        plan.backup.as_ref().unwrap(),
    )
    .unwrap();
    assert!(reconcile::cleanup(&journal, &record).is_err());
    assert_eq!(fs::read(plan.backup.as_ref().unwrap()).unwrap(), b"old");
}

#[test]
fn crash_child() {
    if std::env::var("HORTCMD_RECOVERY_TEST_CHILD").as_deref() != Ok("1") {
        return;
    }
    let root = PathBuf::from(std::env::var_os("HORTCMD_RECOVERY_TEST_ROOT").unwrap());
    let journal = Journal::open(&root.join("journal")).unwrap();
    let mode = std::env::var("HORTCMD_RECOVERY_TEST_MODE").unwrap_or_default();
    journal
        .enqueue(
            "job",
            intent(
                &root,
                if mode == "parallel" {
                    RecoveryKind::Copy
                } else {
                    RecoveryKind::Move
                },
            ),
        )
        .unwrap();
    let run = Run(journal.begin("job").unwrap());
    let mut plan = Plan::staged(&LocalProvider, root.join("output/result.txt"), true);
    plan.native_move = mode == "native";
    plan.track(&run.0, &root.join("input/result.txt"), &LocalProvider)
        .unwrap();
    if mode == "native" {
        plan.writing().unwrap();
        crate::commands::fs::transfer::plan::publish(&LocalProvider, &plan).unwrap();
        plan.recovery
            .as_ref()
            .unwrap()
            .transition(Phase::SourceRemoved)
            .unwrap();
        run.0.finish(None).unwrap();
        return;
    }
    if mode == "parallel" {
        use crate::commands::fs::transfer::{
            parallel::{commit_finished_jobs, parallel_copy_file, CopyTask},
            progress::SharedProg,
        };
        use std::sync::atomic::{AtomicU32, AtomicU64};
        let sp = SharedProg {
            srcp: Arc::new(LocalProvider),
            dstp: Arc::new(LocalProvider),
            sink: Arc::new(Quiet),
            id: "job".into(),
            op: "copy".into(),
            files_total: 1,
            bytes_total: 8,
            bytes_done: AtomicU64::new(0),
            files_done: AtomicU32::new(0),
            last: Mutex::new(Instant::now()),
            rate: Mutex::new((Instant::now(), 0)),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
        };
        plan.writing().unwrap();
        parallel_copy_file(
            &CopyTask {
                idx: 0,
                src: root.join("input/result.txt"),
                dst: plan.write_to.clone(),
            },
            &sp,
            CopyTuning {
                limit: 0,
                verify: true,
                buf_size: 4,
            },
        )
        .unwrap();
        let mut errors = Vec::new();
        assert_eq!(
            commit_finished_jobs(&LocalProvider, &[&plan], &[true], false, &mut errors),
            1
        );
        assert!(errors.is_empty(), "{errors:?}");
        run.0.finish(None).unwrap();
        return;
    }
    let mut errors = Vec::new();
    copy::transfer_one(
        &LocalProvider,
        &LocalProvider,
        &root.join("input/result.txt"),
        &plan,
        &mut progress(),
        CopyTuning {
            limit: 0,
            verify: true,
            buf_size: 4,
        },
        true,
        &mut errors,
    )
    .unwrap();
    assert!(errors.is_empty(), "{errors:?}");
    run.0.finish(None).unwrap();
}

#[test]
fn forced_process_exit_preserves_source_and_old_destination_at_every_commit_boundary() {
    for point in [
        "planned",
        "staging",
        "writing",
        "write",
        "flush",
        "prepared",
        "backup",
        "swapped",
        "before_commit_record",
        "committed",
        "source_removed",
    ] {
        let temp = Temp::new();
        fs::create_dir(temp.0.join("input")).unwrap();
        fs::create_dir(temp.0.join("output")).unwrap();
        fs::write(temp.0.join("input/result.txt"), b"new-data").unwrap();
        fs::write(temp.0.join("output/result.txt"), b"old-data").unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::fs::transfer::recovery::tests::crash_child",
                "--nocapture",
            ])
            .env("HORTCMD_RECOVERY_TEST_CHILD", "1")
            .env("HORTCMD_RECOVERY_TEST_POINT", point)
            .env("HORTCMD_RECOVERY_TEST_ROOT", &temp.0)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .status()
            .unwrap();
        assert_eq!(
            status.code(),
            Some(86),
            "fault point {point} was not exercised"
        );
        let journal = Journal::open(&temp.0.join("journal")).unwrap();
        let record = journal.records().unwrap().remove(0);
        assert!(!record.terminal);
        let view = reconcile::entry(&journal, &record);
        assert_ne!(view.state, RecoveryState::Running);
        if point != "source_removed" {
            assert_eq!(
                fs::read(temp.0.join("input/result.txt")).unwrap(),
                b"new-data",
                "{point}"
            );
        }
        if [
            "planned", "staging", "writing", "write", "flush", "prepared",
        ]
        .contains(&point)
        {
            assert_eq!(
                fs::read(temp.0.join("output/result.txt")).unwrap(),
                b"old-data",
                "{point}"
            );
        } else if point == "backup" {
            let RecoveryLocation::Local { path } = record.items[0].backup.as_ref().unwrap() else {
                panic!()
            };
            assert_eq!(fs::read(path).unwrap(), b"old-data");
            assert!(reconcile::cleanup(&journal, &record).is_err());
        } else {
            assert_eq!(
                fs::read(temp.0.join("output/result.txt")).unwrap(),
                b"new-data",
                "{point}"
            );
        }
        // Merely inspecting recovery never removes a remaining source.
        let source_before = temp.0.join("input/result.txt").exists();
        let _ = reconcile::entry(&journal, &record);
        assert_eq!(temp.0.join("input/result.txt").exists(), source_before);
    }
}

#[test]
fn retry_revalidates_committed_content_and_remaining_sources() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("input")).unwrap();
    fs::create_dir(temp.0.join("output")).unwrap();
    fs::write(temp.0.join("input/result.txt"), b"new-data").unwrap();
    fs::write(temp.0.join("input/remaining.txt"), b"remaining").unwrap();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    let mut request = intent(&temp.0, RecoveryKind::Copy);
    request
        .sources
        .push(local(&temp.0.join("input/remaining.txt")));
    journal.enqueue("job", request).unwrap();
    let run = Run(journal.begin("job").unwrap());
    let mut plan = Plan::staged(&LocalProvider, temp.0.join("output/result.txt"), false);
    plan.track(&run.0, &temp.0.join("input/result.txt"), &LocalProvider)
        .unwrap();
    copy::transfer_one(
        &LocalProvider,
        &LocalProvider,
        &temp.0.join("input/result.txt"),
        &plan,
        &mut progress(),
        CopyTuning {
            limit: 0,
            verify: false,
            buf_size: 4,
        },
        false,
        &mut Vec::new(),
    )
    .unwrap();
    drop(run);
    let record = journal.records().unwrap().remove(0);
    assert_eq!(
        reconcile::entry(&journal, &record).state,
        RecoveryState::Resumable
    );
    let retry = reconcile::retry_intent(&journal, &record, true).unwrap();
    assert_eq!(retry.sources, [local(&temp.0.join("input/remaining.txt"))]);
    assert_eq!(retry.settings.threads, 4);
    fs::write(temp.0.join("output/result.txt"), b"externally changed").unwrap();
    assert!(reconcile::retry_intent(&journal, &record, true).is_err());
    assert_eq!(
        fs::read(temp.0.join("input/result.txt")).unwrap(),
        b"new-data"
    );
}

#[test]
fn direct_write_interruption_is_never_reported_as_committed() {
    let temp = Temp::new();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    let mut request = intent(&temp.0, RecoveryKind::Copy);
    request.settings.safe = false;
    journal.enqueue("job", request).unwrap();
    drop(Run(journal.begin("job").unwrap()));
    assert_eq!(
        reconcile::entry(&journal, &journal.records().unwrap()[0]).state,
        RecoveryState::UnsafeInterrupted
    );
}

#[test]
fn parallel_planning_errors_propagate_before_any_job_can_be_committed() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("source")).unwrap();
    fs::write(temp.0.join("source/a"), b"new").unwrap();
    fs::write(temp.0.join("blocked"), b"old-destination").unwrap();
    let result = crate::commands::fs::transfer::parallel::flatten_tree(
        &LocalProvider,
        &LocalProvider,
        &temp.0.join("source"),
        &temp.0.join("blocked/sub"),
        0,
        &mut Vec::new(),
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(temp.0.join("blocked")).unwrap(),
        b"old-destination"
    );
}

#[test]
fn archive_crash_child() {
    use crate::commands::fs::archive::{extract, pack, safety};
    if std::env::var("HORTCMD_RECOVERY_TEST_CHILD").as_deref() != Ok("1") {
        return;
    }
    let root = PathBuf::from(std::env::var_os("HORTCMD_RECOVERY_TEST_ROOT").unwrap());
    let packing = std::env::var("HORTCMD_RECOVERY_TEST_ARCHIVE").unwrap() == "pack";
    let journal = Journal::open(&root.join("journal")).unwrap();
    let mut request = intent(
        &root,
        if packing {
            RecoveryKind::Pack
        } else {
            RecoveryKind::Extract
        },
    );
    if packing {
        request.destination = local(&root.join("output/result.zip"));
    } else {
        request.sources = vec![local(&root.join("input/source.zip"))];
    }
    journal.enqueue("job", request).unwrap();
    let run = Run(journal.begin("job").unwrap());
    let mut prog = progress();
    prog.files_total = 0;
    prog.bytes_total = 0;
    if packing {
        pack::run_with_recovery(
            &[root.join("input/result.txt").to_string_lossy().into_owned()],
            &root.join("output/result.zip").to_string_lossy(),
            &mut prog,
            safety::Limits::default(),
            &|_| Ok(()),
            Some(&run.0),
        )
        .unwrap();
    } else {
        let (_, errors) = extract::run_with_recovery(
            extract::ExtractJob {
                archive: root.join("input/source.zip").to_string_lossy().into_owned(),
                base_inner: String::new(),
                names: Vec::new(),
                dest_dir: root.join("output").to_string_lossy().into_owned(),
                pw: None,
            },
            &mut prog,
            safety::Limits::default(),
            Some(&run.0),
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:?}");
    }
    run.0.finish(None).unwrap();
}

#[test]
fn archive_process_exit_preserves_old_or_complete_new_data() {
    use std::io::{Read, Write};
    for mode in ["pack", "extract"] {
        for point in [
            "staging",
            "write",
            "flush",
            "prepared",
            "backup",
            "swapped",
            "committed",
        ] {
            let temp = Temp::new();
            fs::create_dir(temp.0.join("input")).unwrap();
            fs::create_dir(temp.0.join("output")).unwrap();
            fs::write(temp.0.join("input/result.txt"), b"new-data").unwrap();
            let source_archive = temp.0.join("input/source.zip");
            let mut zip = zip::ZipWriter::new(fs::File::create(&source_archive).unwrap());
            zip.start_file("result.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"new-data").unwrap();
            zip.finish().unwrap();
            let target = temp.0.join(if mode == "pack" {
                "output/result.zip"
            } else {
                "output/result.txt"
            });
            fs::write(&target, b"old-data").unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "commands::fs::transfer::recovery::tests::archive_crash_child",
                    "--nocapture",
                ])
                .env("HORTCMD_RECOVERY_TEST_CHILD", "1")
                .env("HORTCMD_RECOVERY_TEST_POINT", point)
                .env("HORTCMD_RECOVERY_TEST_ROOT", &temp.0)
                .env("HORTCMD_RECOVERY_TEST_ARCHIVE", mode)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(86), "{mode}/{point}");
            let journal = Journal::open(&temp.0.join("journal")).unwrap();
            let record = journal.records().unwrap().remove(0);
            let first = reconcile::entry(&journal, &record);
            let second = reconcile::entry(&journal, &record);
            assert_eq!(first.state, second.state);
            assert_ne!(first.state, RecoveryState::Running);
            assert_eq!(
                fs::read(temp.0.join("input/result.txt")).unwrap(),
                b"new-data"
            );
            assert!(source_archive.exists());
            if point == "backup" {
                let backup = record
                    .items
                    .iter()
                    .find_map(|item| item.backup.as_ref())
                    .unwrap();
                let RecoveryLocation::Local { path } = backup else {
                    panic!()
                };
                assert_eq!(fs::read(path).unwrap(), b"old-data");
                assert!(reconcile::cleanup(&journal, &record).is_err());
            } else if ["swapped", "committed"].contains(&point) {
                if mode == "pack" {
                    let mut zip = zip::ZipArchive::new(fs::File::open(&target).unwrap()).unwrap();
                    let mut bytes = Vec::new();
                    zip.by_name("result.txt")
                        .unwrap()
                        .read_to_end(&mut bytes)
                        .unwrap();
                    assert_eq!(bytes, b"new-data");
                } else {
                    assert_eq!(fs::read(&target).unwrap(), b"new-data");
                }
            } else {
                assert_eq!(fs::read(&target).unwrap(), b"old-data");
            }
        }
    }
}

#[test]
fn normal_cancel_removes_only_owned_unpublished_stage() {
    use crate::commands::fs::transfer::plan::discard_staging;
    let temp = Temp::new();
    fs::create_dir(temp.0.join("input")).unwrap();
    fs::create_dir(temp.0.join("output")).unwrap();
    fs::write(temp.0.join("input/result.txt"), b"source").unwrap();
    fs::write(temp.0.join("output/result.txt"), b"old").unwrap();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    journal
        .enqueue("job", intent(&temp.0, RecoveryKind::Move))
        .unwrap();
    let run = Run(journal.begin("job").unwrap());
    let mut plan = Plan::staged(&LocalProvider, temp.0.join("output/result.txt"), true);
    plan.track(&run.0, &temp.0.join("input/result.txt"), &LocalProvider)
        .unwrap();
    fs::write(&plan.write_to, b"partial").unwrap();
    let mut errors = Vec::new();
    discard_staging(&LocalProvider, &plan, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!plan.owned_root.unwrap().exists());
    assert_eq!(
        fs::read(temp.0.join("input/result.txt")).unwrap(),
        b"source"
    );
    assert_eq!(fs::read(temp.0.join("output/result.txt")).unwrap(), b"old");
}

#[test]
fn native_and_parallel_process_exit_never_publish_partial_content() {
    for mode in ["native", "parallel"] {
        let points: &[&str] = if mode == "native" {
            &[
                "staging",
                "writing",
                "prepared",
                "backup",
                "swapped",
                "before_commit_record",
                "committed",
                "source_removed",
            ]
        } else {
            &[
                "staging",
                "writing",
                "write",
                "flush",
                "prepared",
                "backup",
                "swapped",
                "before_commit_record",
                "committed",
            ]
        };
        for point in points {
            let temp = Temp::new();
            fs::create_dir(temp.0.join("input")).unwrap();
            fs::create_dir(temp.0.join("output")).unwrap();
            fs::write(temp.0.join("input/result.txt"), b"new-data").unwrap();
            fs::write(temp.0.join("output/result.txt"), b"old-data").unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "commands::fs::transfer::recovery::tests::crash_child",
                    "--nocapture",
                ])
                .env("HORTCMD_RECOVERY_TEST_CHILD", "1")
                .env("HORTCMD_RECOVERY_TEST_POINT", point)
                .env("HORTCMD_RECOVERY_TEST_ROOT", &temp.0)
                .env("HORTCMD_RECOVERY_TEST_MODE", mode)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::inherit())
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(86), "{mode}/{point}");
            let journal = Journal::open(&temp.0.join("journal")).unwrap();
            let record = journal.records().unwrap().remove(0);
            let published = [
                "swapped",
                "before_commit_record",
                "committed",
                "source_removed",
            ]
            .contains(point);
            if mode == "parallel" || !published {
                assert_eq!(
                    fs::read(temp.0.join("input/result.txt")).unwrap(),
                    b"new-data"
                );
            }
            if *point == "backup" {
                let RecoveryLocation::Local { path } = record.items[0].backup.as_ref().unwrap()
                else {
                    panic!()
                };
                assert_eq!(fs::read(path).unwrap(), b"old-data");
                assert!(reconcile::cleanup(&journal, &record).is_err());
            } else {
                assert_eq!(
                    fs::read(temp.0.join("output/result.txt")).unwrap(),
                    if published { b"new-data" } else { b"old-data" }
                );
            }
            assert_eq!(
                reconcile::entry(&journal, &record).state,
                reconcile::entry(&journal, &record).state
            );
        }
    }
}

#[test]
fn cancelled_durability_scan_stops_before_publication() {
    let temp = Temp::new();
    fs::write(temp.0.join("file"), b"content").unwrap();
    let error =
        durability::fingerprint_cancellable(&temp.0.join("file"), true, &AtomicBool::new(true))
            .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    assert_eq!(fs::read(temp.0.join("file")).unwrap(), b"content");
}

#[test]
fn missing_remote_profile_stays_unavailable_without_starting_a_connection() {
    let temp = Temp::new();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    let mut request = intent(&temp.0, RecoveryKind::Copy);
    request.sources = vec![RecoveryLocation::Remote {
        profile: format!("missing-{}", crate::net::unique_id()),
        identity: "unavailable-endpoint".into(),
        path: "/file".into(),
    }];
    journal.enqueue("job", request).unwrap();
    drop(Run(journal.begin("job").unwrap()));
    let record = journal.records().unwrap().remove(0);
    assert_eq!(
        reconcile::entry(&journal, &record).state,
        RecoveryState::Unavailable
    );
    assert!(reconcile::retry_intent(&journal, &record, false).is_err());
    assert!(record.items.is_empty());
}

#[test]
fn missing_source_cannot_be_retried_and_unresolved_records_are_not_evicted() {
    let temp = Temp::new();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    for index in 0..storage::MAX_RECORDS {
        journal
            .enqueue(&format!("job-{index}"), intent(&temp.0, RecoveryKind::Copy))
            .unwrap();
    }
    let record = journal.records().unwrap().remove(0);
    assert!(reconcile::retry_intent(&journal, &record, false).is_err());
    assert!(journal
        .enqueue("overflow", intent(&temp.0, RecoveryKind::Copy))
        .is_err());
    assert_eq!(journal.records().unwrap().len(), storage::MAX_RECORDS);
    drop(journal);
    assert_eq!(
        Journal::open(&temp.0.join("journal"))
            .unwrap()
            .records()
            .unwrap()
            .len(),
        storage::MAX_RECORDS
    );
}

#[test]
fn verified_completed_but_unreported_operation_can_be_acknowledged() {
    let temp = Temp::new();
    fs::create_dir(temp.0.join("input")).unwrap();
    fs::create_dir(temp.0.join("output")).unwrap();
    fs::write(temp.0.join("input/result.txt"), b"new-data").unwrap();
    let journal = Journal::open(&temp.0.join("journal")).unwrap();
    journal
        .enqueue("job", intent(&temp.0, RecoveryKind::Copy))
        .unwrap();
    let run = Run(journal.begin("job").unwrap());
    assert!(journal.acknowledge("job").is_err());
    let mut plan = Plan::staged(&LocalProvider, temp.0.join("output/result.txt"), false);
    plan.track(&run.0, &temp.0.join("input/result.txt"), &LocalProvider)
        .unwrap();
    copy::transfer_one(
        &LocalProvider,
        &LocalProvider,
        &temp.0.join("input/result.txt"),
        &plan,
        &mut progress(),
        CopyTuning {
            limit: 0,
            verify: false,
            buf_size: 4,
        },
        false,
        &mut Vec::new(),
    )
    .unwrap();
    drop(run);
    let record = journal.records().unwrap().remove(0);
    assert!(!record.terminal);
    assert_eq!(
        reconcile::entry(&journal, &record).state,
        RecoveryState::Completed
    );
    journal.acknowledge("job").unwrap();
    drop(plan);
    drop(journal);
    let reopened = Journal::open(&temp.0.join("journal")).unwrap();
    let record = reopened.records().unwrap().remove(0);
    assert!(record.terminal && record.acknowledged);
    assert_eq!(
        fs::read(temp.0.join("input/result.txt")).unwrap(),
        b"new-data"
    );
}
