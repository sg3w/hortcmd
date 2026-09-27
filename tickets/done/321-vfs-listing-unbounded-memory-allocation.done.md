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

## Resolution (2026-09-27)

A new shared `vfs::listing` module owns the one cap: `ListingBudget` counts
entries and name bytes and returns the new `fs.tooManyEntries` error as soon as
a listing passes `MAX_ENTRIES` (200 000) or `MAX_NAME_BYTES` (16 MiB), whichever
comes first. All four remote providers charge each entry to a budget *inside*
the loop that walks the protocol answer — SMB in `dir_entries`/`shares`, FTP in
`entries_of`, WebDAV in the new `entries_of` (extracted out of `list`, which now
just wraps it), SFTP in `list`/`read_dir` — so the error ends the enumeration
instead of arriving after the whole `Vec<DirEntry>` has been built. A caller can
ask for less: the budget never exceeds the global maximum but honours a smaller
limit.

The `visit_dir` contract is extended rather than duplicated per protocol. The
trait gained `read_dir_bounded(path, limit)`, and `visit_dir`'s default goes
through it, so the preflight limit is now passed down to the adapter instead of
being compared against an already materialized listing. Each remote provider
overrides `read_dir_bounded` with a budget built from `limit`; the default
implementation keeps the old after-the-fact check for providers that cannot stop
early, but its allocation is now bounded by `read_dir`'s own cap.

Known remaining upstream gap: russh-sftp's `read_dir`, suppaftp's
`get_lines_from_stream` and reqwest_dav's PROPFIND parse each materialize the
server's answer inside the dependency before our loop sees it, so for those
three the cap bounds the provider's own allocation, not the dependency's. Only
SMB's `query` iterator is abandoned mid-enumeration. Pushing the cap further
down needs a change in each vendored/upstream client and is out of this ticket's
scope.

Unit tests in `vfs::listing` assert the budget refuses the entry that crosses
the cap (not the one after it), that the global maximum overrides a larger
caller limit, and that name bytes bound a listing that stays under the entry
count. Oversized-listing fixtures per protocol remain for the harness tickets.

Validation: `npm run check` passed on macOS.
