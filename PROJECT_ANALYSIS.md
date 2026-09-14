# hortcmd Project Analysis

**Assessment date:** 2026-09-09
**Scope:** Current working tree, including the staged networking and VFS work

## Executive summary

hortcmd already has a substantial and coherent foundation for a cross-platform,
keyboard-oriented file manager. The separation between the React presentation
layer and the Rust backend is clear, IPC types are generated, the local and
remote transfer paths share a VFS abstraction, and the Rust backend has useful
unit coverage. TypeScript checking, the production frontend build, and the Rust
library tests all pass in the assessed state.

The most important remaining risks are concentrated in security and data safety.
SSH currently accepts every server key, which leaves SFTP and SSH terminal
sessions open to machine-in-the-middle attacks. Copy operations write directly
to the final destination and remove an existing target before the replacement is
known to be complete, so an interrupted or failed overwrite can destroy the old
file. The Tauri WebView also has no Content Security Policy. Archive extraction
joins archive-controlled relative names to the destination without one explicit,
format-independent containment check, creating another high-priority input-safety
gap. These findings should be addressed before expanding remote access or exposing
the VFS through the planned plugin and MCP interfaces.

The networking implementation also exposes a product-consistency problem. Core
browsing, preview, and transfer operations use the VFS, while several actions
that remain local-only can still be offered for remote paths. An explicit
capability model is needed so the UI can accurately represent what each provider
supports. Connection attempts additionally need bounded, cancellable lifecycle
management. Finally, automated quality gates and frontend tests are needed to
keep the growing codebase stable.

A second-wave roadmap records ten lower-risk but compounding concerns: startup
bundle size, oversized modules, unstructured errors, documentation drift, live
protocol coverage, unversioned persisted state, provider-unaware refresh behavior,
accessibility and translation consistency, restart-lost transfer state, and the
absence of privacy-preserving diagnostics. These follow-ups should be scheduled
after or alongside the six risk-reduction tickets according to their dependencies.

A third-wave set adds ten product-hardening tickets for hostile archives, transport
security, reversible operations, transfer preflight, dependency governance, signed
releases, performance budgets, provider-aware caching, indexed search, and a typed
canonical VFS path model.

## Assessment method

The review covered:

- Repository documentation, roadmap, active tickets, and completed tickets.
- Frontend architecture, state stores, keyboard handling, dialogs, and IPC calls.
- Rust command registration, file operations, VFS providers, network sessions,
  credential storage, and SSH terminal handling.
- Tauri capabilities and WebView security configuration.
- Existing test coverage, build scripts, source-file size, and CI configuration.
- The staged changes in the working tree; no existing staged or unstaged content
  was modified as part of this assessment.

## Architecture overview

### Frontend

The frontend is a React 19 and TypeScript application built with Vite. Zustand
stores hold pane, transfer, dialog, settings, search, and network state. TanStack
Table and TanStack Virtual support large directory views, while Radix UI provides
dialogs and menus. `src/ipc/client.ts` is the single frontend boundary for Tauri
commands and events and also provides browser-demo fallbacks.

This is a good overall direction: rendering and interaction stay in TypeScript,
while filesystem behavior remains in Rust. Strict TypeScript compiler options
are enabled, including unused-symbol and switch-fallthrough checks.

### Backend and IPC

The Tauri backend registers filesystem, archive, comparison, search, metadata,
network, and terminal commands in `src-tauri/src/lib.rs`. Rust structures that
cross the IPC boundary are exported to `src/ipc/bindings/` with ts-rs. Long file
transfers report progress through events, support pause/cancel, resolve name
collisions interactively, and optionally verify copies with SHA-256.

The generated bindings reduce contract drift, but errors are still mostly plain
strings. Many backend strings are German and are displayed directly by the
frontend, so the selected UI language does not consistently apply to failures.

### Virtual filesystem and networking

The `Provider` trait abstracts directory listing, metadata, streaming reads and
writes, directory creation, rename, and deletion. Local, SFTP, FTP/FTPS, SMB,
and WebDAV providers implement the same transfer primitives. The transfer engine
can therefore copy between local and remote stores without duplicating its core
progress, pause, cancel, collision, and checksum logic.

Saved connection profiles exclude secrets. Passwords and passphrases are stored
through an OS-keychain-backed `CredentialStore`, and SFTP plus the SSH terminal
share an authenticated SSH session. This is a strong base for remote operations.

The abstraction is not yet applied to all features. Search, directory and file
comparison, archive handling, detailed properties, folder size, volume data,
Git state, Finder tags, system opening, and trash contain local filesystem or OS
assumptions. The UI needs provider capabilities so these differences are explicit
instead of failing after invocation.

## Strengths

- Clear Rust-backend/React-frontend responsibility split.
- Strict TypeScript configuration and generated IPC data types.
- A reusable VFS provider interface with cross-store streaming transfers.
- OS keychain storage with no plaintext credential fallback.
- Progress, cancellation, pause, queueing, collision handling, and optional
  checksum verification in the transfer engine.
- Virtualized large result sets for file lists, search, and comparisons.
- A meaningful Rust test suite covering archives, transfers, search, metadata,
  VFS parsing, and provider behavior.
- A detailed architectural concept and ticket format with explicit acceptance
  criteria and Definitions of Done.

## Validation results

| Check | Result | Notes |
|---|---|---|
| `npm run typecheck` | Pass | Strict TypeScript check completed without errors. |
| `npm run build` | Pass with warning | Main JavaScript chunk is approximately 1.16 MB before gzip and 331 KB after gzip. |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | Pass | 83 passed; 6 ignored live/keychain tests. |
| `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check` | Fail | Formatting differences exist across several Rust files. |
| `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` | Fail | 10 findings, including argument count, type complexity, explicit auto-deref, `&PathBuf`, sorting, and constructor naming. |
| Frontend automated tests | Missing | No dedicated test runner or DOM/component test setup. |
| Continuous integration | Missing | No `.github` workflow currently enforces repository checks. |

## Prioritized findings

| Priority | Finding | Impact | Ticket |
|---|---|---|---|
| P0 | SSH accepts every server key | Credentials and remote data can be intercepted by an impersonated host. | [TICKET-028](tickets/028-ssh-host-key-verification.md) |
| P0 | Overwrite copies are not failure-safe | A cancelled or failed replacement can remove the old destination or leave partial data. | [TICKET-029](tickets/029-failure-safe-file-transfers.md) |
| P0 | The production WebView has no CSP | A frontend injection defect would have an unnecessarily large impact in a filesystem application. | [TICKET-030](tickets/030-tauri-webview-security-hardening.md) |
| P1 | Remote feature support is implicit | Local-only commands can receive remote addresses, producing confusing errors or unsupported behavior. | [TICKET-031](tickets/031-vfs-capabilities-and-remote-actions.md) |
| P1 | Connection attempts are unbounded and not cancellable | Slow or unavailable servers can leave ambiguous pending state and duplicate attempts. | [TICKET-032](tickets/032-non-blocking-connection-lifecycle.md) |
| P1 | Quality gates and frontend tests are absent | Regressions in keyboard/dialog/store behavior and cross-platform builds are easy to miss. | [TICKET-033](tickets/033-ci-and-frontend-test-foundation.md) |

## Second-wave improvement roadmap

| Priority | Finding | Impact | Ticket |
|---|---|---|---|
| P1 | Persisted operation state is lost on abnormal exit | Queued and active work cannot be reconciled confidently after a crash or restart. | [TICKET-044](tickets/044-restart-safe-transfer-recovery.md) |
| P1 | Backend failures are display-ready strings | Localization is inconsistent, callers parse text, and sensitive provider details can leak. | [TICKET-036](tickets/036-structured-localizable-errors.md) |
| P1 | Persisted frontend data is not versioned or validated | Upgrades or corrupt localStorage can load invalid settings and undermine safe defaults. | [TICKET-039](tickets/039-versioned-persisted-state.md) |
| P1 | Directory freshness is local-only and race-prone | Remote paths are sent to a local watcher and late asynchronous results can make panes stale. | [TICKET-042](tickets/042-provider-aware-refresh-and-cache-coherence.md) |
| P1 | Remote tests depend on manual infrastructure | Wire-protocol, security, and disconnection regressions are skipped in ordinary validation. | [TICKET-038](tickets/038-protocol-integration-test-harness.md) |
| P2 | Accessibility and localization are not enforced | Hard-coded labels and complex keyboard/focus behavior can exclude users or regress silently. | [TICKET-249](tickets/249-accessibility-and-localization-quality.md) |
| P2 | Production diagnostics are fragmented | Cross-layer failures are difficult to investigate safely without leaking sensitive data. | [TICKET-220](tickets/220-redacted-diagnostics-and-support-bundle.md) |
| P2 | The eager frontend bundle is oversized | Heavy optional features increase startup parsing and obscure performance regressions. | [TICKET-034](tickets/034-frontend-bundle-splitting.md) |
| P2 | Several modules combine unrelated responsibilities | Large transfer, IPC, settings, pane, and translation files raise review and regression cost. | [TICKET-035](tickets/035-modularize-high-complexity-subsystems.md) |
| P2 | Repository documentation describes an older architecture | Contributors cannot reliably discover the current VFS, networking, or validation model. | [TICKET-037](tickets/037-architecture-and-contributor-documentation.md) |

## Third-wave improvement roadmap

| Priority | Finding | Impact | Ticket |
|---|---|---|---|
| P0 | Archive-controlled paths lack one explicit containment layer | Crafted ZIP, tar, or 7z input could escape the selected destination or exhaust resources. | [TICKET-225](tickets/225-safe-archive-extraction.md) |
| P1 | Remote transport strength is not governed centrally | Plain FTP/HTTP and implicit provider defaults can expose credentials or permit unintended downgrade. | [TICKET-047](tickets/047-remote-transport-security-policy.md) |
| P1 | VFS locations are ambiguous strings | Frontend/backend parsing can diverge across IPv6, Unicode, Windows, encoding, and identity cases. | [TICKET-055](tickets/055-canonical-vfs-address-and-path-model.md) |
| P1 | Predictable transfer constraints are discovered late | Self-copy, naming, staging, capacity, and provider-limit failures can surface after work begins. | [TICKET-049](tickets/done/049-transfer-preflight-and-resource-estimation.done.md) |
| P1 | Completed operations have no explicit inverse contract | Users cannot safely undo eligible actions, while a generic undo could overwrite later changes. | [TICKET-048](tickets/done/048-reversible-file-operations-and-undo.done.md) |
| P1 | Release artifacts and updates lack an integrity pipeline | Users cannot independently verify official builds or safely recover from failed updates. | [TICKET-222](tickets/222-signed-reproducible-releases-and-updates.md) |
| P1 | Dependency security and licensing are not continuously checked | Parser, crypto, protocol, and frontend supply-chain risks may reach releases unnoticed. | [TICKET-221](tickets/221-dependency-and-license-governance.md) |
| P2 | Runtime performance has no shared baseline or budget | Optimizations and regressions cannot be assessed consistently across layers. | [TICKET-223](tickets/223-performance-benchmarks-and-budgets.md) |
| P2 | Repeated listings have no shared bounded cache | Remote navigation repeats expensive work, but ad hoc caches would risk stale or cross-session data. | [TICKET-053](tickets/done/053-bounded-provider-aware-cache.done.md) |
| P2 | Search rescans local trees for every query | Large repeated searches are expensive and remote/native index behavior is undefined. | [TICKET-054](tickets/done/054-scalable-indexed-search.done.md) |

## Additional feature proposal

| Priority | Finding | Impact | Ticket |
|---|---|---|---|
| P2 | Recursive folder size has no hierarchical visualization or scan-depth control | Users receive one total but cannot identify the largest branches, while a visual scan needs explicit depth and memory bounds. | [TICKET-209](tickets/209-folder-storage-usage-visualizer.md) |

## Recommended order

The current user-directed grouping is recorded in the
[ticket overview](tickets/README.md). The waves below describe technical
dependencies from the analysis, not the current sequential work queue. Supporting
work is now in 220+, while direct application development keeps its existing
numbers and the first functional plugin is 209.

1. Establish the CI and frontend-test foundation from TICKET-033 so subsequent
   safety work has repeatable gates.
2. Implement SSH host verification (TICKET-028) and WebView hardening
   (TICKET-030) before broadening external integration.
3. Implement failure-safe transfers (TICKET-029) before treating overwrite and
   cross-store move behavior as production-safe.
4. Add provider capability reporting (TICKET-031), then use it as the contract
   for remote UI behavior.
5. Complete bounded and cancellable connection lifecycle handling (TICKET-032).

The priority labels express user risk; the implementation order starts with the
test infrastructure because it reduces the risk of every following change.

For the second wave:

1. Add persisted-state versioning (TICKET-039) before introducing more stored
   settings or durable operations.
2. Implement structured errors (TICKET-036), then build redacted diagnostics on
   that contract (TICKET-220).
3. Use provider capabilities and connection lifecycle state as inputs to the
   refresh work (TICKET-042), and validate provider behavior through the protocol
   harness (TICKET-038).
4. Build restart recovery (TICKET-044) on the safe commit model from TICKET-029
   and the versioned storage rules from TICKET-039.
5. Establish accessibility/localization checks (TICKET-249), then split bundles
   (TICKET-034) and modules (TICKET-035) under the tests from TICKET-033.
6. Update the enduring architecture and contributor documentation (TICKET-037)
   as each contract lands, with a final consistency pass at the end.

For the third wave:

1. Implement archive containment and resource limits (TICKET-225) immediately
   because it closes an untrusted-input boundary independent of the other work.
2. Define canonical typed locations (TICKET-055), then use them in transfer
   preflight (TICKET-049), caching (TICKET-053), and indexed search (TICKET-054).
3. Enforce the transport policy (TICKET-047) on top of SSH host verification and
   connection lifecycle handling.
4. Establish dependency governance (TICKET-221) and the existing CI gates before
   building the signed release/update pipeline (TICKET-222).
5. Add reversible operation plans (TICKET-048) after safe commits, durable journals,
   structured errors, capabilities, and transfer preflight are available.
6. Establish runtime performance budgets (TICKET-223) before measuring the benefits
   of the shared cache (TICKET-053) and scalable index (TICKET-054).

TICKET-209 is the first functional plugin and follows the plugin interface in
TICKET-201 through TICKET-207, plus the provider capabilities required for scanning.
Its scan may reuse the cache only after TICKET-053 establishes identity and
freshness rules. TICKET-223 owns the shared performance infrastructure.

## Additional observations now tracked

The earlier lower-priority observations are now represented by TICKET-034 through
TICKET-038. The expanded review also records versioned persistence (TICKET-039),
provider-aware refresh (TICKET-042), accessibility/localization quality
(TICKET-249), restart-safe operation recovery (TICKET-044), and redacted
diagnostics (TICKET-220). Frontend regression coverage remains part of
TICKET-033, with the concrete Enter-key defect still independently tracked by
TICKET-014.

The third-wave findings are tracked by TICKET-225 and TICKET-047 through TICKET-049,
TICKET-053 through TICKET-055, and TICKET-221 through TICKET-223. They remain
separate from the native menu (TICKET-040), MCP server (TICKET-041), and plugin
series because they harden the core application and its release process regardless
of whether those expansion features are implemented.

TICKET-209 adds the requested folder storage-usage visualization as the first
functional plugin, with explicit recursion-depth and memory limits.

## Relationship to the existing backlog

The new tickets do not replace the active tickets for removable media, dialog
keyboard isolation, native properties, connection-manager discoverability,
KeePass, the native application menu, MCP, or plugins. TICKET-028 follows up the
explicitly deferred SSH `known_hosts` work in the completed SFTP and terminal
tickets. TICKET-031 formalizes the boundary that the completed VFS foundation
described when it deliberately left peripheral commands local-only.

No production API or behavior is changed by this analysis. The twenty-seven assessment
tickets are implementation proposals for later work; TICKET-040, TICKET-041, and
the TICKET-201 through TICKET-208 plugin series retain their existing subjects and
numbering.
