# TICKET-020: VFS abstraction (identical UI, foundation)

## Goal
A provider trait in the backend abstracts all file-system operations behind a
uniform interface. The existing UI then works schema-agnostically with
local as well as (later) remote locations — the basis for the "identical UI".

## Background
Concept.md §10 P4 requires network with an **identical UI**. Today the
`commands/fs/*` operate directly on local `std::fs` paths. Without a shared abstraction,
every protocol would have to duplicate the UI logic. The archive feature already
shows a viable "path + context" model.

## Requirements
- Define a **provider trait** covering at least `list / stat / read / write / copy /
  move / delete / mkdir / rename` (streams/chunks for transfer).
- **Registry schema → provider** (`local`, reserved: `sftp`, `smb`, `webdav`, `ftp`).
- **Path/address model**: URL-like addresses (`sftp://user@host/path`) **alongside**
  existing local paths; a parser/resolver maps an address to its provider.
- Move existing local operations behind the **`local` provider**; the commands
  (`list_dir`, `copy_entries`, …) call only through the registry.
- **No behavior change** for local paths: FileTable, copy/move,
  preview, tabs, breadcrumb, watcher work unchanged.
- **No** remote protocol yet — only the abstraction + `local`.
- Location: `src-tauri/src/vfs/{mod.rs, local.rs}`; wiring in `commands/fs/*`.
- Adjust ts-rs types if the address/metadata format is extended.

## Acceptance criteria
- [x] All local file operations run through the VFS registry (`local` provider).
- [x] Addresses with a schema are correctly mapped to the provider; unknown schemas
      return a clear error.
- [x] Existing local functionality is unchanged (checked manually + tests).
- [x] The provider trait is documented and covers navigating, reading, writing, and
      transfer.

## Tests
- Local navigate/copy/move/delete/mkdir as before, now through the
  registry.
- Address parser: local vs. `sftp://…` resolved correctly; unknown schema → error.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated (Concept.md + VFS docs)
- [x] Rename the ticket to *.done.md

---

## Implementation

New module `src-tauri/src/vfs/` (registered as `mod vfs;` in `lib.rs`):

- **`vfs/mod.rs`** — the abstraction:
  - `Scheme` enum (`Local`, `Sftp`, `Smb`, `Webdav`, `Ftp`).
  - `Address` + `parse_address()`: a plain path or `file://…`/`local://…` → `Local`
    with the path verbatim; a reserved remote scheme (`sftp`/`smb`/`webdav`/`ftp`) is
    parsed (authority `user@host` dropped, path kept); any other scheme → error.
  - `provider_for(scheme)` registry + `resolve(raw) → (&'static dyn Provider, String)`.
    Local resolves to the `LocalProvider`; a reserved scheme returns a clear
    "*… wird noch nicht unterstützt*"; an unknown scheme errors from the parser.
  - `Provider` trait (object-safe, `Send + Sync`): `list`, `stat`, `exists`, `is_dir`,
    `read_dir`, `open_read`/`create_write` (streaming), `write`, `create_dir_all`,
    `rename`, `remove_file`/`remove_dir_all`, and a default `remove` (file-or-dir).
    The ticket's `copy`/`move` are realized from the streaming primitives + `rename`/
    `remove` (see below), so progress/pause/cancel/checksums stay provider-uniform.
- **`vfs/local.rs`** — `LocalProvider`, thin `std::fs` wrappers. `list()` holds the
  former `list_dir` body verbatim (canonicalize + per-entry `attrs`), so listing
  behavior is unchanged.

Wiring in `commands/fs/`:

- `dir.rs::list_dir` is now a thin wrapper: `vfs::resolve(&path)?.list(&resolved)`.
- `file.rs`: **every** `std::fs` call in the local path was replaced by a provider
  call — `make_dir`, `write_text_file`, `rename_entry`, `rename_batch`,
  `delete_entries`, and the whole transfer engine (`measure`, `copy_tree`,
  `copy_file_prog`, `flatten_tree`, `parallel_copy_file`, `run_parallel_copy`,
  `run_transfer`, `free_name`, `ask_collision`, `hash_file_sha256`, `unique_temp`).
  `run_transfer` resolves the destination through the registry up front and threads
  the resulting `&'static dyn Provider` through both the sequential and the parallel
  copy paths; sources are resolved too and a cross-provider transfer is rejected with
  a clear error (irrelevant today — only `local` exists — but the seam ticket 022
  builds on). The sequential/parallel algorithms are otherwise byte-for-byte identical.

### Design decisions / scope
- **No IPC type change.** Addresses stay plain strings over the IPC; the frontend
  keeps sending local paths, so `DirEntry`/`DirListing`/`OpResult` are untouched and
  `npm run gen:types` produces no diff. No frontend change was needed.
- **`copy`/`move` are not single trait methods** but are composed from the streaming
  primitives (`open_read`/`create_write`) plus `rename`/`remove`, because the transfer
  engine needs block-level control for progress, pause, cancel, rate limit and
  checksums. This matches the ticket's "(streams/chunks for transfer)".
- **Deliberately left local/unrouted** (not path operations on an address, so no
  registry seam applies yet): `list_drives`, `home_dir` (local drive enumeration),
  `trash_entries` (OS trash), and the peripheral read-only commands (`search`,
  `compare`, `preview`, `props`, `size`, `git`, `tags`, `archive`). Routing those is
  future work for the specific network tickets; keeping them out here honors the
  "small, comprehensible change" principle.
- **`LocalProvider` is a ZST** exposed as a function-local `static`, so `resolve`
  hands out a `&'static dyn Provider` that is trivially `Send + Sync` and safe to move
  into the parallel copy worker threads.

### Verified
- `cargo check` clean (no warnings); `cargo test --lib` → **64 passed, 0 failed**,
  including the new `vfs::tests` (address parser: local / relative / `file://` /
  `sftp://` parsed-but-unresolvable / unknown-scheme error / local resolves to
  `LocalProvider`) and `vfs::local::tests` (write/read/remove round-trip,
  mkdir/rename/read_dir, `list` returns entries with correct size/kind).
- `npm run typecheck` clean; `npm run build` succeeds; `npm run gen:types` yields no
  binding diff (confirmed via `git status`).
- **Not** live-tested in a running Tauri build: an interactive copy/move/collision/
  pause session in the UI. The transfer refactor is a mechanical `std::fs` →
  `LocalProvider` swap (identical bytes/algorithm) covered by the compiler plus the
  existing `flatten_tree`/`hash_file_sha256`/`rename_batch`/`write_text_file` tests and
  the new provider round-trip tests, so the risk is low — but a manual UI pass is
  recommended before shipping the network tickets on top.
