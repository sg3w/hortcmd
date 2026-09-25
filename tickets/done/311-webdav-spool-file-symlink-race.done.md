# TICKET-311: Use create_new for the WebDAV upload spool file

Discovered during a full-codebase review (2026-09-22).

## Problem

`WebdavWriter::new` in `src-tauri/src/vfs/webdav.rs` (around line 500) opens
the upload spool file with `.create(true)`, not `.create_new(true)`. If
anything — including a pre-existing symlink — already sits at the computed
temp path (`hortcmd-dav-{pid}-{nanos}` in `std::env::temp_dir()`), the write
silently follows/truncates it instead of failing.

On a shared multi-user system, an attacker who can predict or brute-force that
filename and pre-plant a symlink before the victim starts a WebDAV upload gets
the upload content written through that symlink to an arbitrary path the app
process can write. `net/security.rs`'s `net_set_transport_policy` already uses
`OpenOptions::new().create_new(true)` specifically to avoid this class of bug
— `webdav.rs` doesn't follow the established pattern.

## Required change

- Open the spool file with `.create_new(true)`, retrying with a fresh
  (unpredictable) name on collision, matching `net/security.rs`'s pattern.

## Verification

- Test that pre-creating a file (or symlink, where the platform permits) at
  the computed spool path causes the writer to retry with a different name
  rather than writing through it.
- `npm run check`.

## Implementation

Spools use 128-bit random names, exclusive creation, bounded collision retries,
and owner-only Unix permissions. Uploads stream from the original open handle
instead of reopening a replaceable pathname. Tests cover file and symlink
collisions and retry exhaustion. This prevents pre-planted path redirection; it
does not isolate the application from another process running as the same user.
