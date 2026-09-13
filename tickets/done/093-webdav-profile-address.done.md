# TICKET-093: Normalize explicit WebDAV schemes in navigation addresses

## Problem

A profile host such as `https://dav.example` is normalized for the connection key,
but `Profile::address()` embeds the original host. The resulting address contains
a second `://` and cannot be resolved as the connected WebDAV location. Discovered
while implementing TICKET-047, whose plaintext opt-in requires explicit HTTP.

## Requirements

- Build navigation addresses from the same normalized connection key as the registry.
- Preserve the transport choice in the profile; do not downgrade or change its host.
- Cover HTTP/HTTPS prefixes, trailing slash, custom port and start path.

## Acceptance Criteria

- [x] Explicit WebDAV schemes yield resolvable addresses matching the registry key.
- [x] Existing SFTP addresses and empty start paths retain their behavior.
- [x] `npm run check` passes.

## Definition of Done

Commit on its own branch, merge locally into main and rename with `.done.md`.

## Evidence

The new regression test reproduced the malformed address before the fix. The
complete `npm run check` passed after normalization (including existing profile
and SFTP tests). No transport settings or credentials are changed.
