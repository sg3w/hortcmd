# hortcmd compatibility patch (TICKET-096, TICKET-271, TICKET-372)

Base: published `smb` crate 0.12.1, MIT, upstream repository
https://github.com/afiffon/smb-rs, directory `crates/smb`.
Published `.crate` SHA-256: `b60b8c4d8566c3cf19d17837958e67acc0534e68f331e898a642f4f57e9ecc4c`.
Copied from the published archive; packaging markers and the crate-local
Cargo.lock are omitted. Sources otherwise retain upstream text.

The read-reply correction that TICKET-071 backported into 0.11.1
(`ReceiveOptions::new().with_allow_async(true)` in `vendor/smb/src/resource/file.rs`) is
upstream from 0.11.2 on and is therefore no longer a local change. Ticket 372
raised the project MSRV, which is what the old note asked for before dropping
the backport.

Three local changes remain:

## Authenticated session health check (TICKET-096)

`vendor/smb/src/session.rs` exposes `Session::echo`, a small wrapper around the
existing SMB2 ECHO message and normal session `send_recv` path. It retains
message-ID routing, response-command verification, signing/encryption and the
configured connection timeout. It neither creates a new session nor performs
file writes. The provider obtains an already attached share's session; if none
exists, it reports that an authenticated probe is unavailable. Replace this
addition with an equivalent upstream API when one exists.

## Unused-import gate (TICKET-271)

`vendor/smb/src/resource/file_util.rs` gates its `sync_helpers` import on `std-fs-impls`, the
only feature that uses it, so the crate compiles warning-free under the feature
set hortcmd selects.

## sspi pin (TICKET-372)

`Cargo.toml` and `Cargo.toml.orig` pin `sspi` to `=0.22.0` instead of upstream's
`=0.21.3`. Nothing else changes: the same `ring` feature, still with
`default-features = false`.

This is a dependency-resolution fix, not a protocol change. sspi 0.21.3 pins
exact RustCrypto pre-releases (`p256 =0.14.0-rc.14`, `curve25519-dalek
=5.0.0-rc.1`), and every russh release that carries the RUSTSEC-2026-0153/0154
fixes requires the stable `p256 ^0.14` and `curve25519-dalek ^5`. Cargo unifies
those into one version, so smb 0.12.1 as published and a patched russh cannot
coexist. sspi 0.22.0 drops the direct pre-release pins, which lets the whole
crypto graph resolve on stable releases and removes 29 duplicate groups.
`AuthIdentity` and the `Sspi`/`generator` APIs smb uses are unchanged between
0.21.3 and 0.22.0; the vendored crate compiles unmodified against it. Remove the
pin when upstream smb requires an sspi release with a compatible crypto graph.

Verify with `npm run check`, the declared-MSRV build and the real Docker protocol
harness. Do not edit the global Cargo cache to apply these changes.
