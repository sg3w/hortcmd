# TICKET-340: Filter `.`/`..` entries from WebDAV directory listings

Discovered during a full-codebase review (2026-09-22).

## Problem

Unlike SFTP (`sftp.rs:164-165`), FTP (`ftp.rs:250`), and SMB
(`smb.rs:270-272`), which all explicitly skip entries named `.`/`..`,
`WebdavProvider::list` (`src-tauri/src/vfs/webdav.rs`, around lines 245-301)
only skips the href matching the requested collection itself (`norm(&href)
== self_key`) — a distinct `.`/`..` href from the server isn't filtered.

A misbehaving or malicious WebDAV server that includes a bogus `<D:href>`
entry decoding to `..` in a PROPFIND multistatus response gets it returned to
the frontend as a literal directory entry. The immediate blast radius is
limited (`ProviderPath::join` in `location.rs:166` rejects `..` components,
so navigating into it errors out) but it's an inconsistent invariant versus
the three sibling protocols and a needless surprise entry in the listing.

## Required change

- Add the same `name == "." || name == ".."` skip the other three providers
  already have.

## Verification

- Test: a WebDAV PROPFIND response containing a `.`/`..`-named href; assert
  it's filtered from the returned listing.
- `npm run test:harness` (WebDAV), `npm run check`.

## Resolution (2026-09-25)

WebDAV directory listings skip decoded names equal to dot or dot-dot. A loopback HTTP PROPFIND regression covers literal, encoded, and absolute dot hrefs while preserving hidden folders and once-decoded literal percent names. The protocol harness was run against fresh disposable fixtures before and after this change: both runs had 10 passes and the same 12 existing failures, with no new failing tests. All disposable servers were removed.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
