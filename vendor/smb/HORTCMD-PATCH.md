# hortcmd patch: see docs/smb-backport.md

Source: crates.io smb 0.12.1, unchanged version, license and features except the
`sspi` pin recorded below. The three local changes are the authenticated SMB2
ECHO helper (ticket 096), the `std-fs-impls` gate on the `sync_helpers` import
in `vendor/smb/src/resource/file_util.rs` (ticket 271) and the `sspi = "=0.22.0"` pin needed to
resolve one crypto graph together with a patched russh (ticket 372).

The full provenance, rationale and removal conditions live in
`docs/smb-backport.md`.
