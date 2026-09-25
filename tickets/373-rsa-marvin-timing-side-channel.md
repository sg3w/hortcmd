# TICKET-373: Remediate or prove mitigation for both RSA Marvin findings

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `rsa` | `0.10.0-rc.9` | [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | vulnerability |
| `rsa` | `0.9.10` | [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **2 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

rsa 0.9.10 is reached through russh and internal-russh-forked-ssh-key. rsa 0.10.0-rc.9 is reached through picky and sspi. The recorded audit lists no safe upgrade for either line. Presence in the graph alone does not establish which vulnerable private-key operations are reachable.

## Required change

Trace private-key signing/decryption and attacker-observable timing through SSH, SMB/SSPI and authentication features on each platform. Prefer a corrected implementation or remove/disable affected code paths. Do not assume TLS use, trusted servers or host-key checking alone mitigates this advisory. If no fix exists, retain the release block unless a concrete, tested mitigation supports an exact version/advisory exception with owner, control, rationale and expiry.

- Follow [dependency governance](../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Document call-path/feature evidence for both versions and test the selected mitigation and supported authentication flows. Timing tests alone must not be presented as proof of constant-time behavior. Ensure an SSH upgrade in ticket 372 does not leave the SSPI RSA line untracked.

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
