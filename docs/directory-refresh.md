# Directory refresh

Each tab subscribes using its stable ID, current request generation and VFS
address. The Rust backend owns provider selection, native watcher handles,
coalescing, connection checks and retry timing. The frontend only forwards tab
ownership and visibility and applies replies to the matching generation.
Navigation, archive navigation and manual refresh cannot overwrite a newer tab
state. Closing a tab prevents late replies from recreating it. Refresh preserves
selection and cursor by filename, independently for each consumer.

## Provider policies

| Provider | Automatic policy | Timing |
| --- | --- | --- |
| Local | Non-recursive native notifications | 250 ms debounce, at most 1 s postponement during a continuous event burst |
| SFTP | Adaptive polling on a live connection | 5 s after the previous read completes |
| FTP / explicit and implicit FTPS | Adaptive polling on a live connection | 5 s after the previous read completes |
| SMB | Adaptive polling on a live connection | 5 s after the previous read completes |
| WebDAV HTTP / HTTPS | Adaptive polling on a live connection | 5 s after the previous read completes |
| Disconnected or failed native watch | Manual, with a visible status | No freshness guarantee; reconnect resumes automatically |
| Archive view | No external subscription | Reopen or explicitly navigate the archive |

No provider currently supplies server change notifications. Local notifications
never receive remote addresses. Local paths use filesystem-resolved spelling;
notification matching resolves the parent without following a changed symlink.
Remote connection authorities share their provider identity, while path case and
dot components are preserved because server-side symlinks affect their meaning.

While visible, a single 250 ms UI dispatch timer asks for due work; this does not
read directories by itself. Add up to one dispatch tick and the provider's read
time to the policy intervals above. Native OS notification delivery has its own
latency. Read errors retry after 10, 20, 40 and then at most 60 seconds; successful
reads reset the interval. Failed, offline and manual states remain visible beside
the file list. A failed refresh retains the previous rows. There is no bounded
staleness during a disconnected or failing provider operation.

Hiding the window suspends automatic reads and the dispatch timer. Visibility and
connection-state events wake the scheduler. Becoming visible or reconnecting
makes a refresh due immediately. Create, delete, rename and metadata bursts share
one pending refresh; a change during a read schedules another. Notification
errors and queue overflow invalidate the listing rather than silently losing the
change. Watch setup failures are structured errors; an explicit refresh retries
subscription setup through the changed request generation.

## Bounds and lifetime

- At most 128 tab subscriptions and 64 distinct locations; excess requests are
  refused and shown as refresh errors.
- One native watcher, one watch per distinct local location, and at most 128
  queued notification events with at most 128 paths each. Overflow requests a
  complete reread of the subscribed local locations.
- At most 64 directory requests, 32 distinct in-flight reads and four concurrent
  provider reads. Identical concurrent reads share immutable results, never UI
  state. Completed results are removed immediately; this is not a directory cache.
- Removing the last consumer unwatches its location. Removing every local
  consumer drops the watcher. A failed unwatch forces reconstruction from the
  remaining subscriptions so a handle cannot remain permanently orphaned.
- Removing the last refresh subscription cancels its waiting request and skips
  queued filesystem work. An already executing synchronous provider call drains
  under the four-worker bound and provider timeout; its late result cannot update
  the closed tab. Remote results are rejected if the live provider changed.

## Verification

`npm run check` includes controlled out-of-order navigation tests, shared-listing
and cancellation tests, scheduler tests with an explicit clock, frontend
visibility/teardown tests, and a real native create-notification test. The native
test is portable and runs in the existing Linux/macOS/Windows Rust CI matrix; the
local execution for this ticket was macOS. A sandbox that suppresses native
notifications must allow the native watcher test to run outside that sandbox.

`node harness/harness.mjs run` additionally runs
`harness_shared_directory_reads_follow_connection_lifecycle` against all seven
real remote services. It checks simultaneous pane reads, refusal after disconnect
and fresh reads after reconnect. A local macOS run does not claim a native Windows or Linux execution.
