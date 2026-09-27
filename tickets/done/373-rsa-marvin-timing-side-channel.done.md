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

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
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

## Resolution (2026-09-27)

RUSTSEC-2023-0071 has **no patched version** — the refreshed advisory (cargo-deny
0.19.5, RustSec revision `e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) still lists
none for any `rsa` release, including the 0.10 pre-releases. So this ticket could
only ever end in one of the three outcomes governance allows: remove the affected
code paths, keep the release block, or add an exception backed by a concrete
tested mitigation. What follows is the call-path evidence the ticket asks for, and
the disposition it leads to.

### Both recorded pairs are gone; one successor remains

| Recorded pair | State |
| --- | --- |
| `rsa@0.9.10` | **gone** — reached only through `russh 0.53` and `internal-russh-forked-ssh-key`, both removed by the SSH upgrade in ticket 372 |
| `rsa@0.10.0-rc.9` | **gone** — `picky`/`sspi` moved to 0.22.0 with the SMB upgrade in ticket 372 |

The graph now resolves a single `rsa 0.10.0-rc.18`, reached from `ssh-key 0.7.0-rc.11`
(itself from russh) and from `picky 7.0.0-rc.26` → `sspi 0.22.0` → the vendored
`smb`. That is a *new* advisory/version pair for the same advisory, so it is not
silently dropped: it is recorded as ticket 380 and still blocks release.

### Which private-key operations are actually reachable

**No RSA decryption happens anywhere in hortcmd.** That matters, because the
Marvin attack's strong form is a chosen-ciphertext timing oracle against PKCS#1
v1.5 *decryption*:

- SSH never uses RSA for key exchange or encryption — kex is
  curve25519/ECDH/DH and the transport keys are symmetric. russh calls no `rsa`
  decryption API.
- TLS (WebDAV, FTPS, the updater) does not use this crate at all: rustls is
  backed by aws-lc-rs/ring, whose RSA lives in C with its own constant-time
  claims.
- SMB is encrypted with AES-GCM/CCM (`EncryptionMode::Required` in
  `vfs/smb.rs`), with session keys derived from the NTLM exchange.

**SSH client public-key authentication with an RSA key is reachable**, and is the
one genuine private-key operation. `net/ssh.rs` loads the user's key with
`russh::keys::load_secret_key` and signs the authentication request; russh itself
uses the `rsa` crate only for PKCS#1/PKCS#8 key-format conversion
(`keys/format/`), but `ssh-key`'s `RsaKeypair` converts into
`rsa::pkcs1v15::SigningKey`, so the signature is produced by the affected code.
The same applies to an RSA key served by `ssh-agent`, except that there the
signing happens in the agent process, not here. Verifying the *server's* RSA host
key is a public-key operation and carries no secret.

**SMB/SSPI is not reachable.** `sspi`'s RSA use is confined to `pk_init.rs` and
`pku2u/` — PKINIT and PKU2U, i.e. smartcard and certificate-based Kerberos.
hortcmd's SMB path calls `share_connect`/`ipc_connect` with a user name and
password only (`vfs/smb.rs`), the vendored `smb` authenticator builds
`Negotiate` over `NtlmConfig` with `Credentials::AuthIdentity`
(`vendor/smb/src/session/authenticator.rs`), and password-based Kerberos uses
symmetric keys. No certificate or smartcard credential can enter from hortcmd's
UI or profile model, so no RSA private key is ever constructed on this path — on
any platform. Windows is not a special case: the same pure-Rust `sspi` is used
there, not the OS SSPI.

### Disposition

No fix exists, so the only way to remove the finding entirely would be to drop
russh's default `rsa` feature — which would refuse every `id_rsa` key and every
RSA-only server, a real loss of function for a file manager whose users do not
control their servers' host keys. That trade is not this ticket's to make
silently.

**No advisory exception is added and the release block stands.** Governance
allows a bounded exception only where an actual mitigation exists; a call-path
argument is not a mitigation, and this ticket explicitly forbids presenting
timing measurements as proof of constant-time behavior. What the analysis above
does establish is the *shape* of the remaining exposure, so the release decision
can be made on facts: no decryption oracle, one optional signing path, and an
attacker who would need the user to repeatedly authenticate an RSA key against a
server whose host key they have already pinned through the ticket-028 trust
prompt.

### Verification

- Call-path and feature evidence for both recorded versions and for the
  successor, above, read from the resolved sources rather than inferred from the
  dependency graph.
- Supported authentication flows still work, on the protocol harness: SSH
  password authentication, host-key first contact, trust persistence, silent
  reconnect and refusal of a rotated key (`harness_verifies_and_rotates_the_ssh_host_key`),
  the SSH shell channel, and SMB NTLM against Samba across listing, stat,
  mutation, staging, streaming and transfers. The ssh-agent and RSA-key paths are
  unchanged code, exercised by the existing unit tests around `net/ssh.rs`.
- `npm run audit:dependencies`: 2 findings — `rsa@0.10.0-rc.18` (ticket 380) and
  `proc-macro-error@1.0.4` (ticket 376). This ticket's own two pairs no longer
  appear. The gate stays red on purpose.
- `npm run check` passed on macOS (arm64).
