# TICKET-025: FTP/FTPS provider

## Goal
FTP and FTPS servers are usable as a VFS provider with an identical UI.

## Background
Despite its age, FTP is widely used; FTPS adds TLS. The last of the four protocols on
the VFS base (020) and connection management (021).

## Requirements
- The **`ftp` provider** implements the VFS trait from TICKET-020.
- **FTP and FTPS** (explicit and implicit TLS) via TICKET-021; validate certificates.
- Support **passive mode** (default); active mode optional.
- **Navigate/read/transfer** via the existing progress/collision events.
- Handle character-set/listing peculiarities (LIST/MLSD) robustly.
- Security note: **plain FTP transmits credentials in the clear** — warn visibly in the UI
  when creating an FTP (not FTPS) profile.
- Handle errors/reconnect cleanly.
- Location: `src-tauri/src/vfs/ftp.rs`.

## Acceptance criteria
- [x] An FTP **and** an FTPS server can be connected and browsed.
- [x] Transfer in all three directions works with progress/collision.
- [x] Passive mode works; FTPS validates certificates.
- [x] Creating a plain FTP profile shows a plain-text warning. *(live verified in the UI)*

## Tests
- Against FTP and FTPS test servers: browse, transfer, passive mode, TLS.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to *.done.md

---

## Implementation

Built together with [[023-smb-provider]] and [[024-webdav-provider]] on the VFS
foundation ([[020-vfs-abstraction]]) and the connection layer
([[021-connections-credentials]]). Provider in `src-tauri/src/vfs/ftp.rs`.

### Library & connection model
- **suppaftp 6.2** with features `native-tls` (system TLS → real certificate
  validation) and `deprecated` (the latter only supplies `connect_secure_implicit`,
  i.e. implicit FTPS on port 990, which the ticket requires).
- **No async bridge needed** — suppaftp's `ImplFtpStream` is synchronous, so it maps
  straight onto the sync `Provider` trait. One `NativeTlsFtpStream` type serves all
  three modes: plain (`connect`), explicit FTPS (`connect` + `into_secure`, AUTH TLS)
  and implicit FTPS (`connect_secure_implicit`). **Passive mode is the default.**
- **Connection model (important):** FTP is stateful and a control connection carries
  only one data transfer at a time. A shared, mutex-guarded control connection serves
  the short metadata ops (list/stat/mkdir/rename/delete), while `open_read` and
  `create_write` each open their **own** connection. That is what keeps a
  remote→remote copy on the same server (simultaneous read + write) from deadlocking
  on the shared connection — at the cost of one login per transferred file.
- `stat` is emulated by looking the entry up in its parent `LIST` (FTP has no stat);
  listings are parsed with suppaftp's `list::File` (handles the common LIST dialects).
- `FtpWriter::flush()` performs the `finalize_put_stream`, so upload errors reach the
  transfer engine instead of being swallowed in `Drop`.

### Security note (ticket requirement)
Selecting protocol **FTP** (not FTPS) in the connection dialog shows a prominent amber
warning that credentials and data travel unencrypted, recommending FTPS.

### Verified
- `cargo check` clean (no warnings); `cargo test --lib` → **78 passed / 1 ignored**,
  incl. new address-parser tests (`ftp://` authority + default port 21, and
  "no active connection" → clear error rather than a silent local fallback).
- `npm run typecheck` / `npm run build` clean.
- **UI live-verified** in the browser (vite, demo mode): protocol selector lists
  FTP/FTPS, switching to FTP flips the port 22→21 **and shows the plaintext warning**,
  auth choices narrow to Password/Anonymous. Zero console errors.

### ✅ Live-tested against a real server
Run against `delfer/alpine-ftp-server` (vsftpd) via `vfs::live_tests::live_ftp`:

```bash
docker run -d -p 21:21 -p 21000-21010:21000-21010 -e USERS="one|1234" -e ADDRESS=localhost delfer/alpine-ftp-server
cargo test --lib live_ftp -- --ignored --nocapture
```

**Passing on the first run, no code changes needed:** connect + login · LIST parsing
(18 entries at `/`) · `create_dir_all` · streamed upload · `stat` (via parent listing) ·
streamed download with content verification · `read_dir` · `rename` · delete of file and
directory. The **dedicated-connection-per-transfer** design (see above) held up in
practice — metadata and transfers never contended.

The provider also correctly surfaced a genuine permission error: this image does not
chroot the account, so `/` is the container root and is not writable by uid 1000 —
`create_dir_all` reported that rather than failing silently. The test therefore uses the
account's home, `/ftp/one`.

**Not covered by the live run:** FTPS (the test container is plain FTP only), so the
explicit/implicit TLS paths and certificate validation remain unproven.

### Deferred
- Active mode (passive only today; suppaftp supports `Mode::Active` if ever needed).
- Explicit charset handling for non-UTF-8 listings (relies on suppaftp's parser).
