# TICKET-376: Replace the unmaintained proc-macro-error dependency

Priority: **P2** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `proc-macro-error` | `1.0.4` | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) | unmaintained |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **1 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

proc-macro-error 1.0.4 arrives through glib-macros 0.18.5 and gtk3-macros 0.18.2. This is an unmaintained-crate finding, not evidence of a newly identified runtime exploit; the configured advisory gate nevertheless blocks release.

## Required change

Find a maintained upstream GTK/Tauri macro dependency path or a reviewed compatible replacement. The recorded audit lists no safe in-place upgrade. Do not remove Linux support, exclude its target from scans, or suppress all unmaintained advisories. If an upstream migration is unavailable, document maintenance ownership and a narrowly scoped temporary exception only where policy permits it.

- Follow [dependency governance](../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Compile and run relevant tests on Linux with real GTK/WebKit build prerequisites and check Windows/macOS for dependency-resolution regressions. Confirm proc-macro-error 1.0.4 is absent or the exact finding has a justified expiring disposition.

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
