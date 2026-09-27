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

## Implementation

### The traversal test

`vfs::harness::conformance::harness_refuses_to_escape_the_root` now asserts both
halves of the guarantee.

A literal `.` or `..` component is asserted where the refusal happens:
`StreamPath::remote` must return `validation.badInput` with
`reason: "traversal"`. All three original escape attempts, the write attempt and
a `./` path go through that assertion, so the test fails — rather than silently
passing — if construction-time rejection ever moves back into the providers.

The provider-level assertion is kept for the paths that are still constructible:
an *encoded* traversal (`%2e%2e/%2e%2e`, and `..%2f..%2f` hiding the separator)
is a single path component as far as `ProviderPath::from_raw` is concerned, so it
reaches the provider. There the original assertion applies unchanged — nothing
outside the tree may be read or written on a confined server, and an unconfined
SSH login still only prints what it can reach.

### The roots

`Service` gained the two accessors the call sites were open-coding, so the two
uses of a root cannot be confused again:

- `Service::prefix()` — the root without its trailing slash, for building
  `{prefix}/tree/readme.txt`. A root of `"/"` correctly trims to nothing here.
- `Service::dir()` — the root as an address, for handing to a provider. `"/"`
  stays `"/"`, because the empty string is not a path and `vfs-location` rejects
  it with `validation.badInput` before a provider is reached.

`harness_rejects_an_untrusted_certificate` uses `dir()`, which is what made
`ftps`, `ftpsi` and `webdavs` — the three services with `root: "/"`, and exactly
the three TLS services — fail before TLS trust was exercised. Every other
`trim_end_matches('/')` on a root in `harness/` (`mod.rs`, `conformance.rs`,
`transfers.rs`, `mcp.rs`, `failure.rs`) now goes through `prefix()`.

### Verification

- `npm run test:harness` on macOS (arm64): both tests pass. 21 of 22 tests pass;
  the one that remains is `harness_copies_between_local_and_remote`, an
  unrelated third defect in the same family, recorded as
  [ticket 381](381-harness-local-paths-built-as-remote.md).
- The CA test still refuses to pass without the anchor: run against a kept
  harness with `HORTCMD_EXTRA_CA` unset, it fails at its own guard
  (`failure.rs:312`, "the harness must point HORTCMD_EXTRA_CA at the run's own
  CA") rather than reporting success.
- `npm run check` passed on macOS (arm64).
