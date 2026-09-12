// ============================================================
// One contract, seven servers (TICKET-038).
//
// Every provider gets the *same* suite against the *same* seeded tree,
// because the promise the application makes is that a remote store
// behaves like a local one. Where a protocol genuinely cannot keep that
// promise, the difference is recorded here by name rather than papered
// over with a per-provider special case — a suite that quietly excuses
// each server in turn proves nothing.
// ============================================================

use super::{for_each_service, harness_or_skip, SeedEntry};
use crate::vfs::caps::{self, Capability, UNSUPPORTED};
use crate::vfs::Provider;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

/// Where a test may write. Seeded next to the fixture tree so a failure
/// never damages what the next assertion reads.
fn work_dir(root: &str, name: &str) -> String {
    format!("{}/work/{}", root.trim_end_matches('/'), name)
}

fn read_all(p: &dyn Provider, path: &str) -> Vec<u8> {
    let mut got = Vec::new();
    p.open_read(Path::new(path))
        .unwrap_or_else(|e| panic!("open_read {}: {}", path, e))
        .read_to_end(&mut got)
        .unwrap_or_else(|e| panic!("read {}: {}", path, e));
    got
}

fn write_all(p: &dyn Provider, path: &str, data: &[u8]) {
    let mut w = p
        .create_write(Path::new(path))
        .unwrap_or_else(|e| panic!("create_write {}: {}", path, e));
    w.write_all(data)
        .unwrap_or_else(|e| panic!("write {}: {}", path, e));
    // flush() is where a buffering remote writer actually uploads, so an
    // error here is a failed transfer and must not be swallowed.
    w.flush()
        .unwrap_or_else(|e| panic!("flush {}: {}", path, e));
}

/// The seeded bytes for a file, reproducing `harness.mjs`'s filler.
fn filler(size: usize) -> Vec<u8> {
    (0..size)
        .map(|i| ((i * 31 + (i >> 8) * 7) & 0xff) as u8)
        .collect()
}

// ---------- 1. Listing and stat against the manifest ----------

/// Every seeded entry is found, with the right kind and the right size.
///
/// This is the test that catches a provider inventing entries, losing
/// dotfiles, mangling a non-ASCII name, or reporting a size it did not
/// measure — none of which a unit test with a fake file system can see.
#[test]
#[ignore]
fn harness_listing_matches_the_seeded_tree() {
    let h = harness_or_skip!("harness_listing_matches_the_seeded_tree");
    for_each_service(&h, "listing", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        let root = svc.root.trim_end_matches('/');

        // What the harness says is there, by directory.
        let mut expected: BTreeMap<String, Vec<&SeedEntry>> = BTreeMap::new();
        for entry in &svc.seed {
            let parent = match entry.path.rfind('/') {
                Some(at) => entry.path[..at].to_string(),
                None => String::new(),
            };
            expected.entry(parent).or_default().push(entry);
        }

        for (dir, entries) in &expected {
            let path = if dir.is_empty() {
                root.to_string()
            } else {
                format!("{}/{}", root, dir)
            };
            let listing = p
                .list(&path)
                .unwrap_or_else(|e| panic!("list {}: {}", path, e));
            let found: BTreeMap<&str, &crate::commands::fs::dir::DirEntry> = listing
                .entries
                .iter()
                .map(|e| (e.name.as_str(), e))
                .collect();

            for want in entries {
                let base = want.path.rsplit('/').next().unwrap();
                let got = found.get(base).unwrap_or_else(|| {
                    panic!(
                        "{}: {} missing from {} — found {:?}",
                        name,
                        base,
                        path,
                        found.keys().collect::<Vec<_>>()
                    )
                });
                assert_eq!(got.is_dir, want.dir, "{}: kind of {}", name, want.path);
                if !want.dir {
                    assert_eq!(got.size, want.size, "{}: size of {}", name, want.path);
                }
            }
        }
        println!("   {} entries confirmed", svc.seed.len());
    });
}

/// `stat` agrees with the listing, and answers for a path the listing
/// never mentioned — the shape used before a transfer.
#[test]
#[ignore]
fn harness_stat_agrees_with_the_listing() {
    let h = harness_or_skip!("harness_stat_agrees_with_the_listing");
    for_each_service(&h, "stat", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        let root = svc.root.trim_end_matches('/');

        for entry in &svc.seed {
            let path = format!("{}/{}", root, entry.path);
            let st = p
                .stat(Path::new(&path))
                .unwrap_or_else(|e| panic!("{}: stat {}: {}", name, path, e));
            assert_eq!(st.is_dir, entry.dir, "{}: stat kind of {}", name, path);
            if !entry.dir {
                assert_eq!(st.len, entry.size, "{}: stat size of {}", name, path);
            }
            assert!(p.exists(Path::new(&path)), "{}: exists {}", name, path);
            assert_eq!(
                p.is_dir(Path::new(&path)),
                entry.dir,
                "{}: is_dir {}",
                name,
                path
            );
        }

        // A name that is not there must say so rather than error out in
        // a way the caller cannot distinguish from a broken connection.
        let missing = format!("{}/definitely-not-here-{}", root, h.id);
        assert!(!p.exists(Path::new(&missing)), "{}: phantom entry", name);
        assert!(
            p.stat(Path::new(&missing)).is_err(),
            "{}: stat of a missing path must fail",
            name
        );
    });
}

// ---------- 2. Streaming read and write ----------

/// A file larger than the copy buffer survives a round trip byte for
/// byte, in one read and in small chunks.
#[test]
#[ignore]
fn harness_streams_large_content_unchanged() {
    let h = harness_or_skip!("harness_streams_large_content_unchanged");
    for_each_service(&h, "streaming", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        let root = svc.root.trim_end_matches('/');

        // Read the seeded large file and compare with what was written.
        let large = format!("{}/tree/large.bin", root);
        let want = filler(
            svc.seed
                .iter()
                .find(|e| e.path == "tree/large.bin")
                .expect("the seed must contain large.bin")
                .size as usize,
        );
        let got = read_all(&*p, &large);
        assert_eq!(got.len(), want.len(), "{}: length of large.bin", name);
        assert!(got == want, "{}: large.bin came back different", name);

        // Write one back and read it again — the upload path, including
        // whatever buffering the provider does.
        let dir = work_dir(root, "stream");
        p.create_dir_all(Path::new(&dir)).expect("create work dir");
        let target = format!("{}/round-trip.bin", dir);
        write_all(&*p, &target, &want);
        assert_eq!(
            p.stat(Path::new(&target)).expect("stat upload").len,
            want.len() as u64,
            "{}: uploaded size",
            name
        );
        assert!(read_all(&*p, &target) == want, "{}: upload differs", name);

        // The same content in many small writes: a provider that only
        // works when handed one big buffer is broken for real copies.
        let chunked = format!("{}/chunked.bin", dir);
        {
            let mut w = p.create_write(Path::new(&chunked)).expect("create_write");
            for chunk in want.chunks(7919) {
                w.write_all(chunk).expect("chunked write");
            }
            w.flush().expect("chunked flush");
        }
        assert!(
            read_all(&*p, &chunked) == want,
            "{}: chunked upload differs",
            name
        );

        // An empty file is a file.
        let empty = format!("{}/empty.bin", dir);
        write_all(&*p, &empty, b"");
        assert_eq!(p.stat(Path::new(&empty)).expect("stat empty").len, 0);
        assert!(read_all(&*p, &empty).is_empty(), "{}: empty file", name);

        let _ = p.remove_dir_all(Path::new(&dir));
    });
}

// ---------- 3. Create, rename, delete ----------

#[test]
#[ignore]
fn harness_creates_renames_and_deletes() {
    let h = harness_or_skip!("harness_creates_renames_and_deletes");
    for_each_service(&h, "mutation", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        let root = svc.root.trim_end_matches('/');
        let dir = work_dir(root, "mutate");
        let _ = p.remove_dir_all(Path::new(&dir));

        // Nested creation in one call.
        let deep = format!("{}/a/b/c", dir);
        p.create_dir_all(Path::new(&deep))
            .unwrap_or_else(|e| panic!("{}: create_dir_all: {}", name, e));
        assert!(p.is_dir(Path::new(&deep)), "{}: nested dir", name);

        let file = format!("{}/report.txt", deep);
        write_all(&*p, &file, b"first");

        // Rename inside a directory...
        let renamed = format!("{}/final.txt", deep);
        p.rename(Path::new(&file), Path::new(&renamed))
            .unwrap_or_else(|e| panic!("{}: rename: {}", name, e));
        assert!(!p.exists(Path::new(&file)), "{}: old name survived", name);
        assert_eq!(
            read_all(&*p, &renamed),
            b"first",
            "{}: renamed content",
            name
        );

        // ...and across directories, which some servers implement
        // separately from a plain rename.
        let moved = format!("{}/a/moved.txt", dir);
        p.rename(Path::new(&renamed), Path::new(&moved))
            .unwrap_or_else(|e| panic!("{}: cross-directory rename: {}", name, e));
        assert_eq!(read_all(&*p, &moved), b"first", "{}: moved content", name);

        // The whole-file helper.
        p.write(Path::new(&file), b"second").expect("write helper");
        assert_eq!(p.stat(Path::new(&file)).expect("stat").len, 6);

        // Delete a file, then the tree.
        p.remove_file(Path::new(&moved))
            .unwrap_or_else(|e| panic!("{}: remove_file: {}", name, e));
        assert!(
            !p.exists(Path::new(&moved)),
            "{}: deleted file lingers",
            name
        );
        p.remove_dir_all(Path::new(&dir))
            .unwrap_or_else(|e| panic!("{}: remove_dir_all: {}", name, e));
        assert!(!p.exists(Path::new(&dir)), "{}: deleted tree lingers", name);
    });
}

/// The staging/commit contract failure-safe transfers rely on
/// (TICKET-029), which every provider implements with its own rename
/// semantics — the reason it is checked per server and not once.
#[test]
#[ignore]
fn harness_stages_and_commits() {
    use crate::commands::fs::staging_test_support::commit;

    let h = harness_or_skip!("harness_stages_and_commits");
    for_each_service(&h, "staging", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        assert!(
            p.supports_staging(),
            "{}: a provider used for safe transfers must declare staging",
            name
        );

        let root = svc.root.trim_end_matches('/');
        let dir = work_dir(root, "stage");
        let _ = p.remove_dir_all(Path::new(&dir));
        p.create_dir_all(Path::new(&dir)).expect("create work dir");

        let target = format!("{}/report.txt", dir);
        let first = format!("{}/.hortcmd-stage-a", dir);
        let second = format!("{}/.hortcmd-stage-b", dir);

        write_all(&*p, &first, b"first");
        commit(&*p, Path::new(&first), Path::new(&target), false).expect("commit new");
        assert_eq!(
            read_all(&*p, &target),
            b"first",
            "{}: committed bytes",
            name
        );
        assert!(!p.exists(Path::new(&first)), "{}: staging entry left", name);

        write_all(&*p, &second, b"second");
        commit(&*p, Path::new(&second), Path::new(&target), true).expect("commit replace");
        assert_eq!(read_all(&*p, &target), b"second", "{}: replacement", name);

        let left: Vec<String> = p
            .read_dir(Path::new(&dir))
            .expect("read_dir")
            .iter()
            .filter_map(|c| c.file_name()?.to_str().map(str::to_string))
            .filter(|n| n.starts_with(".hortcmd-"))
            .collect();
        assert!(
            left.is_empty(),
            "{}: leftover working entries {:?}",
            name,
            left
        );

        let _ = p.remove_dir_all(Path::new(&dir));
    });
}

// ---------- 4. Staying inside the root ----------

/// A `..` in a path must not walk out of the served tree.
///
/// Where the server confines the account — every service here except
/// SSH — that is a hard assertion: nothing outside the tree may be read
/// or written, whatever path is handed in.
///
/// Where it does not, hortcmd cannot invent a confinement. An SSH
/// account is a login account, and a client that silently rewrote the
/// user's path would be lying about what it did. So the assertion there
/// is the one hortcmd *does* owe: whatever the path does, it stays on
/// the addressed store, and the result is reported truthfully rather
/// than as a success.
#[test]
#[ignore]
fn harness_refuses_to_escape_the_root() {
    let h = harness_or_skip!("harness_refuses_to_escape_the_root");
    for_each_service(&h, "traversal", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        let root = svc.root.trim_end_matches('/');

        let attempts = [
            format!("{}/tree/../../etc/passwd", root),
            format!("{}/tree/../../../etc/passwd", root),
            format!("{}/../etc/passwd", root),
        ];

        for escape in &attempts {
            let read = p.open_read(Path::new(escape)).map(|mut r| {
                let mut buf = Vec::new();
                let _ = r.read_to_end(&mut buf);
                buf
            });
            let reached = match &read {
                Err(_) => false,
                Ok(bytes) => String::from_utf8_lossy(bytes).contains("root:"),
            };
            if svc.confined {
                assert!(!reached, "{}: {} escaped the served tree", name, escape);
            } else if reached {
                // Expected for an unconfined login, and worth printing:
                // a server that starts confining would change this line
                // rather than fail a test for doing the right thing.
                println!("   (not confined by the server: {} is readable)", escape);
            }
        }

        // Writing outside is the dangerous direction, and the entry must
        // never appear under the name that was asked for either way.
        let out = format!("{}/tree/../../escaped-{}.txt", root, h.id);
        if let Ok(mut w) = p.create_write(Path::new(&out)) {
            let _ = w.write_all(b"x");
            if w.flush().is_ok() && svc.confined {
                // A confined server may have resolved the path *inside*
                // the tree, which is fine — what must not happen is a
                // write landing outside it.
                assert!(
                    !p.exists(Path::new("/etc/escaped.txt")),
                    "{}: a write with .. left the served tree",
                    name
                );
            }
            let _ = p.remove_file(Path::new(&out));
        }

        // Whatever happened, it happened on this store and not another.
        assert_eq!(
            p.store_id(),
            h.connect(name).store_id(),
            "{}: the store identity must not depend on the path",
            name
        );
    });
}

// ---------- 5. Error mapping ----------

/// Failures come back as failures, of the right shape.
///
/// The concrete code differs by protocol — an FTP server says 550 for
/// much of what SFTP distinguishes — so what is asserted is the part the
/// application depends on: an error rather than a success, and a message
/// that names something.
#[test]
#[ignore]
fn harness_maps_failures_to_errors() {
    let h = harness_or_skip!("harness_maps_failures_to_errors");
    for_each_service(&h, "error mapping", |h, name| {
        let svc = h.service(name);
        let p = h.connect(name);
        let root = svc.root.trim_end_matches('/');
        let missing_dir = format!("{}/no-such-directory-{}", root, h.id);

        /// One thing that must fail, and the name to report it under.
        type Case<'a> = (&'a str, Box<dyn Fn() -> Result<(), String>>);

        let cases: Vec<Case> = vec![
            ("list a missing directory", {
                let p = p.clone();
                let path = missing_dir.clone();
                Box::new(move || p.list(&path).map(|_| ()).map_err(|e| e.to_string()))
            }),
            ("read a missing file", {
                let p = p.clone();
                let path = format!("{}/no-such-file.txt", missing_dir);
                Box::new(move || {
                    p.open_read(Path::new(&path))
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            }),
            ("rename something that is not there", {
                let p = p.clone();
                let from = format!("{}/no-such-file.txt", missing_dir);
                let to = format!("{}/still-not-there.txt", missing_dir);
                Box::new(move || {
                    p.rename(Path::new(&from), Path::new(&to))
                        .map_err(|e| e.to_string())
                })
            }),
            ("delete something that is not there", {
                let p = p.clone();
                let path = format!("{}/no-such-file.txt", missing_dir);
                Box::new(move || p.remove_file(Path::new(&path)).map_err(|e| e.to_string()))
            }),
            // Through the flush, not just the open: a buffering remote
            // writer (WebDAV PUTs on flush) has not spoken to the server
            // at all when `create_write` returns, so stopping there
            // would assert nothing.
            ("write into a directory that is not there", {
                let p = p.clone();
                let path = format!("{}/nope.txt", missing_dir);
                Box::new(move || {
                    let mut w = p
                        .create_write(Path::new(&path))
                        .map_err(|e| e.to_string())?;
                    w.write_all(b"x").map_err(|e| e.to_string())?;
                    w.flush().map_err(|e| e.to_string())
                })
            }),
        ];

        for (what, run) in cases {
            match run() {
                Ok(()) => panic!("{}: {} unexpectedly succeeded", name, what),
                Err(message) => {
                    assert!(
                        !message.trim().is_empty(),
                        "{}: {} failed with an empty message",
                        name,
                        what
                    );
                    println!("   {}: {}", what, message.lines().next().unwrap_or(""));
                }
            }
        }
    });
}

// ---------- 6. The capability contract, against live connections ----------

/// What the backend advertises for a live connection matches what the
/// suite above just did against the real server (TICKET-031).
#[test]
#[ignore]
fn harness_capabilities_match_behaviour() {
    let h = harness_or_skip!("harness_capabilities_match_behaviour");
    for_each_service(&h, "capabilities", |h, name| {
        let p = h.connect(name);
        let address = h.address(name);
        let store_id = p.store_id();
        crate::net::session::register_live(p);

        let caps = caps::for_address(&address).expect("capabilities of a live address");
        assert!(caps.connected, "{}: should be connected", name);
        assert_eq!(caps.store_id, store_id, "{}: cache identity", name);

        for cap in [
            Capability::Browse,
            Capability::Preview,
            Capability::CopyFrom,
            Capability::CopyTo,
            Capability::Move,
            Capability::Create,
            Capability::DeletePermanent,
        ] {
            assert!(caps.has(cap), "{}: must advertise {}", name, cap.id());
            caps::require(&address, cap).expect("an advertised capability must pass the gate");
        }

        // A shell only where the connection really carries one.
        assert_eq!(
            caps.has(Capability::SshTerminal),
            name == "sftp",
            "{}: ssh terminal",
            name
        );

        // Local-only command implementations stay refused, with the
        // typed marker the frontend keys off.
        for cap in [
            Capability::Trash,
            Capability::SystemOpen,
            Capability::OpenWith,
            Capability::QuickLook,
            Capability::LocalTerminal,
            Capability::VolumeInfo,
            Capability::GitStatus,
            Capability::FinderTags,
            Capability::Search,
            Capability::RecursiveSize,
            Capability::Checksums,
            Capability::Stat,
            Capability::Permissions,
            Capability::CompareDirs,
            Capability::CompareFiles,
            Capability::ArchiveBrowse,
            Capability::ArchiveExtractFrom,
            Capability::ArchivePackFrom,
        ] {
            assert!(!caps.has(cap), "{}: must not advertise {}", name, cap.id());
            let err = caps::require(&address, cap).expect_err("must be refused");
            assert!(
                err.is(UNSUPPORTED),
                "{}: untyped refusal for {}",
                name,
                cap.id()
            );
        }
    });
}
