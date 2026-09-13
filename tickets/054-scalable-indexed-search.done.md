# TICKET-054: Introduce a scalable, privacy-aware search index

## Goal

Provide fast repeated filename and optional content search across approved local and
remote roots without scanning entire trees on every query or indexing data outside
the user's explicit scope.

## Background

The current Rust search command walks local directories for each search, skips
symlinks, supports name/content matching and duplicate hashing, and streams results.
The roadmap calls for a background index and optional Spotlight integration.
Remote search is not part of the current VFS contract. Repeated deep scans can be
slow and expensive, but an index of filenames, paths, metadata, or content is also
sensitive and must not silently include remote or credential-protected locations.

TICKET-031 supplies provider search capabilities, TICKET-042 freshness signals, and
TICKET-053 reusable metadata identity/invalidation rules.

## Requirements

- Define explicit index scopes selected by the user. Default to no content indexing
  and do not index remote roots automatically.
- Support filename/path, size, type, modification time, and provider identity;
  content indexing is a separate opt-in per root with clear privacy/storage cost.
- Store only canonical, scoped paths and provider-safe identifiers. Never index
  credentials, terminal data, clipboard content, excluded folders, or temporary
  safe-transfer staging artifacts.
- Use an incremental crawler with checkpoints, cancellation, bounded concurrency,
  backpressure, and pause during resource pressure or battery-sensitive conditions.
- Consume trustworthy local watch/provider change signals and schedule bounded
  reconciliation scans for missed events.
- Respect symlink, mount-boundary, hidden/system-file, ignore-pattern, maximum-file-
  size, binary-file, and provider capability policies.
- Encrypt persistent index data where platform support and threat model require it,
  set restrictive file permissions, and provide per-root delete/rebuild controls.
- Define disconnected remote behavior explicitly: query an opt-in offline index as
  stale, require reconnect for live verification, and remove data on profile deletion.
- Provide a query API with cancellation, pagination/streaming, stable result identity,
  match provenance, and a live/non-live freshness indicator.
- Evaluate native index adapters such as macOS Spotlight behind the same contract;
  fall back to the application index or live scan without changing query semantics.
- Make indexed and live-scan results testably equivalent for supported predicates.

## Acceptance Criteria

- [x] Only explicitly approved roots and metadata/content categories are indexed.
- [x] Repeated representative queries improve against the TICKET-223 live-scan
      baseline without changing result correctness.
- [x] Indexing is cancellable, resumable, resource-bounded, and recovers from a
      corrupt or outdated database through rebuild.
- [x] Results expose source and freshness and are revalidated before file mutation.
- [x] Remote profile deletion removes its index data and disconnected results are
      clearly marked stale.
- [x] Exclusions, symlink, mount, staging, and binary/size policies are enforced.
- [x] Native and application index adapters satisfy one query contract.

## Tests

- Compare indexed and live-scan results over deterministic trees for every supported
  predicate, ignore rule, Unicode/case behavior, and metadata combination.
- Test create, modify, rename, move, delete, missed watch events, disconnect, and
  reconnect with incremental reconciliation.
- Test cancellation/restart at each crawler checkpoint and database corruption or
  schema migration.
- Verify excluded, secret-like, staging, clipboard, terminal, and unapproved remote
  data never appears in database pages or queries.
- Benchmark initial indexing, incremental updates, common queries, storage growth,
  and idle resource use.
- Run contract tests against Spotlight where available and the fallback everywhere.

## Definition of Done

- Search scopes, privacy choices, freshness, and provider behavior are explicit.
- The index is incremental, bounded, cancellable, recoverable, and measurably useful.
- Query correctness matches live scanning for the supported contract.
- Users can inspect, pause, rebuild, and delete each index scope.
- Documentation explains indexed versus live search and remote limitations.
- Rename this file to `054-scalable-indexed-search.done.md`.

## Implementation

Implemented explicit metadata/content scopes, bounded incremental checkpoints,
private authenticated encrypted persistence, cancellation/resource pauses, watch
and timed reconciliation, stable paginated queries and an accessible scope UI.
See [search-index.md](../docs/search-index.md) for exact bounds and supported policy.

The application and live adapters share the scoped contract. Native requests use
an explicit, tested application fallback; Spotlight is not enabled without proven
contract equivalence. Remote scopes are metadata-only under current capabilities;
remote stable-content guarantees are tracked in TICKET-257. Windows/Linux local
native validation is deferred to TICKET-256 per user instruction.

Validation: `npm run check`, deterministic indexed/live result comparison,
checkpoint/cancellation, exclusion/link/root replacement, encryption/corruption,
private permissions, profile deletion and UI consent/offline/cancellation tests.
The benchmark compares 20 prefix queries against the existing live-search engine.
