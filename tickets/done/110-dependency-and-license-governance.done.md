# TICKET-110: Establish dependency, vulnerability, and license governance

## Backlog placement

Moved from 050 to 221 (now 110) as dependency and license governance. This is supporting
work in the support backlog; the requirements and open status are unchanged.

## Goal

Continuously identify vulnerable, abandoned, duplicated, unexpectedly permissive,
or license-incompatible Rust and npm dependencies before they reach a release.

## Background

The application combines a Tauri/WebView frontend with archive parsers, TLS and SSH
stacks, OS integration, protocol clients, and JavaScript content renderers. Lockfiles
exist, but the repository has no automated vulnerability, advisory, provenance,
duplicate-version, or license-policy checks. The roadmap already notes that RAR
support has licensing constraints. Security-sensitive parser and network libraries
make dependency review part of the product boundary rather than routine cleanup.

TICKET-033 owns general CI; this ticket adds supply-chain-specific policy and release
evidence.

## Requirements

- Document allowed, review-required, and prohibited licenses for production,
  development, test, and optional components.
- Generate a machine-readable software bill of materials for Rust and npm
  dependencies, including version, source, checksum/provenance, license, and direct/
  transitive relationship.
- Run Rust advisory/yank checks and npm vulnerability checks in CI using pinned tool
  versions and a documented severity policy.
- Add license scanning that handles dual licensing, bundled assets, fonts, icons,
  native libraries, and copied source—not only package metadata.
- Fail on git/path dependencies or install scripts unless explicitly reviewed and
  recorded with owner, reason, and expiration/review date.
- Report materially duplicated crate/package versions and require justification for
  security-critical duplicates such as TLS, crypto, URL, and serialization stacks.
- Use an update automation or documented scheduled process that opens small,
  reviewable dependency groups and never auto-merges security-sensitive upgrades.
- Require changelog, upstream security history, MSRV, platform support, and feature-
  flag review for new direct dependencies.
- Maintain narrowly scoped advisory exceptions with rationale, compensating control,
  owner, and expiry; expired exceptions fail CI.
- Include SBOM and required notices in release artifacts and verify their contents.
- Do not send private repository or filesystem information to third-party scanners
  without explicit configuration and documentation.

## Acceptance Criteria

- [ ] CI detects known Rust/npm advisories according to the documented severity
      threshold and expired exceptions.
- [ ] Every shipped dependency and asset has an identified compatible license.
- [ ] Release artifacts contain an accurate SBOM and third-party notices.
- [ ] Unreviewed git/path dependencies and package install scripts fail policy.
- [ ] Security-critical duplicate dependency versions are visible and justified.
- [ ] Update proposals are reproducible, scoped, and require human review.
- [ ] The policy explains how to handle an advisory with no upstream fix.

## Tests

- Inject fixtures representing a vulnerable package, yanked crate, prohibited or
  unknown license, expired exception, git dependency, and unexpected install script.
- Compare generated SBOM components against both lockfiles and packaged artifacts.
- Verify offline/reproducible scanner operation where supported and pin advisory
  database revisions for deterministic test fixtures.
- Build on Windows, macOS, and Linux after representative dependency updates.
- Verify optional and target-specific dependencies appear in policy output.
- Inspect a release bundle for notices, versions, and absence of development-only
  packages incorrectly marked as shipped.

## Definition of Done

- Dependency security and licensing policy is documented and enforced in CI.
- Releases include reproducible SBOM and notice artifacts.
- Exceptions are narrow, owned, justified, and automatically expire.
- Direct dependency additions have a review checklist.
- Scheduled update and advisory response processes have clear owners.
- Rename this file to `110-dependency-and-license-governance.done.md`.

## Implementation verification

The offline policy, fixture tests, SBOM/notices generation and verification, and
full `npm run check` pass. The separate live advisory gate correctly blocks on
18 existing Rust findings (npm: none); no blanket exceptions were added. See
[dependency governance](../../docs/dependency-governance.md) for remediation and
release restrictions. This completion records the governance implementation, not
a vulnerability-free certification of the existing dependency graph.
