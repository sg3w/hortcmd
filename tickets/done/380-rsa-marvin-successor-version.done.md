# TICKET-380: Decide the release disposition for the surviving RSA Marvin finding

Priority: **P1**. Status: **open**.
Successor to ticket 373, recorded 2026-09-27.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `rsa` | `0.10.0-rc.18` | [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html) | vulnerability |

Ticket 373 removed both originally recorded pairs (`rsa@0.9.10` and
`rsa@0.10.0-rc.9`) by moving the SSH and SMB stacks forward, and traced the
remaining exposure. This pair is what is left, and it blocks release.

## Problem

RUSTSEC-2023-0071 still has no patched `rsa` release. The
[reachability analysis in ticket 373](373-rsa-marvin-timing-side-channel.done.md)
establishes that no RSA *decryption* path exists in hortcmd, and that the only
reachable private-key operation is PKCS#1 v1.5 *signing* during SSH public-key
authentication with an RSA key, through `ssh-key`'s `pkcs1v15::SigningKey`. The
`sspi`/`picky` occurrence is confined to PKINIT/PKU2U, which hortcmd's
password-only SMB path never enters.

No exception was added, because a call-path argument is not a mitigation.

## Required change

Pick one, with the reasoning recorded:

- wait for, or contribute to, a constant-time `rsa` release, and upgrade;
- route SSH RSA signing through a constant-time implementation (aws-lc-rs is
  already in the graph for russh) instead of the `rsa` crate;
- drop russh's default `rsa` feature, accepting that `id_rsa` keys and RSA-only
  servers stop working, and say so in the UI and the README;
- or decide, at the release level, that this exact version/advisory pair gets a
  bounded exception with owner, compensating control, rationale and expiry —
  which requires an actual control, not the analysis alone.

Do not close this by excluding the crate, the advisory or the severity class.

## Verification

- `npm run audit:dependencies` reports either no `rsa` finding or exactly one
  with a policy-valid exception.
- SSH authentication with RSA, Ed25519 and ECDSA keys, ssh-agent, and SMB NTLM
  all still work on the protocol harness.
- `npm run check`.

## Implementation

**Decision: option two — route SSH RSA signing through a constant-time
implementation — plus the bounded exception the remaining graph entry then
earns.**

Why not the others. Waiting for a constant-time `rsa` is not available: the
advisory still carries `patched = []` on purpose, and the pinned RustSec
revision (`e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`) says both 0.9.10 and
0.10.0-rc.18 are affected. Dropping russh's default `rsa` feature would take
more than `id_rsa` files with it: without that feature russh also drops the RSA
*host-key* algorithms (`keys/key.rs`), so every server whose only host key is
RSA becomes unreachable — a large functional loss for a client whose users do
not choose their servers' host keys. And an exception on the analysis alone is
what ticket 373 already refused, correctly.

### The control

`src-tauri/src/net/ssh.rs`. The `rsa` crate's reachable private-key operation
was exactly one: PKCS#1 v1.5 signing during SSH public-key authentication with
an RSA key. That signature now comes from AWS-LC, whose RSA is blinded, and
which was already in the locked graph as russh's default backend (`aws-lc-rs`
1.18.1 — the direct dependency adds no crate, no version and no native build).

- `RsaSigner` implements russh's `Signer` trait, the same interface ssh-agent
  authentication uses, so russh asks *us* for the signature instead of signing
  internally. The private key reaches AWS-LC as a PKCS#8 copy that is wiped when
  it goes out of scope (`KeyBytes`).
- `sign_rsa` returns the signed buffer with the signature appended as the SSH
  string pair (`u32` length, algorithm name, blob) — the same shape an agent
  returns, which is what russh splices into the userauth request.
- `RSA_HASHES` offers `rsa-sha2-512` first and `rsa-sha2-256` if the server
  refuses it. SHA-1 `ssh-rsa` is gone: AWS-LC does not sign with SHA-1, and
  OpenSSH has refused that algorithm by default since 8.8 (2021). This is the
  one behavioral change, recorded in README.md.
- Ed25519, ECDSA, ssh-agent and password authentication take their previous
  paths unchanged.

What is left inside `rsa` is parsing a key file and verifying a server's RSA
host key. Neither is a private-key operation, so neither can leak a private
key. The `sspi`/`picky` occurrence stays confined to PKINIT/PKU2U, which
hortcmd's password-only SMB path never enters (ticket 373).

### The exception

`policy/dependencies.json` gains exactly one entry — the exact pair
`cargo:rsa@0.10.0-rc.18:RUSTSEC-2023-0071`, owner, reason, control and an
expiry of 2026-12-27. The control is the code change above and the two tests
that hold it in place, not the reachability argument. No crate, advisory or
severity class is excluded, and `proc-macro-error@1.0.4` (ticket 376) still
blocks release. Recorded in
[dependency governance](../../docs/dependency-governance.md).

### Verification

- `npm run audit:dependencies`: 2 findings, 1 blocker. `rsa@0.10.0-rc.18` is
  reported with `excepted: true`; the remaining blocker is ticket 376's
  unmaintained `proc-macro-error`.
- New unit tests: `net::ssh::tests::rsa_signatures_verify_and_carry_the_negotiated_algorithm`
  signs with a generated RSA key and verifies the result through ssh-key's own
  verifier for both hashes — the signature an SSH server checks, byte for byte,
  including its encoding and algorithm name. `rsa_signing_refuses_an_algorithm_it_did_not_negotiate`
  covers the missing-hash case.
- The harness now provisions client keys: `harness.mjs` generates an RSA, an
  ECDSA and an Ed25519 key per run, seeds the SSH account's `authorized_keys`
  from them, and `vfs::harness::conformance::harness_authenticates_with_client_keys`
  authenticates with all three against real OpenSSH. Run on macOS (arm64): all
  three accepted, and the sshd log records `Accepted publickey … RSA` on the
  first attempt, so `rsa-sha2-512` was taken. 22 of 23 harness tests pass; the
  failure is ticket 381's, unrelated.
- ssh-agent, password authentication, the host-key first contact/rotation tests
  and SMB NTLM against Samba all still pass in the same run
  (`harness_verifies_and_rotates_the_ssh_host_key`, `harness_opens_an_ssh_shell`,
  and the full conformance and transfer suites for `smb`).
- `npm run check` passed on macOS (arm64).
