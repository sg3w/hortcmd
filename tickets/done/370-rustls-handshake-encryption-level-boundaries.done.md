# TICKET-370: Upgrade rustls to enforce TLS handshake encryption levels

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `rustls` | `0.23.43` | [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **1 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

rustls 0.23.43 is shared by hyper-rustls, reqwest, rustls-platform-verifier, suppaftp and tokio-rustls. The finding concerns TLS 1.3 handshake messages accepted at incorrect encryption levels.

## Required change

Resolve every affected rustls instance to a non-affected release (recorded minimum 0.23.45). Preserve platform trust, custom certificate handling, hostname verification and explicit/implicit FTPS behavior.

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Verify rejected out-of-order/encryption-level handshake messages using upstream/local adversarial fixtures, plus valid TLS, untrusted/expired certificates, hostname mismatch, WebDAV and both FTPS modes.

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
`e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) and still reported
`rustls@0.23.43 RUSTSEC-2026-0285`. The graph resolves exactly one rustls
instance, shared by hyper-rustls, reqwest, rustls-platform-verifier, suppaftp
and tokio-rustls, so a single precise bump to 0.23.45 — the recorded minimum —
covers every consumer:

- `rustls` 0.23.43 → 0.23.45, `rustls-webpki` 0.103.13 → 0.103.15,
  `aws-lc-rs` 1.17.3 → 1.18.1, `aws-lc-sys` 0.43.0 → 0.45.0.
- No package was added or removed and no duplicate group changed; MSRV stays
  well below the project floor (rustls and aws-lc both declare 1.71) and the
  license expressions are unchanged, so no license review was affected.
- The build-script reviews for rustls, aws-lc-rs and aws-lc-sys and the native
  source review for aws-lc-sys were re-pointed at the new exact versions and
  checksums in `policy/dependencies.json`; stale entries for the old versions
  were removed rather than left to grandfather a version that no longer
  resolves.

The fix itself is inside rustls' handshake deframer: `HandshakeDeframer::
is_aligned` now reports "aligned" only when no handshake message is pending at
all, instead of when no *partial* fragment is pending, so a handshake message
packed into the same record across a key change is rejected. That boundary is
entirely internal to rustls and is covered by its own
`msgs/deframer`/`handshake_test`/`message_test` suites; hortcmd adds no test
that would only restate rustls' record-layer behavior.

Behavioral verification used the protocol harness (`npm run test:harness`,
Docker, all seven services). TLS remains functional: SFTP, FTP, explicit FTPS,
implicit FTPS and SMB pass listing, stat, mutation, staging, streaming,
transfers, the connect-timeout bound, host-key rotation and server-death
recovery, and WebDAV/WebDAVS still connect, authenticate and read over TLS.

The harness is, however, **already red on `main`** for reasons unrelated to this
ticket. A baseline run on `main` (`5e42f1`) fails 15 of 22 tests; this branch
(`deb684`) fails 12, and every one of those 12 also fails on the baseline — the
upgrade introduces no new failure and clears three flaky timing tests. The two
pre-existing root causes block exactly the WebDAV and certificate-refusal
coverage this ticket's criteria ask for, so they are recorded as new findings
(378 for the WebDAV collection redirect, 379 for the empty-root path in the
CA test) rather than silently absorbed here.

- `npm run audit:dependencies`: 15 Rust findings, `RUSTSEC-2026-0285` gone, no
  new advisory. Remaining findings belong to tickets 371–373, 375–377.
- `npm run check` passed on macOS (arm64); `npm run supply:bundle` and
  `npm run supply:verify` regenerated and verified the release evidence.
