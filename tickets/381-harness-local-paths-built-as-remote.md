# TICKET-381: Build the local side of a transfer as a local path

Priority: **P2**. Status: **open**.
Discovered while verifying ticket 379 against the protocol harness (2026-09-27).

## Problem

`vfs::harness::transfers::harness_copies_between_local_and_remote` fails for all
seven services with

```
copy /data/tree/large.bin → …/local/sftp-down-direct/large.bin: remote path at native boundary
```

`transfers.rs::transfer` (and `read_all` beside it) wraps *every* path in
`crate::vfs::StreamPath::remote(…)`, including the ones that belong to the
`LocalProvider` it is handed. `StreamPath::native()` refuses a `Remote` variant
by design (`stream_path.rs:22`, "remote path at native boundary"), so the local
half of a local↔remote copy cannot reach the file system at all.

This is a test defect of the same family as ticket 379: the harness asserts
something the production path does differently. The real transfer engine builds
a local destination as `StreamPath::Local`, so the failure is not evidence of a
product bug — but the red test hides the ones that would be.

`harness_copies_within_one_server` and `harness_copies_between_different_servers`
pass, because both of their sides really are remote.

## Required change

- Give `transfer` and `read_all` the path *kind* rather than assuming remote:
  pass a `StreamPath` built by the caller, or derive it from the provider the
  path belongs to, so a local path becomes `StreamPath::Local`.
- Keep both directions and both modes (direct and staged) covered, and keep the
  byte comparison against `seeded_large`.
- Check the rest of `harness/` for the same assumption — anywhere a local
  scratch path from `local_dir()` is wrapped as remote.

## Verification

- `npm run test:harness`: `harness_copies_between_local_and_remote` passes for
  all seven services, and the whole suite is green.
- `npm run check`.
