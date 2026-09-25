# Scoped search index

Open Search → Search index and approve a root. No scope, remote connection or file
content is indexed automatically. The default retains filename/path, type, size,
modification time and provider identity in memory for this process only. Each scope
can be inspected, paused, resumed, rebuilt or deleted. Closing the index view stops
scheduling crawl steps and cancels its current request; reopening resumes an
unpaused selected scope. Other scopes are not crawled in the background implicitly.

Text content requires a separate checkbox per scope. Only UTF-8 text without NUL
bytes and at most 1 MiB per file is eligible; files with multiple hard links are not
read for content. Content may contain personal information. Known credential
folders/files (including SSH/cloud credentials, .env, private-key and KeePass
files), user ignore patterns, and hortcmd transfer/undo staging names are excluded
regardless of the hidden-file option. The index never reads clipboard, terminal or
credential APIs. This is an explicit scope/content choice, not a semantic detector
for every possible secret embedded in otherwise ordinary text.

On macOS, canonical roots are bound to device/inode identity. Root and component
opens refuse symlinks, traversal stays on the approved device, hidden flags follow
the selected policy, and system-restricted entries are excluded. A replaced root
requires fresh approval. Root-relative metadata is read through directory handles;
content handles are checked against the discovered object and checked again after
reading. Excluded parent directories are pruned. Non-Unicode names, inaccessible
folders or changed files pause the scan with an error rather than silently claiming
complete coverage. Windows/Linux local adapters are deferred to ticket 121.

Remote scopes must refer to an explicitly selected saved profile and an already
verified connection. Stored identity contains the profile ID and endpoint digest,
not a credential-bearing URL. The provider namespace below that approved root is
the remote boundary, including server-mounted subfolders; physical remote mount
identity is not available. Reported symlinks are skipped. Remote content indexing
is refused because current providers cannot prove no-follow stable content reads
(ticket 122). No profile is connected automatically. Disconnects require reconnect
unless offline viewing was explicitly approved. Offline results have no actionable
address. Profile deletion or edits erase both open scopes and locked encrypted
snapshots, including interrupted-write files. Reconnect generations force a new
reconciliation and old results remain labelled stale/incomplete meanwhile.

## Search dialog session

Reopening the index dialog from the same initial root restores its selected scope,
creation preferences, query filters, last result page and query status for the
current application session. The snapshot is kept only in memory; passwords are
never included. Opening from another initial root starts a fresh dialog session.

Cached results and errors remain hidden until the backend confirms the selected
scope is still available with the same root and policy. Changing scopes, locking
or removing a scope, changing its policy/root, or losing permitted offline access
clears its results and content query. Unlocking does not revive those results.
Reopening never automatically submits a search. Existing approved crawl scheduling
continues according to the scope state described above.

Closing or stopping a query cancels its request and preserves a cancelled status.
Late query, creation and control responses cannot overwrite a closed dialog or a
new selection. Completed queries revalidate the scope before displaying results.
The retained result page remains bounded by the existing 200-hit query limit.

## Bounds, checkpoints and freshness

One crawler and at most four queries run at once, with no unbounded request queue.
A crawl step processes up to eight directories or 200 ms between directories. A
single provider directory request can take longer; cancellation is checked per
local entry/content chunk and before/after synchronous remote library calls. A
checkpoint is saved after each completed step, including cancellation/partial
failure. An interrupted directory is retried without duplicating rows. Encrypted
checkpoints reopen paused and never resume work silently.

Each scope holds at most 50,000 rows, 5,000 queued directories and depth 64. A
single directory is limited to 50,000 entries; providers with a vector-only listing
API may allocate that listing before the application can reject it. Retained data
has an 8 MiB weighted working target and a 16 MiB serialized ceiling. There are at
most eight unlocked scopes and 64 persistent scopes. Limits are reported; narrow a
scope instead of treating an incomplete answer as complete. This bounded snapshot
format targets repeated searches within selected working trees, not an unlimited
whole-disk database.

Recursive local watch events inside the approved scope mark its snapshot stale.
Unrelated pane refreshes and the application-wide listing cache do not restart
indexes or invalidate their pagination revisions. Every five minutes a selected
active scope also schedules reconciliation to catch
missed events; remote roots use this bounded polling fallback. Scans reuse unchanged
local content by file identity/version and remove unseen rows only after completing
a whole cycle. The directory queue, cycle and version tags persist in checkpoints.
Low available memory (below 256 MiB) or battery operation pauses work. Battery work
requires a separate opt-in. Native battery/root validation on other operating
systems is deferred rather than silently inferred.

The query contract supports name/path glob, literal text, case selection, type,
size bounds and modification time. Single-pattern glob semantics are shared by
all index adapters; they do not reinterpret spaces as multiple patterns. Pages
contain at most 500 hits, stable scope/path identities, metadata/content provenance,
revision and next cursor. A changed revision rejects an old cursor. Results always
identify snapshot, incomplete, stale or offline status; no index entry authorizes
file mutation. Existing provider preflight, undo and commit checks remain live.

The `application` adapter reads the approved index. `live` rebuilds the approved
scope with the same crawler/predicates before answering; it still reports a snapshot
rather than promising future authority. A `native` request explicitly falls back
to the application adapter (`fallback: true`). Spotlight is not enabled: equivalence
with this explicit content/exclusion and freshness contract has not been established.
This selection keeps the query contract and scope consent unchanged; it does not
claim that arbitrary native results passed the contract tests.

## Optional encrypted persistence

A passphrase of at least 12 characters enables per-scope encrypted checkpoints.
AES-256-GCM authenticates the complete snapshot and its scope ID; PBKDF2-HMAC-SHA256
uses 210,000 iterations and a random 16-byte salt. Each write uses a new random
12-byte nonce. Only the derived key stays in the unlocked process; the passphrase
is not persisted. There is no plaintext index or plaintext write-ahead log.

The index directory is mode 0700 and snapshots are mode 0600. A new private file is
written, synced and renamed, then its parent is synced. Corrupt, tampered, wrongly
unlocked or outdated files are rejected. Delete the locked scope and approve a new
one to rebuild if its passphrase is lost or its snapshot cannot be recovered.
Encrypted filenames contain only an opaque profile digest and random scope ID,
which permits deletion without decrypting the snapshot. Encryption protects stored
metadata/content against readers without the passphrase, not an already compromised
unlocked application process. Deletion removes index files, not original files or
independent filesystem backups.

## Validation and performance

Tests cover consent defaults, text/binary/size rules, secret/staging exclusions,
symlink and root replacement, cancellation, encrypted checkpoint resume, changes
missed by watch, Unicode/case and metadata predicates, pagination revision checks,
native fallback, authenticated corruption rejection, restrictive file permissions,
profile deletion and offline UI behaviour.

A deterministic 1,000-file prefix query is compared with the existing live-search
engine and yields the same 111 paths. Twenty repeated index queries enumerate zero
files. The regression prints initial crawl and both repeated-query timings; timings
are observational, not a flaky CI assertion. Broader cold/warm, filesystem and
platform measurements remain in ticket 112.
