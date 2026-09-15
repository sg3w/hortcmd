// ============================================================
// The transfer matrix (TICKET-038).
//
// Four directions matter and they are not the same code path:
//
//   local  → remote    upload
//   remote → local     download
//   remote → remote    same server — read and write on one connection at
//                      once, which is where a shared control connection
//                      deadlocks if it was not thought about
//   remote → remote    *different* servers, the case with no shortcut
//
// Driven through the engine's own copy layer rather than the Tauri
// command: `run_transfer` needs an `AppHandle`, and the part worth
// testing here is what reaches the wire.
// ============================================================

use super::{for_each_service, harness_or_skip, Harness};
use crate::commands::fs::transfer::copy::{copy_tree, CopyTuning};
use crate::commands::fs::transfer::events::OpProgress;
use crate::commands::fs::transfer::plan::{plan_for, publish};
use crate::commands::fs::transfer::preflight::measure;
use crate::commands::fs::transfer::progress::{Prog, ProgressSink};
use crate::vfs::local::LocalProvider;
use crate::vfs::Provider;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Collects progress so a test can assert that it was reported at all —
/// a copy that works but never reports leaves the UI frozen at 0 %.
#[derive(Default)]
struct Recorder {
    reports: Mutex<Vec<OpProgress>>,
}

impl ProgressSink for Recorder {
    fn progress(&self, report: OpProgress) {
        self.reports.lock().unwrap().push(report);
    }
}

fn prog(sink: Arc<Recorder>, files: u32, bytes: u64) -> Prog {
    Prog {
        sink,
        id: "harness".into(),
        op: "copy".into(),
        files_total: files,
        bytes_total: bytes,
        files_done: 0,
        bytes_done: 0,
        last: Instant::now(),
        cancel: Arc::new(AtomicBool::new(false)),
        pause: Arc::new(AtomicBool::new(false)),
    }
}

fn read_all(p: &dyn Provider, path: &Path) -> Vec<u8> {
    let mut got = Vec::new();
    p.open_read(&crate::vfs::StreamPath::remote(&(path).to_string_lossy()).unwrap())
        .unwrap_or_else(|e| panic!("open_read {}: {}", path.display(), e))
        .read_to_end(&mut got)
        .expect("read_to_end");
    got
}

/// Copies one entry the way the engine does: measure, plan, copy,
/// publish. `safe` selects the staged path (TICKET-029).
fn transfer(
    srcp: &dyn Provider,
    src: &Path,
    dstp: &dyn Provider,
    dst: &Path,
    safe: bool,
) -> Arc<Recorder> {
    let (files, bytes) = measure(
        srcp,
        &crate::vfs::StreamPath::remote(&(src).to_string_lossy()).unwrap(),
    )
    .unwrap();
    let sink = Arc::new(Recorder::default());
    let mut p = prog(sink.clone(), files.max(1), bytes);
    let plan = plan_for(
        dstp,
        crate::vfs::StreamPath::remote(&(dst.to_path_buf()).to_string_lossy()).unwrap(),
        dstp.exists(&crate::vfs::StreamPath::remote(&(dst).to_string_lossy()).unwrap()),
        safe,
    )
    .unwrap();
    copy_tree(
        srcp,
        dstp,
        &crate::vfs::StreamPath::remote(&(src).to_string_lossy()).unwrap(),
        &plan.write_to,
        &mut p,
        CopyTuning {
            limit: 0,
            verify: true,
            buf_size: 256 * 1024,
        },
    )
    .unwrap_or_else(|e| panic!("copy {} → {}: {}", src.display(), dst.display(), e));
    publish(dstp, &plan).unwrap_or_else(|e| panic!("publish {}: {}", dst.display(), e));
    sink
}

/// A local scratch directory for this run, cleaned up by the caller.
fn local_dir(h: &Harness, what: &str) -> PathBuf {
    let dir = PathBuf::from(&h.dir).join("local").join(what);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create local scratch");
    dir
}

/// The seeded tree's own large file, as bytes, for comparison.
fn seeded_large(h: &Harness, name: &str) -> Vec<u8> {
    let svc = h.service(name);
    let size = svc
        .seed
        .iter()
        .find(|e| e.path == "tree/large.bin")
        .expect("large.bin")
        .size as usize;
    (0..size)
        .map(|i| ((i * 31 + (i >> 8) * 7) & 0xff) as u8)
        .collect()
}

// ---------- local ↔ remote ----------

/// Upload and download the seeded tree, in both modes, and compare.
#[test]
#[ignore]
fn harness_copies_between_local_and_remote() {
    let h = harness_or_skip!("harness_copies_between_local_and_remote");
    for_each_service(&h, "local ↔ remote", |h, name| {
        let remote = h.connect(name);
        let local: Arc<dyn Provider> = Arc::new(LocalProvider);
        let root = h.service(name).root.trim_end_matches('/').to_string();
        let want = seeded_large(h, name);

        for safe in [false, true] {
            let mode = if safe { "safe" } else { "direct" };

            // --- download: remote → local ---
            let down = local_dir(h, &format!("{}-down-{}", name, mode));
            let target = down.join("large.bin");
            let sink = transfer(
                &*remote,
                Path::new(&format!("{}/tree/large.bin", root)),
                &*local,
                &target,
                safe,
            );
            assert!(
                std::fs::read(&target).expect("downloaded file") == want,
                "{} ({}): download differs",
                name,
                mode
            );
            assert!(
                !sink.reports.lock().unwrap().is_empty(),
                "{} ({}): a copy that reports no progress freezes the UI",
                name,
                mode
            );

            // --- upload: local → remote ---
            let up = format!("{}/work/{}-up-{}", root, name, mode);
            let _ = remote.remove_dir_all(
                &crate::vfs::StreamPath::remote(&(Path::new(&up)).to_string_lossy()).unwrap(),
            );
            remote
                .create_dir_all(
                    &crate::vfs::StreamPath::remote(&(Path::new(&up)).to_string_lossy()).unwrap(),
                )
                .expect("create remote dir");
            let uploaded = format!("{}/large.bin", up);
            transfer(&*local, &target, &*remote, Path::new(&uploaded), safe);
            assert!(
                read_all(&*remote, Path::new(&uploaded)) == want,
                "{} ({}): upload differs",
                name,
                mode
            );

            // --- a whole directory, not just one file ---
            let tree_up = format!("{}/tree-copy", up);
            transfer(&*local, &down, &*remote, Path::new(&tree_up), safe);
            assert!(
                remote.exists(
                    &crate::vfs::StreamPath::remote(
                        &(Path::new(&format!("{}/large.bin", tree_up))).to_string_lossy()
                    )
                    .unwrap()
                ),
                "{} ({}): directory copy lost its content",
                name,
                mode
            );

            let _ = remote.remove_dir_all(
                &crate::vfs::StreamPath::remote(&(Path::new(&up)).to_string_lossy()).unwrap(),
            );
            let _ = std::fs::remove_dir_all(&down);
        }
    });
}

// ---------- remote → remote, same server ----------

/// A copy within one server reads and writes at the same time. For FTP
/// that means two connections at once, which is exactly the deadlock the
/// provider's design has to avoid.
#[test]
#[ignore]
fn harness_copies_within_one_server() {
    let h = harness_or_skip!("harness_copies_within_one_server");
    for_each_service(&h, "remote → same remote", |h, name| {
        let p = h.connect(name);
        let root = h.service(name).root.trim_end_matches('/').to_string();
        let want = seeded_large(h, name);

        let dir = format!("{}/work/{}-same", root, name);
        let _ = p.remove_dir_all(
            &crate::vfs::StreamPath::remote(&(Path::new(&dir)).to_string_lossy()).unwrap(),
        );
        p.create_dir_all(
            &crate::vfs::StreamPath::remote(&(Path::new(&dir)).to_string_lossy()).unwrap(),
        )
        .expect("create work dir");

        let dst = format!("{}/copy.bin", dir);
        transfer(
            &*p,
            Path::new(&format!("{}/tree/large.bin", root)),
            &*p,
            Path::new(&dst),
            false,
        );
        assert!(
            read_all(&*p, Path::new(&dst)) == want,
            "{}: same-server copy",
            name
        );

        // And the staged path, which adds a rename on the same server.
        let staged = format!("{}/staged.bin", dir);
        transfer(
            &*p,
            Path::new(&format!("{}/tree/large.bin", root)),
            &*p,
            Path::new(&staged),
            true,
        );
        assert!(
            read_all(&*p, Path::new(&staged)) == want,
            "{}: staged copy",
            name
        );

        let _ = p.remove_dir_all(
            &crate::vfs::StreamPath::remote(&(Path::new(&dir)).to_string_lossy()).unwrap(),
        );
    });
}

// ---------- remote → remote, different servers ----------

/// Every ordered pair of protocols, which is the case with no shortcut:
/// the bytes cross two different client implementations.
#[test]
#[ignore]
fn harness_copies_between_different_servers() {
    let h = harness_or_skip!("harness_copies_between_different_servers");

    // One representative per protocol — all seven pairs squared would
    // spend most of its time re-proving that FTPS is FTP with TLS.
    let pairs = [
        ("sftp", "webdav"),
        ("webdav", "smb"),
        ("smb", "ftp"),
        ("ftp", "sftp"),
        ("ftps", "webdavs"),
        ("ftpsi", "sftp"),
    ];

    let mut failed = Vec::new();
    for (from, to) in pairs {
        println!("\n── {} → {} ──", from, to);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let srcp = h.connect(from);
            let dstp = h.connect(to);
            let src_root = h.service(from).root.trim_end_matches('/').to_string();
            let dst_root = h.service(to).root.trim_end_matches('/').to_string();
            let want = seeded_large(&h, from);

            let dir = format!("{}/work/{}-from-{}", dst_root, to, from);
            let _ = dstp.remove_dir_all(
                &crate::vfs::StreamPath::remote(&(Path::new(&dir)).to_string_lossy()).unwrap(),
            );
            dstp.create_dir_all(
                &crate::vfs::StreamPath::remote(&(Path::new(&dir)).to_string_lossy()).unwrap(),
            )
            .expect("create work dir");

            let dst = format!("{}/large.bin", dir);
            transfer(
                &*srcp,
                Path::new(&format!("{}/tree/large.bin", src_root)),
                &*dstp,
                Path::new(&dst),
                true,
            );
            assert!(
                read_all(&*dstp, Path::new(&dst)) == want,
                "{} → {}: content differs",
                from,
                to
            );

            // A name with spaces and non-ASCII, which is where a path
            // that is quoted, escaped or URL-encoded once too often
            // shows up.
            let awkward = format!("{}/Ümlaut Grüße 100%.txt", dir);
            transfer(
                &*srcp,
                Path::new(&format!("{}/tree/Ümlaut-Grüße.txt", src_root)),
                &*dstp,
                Path::new(&awkward),
                false,
            );
            assert_eq!(
                read_all(&*dstp, Path::new(&awkward)),
                b"unicode\n",
                "{} → {}: awkward name",
                from,
                to
            );

            let _ = dstp.remove_dir_all(
                &crate::vfs::StreamPath::remote(&(Path::new(&dir)).to_string_lossy()).unwrap(),
            );
            println!("   {} → {} OK", from, to);
        }));
        if result.is_err() {
            failed.push(format!("{}→{}", from, to));
        }
    }
    assert!(
        failed.is_empty(),
        "cross-protocol copies failed: {:?}",
        failed
    );
}
