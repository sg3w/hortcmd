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

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
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

## Resolution (2026-09-27)

The advisories were refreshed first (cargo-deny 0.19.5, RustSec revision
`e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) and both still applied to the locked
`russh@0.53.0` / `russh-cryptovec@0.52.0`. Reading them changed the severity
picture: russh 0.53 predates 0.58.0, so besides the local ssh-agent frame path
it is still exposed on the paths the advisory calls historical — transport packet
reads and zlib decompression output — which are *remote* input, not just the
agent socket. Upstream fixed both in commit `a2d48a71` by capping agent frames
at 256 KiB and making every `CryptoVec` growth operation checked.

### Why this ticket also had to move smb

A patched russh could not be resolved next to the locked SMB stack. Every russh
release from 0.60.3 on requires the stable RustCrypto releases
(`curve25519-dalek ^5`, `p256 ^0.14`, `ecdsa ^0.17`, `hmac ^0.13`,
`crypto-common ^0.2.1`), while `smb 0.11.1` pins `sspi =0.18.x`, which pins the
exact pre-releases of the same crates (`crypto-common =0.2.0-rc.8`,
`p256 =0.14.0-rc.14`, …). Cargo unifies each of those into one version, so the
combination is unsatisfiable — every russh candidate from 0.60.3 to 0.63.3 fails
on one of `crypto-common`, `curve25519-dalek`, `ecdsa` or `p256`. The choice was
therefore between forking russh to backport unsafe allocation code, and moving
the SMB line forward. Moving forward is the smaller and more honest change:

- `vendor/smb` is re-vendored from published **smb 0.12.1** (SHA-256
  `b60b8c4d…c8`). The ticket-071 async read-reply backport is upstream from
  0.11.2 on and is now gone from the patch; only the ticket-096 `Session::echo`
  helper and the ticket-271 feature gate remain, plus one pin.
- That pin is `sspi = "=0.22.0"` in place of upstream's `=0.21.3`: 0.21.3 still
  pins `p256 =0.14.0-rc.14` and `curve25519-dalek =5.0.0-rc.1`, and 0.22.0 drops
  the direct pre-release pins. The `AuthIdentity` and `Sspi`/generator APIs smb
  uses are unchanged between the two, and the vendored crate compiles against
  0.22.0 without a source edit. `docs/smb-backport.md` and
  `vendor/smb/HORTCMD-PATCH.md` record the provenance and the removal condition.
- `russh` moves 0.53 → **0.63.3** (with `russh-cryptovec 0.62.0`), both past the
  recorded minimum 0.60.3. `russh-sftp` 2.4.0 needed no change.

### MSRV

smb 0.12.1 declares Rust 1.89, russh 0.63.3 declares 1.89 and sspi 0.22.0
declares 1.93, so the project floor is raised deliberately from 1.85.1 to
**1.93** in `src-tauri/Cargo.toml`, `README.md`, the `msrv` CI job and
`docs/dependency-governance.md`, with the 90 dependency-review checklists
updated to name it. The old floor existed only to keep smb 0.11.1 and the pinned
keepass release compiling (tickets 068/071); both constraints are what the
advisory backlog now has to move past, and `docs/smb-backport.md` explicitly
listed "the project deliberately raises its supported MSRV" as the condition for
dropping the backport. `cargo +1.93 check --locked --all-targets --all-features`
passes, so the declared floor is real rather than aspirational.

### Code changes

Two russh API changes reached hortcmd, both in `net/ssh.rs`:

- `client::Handler::check_server_key` now receives `PublicKeyOrCertificate`. The
  `PublicKey` arm keeps the existing trust-store check unchanged. A host
  *certificate* is refused with `security.hostKeyUnknown` and an explicit
  detail: verifying one means checking a CA, and hortcmd's trust stores hold host
  keys. russh 0.53 never handed this callback a certificate, so refusing keeps
  the guarantee the stores actually make instead of silently widening it.
- `AgentClient::request_identities` now yields `AgentIdentity`. Plain public keys
  are offered to `authenticate_publickey_with` exactly as before; certificate
  identities are skipped, which matches the pre-upgrade behavior, where the agent
  API only returned public keys.

### Dependency graph and policy

The lockfile shrinks from 795 to 766 components and from 133 to 104 duplicate
groups, because the RustCrypto pre-release halves collapse onto their stable
releases. `internal-russh-forked-ssh-key` disappears in favour of
`ssh-key 0.7.0-rc.11`, and with it `rsa@0.9.10` — the two RSA lines in ticket
373 become a single `rsa@0.10.0-rc.18`. `policy/dependencies.json` was updated
accordingly: new direct reviews with full checklists for russh 0.63.3 and
smb 0.12.1, the vendored smb source review re-fingerprinted, build-script and
native-source reviews re-pointed for `curve25519-dalek 5.0.0` and `nix 0.31.3`,
the nine security-critical duplicate groups re-fingerprinted, a new group for
`sha3`, and the dead `curve25519-dalek@4.1.3` review removed.

### Verification

The allocation bounds themselves live inside russh and russh-cryptovec, and the
advisory's own reproduction is upstream's; hortcmd adds no test that would only
restate `CryptoVec`'s internal arithmetic. What hortcmd owns is verified:

- The SFTP packet-bound regression from ticket 303 (`vfs::sftp_frames::tests`)
  still passes unchanged, including the partial-prefix and oversized-body cases.
- `npm run check:rust`: 679 library tests pass.
- Protocol harness (`npm run test:harness`, Docker): SFTP connect,
  authentication, listing, stat, mutation, staging, streaming and transfers pass;
  `harness_verifies_and_rotates_the_ssh_host_key` passes, so first contact,
  trust persistence, the silent reconnect and the refusal of a rotated key all
  still work across the new host-key callback; `harness_opens_an_ssh_shell`
  passes for the terminal channel; `harness_survives_a_server_dying_mid_copy`
  and the monitor/reconnect tests pass, so cancellation and reconnect are intact;
  SMB passes every conformance and transfer section against Samba on the new
  0.12.1 stack. Both providers pass every section they take part in except
  `harness_refuses_to_escape_the_root`, which fails for all seven services on
  the `main` baseline too (ticket 379). The run fails 12 of 22 tests, all 12 of
  them also failing on the baseline's 15 (tickets 378 and 379); it introduces
  none.
- `npm run audit:dependencies`: `RUSTSEC-2026-0154` and `RUSTSEC-2026-0153` are
  gone and no new advisory appeared; 12 findings remain, for tickets 371, 373,
  375, 376 and 377. The first run after the upgrade also failed the gate on a
  *yanked* crate rather than an advisory — `chacha20 0.10.1`, reached through
  `rand 0.10.2` from both russh and picky — so it was raised to the released
  0.10.2. A yank is not waivable as an advisory exception, and leaving it would
  have swapped one release blocker for another.
- Every `npm run check` stage passed on macOS (arm64): the offline supply-chain
  gate, `cargo fmt --check`, strict Clippy over all targets and features, the
  Rust tests, the ts-rs bindings (clean tree), error codes, boundaries, docs,
  UI text, `tsc --noEmit`, 797 Vitest tests, the production build and the bundle
  budget. They were run as individual `check:*` steps because the aggregated run
  was interrupted mid-doc-test; the commands and their order are the same ones
  `check` chains. `npm run supply:bundle` and `npm run supply:verify`
  regenerated and verified the release evidence.
- `cargo +1.93 check --locked --all-targets --all-features` passes, and the
  `msrv` CI job now pins 1.93.
