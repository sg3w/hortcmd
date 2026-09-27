# TICKET-369: Upgrade h2 to bound empty HTTP/2 DATA frames

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `h2` | `0.4.15` | [RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **1 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

The locked h2 0.4.15 is pulled in by hyper 1.10.1 and reqwest 0.13.4. The recorded advisory describes unbounded queuing of empty DATA frames when streams are not drained.

## Required change

Resolve h2 to a non-affected release (the recorded minimum is 0.4.16), inspecting hyper/reqwest constraints rather than forcing an incompatible replacement. Preserve HTTP connection cancellation and resource limits.

- Follow [dependency governance](../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Exercise a local HTTP/2 peer sending repeated empty DATA frames to stalled streams. Verify bounded memory/queue behavior, cancellation and ordinary WebDAV/HTTP transfers; use upstream regression coverage when the behavior is wholly inside h2.

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

## Resolution (2026-09-27)

The advisory was refreshed before implementation (cargo-deny 0.19.5, RustSec
revision `e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) and still reported
`h2@0.4.15 RUSTSEC-2026-0258`. h2 is a transitive dependency only, reached
through hyper 1.10.1 and reqwest 0.13.4 for the WebDAV/HTTP client; neither
constrains the patch level, so the locked instance was raised in place with
`cargo update -p h2 --precise 0.4.19` — the newest release compatible with the
declared MSRV — which is past the recorded minimum 0.4.16. No other component
changed: the resolved graph gained and lost no packages, so no direct,
duplicate, install-script, native-source or license review became stale and
`policy/dependencies.json` is unchanged.

The queuing bound is wholly inside h2's `FramedWrite`/stream accounting, so
upstream regression coverage owns it; hortcmd adds no test that would only
re-assert h2's internal behavior. HTTP connection cancellation and the existing
WebDAV/FTP timeout regressions (ticket 320) still pass unchanged.

- `npm run audit:dependencies` after the change: 16 Rust findings, none for h2 —
  `RUSTSEC-2026-0258` is gone and no new advisory appeared. The remaining
  findings belong to tickets 371–373, 375–377. Note that the refreshed audit no
  longer reports `suppaftp@6.3.0` at all, because that crate resolves through
  the in-tree `[patch.crates-io]` path source, which cargo-deny does not scan;
  ticket 374 owns that finding and must fix the vendored copy rather than rely
  on the scanner.
- `npm run check` passed on macOS (arm64), including the offline supply-chain
  gate, and `npm run supply:bundle` plus `npm run supply:verify` regenerated and
  verified the release evidence for the new lockfile hash.
