// ============================================================
// Comparison benchmark for the local traversal (Ticket 013).
//
// Ignored by default – it builds a synthetic tree of ~50k files and is
// far too slow for the normal test run. Reproduce with:
//
//   cargo test --lib walk::bench -- --ignored --nocapture
//
// Measured per candidate: time to the first entry, total scan time,
// peak RSS of the process while the scan runs, and the number of
// entries (as a correctness cross-check – all candidates must agree).
//
// The results are recorded in `docs/traversal-benchmark.md`.
// ============================================================

use super::*;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const IGNORED: &[&str] = &["node_modules", ".git"];

/// Nesting levels of the deep-tree fixture (two path bytes each).
const DEEP_LEVELS: usize = 400;

struct Sample {
    label: String,
    first: Duration,
    total: Duration,
    peak_kb: u64,
    entries: u64,
}

fn rss_kb() -> u64 {
    Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
}

/// Growth of the process RSS in KB while `body` runs (sampled via `ps`;
/// 0 where `ps` is unavailable, e.g. on Windows). Absolute RSS is
/// useless here because the allocator never gives memory back – only
/// the growth caused by the walk is comparable.
/// The timer starts *after* the baseline sample, so the `ps` call does
/// not show up in the measured times.
fn with_peak_rss<T>(body: impl FnOnce(Instant) -> T) -> (T, u64, Duration) {
    let base = rss_kb();
    let done = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let (d, p) = (Arc::clone(&done), Arc::clone(&peak));
    let pid = std::process::id();
    let sampler = thread::spawn(move || {
        while !d.load(Ordering::Relaxed) {
            if let Ok(out) = Command::new("ps")
                .args(["-o", "rss=", "-p", &pid.to_string()])
                .output()
            {
                if let Ok(kb) = String::from_utf8_lossy(&out.stdout).trim().parse::<u64>() {
                    p.fetch_max(kb, Ordering::Relaxed);
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
    });
    let start = Instant::now();
    let value = body(start);
    let total = start.elapsed();
    done.store(true, Ordering::Relaxed);
    let _ = sampler.join();
    (
        value,
        peak.load(Ordering::Relaxed).saturating_sub(base),
        total,
    )
}

/// Builds `branch^depth` folders with `files` files each, plus one
/// `node_modules` subtree per level that must be pruned.
fn build_tree(root: &Path, depth: usize, branch: usize, files: usize) {
    fs::create_dir_all(root).unwrap();
    for i in 0..files {
        fs::write(root.join(format!("f{}.txt", i)), b"x").unwrap();
    }
    let nm = root.join("node_modules");
    fs::create_dir_all(&nm).unwrap();
    fs::write(nm.join("pkg.js"), b"x").unwrap();
    if depth == 0 {
        return;
    }
    for b in 0..branch {
        build_tree(&root.join(format!("d{}", b)), depth - 1, branch, files);
    }
}

/// The handwritten walker this ticket replaces: recursive, collects
/// everything before the first result can be produced.
fn legacy_walk(root: &Path, ignore: &[String], out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let Ok(m) = entry.metadata() else { continue };
        if m.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        if m.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if ignore.iter().any(|ig| ig == &name) {
                continue;
            }
            out.push(path.clone());
            legacy_walk(&path, ignore, out);
        } else {
            out.push(path);
        }
    }
}

fn run_legacy(root: &Path) -> Sample {
    let ignore: Vec<String> = IGNORED.iter().map(|s| s.to_string()).collect();
    let (entries, peak_kb, total) = with_peak_rss(|_| {
        let mut out = Vec::new();
        legacy_walk(root, &ignore, &mut out);
        out.len() as u64
    });
    Sample {
        label: "handwritten (collect-all)".into(),
        // Nothing can be emitted before the whole tree is collected.
        first: total,
        total,
        peak_kb,
        entries,
    }
}

fn run_walkdir(root: &Path) -> Sample {
    let ((entries, first), peak_kb, total) = with_peak_rss(|start| {
        let mut first = None;
        let mut n = 0u64;
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                !(e.file_type().is_symlink()
                    || (e.file_type().is_dir()
                        && IGNORED.contains(&e.file_name().to_string_lossy().as_ref())))
            });
        for item in walker {
            let Ok(entry) = item else { continue };
            let _ = entry.metadata().map(|m| m.len());
            if first.is_none() {
                first = Some(start.elapsed());
            }
            n += 1;
        }
        (n, first.unwrap_or_default())
    });
    Sample {
        label: "walkdir (single-threaded)".into(),
        first,
        total,
        peak_kb,
        entries,
    }
}

fn run_jwalk(root: &Path, threads: usize) -> Sample {
    let opts = WalkOptions {
        ignore_dirs: IGNORED.iter().map(|s| s.to_string()).collect(),
        want_files: true,
        want_dirs: true,
        detect_empty_dirs: false,
        threads,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let ((entries, first), peak_kb, total) = with_peak_rss(|start| {
        let mut first = None;
        let mut n = 0u64;
        walk_local(root, &opts, &cancel, |_entry| {
            if first.is_none() {
                first = Some(start.elapsed());
            }
            n += 1;
            Flow::Continue
        });
        (n, first.unwrap_or_default())
    });
    Sample {
        label: format!("jwalk ({} thread(s))", threads),
        first,
        total,
        peak_kb,
        entries,
    }
}

/// Fastest of `n` runs – the file system cache makes single runs noisy.
fn best_of(n: usize, mut run: impl FnMut() -> Sample) -> Sample {
    let mut best = run();
    for _ in 1..n {
        let next = run();
        if next.total < best.total {
            best = next;
        }
    }
    best
}

fn report(title: &str, samples: &[Sample]) {
    println!("\n### {}", title);
    println!("| walker | entries | time to 1st | total | RSS growth |\n|---|---:|---:|---:|---:|");
    for s in samples {
        println!(
            "| {} | {} | {:?} | {:?} | {} KB |",
            s.label, s.entries, s.first, s.total, s.peak_kb,
        );
    }
}

#[test]
#[ignore = "benchmark: builds a ~50k file tree, run explicitly"]
fn traversal_benchmark() {
    let base = std::env::temp_dir().join(format!("hortcmd-walkbench-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    // Wide/deep tree: 4^6 folders × 10 files ≈ 55k files.
    let wide = base.join("wide");
    build_tree(&wide, 6, 4, 10);
    // Deep tree: nested folders with one-character names, one file per
    // level – checks that no candidate grows the call stack. The depth is
    // capped by the platform's PATH_MAX (~1024 bytes on macOS/Linux).
    let deep = base.join("deep");
    let mut cur = deep.clone();
    for _ in 0..DEEP_LEVELS {
        cur = cur.join("d");
        fs::create_dir_all(&cur).unwrap();
        fs::write(cur.join("f.txt"), b"x").unwrap();
    }

    for (title, root) in [("wide tree", &wide), ("deep tree", &deep)] {
        // Warm the directory cache so the first candidate is not punished.
        let _ = run_walkdir(root);
        let samples = vec![
            best_of(3, || run_legacy(root)),
            best_of(3, || run_walkdir(root)),
            best_of(3, || run_jwalk(root, 1)),
            best_of(3, || run_jwalk(root, 2)),
            best_of(3, || run_jwalk(root, 4)),
        ];
        let expected = samples[0].entries;
        for s in &samples {
            assert_eq!(
                s.entries, expected,
                "{} disagrees on the entry count",
                s.label
            );
        }
        report(title, &samples);
    }

    let _ = fs::remove_dir_all(&base);
}
