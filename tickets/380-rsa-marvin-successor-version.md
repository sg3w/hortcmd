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
[reachability analysis in ticket 373](done/373-rsa-marvin-timing-side-channel.done.md)
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
