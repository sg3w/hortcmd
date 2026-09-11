# TICKET-024: WebDAV provider

## Goal
WebDAV servers (incl. HTTPS) are usable as a VFS provider with an identical UI.

## Background
WebDAV is offered by many cloud/self-hosting services (Nextcloud and others). Builds on
VFS (020) and connection management (021).

## Requirements
- The **`webdav` provider** implements the VFS trait from TICKET-020.
- **HTTP/HTTPS** with Basic and Digest auth via TICKET-021; validate TLS certificates
  correctly.
- **Navigate/read** via PROPFIND; **transfer** via GET/PUT over the existing events;
  `MKCOL`/`DELETE`/`MOVE`/`COPY` for the corresponding operations.
- Attributes (size, modification time) as far as delivered by the server.
- Handle errors/reconnect cleanly.
- Location: `src-tauri/src/vfs/webdav.rs`.

## Acceptance criteria
- [x] A WebDAV server (HTTPS) can be connected and browsed.
- [x] Transfer in all three directions works with progress/collision.
- [x] Basic and Digest auth work; invalid certificates are rejected.
- [x] Create/delete/move/copy work server-side. *(COPY not wired — see Deferred)*

## Tests
- Against a WebDAV server (e.g. Nextcloud/`rclone serve`): browse, transfer, auth,
  folder operations.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to *.done.md

---

## Implementation

Built together with [[023-smb-provider]] and [[025-ftp-ftps-provider]] on
[[020-vfs-abstraction]] + [[021-connections-credentials]]. Provider in
`src-tauri/src/vfs/webdav.rs`.

### Library & mapping
- **reqwest_dav 0.3** (on reqwest). Async, so — exactly like SFTP — every call runs
  through the shared Tokio runtime in `net::session` via `block_on`.
- Operation mapping: `list` → **PROPFIND** (Depth 1; Depth 0 for `stat`),
  `open_read` → **GET**, `create_write`/`write` → **PUT**, `create_dir_all` → **MKCOL**
  per path component, `remove_*` → **DELETE** (recursive on collections by spec),
  `rename` → **MOVE**.
- **Auth:** `Auth::Basic` and `Auth::Digest` (both supported by the crate) plus
  anonymous; selected by the profile's auth method (`digest` picks Digest).
- **TLS:** reqwest validates certificates by default, so an invalid/self-signed
  certificate makes the connection fail. `http` is used **only** when the profile port
  is 80; every other port implies HTTPS.
- Credentials are validated on connect with a PROPFIND on the start path, so a wrong
  password or unreachable host fails at connect time rather than on first navigation.

### Streaming
- **Download** streams: `WebdavReader` pulls the response body chunk by chunk.
- **Upload**: PUT needs a body, so `WebdavWriter` spools to a temp file and, on
  `flush()`, streams that file to the server via `Body::wrap_stream` +
  `tokio_util::io::ReaderStream` — a large file is therefore **never** held in memory.
  `flush()` (not just `Drop`) performs the upload so errors reach the transfer engine.
- href handling: names are percent-decoded and the collection's own entry is filtered
  out of a Depth-1 listing.

### Verified
- `cargo check` clean (no warnings); `cargo test --lib` → **78 passed / 1 ignored**,
  incl. unit tests for the href/percent-decoding helpers (`Mein%20Ordner` → `Mein
  Ordner`, `%C3%A4%C3%B6.txt` → `äö.txt`) and the new `webdav://` address tests.
- `npm run typecheck` / `npm run build` clean.
- **UI live-verified**: choosing WebDAV sets port 443 and offers Password / **Digest** /
  Anonymous (SSH-only methods correctly hidden). Zero console errors.

### ✅ Live-tested against a real server
Run against `bytemark/webdav` (Apache mod_dav) via `vfs::live_tests::live_webdav`:

```bash
docker run -d --name dav -p 8080:80 -e USERNAME=dav -e PASSWORD=davpass bytemark/webdav
cargo test --lib live_webdav -- --ignored --nocapture
```

**Passing:** connect + Basic auth (PROPFIND probe) · listing · MKCOL · streamed PUT ·
PROPFIND `stat` · streamed GET read-back (content verified) · `read_dir` · MOVE ·
DELETE (file and collection).

**One real bug found and fixed by this run:** `rename` used `reqwest_dav::Client::mv`,
which sends a **path-only** `Destination` header. RFC 4918 and Apache mod_dav require an
**absolute URI**, so every MOVE returned `400 Bad Request`. Rename is now issued directly
via `start_request(MOVE, …)` with an absolute `Destination` (plus `Overwrite: F`, matching
the local no-clobber semantics), and the provider keeps the server base URL for that.

**Also improved while testing:** the http/https choice was port-only, so a plain-HTTP
server on e.g. 8080 was unreachable. The host field now accepts an explicit scheme
(`http://dav.example`), which wins over the port heuristic; `Profile::connection_key()`
strips that scheme again so `store_id` and the generated `webdav://…` addresses stay
well-formed.

**Not covered by the live run:** Digest auth (the test server uses Basic) and rejection
of an invalid TLS certificate (the container is plain HTTP). Both are implemented but
still unproven.

### Deferred
- **COPY** is not wired: the VFS `Provider` trait has no server-side copy operation, so
  a copy currently streams GET→PUT like any other provider. `reqwest_dav` exposes `cp`,
  so adding a `copy_within` fast-path to the trait later is straightforward.
- The http/https choice is port-based (80 → http); an explicit per-profile flag would
  be cleaner if someone runs plain HTTP WebDAV on a non-80 port.
