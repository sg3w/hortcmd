# TICKET-068: Restore the declared Rust minimum version

## Finding
A locked Rust 1.85.0 build fails before compiling project code. The existing
lockfile includes smb 0.11.2 (Rust 1.89), darling/serde_with/plist/home/time
releases requiring 1.88, ICU 2.2 and idna_adapter requiring 1.86. The exactly
pinned keepass 0.10.5 requires 1.85.1. This contradicts Cargo.toml's 1.85 promise
and the MSRV CI job. Found while checking the new 046 dependencies, whose
Windows helper and cap-std dependency do compile with 1.85.0.

## Acceptance
- Resolve and lock dependencies compatible with the intended MSRV, accounting for
  keepass's patch-level minimum, rather than silently raising the major/minor floor.
- Keep the manifest, documentation and CI toolchain in agreement.
- Check the entire application with the declared toolchain and --locked.
- Add resolver policy to avoid incompatible transitive upgrades.
- npm run check passes. Commit only this ticket; do not push.

Priority: fix before considering the CI/build guarantees complete.
