# TICKET-374: Prevent FTP control-channel command injection

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `suppaftp` | `6.3.0` | [RUSTSEC-2026-0271](https://rustsec.org/advisories/RUSTSEC-2026-0271.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **1 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

The direct suppaftp 6.3.0 dependency triggers a CRLF command-injection finding for FTP control-channel arguments. Ordinary filenames and credentials can become protocol arguments; TLS does not neutralize command injection.

## Required change

Migrate to a non-affected compatible suppaftp release (recorded minimum 10.0.2), or implement a fully scoped reviewed fix if migration is blocked. Inventory every control-channel argument, including authentication, paths, rename and listing operations. Preserve encoding and explicit/implicit FTPS trust verification.

- Follow [dependency governance](../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Use a local scripted FTP server to verify that CR/LF-bearing arguments cannot produce a second command. Exercise affected argument classes, valid Unicode/spaces, FTP and both FTPS modes, rename/list/transfer, and cancellation. Never record canary credentials or raw authentication commands in logs.

- [ ] Every advisory/version pair in the table is removed or has a documented,
      policy-valid, narrowly scoped mitigation; no pair is silently dropped.
- [ ] Run `npm run audit:dependencies` and compare findings by crate, version and
      advisory ID. This ticket need not clear unrelated tickets, but must not
      introduce new unaddressed findings. All 18 require disposition before release.
- [ ] Pass `npm run check`; run affected protocol tests explicitly because they
      are ignored in ordinary CI, and obtain relevant Windows/macOS/Linux results.
- [ ] Regenerate and verify release evidence with `npm run supply:bundle` and
      `npm run supply:verify`; check licenses, notices, optional/target coverage
      and security-sensitive duplicate versions.
- [ ] Follow the per-ticket branch, local commit and merge workflow. Move this
      ticket to `done/` only after its remediation and verification are complete.
