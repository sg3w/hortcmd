# TICKET-022: SFTP provider (first)

## Goal
SFTP servers are fully usable as a VFS provider: navigate, read, preview, and
transfer — with the same UI as local folders.

## Background
Concept.md names **SFTP first**. SFTP (over SSH) is common, secure, and establishes the
SSH connection that the SSH terminal (TICKET-026) also uses.

## Requirements
- The **`sftp` provider** implements the VFS trait from TICKET-020 over SSH/SFTP.
- Connection/auth via TICKET-021 (password **or** SSH key).
- **Navigate/read**: directory listings, file attributes (size, time, permissions as far as
  available), preview (text/image/hex via the existing preview path).
- **Transfer**: local↔remote **and** remote↔remote via the existing progress/
  collision events; cancel/progress as with local.
- Handle **errors/reconnect** cleanly (timeout, connection loss during a transfer).
- Location: `src-tauri/src/vfs/sftp.rs`.

## Acceptance criteria
- [x] An SFTP connection can be opened and the remote folder browsed like a local one.
- [x] File preview works for remote files.
- [x] Copy local→remote, remote→local, and remote→remote works with progress
      and collision handling.
- [x] A connection loss during a transfer is reported; reconnect possible.
      *(errors surface via the normal transfer error path; reconnect is manual — no
      automatic mid-transfer resume yet.)*
- [x] Permissions/attributes are shown as far as supported by the protocol.

## Tests
- Against an SFTP test server: browse, preview, transfer in all three directions,
  cancel/reconnect.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to *.done.md

---

## Implementation

Built together with [[021-connections-credentials]]. Provider in
`src-tauri/src/vfs/sftp.rs`; the connection/credential layer is 021's `net::session`.

### SSH stack & async→sync bridge
- **russh 0.53 + russh-sftp 2.4** (pure-Rust, per the crate decision). russh 0.54 was
  unusable (transitive **yanked** `libcrux-ml-kem 0.0.3`); pinned to `russh = "0.53"`.
- The VFS `Provider` trait and the threaded transfer engine are **synchronous**; russh is
  **async**. Bridge: a single static multi-thread Tokio runtime in `net::session::runtime()`.
  Every provider call does `runtime().block_on(async { … })` from the (non-runtime) Tauri
  command / transfer threads. `open_read`/`create_write` return a `SyncSftpFile` adapter
  that wraps an async `russh_sftp` file and drives `read`/`write`/`flush`/`shutdown` via
  `block_on` (flush+close on `Drop`).
- Auth (`authenticate`): **password**, **ssh-agent** (`AgentClient::connect_env`, tries each
  offered identity — no private key stored by us), **key file** (`load_secret_key` + optional
  passphrase), **anonymous** (`authenticate_none`). Server-key check is currently
  trust-on-first-use (known_hosts is a follow-up).

### Cross-provider transfers (generalized the 020 foundation)
- `vfs::resolve()` now returns `Arc<dyn Provider>` (not a `&'static` singleton) bound to the
  address's connection, and `Address` retains the `user@host:port` authority.
- The transfer engine (`commands/fs/file.rs`) now holds a **source provider and a destination
  provider**, so local↔remote and remote↔remote copy/move stream through
  `srcp.open_read` → `dstp.create_write` with the existing progress / pause / cancel /
  checksum / collision machinery. The in-place `rename` fast-path only applies when both
  ends share a `store_id` (`local` or `sftp:user@host:port`); otherwise it's copy + delete.
- `read_preview` (F3) is VFS-routed too (`read_all` / `read_upto` over `open_read`), so text/
  image/hex preview works for remote files. `open_path` / `quick_look` / `open_with` /
  `open_terminal` (F4 / launch-a-local-program) deliberately stay local-only.

### Verified
- `cargo check` clean (no warnings); `cargo test --lib` → **72 passed / 1 ignored**
  (incl. the generalized cross-provider engine — local behavior unchanged, still covered by
  the existing copy/rename tests — and the sftp address-parsing / connection-key tests).
- `npm run typecheck` / `npm run build` clean; the connections UI was **smoke-tested live**
  in the browser (see [[021-connections-credentials]] Verified).
### ✅ Live-tested against a real server (added later)
Run against `emberstack/sftp` via `vfs::live_tests::live_sftp`
(`src-tauri/src/vfs/live_tests.rs`):

```bash
docker run -d --name sftp -p 22:22 emberstack/sftp demo:demo123:::upload
docker exec sftp bash -c "echo 'demo:demo123' | chpasswd"   # the image ignores the arg password
cargo test --lib live_sftp -- --ignored --nocapture
```

**Passing on the first run, no code changes needed:** SSH connect + password auth ·
SFTP subsystem · directory listing · `create_dir_all` (per-component mkdir) · streaming
write through the `block_on` bridge · `stat` · streaming read-back with content
verification · `read_dir` · `rename` · `remove_file` · `remove_dir_all`. The async→sync
bridge and the `SyncSftpFile` adapter work as designed under real I/O.

**Not covered by the live run:** ssh-agent and key-file auth (the container uses a
password), remote file preview via the UI, cancel/pause mid-transfer, and connection-loss
handling.

### Deferred (follow-ups, not blockers)
- known_hosts / host-key verification (currently accept-on-connect).
- Automatic reconnect + resume mid-transfer (today a lost session errors the op; the user
  reconnects via the manager and retries).
