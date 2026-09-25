# Storage usage

Install this directory with Settings → Plugins, review its four read-only/context/UI
permissions, and activate. Commands → Analyze storage usage, the pane menu, folder
Properties and the plugin command bar all open this package's window.

The root is depth 0. Default depth is 32; the form allows 0–128 and the broker
rejects depths above 256. Depth 0 retains only the explicitly incomplete root.
Zoom and breadcrumbs inspect the existing snapshot. “Use this folder for a new
scan” requires a separate Start action and reapplies all limits.

Logical bytes count every directory entry (including each hard link); directories
contribute no allocation metadata. Links contribute their own metadata size and
are never traversed. Unix allocated bytes use blocks × 512; deduplicated bytes
count each device/inode once in traversal order, so a later alias can display zero.
When the identity budget runs out, deduplicated totals become unavailable. Windows
allocated and deduplicated metrics are unavailable, rather than estimated. Sparse
files can have logical bytes greater than allocation. Existing Properties totals
skip links, so they differ by link-entry bytes. These are observations during a
scan, not an atomic filesystem snapshot. Changed directory metadata, read failures,
limits and cancellation mark partial results. A global listing-cache invalidation
or 60 seconds of age marks the snapshot stale; no scan results are persisted/shared.

Defaults: 100,000 entries, 1,000 retained nodes, 30 seconds. Hard ceilings:
1,000,000 entries, 5,000 nodes, 300 seconds. Each pull processes at most 256 entries
or 25ms between filesystem calls and returns at most 64 changed nodes. Paused time
counts toward the deadline. One request per instance is allowed. A stalled OS call
can outlast the cooperative deadline; it runs on the broker's blocking worker.

DFS retains only open ancestors and bounded nodes, identities and dirty IDs.
Unretained entries accumulate as Other; they do not become a second hidden tree.
The service reserves a conservative scan-memory budget (8,768 bytes per retained
node plus 73,728 per permitted depth) from a shared 128 MiB pool. This includes
path and estimated iterator overhead, not measured process RSS. The UI retains
at most the node limit, renders 200 table rows and roughly 60 treemap tiles.

Local folders are currently the only capable provider. SFTP, FTP/FTPS, SMB and
WebDAV do not advertise recursive-size/stat support in the current capability
matrix, and the broker rejects them before any provider request. No remote
connections, retries or generation transitions are initiated by this package.
Adding a remote adapter requires bounded, no-follow metadata traversal and
connection-generation validation first. Archives are unsupported.

No file content is read. Reveal uses the normal broker operation; the plugin
provides no mutations. Closing/deactivation drops scan handles and snapshots.
