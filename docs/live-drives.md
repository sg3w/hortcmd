# Live drive updates

TICKET-057 adds one app-lifetime Rust worker that checks mounted volumes every
two seconds. `list_drives` and the worker share the same snapshot builder.
Snapshots are sorted and deduplicated by mount point; the home shortcut is added
only when it is not already a mount.

Mount/name/total-capacity changes publish `drives-changed` with `Drive[]`.
Free-space changes publish at most every ten seconds and require a cumulative
1 MiB difference from the last emitted value. Unchanged snapshots and small
jitter produce no event. Failed/empty reads retain the last snapshot.

On macOS, `getfsstat(MNT_NOWAIT)` checks cached mount metadata every two seconds;
the more expensive sysinfo snapshot is rebuilt on topology changes or every ten
seconds. Cached network-mount entries supplement sysinfo, which excludes them.
Intentionally hidden (`nobrowse`) volumes remain hidden. Other platforms retain
the common sysinfo polling fallback, pending the native checks in TICKET-260.
No directory traversal or file-content reads are performed by the watcher.

The frontend shares a reference-counted subscription and snapshot between the
app and file browser. A late initial command response cannot overwrite a newer
event. Dropped consumers unsubscribe, and empty reads do not remove all drives.
Mount membership checks component boundaries, so removing `/Volumes/card`
does not affect `/Volumes/card2` or a remote VFS address.

Affected active panels and the open file browser return to home with a notice.
Async home lookup checks navigation identity before applying the fallback.
Removed mounts are retained in bounded session memory until remounted, allowing
inactive tabs or failed pending reads to recover when they become active.
Remote connections and favorites keep their independent picker entries.

## Validation

Rust tests cover insertion/removal, stable snapshots over 120 simulated seconds,
capacity cadence and jitter. Frontend tests cover event fan-out, subscription
cleanup, stale startup responses, open Radix menus, file-browser recovery and
navigation races.

An explicit `native_volume_probe` test can be run while mounting/unmounting an
owned temporary image. It samples the real platform implementation for 80
seconds and reports changes and time spent collecting metadata. Physical media,
network availability and sleeping-drive checks remain in
[TICKET-260](../tickets/260-removable-media-native-verification.md).

Local macOS verification used an owned 16 MiB HFS+ DMG. Its visible mount
appeared, writing 2 MiB updated available space, and detaching it removed the
mount from the sampled list. The optimized 80-second probe spent 341 ms in 40
metadata polls (aggregate wall time, not CPU utilization). Background writes
changed root capacity during this run, so a quiet-machine idle/CPU measurement
and physical-media checks remain in TICKET-260.
