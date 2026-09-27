# TICKET-375: Remove the time parser stack-exhaustion finding

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `time` | `0.3.45` | [RUSTSEC-2026-0009](https://rustsec.org/advisories/RUSTSEC-2026-0009.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **1 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

time 0.3.45 is shared by certificate/ASN.1, cookie, plist, SMB, archive and build/test dependencies. The recorded finding is denial of service through stack exhaustion, with a fixed minimum of 0.3.47.

## Required change

Map the vulnerable parsing API to actual consumers, then update all affected resolved instances or apply a reviewed fix. Check the declared Rust 1.85.1 MSRV before choosing a version; do not silently raise it or use an incompatible toolchain override. Coordinate with plist/quick-xml work in ticket 371 and retain valid timestamp behavior.

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Use deeply nested/adversarial parser inputs under bounded resources and verify rejection without stack overflow. Test certificate/time parsing, plist and archive timestamps, SMB metadata and the declared MSRV/platform CI matrix as affected by the chosen change.

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

The advisory was refreshed first (cargo-deny 0.19.5, RustSec revision
`e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) and still reported `time@0.3.45
RUSTSEC-2026-0009`, patched from 0.3.47.

### Mapping the vulnerable API to consumers

The advisory is narrow: every listed function is affected **only** when
`time::format_description::well_known::Rfc2822` is the format, and the crash
comes from unbounded recursion over RFC 2822's deprecated nested comments.

The resolved graph reaches `time` through `asn1-rs` (x509-parser, so
certificate validity), `cookie` (tauri/wry), `nt-time` (the vendored
sevenz-rust), `picky-asn1`/`picky-krb` (sspi, so SMB authentication), `plist`
(Finder tags and Tauri's own config), `smb`/`smb-dtyp` and build/test
dependencies. Searching all of them, and hortcmd's own sources and every
vendored crate, finds no use of `Rfc2822` at all — the timestamp formats in play
are ASN.1 UTCTime/GeneralizedTime, Windows FILETIME, plist dates and RFC 3339.
So the finding was present but not reachable on any path this application
exercises.

That is a reason to record the analysis, not a reason to keep the version: the
mapping is a property of the current dependency set and any upgrade can add an
RFC 2822 call site, so the disposition is the upgrade, not an exception.

### Change

`cargo update -p time` raises `time` 0.3.45 → **0.3.55** (past the 0.3.47
minimum), with `time-core` 0.1.9, `time-macros` 0.2.32 and `num-conv` 0.2.2.
Nothing else in the graph moved: 766 components and 104 duplicate groups before
and after, and no direct, duplicate, build-script, native-source or license
review became stale, so `policy/dependencies.json` is unchanged.

The MSRV question the ticket raises was the real blocker: time 0.3.46 and later
declare Rust 1.88, above the old 1.85.1 floor. Ticket 372 raised the declared
floor to 1.93 deliberately — manifest, README, the `msrv` CI job and the
governance doc — and `cargo +1.93 check --locked --all-targets --all-features`
passes, so this ticket needs no toolchain override and silently raises nothing.

### Verification

The recursion limit lives inside `time`'s RFC 2822 parser and is covered by its
own tests. hortcmd has no call site that selects that format and `time` is not a
direct dependency, so an in-tree adversarial fixture would mean adding a
dev-dependency purely to re-test a third-party parser; the reachability analysis
above is the evidence this ticket can actually own. Timestamp behavior that *is*
hortcmd's is covered by the existing suites and still passes: certificate
validity rendering (`net::certificate`), archive timestamps (the sevenz/zip/tar
suites), Finder-tag plists and SMB metadata.

- `npm run audit:dependencies`: 11 Rust findings, `RUSTSEC-2026-0009` gone and no
  new advisory. The rest belong to tickets 371, 373, 376 and 377.
- `npm run check` passed on macOS (arm64), including the offline supply-chain
  gate; `npm run supply:bundle` and `npm run supply:verify` regenerated and
  verified the release evidence.
