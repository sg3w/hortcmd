# Dependency and license governance

The maintainer owning a dependency change also owns its review and advisory
response. `policy/dependencies.json` is the checked-in baseline, including exact
versions, source/asset fingerprints, owners, rationale and review deadlines.
`npm run check:dependencies` is offline and part of `npm run check`. Run
`cargo fetch --manifest-path src-tauri/Cargo.toml --locked` first on a fresh host;
Python 3.11+ is required (CI pins 3.12.10). No package code is executed by the scanner.

## License policy

The same license rules apply to production, build, development, tests, optional and
platform-specific dependencies. Permissive SPDX licenses in `allowedLicenses` are
accepted; MPL-2.0 and CC-BY-4.0 require component/version-specific review. MPL code
must stay unmodified or its modified covered source must be provided; preserve its
notices and exact upstream source availability. CC-BY attribution must ship. GPL,
AGPL, SSPL, source-available restrictions and unknown licenses fail unless a valid
permissive alternative is selected in an OR expression. AND requires every term.
WITH exceptions are distinct license terms. Unknown identifiers always fail.
Reviews cannot turn a prohibited term into an allowed term.

The initial MPL and attribution reviews cover existing unchanged upstream code;
they do not authorize copying it into project source or silently modifying covered
files. Rust legacy slash expressions are treated as OR. All transitive licenses
are evaluated from Cargo metadata and npm's locked package metadata, not from
substring matches. SPDX reference texts are pinned under `policy/license-texts`.
Package-specific LICENSE, NOTICE, COPYING and copyright declarations are retained
in generated notices, including bundled native-library subdirectories. Published
packages omitting their license files contribute source copyright declarations,
manifest authors and the canonical license text; missing attribution must be fixed
upstream if discovered, not replaced with a fabricated copyright holder.

Repository icons/assets have exact hash reviews. Adding a font, icon, copied source
or native library requires recording origin, license, obligations and hash before
shipping. Vendored code is checked as a whole tree, with LF-normalized text hashes;
patches invalidate the source and build-script reviews. Platform-provided WebView,
OS frameworks and Linux shared libraries are listed separately in the manifest.
The release overlay requires an already installed Windows WebView2 runtime and does not embed its bootstrapper. Their API-wrapper license does **not** license the underlying runtime. Any newly
bundled native runtime needs its own inventory, notices and source obligations;
AppImage publication remains blocked pending that distribution inventory. The
release gate must not be bypassed by relabeling bundled libraries as external.

## Security and provenance

`npm run audit:dependencies` runs cargo-deny **0.19.5** and npm **11.16.0**. The
workflow installs those exact versions. Rust vulnerabilities, unsound/notice and
unmaintained advisories, plus yanked crates, block. npm high/critical findings block;
all severities are retained in the report. A registry failure or malformed/missing
report fails closed. RustSec revisions and tool versions are recorded in each
run's audit summary. The scheduled workflow refreshes advisories weekly, even when
no lockfile changes. Tests use a fixed synthetic revision and need no network.

The [cargo-deny advisory contract](https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html)
defines yank and advisory handling. [npm audit](https://docs.npmjs.com/cli/v11/commands/npm-audit/)
sends package names and versions to a registry. This repository explicitly enables
**public** npm audit against `https://registry.npmjs.org` in policy. RustSec is
downloaded and matched locally. No source, absolute workspace paths, file contents,
credentials or private scanner integrations are uploaded. Before adding private
packages, replace/disable that endpoint by an explicitly approved internal scanner;
the public audit gate must not be pointed at private data silently.

An exception is exact ecosystem/name/version/advisory ID, with owner, reason,
compensating control and expiry. Expired entries fail the offline gate. Unknown
Rust diagnostic errors and yanks cannot be waived as advisory IDs. A no-fix finding
requires a bounded exception **only if an actual mitigation exists**, otherwise
remove/disable the affected feature or block release. There are no baseline
advisory exceptions. A scanner detecting existing risk is working; do not make CI
green by suppressing a whole crate family or severity category.

The first live scan on 2026-09-25 found 18 Rust findings and no npm findings.
Existing Rust release blockers include h2, quick-xml (two versions/two advisories),
rsa (two versions), russh, russh-cryptovec, rustls, suppaftp and time, plus six
unmaintained-crate findings. The generated audit report carries IDs and versions.
The [remediation backlog](../tickets/README.md#rust-advisory-remediation-2026-09-25)
maps all 18 findings to tickets 369–377.
These need remediation/justified narrowly scoped mitigation before release; this
governance ticket does not certify the existing dependency graph vulnerability-free.

As of 2026-09-27 two findings remain and the gate holds exactly one advisory
exception: `cargo:rsa@0.10.0-rc.18:RUSTSEC-2023-0071`, expiring 2026-12-27
([ticket 380](../tickets/done/380-rsa-marvin-successor-version.done.md)). Its
compensating control is a code change, not an argument: SSH RSA signing was moved
to AWS-LC, so no reachable RSA private-key operation is left in the `rsa` crate,
which stays in the graph only for key parsing and public-key verification. The
second finding, `proc-macro-error@1.0.4`
([ticket 376](../tickets/376-replace-unmaintained-proc-macro-error.md)), has no
exception and still blocks release.

Unknown git/path sources, registries, missing checksums, npm install hooks and Rust
build scripts fail without exact reviews. CI uses `npm ci --ignore-scripts`; a
review never implicitly executes a script. Duplicate versions are reported in full;
TLS, crypto, URL, HTTP and serialization duplicates require an exact version-set
justification. Current parallel crypto lines remain for independently versioned
SSH/SMB/TLS consumers; new versions or source changes invalidate the reviews.

## Adding/updating direct dependencies

A direct review must name the exact resolved version and cover:

- upstream changelog/API changes and security history;
- project MSRV (Rust 1.93), Node engines and Windows/macOS/Linux support;
- feature/default-feature changes, optional dependencies, scripts and native code;
- license, source provenance, notice/source-distribution obligations;
- tests and all three platform CI builds before merging.

Dependabot proposes small weekly npm/Cargo changes (at most three open each).
Only testing/type patch updates are grouped; crypto, parsers, TLS, transports and
Tauri upgrades stay individually reviewable. No auto-merge is configured. Reviewers
regenerate SBOM/notices, inspect duplicate changes, run the full gate and evaluate
the security scan. Baseline direct reviews grandfather existing exact versions;
they are not blanket approval of future upgrades. All review deadlines are explicit.

## Release evidence

`npm run supply:bundle` emits CycloneDX 1.6 `sbom.cdx.json`, full notices, duplicate
and asset/external-runtime inventories, and a SHA-256 manifest in
`artifacts/supply-chain/`. Every Cargo.lock and package-lock component appears,
including target-specific and optional dependencies, source/checksum, directness,
features and dependency edges. Build/dev-only components use `scope: excluded`;
`runtimeCandidate` is the cross-target runtime closure, not a claim that every
component is linked into each executable. Lock hashes bind the evidence to the
build. No absolute local manifest paths are emitted.

`npm run supply:verify` recomputes the graph and verifies scopes, relationships,
lock hashes and artifact hashes. Generated evidence is reproducible and untracked.
The supported packaging command is `npm run release:bundle`: full checks, live
audit, evidence generation, then Tauri packaging with the release resource overlay,
then evidence verification. Both the SBOM and notices are embedded under
`supply-chain/` with the manifest and inventories. The dependency workflow retains
evidence even on audit failure. It does not publish or upload an installer.
Signing, native runtime distribution validation and publication remain subject to
the release workflow in ticket 111; failed advisory checks block packaging now.

After packaging, `npm run release:handoff` creates a new installer/evidence ZIP and
reads every member back to verify its hash against the input. It refuses empty
bundles, overwriting an existing handoff, and AppImages pending the native-runtime
license inventory. This preserves notices as sidecars even for users who inspect
an installer before installing. It is not a signature or publication step.

Bundled C code is checked separately from its Rust wrapper license. Versioned,
hashed native records cover libbzip2 1.0.8 (bzip2 terms), liblzma 5.2.5 (the explicit
public-domain grant in XZ COPYING), Zstandard 1.5.7 (BSD alternative) and optional
D-Bus 1.14.4 (AFL alternative). The SBOM embeds these as nested components of their
source crates. Their full native notices are collected recursively. Source changes
invalidate the native review; new crates containing C/assembly require a review.
GPL-only utilities/build files in source archives are not permission to ship those
utilities; the reviewed scope covers the linked libraries only.
