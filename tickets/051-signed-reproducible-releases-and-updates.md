# TICKET-051: Produce signed, reproducible releases with safe updates

## Goal

Deliver verifiable Windows, macOS, and Linux artifacts through a documented release
pipeline and provide an update process that authenticates metadata and preserves a
working installation when an update fails.

## Background

Tauri bundling is configured for all targets, but the repository has no release
workflow, code-signing/notarization policy, update metadata, provenance, rollback
procedure, or artifact-retention rules. Users of a filesystem and credential-aware
application need to distinguish official builds from modified packages. Automatic
updates would increase risk unless signatures, channels, compatibility, settings
migrations, and failure recovery are designed together.

TICKET-033 supplies build gates, TICKET-039 owns persisted-state migrations, and
TICKET-050 supplies dependency/SBOM evidence.

## Requirements

- Define supported installer/package formats and minimum OS versions for Windows,
  macOS, and selected Linux distributions.
- Build release artifacts from immutable tags with locked dependencies, pinned
  toolchains, clean source, and recorded build inputs.
- Add platform-appropriate signing: Authenticode for Windows, Developer ID signing
  plus notarization/stapling for macOS, and signed packages/checksums for Linux.
- Store signing credentials only in protected release environments with least
  privilege, audited access, rotation, and revocation procedures.
- Generate cryptographic checksums, signed update metadata, SBOM, third-party
  notices, and build provenance for every artifact.
- Establish stable, beta, and optional development channels with explicit channel
  switching and no silent downgrade.
- Verify update metadata and payload signatures before installation. Reject expired,
  replayed, wrong-channel, wrong-platform, and lower-version metadata unless the
  user explicitly performs a supported rollback.
- Download updates to a private staging location and preserve the current working
  installation until the replacement is fully verified and installable.
- Run persisted-state compatibility checks before update and retain a safe backup
  where migrations are not backward-compatible.
- Never update during active transfers, archive writes, or unresolved recovery;
  present defer/restart choices instead.
- Publish release notes with security-relevant changes and a documented emergency
  revocation/rollback process.

## Acceptance Criteria

- [ ] Official artifacts for every supported platform are signed and independently
      verifiable using documented commands.
- [ ] Release inputs, checksums, SBOM, notices, and provenance are published together.
- [ ] Tampered, replayed, expired, cross-channel, and downgrade update metadata is
      rejected before installation.
- [ ] Interrupted downloads or installation failures leave the previous application
      launchable.
- [ ] Updates wait for active file operations and recovery work to reach a safe state.
- [ ] Settings and operation-journal compatibility is checked before replacement.
- [ ] Signing-key rotation, compromise, revocation, and rollback are documented.

## Tests

- Verify signatures and notarization/package status on clean supported systems.
- Attempt update installation with modified payloads, wrong keys, expired metadata,
  replayed metadata, wrong platform/channel, and lower versions.
- Interrupt download and installation at deterministic points and verify recovery to
  the prior version.
- Upgrade from every supported persisted schema and test an intentionally
  incompatible downgrade.
- Start updates during local/remote transfers and archive extraction and verify safe
  deferral.
- Compare two controlled builds, document unavoidable non-determinism, and verify
  provenance links every artifact to its source revision.

## Definition of Done

- A protected release pipeline produces signed, attributable artifacts on all
  supported platforms.
- Update verification, staging, deferral, and rollback are implemented and tested.
- Release artifacts include checksums, SBOM, notices, provenance, and notes.
- Key-management and emergency procedures have named owners.
- Users can verify an official release without trusting the download channel alone.
- Rename this file to `051-signed-reproducible-releases-and-updates.done.md`.
