# TICKET-042: Make pane refresh provider-aware and race-safe

## Goal

Keep visible directory contents coherent when files change externally, connections
drop, requests finish out of order, or multiple tabs point at the same location,
without unnecessary polling or UI flicker.

## Background

The backend directory watcher uses one global `notify` watcher rebuilt from the
currently displayed paths. Watch creation and individual watch failures are
silently ignored. The frontend sends every non-archive pane path, including remote
addresses, even though `notify` only observes local filesystem paths. Events carry
only a derived directory string and trigger a 250 ms debounced refresh. Remote
providers have no refresh policy, and asynchronous directory loads need explicit
protection against stale responses overwriting newer navigation state.

TICKET-031 defines provider capabilities; this ticket uses those capabilities to
select native watching, polling, manual refresh, or provider-specific change
notifications.

## Requirements

- Define a refresh capability and policy per provider: native watch,
  provider-notification, adaptive polling, or manual-only.
- Restrict the local `notify` watcher to validated local paths and return structured
  setup failures instead of silently ignoring them.
- Replace the single implicit global watcher with subscription identifiers owned by
  pane/tab locations; release subscriptions when their last consumer leaves.
- Normalize event paths consistently across Windows, macOS, Linux, and remote
  address forms before matching a visible location.
- Coalesce bursts while preserving meaningful create, delete, rename, and metadata
  changes. Define the maximum staleness and refresh latency for each policy.
- Tag directory requests with a location generation or request ID so a response
  for an older path cannot replace a newer tab state.
- Share safe in-flight results for identical locations while keeping selection,
  cursor, scroll, and per-tab errors independent.
- Suspend or back off remote polling when the window is hidden, the profile is
  disconnected, or repeated requests fail; refresh immediately after reconnect.
- Surface a non-blocking stale/offline indicator when automatic freshness cannot
  be guaranteed.
- Bound memory, timers, watcher handles, and queued refresh work.

## Acceptance Criteria

- [ ] Remote addresses are never passed to the OS filesystem watcher.
- [ ] Navigating rapidly cannot display a late result from an earlier location.
- [ ] Two panes or tabs at the same location share backend work without losing
      independent UI state.
- [ ] Local external changes appear within the documented debounce window.
- [ ] Remote refresh obeys capabilities, visibility, backoff, and reconnect state.
- [ ] Watch and polling failures are visible and actionable without modal spam.
- [ ] All subscriptions, timers, and in-flight requests are released when unused.

## Tests

- Unit-test path normalization and matching for local roots, UNC paths, Unicode,
  case-sensitive/case-insensitive platforms, and every remote scheme.
- Use controllable promises to complete navigation requests out of order and prove
  only the current generation applies.
- Test create/delete/rename storms, watcher overflow, and event coalescing.
- Test shared locations across two panes and multiple tabs, then close consumers in
  different orders and verify resource cleanup.
- Test remote polling backoff, hidden-window suspension, disconnect, and reconnect.
- Integration-test native watcher behavior on Windows, macOS, and Linux.

## Definition of Done

- Refresh behavior is explicit for every VFS provider.
- Directory results are race-safe and external changes update the correct tabs.
- Resource usage and failure behavior are bounded and observable.
- UI state remains stable across background refreshes.
- Provider freshness guarantees and limitations are documented.
- Rename this file to `042-provider-aware-refresh-and-cache-coherence.done.md`.
