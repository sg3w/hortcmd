# TICKET-320: Add read/write timeouts to FTP and WebDAV connections

Discovered during a full-codebase review (2026-09-22).

## Problem

Neither the FTP control/data connections (`src-tauri/src/vfs/ftp.rs`,
`Params::open`, lines 56-151) nor the WebDAV `reqwest::Client`
(`src-tauri/src/vfs/webdav.rs`, `http_agent`, lines 32-46) ever get a
read/write/request timeout set, outside the transient one FTP's
`health_check` applies and restores.

`RustlsFtpStream::connect` uses plain `TcpStream::connect` (no
`connect_timeout`) and nothing later calls `set_read_timeout`/
`set_write_timeout` on the resulting stream; `http_agent()` builds
`reqwest::Client::builder()` with no `.timeout(...)`. Only the *initial*
`connect()` is externally time-bounded, via `net::session`'s
spawned-thread + `rx.recv_timeout(OVERALL_TIMEOUT)` pattern — every later
call (`list`, `stat`, `open_read`/`create_write`, `rename`, `remove_*`, and
the `Drop`-triggered cleanup on a cancelled FTP transfer) runs directly on
the calling thread with no bound at all. A server that accepts the
connection/request and then simply stops responding hangs that thread
forever. SMB, by contrast, explicitly threads `config.connection.timeout`
from the profile's monitor settings through `ClientConfig`
(`smb.rs:41-43`).

## Required change

- Set `.timeout(...)` on the WebDAV `reqwest::Client`.
- Set `set_read_timeout`/`set_write_timeout` on every FTP stream (control and
  each dedicated transfer connection).
- Source both from the same profile-driven timeout SMB already uses, so a
  stalled peer fails the operation instead of hanging the thread
  indefinitely.

## Verification

- Extend the protocol harness with a stalled-peer case per protocol (accept
  the connection, never respond) and assert the operation fails within the
  configured timeout rather than hanging.
- `npm run test:harness` (FTP, FTPS, WebDAV), `npm run check`.

## Resolution (2026-09-25)

FTP/FTPS now applies the profile timeout before control greetings/TLS, on passive data sockets, and through transfer finalization. WebDAV has connect and full-request deadlines. The pinned suppaftp patch adds only an implicit-TLS constructor accepting a configured socket; source review and license evidence are retained. Stalled socket tests and the live stalled-peer regression pass for FTP, explicit/implicit FTPS and HTTP/HTTPS WebDAV. The complete protocol suite ran: 10 passed, 12 failed. All 12 failures also reproduced at unchanged baseline c42ca3b (8 passed, 13 failed, including an additional reused-fixture host-key failure); they concern existing WebDAV behavior and obsolete path/security fixtures, not the timeout change. Native Windows/Linux verification is not claimed.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
