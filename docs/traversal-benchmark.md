# Local directory traversal: choosing the walker (Ticket 013)

The search used to walk the tree with a handwritten recursive
`fs::read_dir` function that collected *every* file and folder before the
first match could be produced. This document records the measurement that
replaced it.

## Candidates

| | |
|---|---|
| **handwritten** | the previous recursive walker, collect-all (baseline) |
| **walkdir 2** | single-threaded, iterative, `filter_entry` for pruning |
| **jwalk 0.9** | `walkdir`-shaped API, directory reads on a rayon pool, `process_read_dir` for pruning |

## Method

`cargo test --release --lib walk::bench -- --ignored --nocapture`
(`src-tauri/src/commands/fs/walk_bench.rs`). The benchmark builds two
fixtures in the temp directory and measures the fastest of three runs per
candidate after warming the directory cache:

* **wide tree** – `4^6` folders × 10 files ≈ 60 000 entries, with a
  `node_modules` folder per level that has to be pruned.
* **deep tree** – 400 nested folders, one file each (a chain that cannot
  be parallelised; capped by `PATH_MAX`).

Recorded per candidate: number of entries (cross-check – all candidates
must agree), time to the **first** entry, total scan time, and the growth
of the process RSS while the scan runs (absolute RSS is meaningless
because the allocator never returns memory).

Machine: Apple Silicon (macOS 25.6), APFS, release build.

## Results

### wide tree (60 070 entries)

| walker | time to 1st | total | RSS growth |
|---|---:|---:|---:|
| handwritten (collect-all) | 225.4 ms | 225.4 ms | 16 KB |
| walkdir (single-threaded) | 79 µs | 353.4 ms | 16 KB |
| jwalk (1 thread) | 263 µs | 279.1 ms | 272 KB |
| **jwalk (2 threads)** | **153 µs** | **168.4 ms** | **80 KB** |
| jwalk (4 threads) | 196 µs | 171.1 ms | 3 680 KB |

### deep tree (400 levels, 800 entries)

| walker | time to 1st | total | RSS growth |
|---|---:|---:|---:|
| handwritten (collect-all) | 22.5 ms | 22.5 ms | 16 KB |
| walkdir (single-threaded) | 100 µs | 27.1 ms | 16 KB |
| jwalk (1 thread) | 105 µs | 28.6 ms | 32 KB |
| jwalk (2 threads) | 167 µs | 38.4 ms | 64 KB |
| jwalk (4 threads) | 159 µs | 37.9 ms | 96 KB |

The RSS figures understate the real difference: the handwritten walker
holds all 60 070 paths plus their metadata in two `Vec`s before it can
emit anything, while every streaming candidate buffers only the
directories currently queued. The 16 KB it shows here is an artefact of
the small fixture – on a home directory that buffer is the whole tree.

## Decision: jwalk, two reader threads

* **Time to the first result** improves by three orders of magnitude
  (225 ms → 0.15 ms) simply by streaming; that is the dominant win and
  both libraries deliver it.
* On the **wide** tree – the realistic case for a file manager – jwalk
  with two threads is **2.1× faster than walkdir** and 1.3× faster than
  the old collect-all walker, while reading the same entries.
* **Four threads buy nothing** (168 ms vs 171 ms) but cost a rayon pool
  of ~3.7 MB, so the default is two; `WalkOptions::threads` is clamped to
  `1..=MAX_WALK_THREADS` (4).
* On a **pathological deep chain** jwalk pays ~10 ms of pool overhead for
  parallelism it cannot use. A 400-level-deep directory chain is not a
  real workload, and 38 ms for the whole scan is far below anything the
  user notices.
* `process_read_dir` prunes ignored folders *and* answers "is this folder
  empty?" from the read that already happened, which removes one extra
  `read_dir` per folder from the empty-folder mode.

`walkdir` stays in `[dev-dependencies]` so this comparison remains
reproducible; it is not used by the application.

## Limits enforced by the new implementation

| limit | value | reported as |
|---|---|---|
| matches per search | 50 000 (`MAX_HITS`) | `status: "truncated"` |
| matches per channel message | 200 (`BATCH`) | – |
| candidates buffered in duplicate mode | 300 000 (`MAX_DUP_CANDIDATES`) | `status: "limited"` |
| candidates buffered in large-file mode | 50 000, bounded heap | `status: "limited"` |
| bytes read per file for content search | 8 MB (`MAX_CONTENT_BYTES`) | file skipped |
| reader threads | 2, clamp 1–4 | – |

Error behaviour: unreadable, vanished and changing entries are counted in
`SearchSummary::unreadable` and skipped; a scan that finished but had to
skip something reports `status: "partial"`. A cancel token or a closed
result channel yields `status: "cancelled"`. Only `status: "complete"`
means the list is the full answer.

Symbolic links are never followed and never reported, which makes
`a → b → a` loops impossible. On Windows this covers junctions and mount
points as well, because `FileType::is_symlink` is true for
`IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT`; other reparse
points (cloud placeholders, for instance) stay ordinary entries.

Remote trees are enumerated through the VFS provider interface; this
walker only ever receives local paths.
