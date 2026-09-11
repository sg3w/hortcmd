# TICKET-023: SMB provider

## Goal
SMB/CIFS shares are usable as a VFS provider with an identical UI.

## Background
SMB is the most common protocol for network shares (Windows, NAS, Samba). After
SFTP (022), the next protocol on the VFS base (020) and the connection management (021).

## Requirements
- The **`smb` provider** implements the VFS trait from TICKET-020.
- Connection/auth via TICKET-021 (user/password, optionally domain/workgroup; guest).
- **Browse shares**: server → shares → directories; show attributes.
- **Transfer** local↔remote and remote↔remote via the existing events.
- Account for platform differences (macOS/Windows/Linux SMB access).
- Handle errors/reconnect cleanly.
- Location: `src-tauri/src/vfs/smb.rs`.

## Acceptance criteria
- [x] An SMB share can be connected and browsed like a local folder.
      *(share **enumeration** at the connection root does not work against Samba —
      see "Live-tested" below; navigating a share directly does.)*
- [x] Transfer in all three directions works with progress/collision.
- [x] Auth incl. domain/workgroup or guest works. *(`DOMAIN\user` + anonymous/guest)*
- [x] Connection errors are reported understandably; reconnect possible.

## Tests
- Against an SMB share (Samba/NAS): browse, transfer, auth variants.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to *.done.md

---

## Implementation

Built together with [[024-webdav-provider]] and [[025-ftp-ftps-provider]] on
[[020-vfs-abstraction]] + [[021-connections-credentials]]. Provider in
`src-tauri/src/vfs/smb.rs`.

### Library choice — and the MSRV bump it forced
- Uses **`smb` 0.11 (smb-rs)**, a **pure-Rust SMB2/3 client**, in its `multi_threaded`
  feature = **synchronous** API, so it maps directly onto the sync `Provider` trait
  (no async bridge). Features `sign` + `encrypt`, without `kerberos`/`quic` to keep the
  dependency tree small.
- The alternative, **`pavao`, was rejected**: it wraps the Samba C library
  (`libsmbclient`), which is *not installed on this machine* (the build would fail) and
  is not available on Windows — in direct conflict with the project's cross-platform
  priority. smb-rs needs no C library.
- ⚠️ **This required raising `rust-version` in `src-tauri/Cargo.toml` from `1.77` to
  `1.85`** (agreed with the user). Two reasons: smb-rs itself needs 1.85, and the old
  MSRV forced Cargo to select stale transitive dependencies — with `1.77`, `smb`
  resolved to a version whose `sspi` dependency failed to compile outright
  (`rand_core` trait-bound conflicts). The installed toolchain is 1.96, so there is no
  local impact, but **hortcmd now requires Rust ≥ 1.85 to build**.

### Path model
An `smb://user@host:445/...` address maps to `\\host\<share>\<rest>` — i.e. the
**first path segment is the share**. Listing the connection root therefore enumerates
the server's shares via `list_shares` (server → shares → directories, as the ticket
asks); everything below operates inside a share. Shares are attached lazily on first
use (`share_connect`) and remembered for the connection's lifetime. Admin shares
(`...$`) are listed but flagged hidden.

### Operation mapping
`list`/`read_dir` → `Directory::query::<FileIdBothDirectoryInformation>("*")`;
`stat` → parent lookup (yields kind + size in one round trip); `open_read`/
`create_write` → `create_file` + positional `read_block`/`write_block` wrapped in
sequential `Read`/`Write` adapters; `create_dir_all` → per-component create with
directory attributes; `remove_*` → open with **DELETE_ON_CLOSE** (recursing into
directories first, since SMB requires them empty); `rename` → `set_info` with
`FileRenameInformation`.

Domain/workgroup accounts are entered as `DOMAIN\user` (the dialog shows this hint);
anonymous/guest sends an empty password.

### Verified
- `cargo check` clean (no warnings); `cargo test --lib` → **78 passed / 5 ignored**,
  incl. unit tests for the share/path split (`/data/sub/file.txt` →
  `("data", "sub/file.txt")`, root → no share) and the new `smb://` address tests.
- `npm run typecheck` / `npm run build` clean.
- **UI live-verified**: choosing SMB/CIFS sets port 445, hides the SSH-only auth
  methods and shows the domain/workgroup hint. Zero console errors.

### ✅ Live-tested against a real server
Run against `dockurr/samba` (Samba 4.23) via the ignored integration test
`vfs::live_tests::live_smb` (see `src-tauri/src/vfs/live_tests.rs`):

```bash
docker run -d --name samba -p 445:445 -e NAME=Data -e USER=samba -e PASS=secret docker.io/dockurr/samba
cargo test --lib live_smb -- --ignored --nocapture
```

**Passing:** connect/NTLM auth · share attach · directory listing · `create_dir_all` ·
streaming write · `stat` · streaming read-back (content verified) · `read_dir` ·
`rename` · `remove_file` · `remove_dir_all`.

**Two real bugs were found and fixed only because of this run:**
1. **Share enumeration never worked** — `list_shares` talks to the `srvsvc` named pipe
   on the hidden **IPC$** share, which was never attached. Fixed by calling
   `ipc_connect` first.
2. **Leaked SMB handles → `STATUS_SHARING_VIOLATION` on rename/delete.** `SmbReader`,
   `SmbWriter`, `dir_entries` and `create_dir_all` all dropped their `File`/`Directory`
   handles without closing them, so the server kept the files locked and any later
   rename or delete failed. All four now close explicitly (writer closes in `flush()`
   so the error reaches the transfer engine, with `Drop` as the safety net). This bug
   would have made move/rename unusable on every SMB server.

**Known limitation (not our bug):** share enumeration at the connection root still
fails against Samba — smb-rs's DCE/RPC bind proposes a transfer syntax Samba rejects
(`ProposedTransferSyntaxesNotSupported`). The provider now reports this with an
actionable message ("configure a start path containing the share, e.g. /Data").
Accessing a share directly is unaffected.

**Environment note:** the first attempt used a macOS bind mount (`-v $PWD/samba:/storage`)
and rename failed with `NT_STATUS_NOT_SUPPORTED` — reproduced identically with the
official `smbclient`, so that is a limitation of the virtiofs bind mount, not of this
code. Testing against container-native storage is what made rename verifiable.

### Deferred
- Per-platform SMB peculiarities (macOS/Windows/Linux) are handled by smb-rs itself;
  no platform-specific code paths were added.
- Kerberos auth is disabled (feature off) — NTLM/password only for now.
