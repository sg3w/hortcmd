# TICKET-379: Repair the harness traversal and CA tests after path validation moved

Priority: **P2**. Status: **open**.
Discovered while verifying ticket 370 against the protocol harness (2026-09-27).

## Problem

Two `#[ignore]`d harness tests no longer test what they claim, and fail for
their own reasons on `main` — which hides real regressions behind a red suite.

1. `vfs::harness::conformance::harness_refuses_to_escape_the_root` builds the
   escape attempts and then calls
   `crate::vfs::StreamPath::remote(&…).unwrap()` (conformance.rs around line
   487). Since traversal became a construction-time rejection in the versioned
   locations (`vfs/location.rs`, `reason: "traversal"`), that `unwrap` panics
   before any provider is asked, for all seven services. The production
   behavior is correct — the path is refused — but the test reports a panic
   instead of asserting the refusal, so it would also pass unnoticed if the
   refusal ever moved back into the providers.

2. `vfs::harness::failure::harness_rejects_an_untrusted_certificate` derives the
   listing path with `h.service(name).root.trim_end_matches('/')`. The `ftps`,
   `ftpsi` and `webdavs` services declare `root: "/"` (harness.mjs), so the
   trimmed path is the empty string and `p.list("")` fails with
   `validation.badInput` from `vfs-location` before TLS trust is exercised at
   all. The test therefore never reaches its own assertion that the extra CA
   anchor is what makes those services reachable.

## Required change

- Assert the traversal refusal where it now happens: expect
  `StreamPath::remote` to reject a traversing path with
  `validation.badInput` / `reason: "traversal"`, and keep the
  provider-level assertion for the paths that are still constructible, so both
  halves of the guarantee stay covered.
- Use `"/"` rather than `""` when a service root trims to nothing, in the CA
  test and anywhere else `trim_end_matches('/')` is applied to a root
  (failure.rs lines 86, 178, 320 and 610).

## Verification

- `npm run test:harness`: both tests pass, and the CA test still fails when
  `HORTCMD_EXTRA_CA` is removed.
- `npm run check`.
