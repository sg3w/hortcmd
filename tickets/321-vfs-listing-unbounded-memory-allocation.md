# TICKET-321: Bound remote directory-listing size while streaming, not after

Discovered during a full-codebase review (2026-09-22).

## Problem

`Provider::list`/`read_dir` (`src-tauri/src/vfs/mod.rs`, around lines
208-222) and all four remote implementations (`sftp.rs:153-232`,
`ftp.rs:221-277`, `smb.rs:254-290`, `webdav.rs:245-301`) fully materialize the
server's entire reported directory listing into a `Vec` before any size check
happens. The one existing safeguard (`visit_dir`'s default preflight
directory limit) calls `self.read_dir(path)?` first and only checks
`entries.len() > limit` *afterward*, so it never actually bounds the
allocation.

A malicious or compromised SFTP/FTP/SMB/WebDAV server answering an ordinary
directory browse with millions of fabricated entries (a single oversized
`READDIR`/`LIST`/`FileIdBothDirectoryInformation` enumeration/PROPFIND
multistatus) makes every protocol here collect the full `Vec<DirEntry>` in
memory with no cap before returning — a plain "open this folder" from the UI
can exhaust client memory. This is the same "allocate before validate" bug
class ticket 303 fixed for SFTP packet framing (a peer-controlled length
field consumed before being bounded), just at the directory-entry-count layer
instead of the packet-length layer, and it is not SFTP-specific.

## Required change

- Add an entry-count/byte-size cap enforced *while streaming* the listing
  (protocol adapters bail out as soon as the cap is exceeded) rather than
  after collection — this belongs in the shared VFS layer (e.g., extend the
  `visit_dir` contract or add a bounded variant of `list`) so all four
  providers get it uniformly instead of each protocol growing its own cap.

## Verification

- Extend the protocol harness with an oversized-listing fixture per protocol
  and assert the client bails out at the cap instead of allocating the full
  listing.
- `npm run test:harness`, `npm run check`.
