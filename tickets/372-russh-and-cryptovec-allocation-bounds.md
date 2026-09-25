# TICKET-372: Fix SSH transport and CryptoVec allocation bounds

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `russh` | `0.53.0` | [RUSTSEC-2026-0154](https://rustsec.org/advisories/RUSTSEC-2026-0154.html) | vulnerability |
| `russh-cryptovec` | `0.52.0` | [RUSTSEC-2026-0153](https://rustsec.org/advisories/RUSTSEC-2026-0153.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **2 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

Direct russh 0.53.0 and its russh-cryptovec 0.52.0 dependency have separate allocation findings. Both SFTP and the SSH terminal use the SSH transport. The completed SFTP frame limit in ticket 303 does not establish that the lower SSH/CryptoVec allocation paths are safe.

## Required change

Upgrade the SSH stack to non-affected compatible versions (both recorded minima are 0.60.3), adapting russh-sftp and key APIs as needed. Check MSRV, host-key verification, credential handling and algorithm negotiation. Coordinate crypto changes with ticket 373; do not treat removal of one finding as resolving the other.

- Follow [dependency governance](../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Use malformed/oversized SSH length and CryptoVec growth fixtures, including 32-bit boundary cases without enormous real allocations. Run SFTP and SSH-terminal connection, authentication, host-key challenge, cancellation and reconnect tests. Preserve the existing SFTP packet-bound regression.

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
