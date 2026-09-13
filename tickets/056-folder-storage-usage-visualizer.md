# TICKET-056: Add a folder storage-usage visualizer with bounded scan depth

## Goal

Provide an interactive visual breakdown of how a folder consumes storage while
keeping traversal depth, node count, memory use, provider load, and scan duration
explicitly bounded.

## Background

The Properties dialog currently calls `dir_size` to calculate one recursive total
for a local folder. The backend walk is iterative, cancellable, skips symbolic
links, and counts unreadable entries, so it does not risk call-stack overflow or
symlink loops. It does not retain per-child usage, expose scan depth, or provide a
visual explanation of which subfolders and files account for the total.

For this ticket, “memory usage” means filesystem storage usage, not process RAM.
The visualizer must report its own bounded RAM consumption separately. TICKET-031
defines remote-provider capabilities, TICKET-052 supplies performance budgets, and
TICKET-053 supplies cache rules that may later be reused without making stale data
look current.

## Requirements

- Once implemented, expose **Befehle → Speicherbelegung analysieren…** through
  the shared command definition from [TICKET-040](040-native-application-menu.md).
  The existing folder/volume size display stays in Properties; the new analyzer
  is a distinct workflow. Do not add a non-functional menu placeholder in 040.

### Scan model and recursion depth

- Add a typed scan request with root location, maximum depth, item limit, size mode,
  hidden-file policy, and provider-specific options.
- Define the selected root as depth `0`, its direct children as depth `1`, and so
  on. Use this definition consistently in Rust, TypeScript, labels, and tests.
- Default the maximum depth to `32` for a new scan.
- Allow the user to select a maximum depth from `0` through `128`; `0` scans only
  the selected root and reports its children as depth-limited.
- Enforce an absolute safety ceiling of `256` in the backend, regardless of a
  malformed or future client request.
- Keep traversal iterative rather than using the language call stack.
- When the depth limit is reached, retain an explicit truncated node with known
  direct totals and an `incompleteBelowDepth` marker. Never present a truncated
  branch as fully measured.
- Drilling into a branch starts a new explicitly approved scan rooted at that
  branch; it must not silently bypass the active depth and resource limits.

### Usage accounting

- Return a hierarchical result with stable node ID, canonical location, display
  name, kind, depth, direct bytes, recursive bytes, file count, folder count,
  unreadable count, and completeness state.
- Report logical file size everywhere. Report allocated/on-disk size only where a
  provider exposes it reliably, and label unavailable values instead of estimating
  them silently.
- Do not follow symbolic links, junctions, reparse points, or remote link types by
  default. Show their own entry size when available and mark them as links.
- Define hard-link accounting explicitly. When stable file identities are
  available, offer apparent-size and deduplicated-size views; otherwise label the
  total as apparent usage.
- Use overflow-safe counters and define behavior when files change during a scan.
- Count unreadable, disappeared, cyclic, unsupported, and provider-error entries
  separately rather than treating them as zero-sized successful results.

### Visualization and interaction

- Provide a treemap as the primary visualization, sized by the selected usage
  metric, with a sortable table/list alternative showing the same data.
- Support zoom/drill-down, breadcrumbs, back navigation, hover/focus details,
  search/filter, and “reveal in pane” without losing the completed scan.
- Group nodes below a visual threshold into an “Other” aggregate while retaining
  access to the underlying sorted list.
- Use labels, patterns, and values in addition to color. Respect high-contrast,
  reduced-motion, keyboard, screen-reader, and 200% zoom requirements from
  TICKET-249.
- Show scan status, elapsed time, current path, counts, applied limits, truncation,
  stale data, and partial-error summaries continuously.
- File mutations launched from the visualizer must use normal capabilities,
  confirmations, undo, and live revalidation; visualization data is never authority
  for a destructive action.

### Resource control and providers

- Stream bounded result batches to the frontend and support pause, resume, and
  cancellation without waiting for the entire tree.
- Add configurable soft limits for scanned entries, elapsed time, and retained
  visualization nodes, with conservative defaults and backend hard ceilings.
- Keep backend and frontend memory proportional to retained nodes, not total files.
  Aggregate low-priority nodes during traversal when the retained-node limit is
  reached.
- Apply bounded concurrency and provider-specific rate limits. A slow remote scan
  must not block navigation, transfers, terminal traffic, or connection cleanup.
- Enable the feature only for providers advertising recursive-listing and metadata
  capabilities. Explain unsupported or incomplete metrics before the scan begins.
- Cancel or suspend remote work on connection-generation change and require normal
  verified reconnection before resuming or rescanning.
- Cache results only under TICKET-053 identity, freshness, invalidation, privacy,
  and capacity rules.

## Acceptance Criteria

- [ ] Users can open a folder usage treemap and equivalent sortable table from the
      pane or folder Properties dialog.
- [ ] Root depth is `0`; scans stop at the requested limit and never exceed the
      backend hard ceiling of `256`.
- [ ] Every limited, unreadable, changed, cancelled, or provider-incomplete branch
      is visibly marked and excluded from claims of a complete total.
- [ ] Symlinks and equivalent link types are not followed by default.
- [ ] Logical and allocated/deduplicated sizes are clearly distinguished according
      to provider support.
- [ ] Scan time, entries, concurrency, retained nodes, backend memory, and frontend
      memory are bounded by documented limits.
- [ ] The visualizer remains responsive, cancellable, keyboard-accessible, and
      usable at 200% zoom.
- [ ] Destructive actions revalidate live provider state and use the ordinary
      capability, confirmation, and undo contracts.

## Tests

- Test exact depth boundaries at `0`, `1`, the default `32`, user maximum `128`,
  hard ceiling `256`, and rejected negative/overflowing/malformed requests.
- Generate a tree deeper than 256 levels and verify iterative traversal, truncation
  markers, stable cancellation, and no stack overflow.
- Test wide trees with millions of synthetic entries against item, time, retained-
  node, event-batch, and memory ceilings.
- Test symlink loops, junctions/reparse points, hard links, sparse files, changing
  files, deleted paths, permission failures, counter overflow, and mount boundaries.
- Compare hierarchical totals with the existing `dir_size` result for complete
  supported local scans and document metric differences.
- Run provider contract tests for local, SFTP, FTP/FTPS, SMB, and WebDAV, including
  disconnect, reconnect, latency, and rate limiting.
- Component-test treemap/table parity, “Other” aggregation, drill-down, breadcrumbs,
  stale/partial states, keyboard navigation, accessible names, and reduced motion.
- Benchmark representative deep and wide trees through TICKET-052 and assert agreed
  backend/frontend peak-memory budgets.

## Definition of Done

- Folder storage use is available as an interactive treemap and accessible table.
- Scan depth semantics, default `32`, selectable maximum `128`, and backend ceiling
  `256` are enforced and documented.
- Partial and truncated results can never be mistaken for complete totals.
- Traversal, streaming, retained nodes, concurrency, time, and memory are bounded.
- Local and capable remote providers have contract and performance coverage.
- Concept.md and provider documentation describe metrics, limits, and link behavior.
- Rename this file to `056-folder-storage-usage-visualizer.done.md`.
