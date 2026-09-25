# TICKET-371: Remove both quick-xml denial-of-service vulnerabilities from both dependency lines

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `quick-xml` | `0.38.4` | [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html) | vulnerability |
| `quick-xml` | `0.38.4` | [RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html) | vulnerability |
| `quick-xml` | `0.39.4` | [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html) | vulnerability |
| `quick-xml` | `0.39.4` | [RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **4 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

quick-xml 0.38.4 arrives through plist 1.8.0; 0.39.4 arrives through keepass 0.10.5. Both versions trigger duplicate-attribute quadratic-time and namespace-allocation findings. This ticket owns all four advisory/version pairs.

## Required change

Update the plist and KeePass dependency paths to non-affected quick-xml (recorded minimum 0.41.0), or backport the fixes with reviewed provenance if compatible upstream upgrades are unavailable. Check the existing KeePass/hmac/SMB constraint before changing keepass. Preserve KDBX read/write compatibility and credential safety; file-size limits alone do not prove bounded parser work.

- Follow [dependency governance](../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Test many duplicate attributes and namespace declarations with time/memory bounds, malformed and valid plist/XML, and KeePass unlock/save/reopen with canary secrets excluded from diagnostics. Cover both resolved quick-xml lines until both are removed or fixed.

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
