# TICKET-053: Add a bounded provider-aware directory and metadata cache

## Goal

Reduce repeated directory and metadata work while guaranteeing that cached content
cannot cross connection identities, outlive trust changes indefinitely, or conceal
known filesystem updates.

## Background

Pane refreshes currently request complete directory listings. The directory tree
keeps a frontend-local subfolder cache, but there is no shared VFS cache with
provider identities, freshness rules, invalidation, memory bounds, or reconnect
semantics. Remote listings can be expensive, while careless caching can show stale
permissions, sizes, or entries and can mix data after a profile reconnects to a
different server.

TICKET-031 defines capabilities, TICKET-032 connection generations, and TICKET-042
refresh/invalidation events. This ticket introduces reusable storage only after
those identity and freshness contracts exist.

## Requirements

- Define cache keys from canonical address, provider/store identity, connection
  generation, authenticated principal, path, listing options, and relevant capability
  version. Display names alone are never cache identity.
- Cache directory listings and selected cheap metadata separately with independent
  size, item-count, and time budgets.
- Use weighted bounded eviction and expose current size/hit/miss/eviction metrics in
  redacted diagnostics.
- Coalesce identical in-flight reads and propagate cancellation when all consumers
  leave without cancelling work still needed by another pane.
- Invalidate or update entries after successful application mutations, native watch
  events, provider notifications, reconnect, capability change, host-key/certificate
  trust change, and explicit refresh.
- Represent freshness as fresh, stale-while-refreshing, offline snapshot, or invalid;
  show stale/offline status to users rather than presenting it as live truth.
- Never use stale cache data to authorize deletes, overwrites, undo, or commit-time
  safety checks. Mutating operations must revalidate against the provider.
- Clear sensitive remote metadata on profile deletion and according to a documented
  session/end retention policy.
- Keep the default cache in memory unless a separate privacy-reviewed option enables
  encrypted persistent offline metadata.
- Prevent one provider or huge directory from evicting all useful data through fair
  per-provider limits.

## Acceptance Criteria

- [x] Repeated reads of the same unchanged location avoid duplicate provider work.
- [x] Cache entries cannot be reused across connection generations, principals, or
      trust identities.
- [x] Known mutations and refresh events invalidate or update all affected views.
- [x] Memory, item count, age, and per-provider consumption are bounded.
- [x] Stale/offline content is visibly distinct and never used for destructive
      authorization or commit decisions.
- [x] Profile deletion removes associated cached metadata.
- [x] Cache metrics contain no raw sensitive paths or server identities by default.

## Tests

- Use counting providers to verify hit, miss, coalescing, eviction, cancellation,
  and per-provider fairness.
- Test two profiles with identical paths but different hosts, users, trust state, and
  connection generations for strict isolation.
- Apply create, delete, rename, copy, move, metadata, watch, disconnect, and reconnect
  events and verify every affected key.
- Race an in-flight load with invalidation and ensure the late result cannot restore
  invalid data.
- Test huge directories and adversarial access patterns against memory budgets.
- Verify destructive operations consult live provider state even when a fresh-looking
  cache entry exists.

## Definition of Done

- Directory and metadata caching uses canonical, security-aware identities.
- Capacity, freshness, invalidation, isolation, and mutation rules are documented
  and enforced.
- Cache use measurably improves selected scenarios from TICKET-112.
- Stale data cannot silently drive destructive decisions.
- Privacy and profile-deletion behavior are tested.
- Rename this file to `053-bounded-provider-aware-cache.done.md`.

## Implementation

Implemented memory-only listing and local presentation metadata caches, security-bound
keys, bounded fair eviction, shared cancellation and conservative mutation/session/
watch invalidation. Details and measured avoided provider work are in
[provider-cache.md](../../docs/provider-cache.md).

Validation: `npm run check`; isolated real-provider regression plus cache identity,
budget, expiry, race and cancellation tests; frontend refreshing/stale-state coverage.
