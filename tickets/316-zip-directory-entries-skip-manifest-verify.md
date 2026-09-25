# TICKET-316: Verify ZIP directory entries through the manifest like tar/7z

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/commands/fs/archive/extract.rs`, ZIP directory entries never
go through `Manifest::verify`, while tar (`extract_tar`, ~line 290) and 7z
(`extract_7z`, ~line 353) directories do — an asymmetry in the TOCTOU defense
the manifest exists to provide.

For tar and 7z, the real extraction pass calls `manifest.verify(name, is_dir,
size, link)` for every entry, directories included, before acting — so an
archive mutated on disk between `preflight()` and the real pass is caught and
the whole operation is discarded. For ZIP, `extract_zip`'s Pass 1 (around
lines 158-175) creates matched directories via `dest.directory(&rel, ...)`
directly from a fresh `by_index_raw` read, and Pass 2 only iterates file
indices — no directory ever reaches `manifest.verify`.

If the archive file is modified concurrently (sits on a writable share, or a
racing process rewrites it) between preflight and the ZIP-specific reopen, an
injected/altered directory entry bypasses the manifest's collision/limit
bookkeeping entirely and gets published, whereas the equivalent tar/7z case
fails closed.

## Required change

- In `extract_zip`, call `manifest.verify(e.name(), true, 0, link)` before
  `dest.directory(...)` in Pass 1 (or move directory creation into a verified
  Pass 2, mirroring 7z), so all three formats get the same TOCTOU check.

## Verification

- Extend the archive test suite (`archive.rs` malicious-fixture tests) with a
  case that mutates a ZIP's directory entries between preflight and extract;
  assert extraction fails closed like the tar/7z equivalent.
- `npm run check`.
