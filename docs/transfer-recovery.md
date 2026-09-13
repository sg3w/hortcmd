# Transfer recovery

On macOS, desktop copy, move, extract and pack operations record their intent in
`transfers-v1` inside the application configuration directory. Copy queue entries
are persisted before they can run. Opening **Commands → Transfer recovery** or
restarting the application reads the journal; neither action restarts work or
removes sources. A surviving native operation can be reattached after a webview
reload. Windows/Linux enablement and native verification are tracked in
[TICKET-251](../tickets/251-windows-linux-transfer-recovery.md).

## Recovery choices

- **Queued:** the saved order and settings survive; starting requires an explicit
  action. Current collision prompts still apply.
- **Resumable:** completed local top-level items have verified destination content;
  only remaining sources are submitted again. There is no speculative byte-offset
  resume. Changed committed content prevents this action.
- **Retryable:** retry starts a new operation, with confirmation. It does not inherit
  an earlier overwrite approval.
- **Completed:** a durable terminal success or verified final state was found.
  Inspection does not authorize deleting any remaining move source.
- **Cleanup required:** staging, backups or a remaining move source need attention.
  Confirmed cleanup removes only identifiable private staging directories. It never
  deletes sources or final destinations. An unverified backup is retained.
- **Unsafe interruption:** a direct-write operation stopped without a terminal
  result. Its destination may contain partial data; it is never called committed.
- **Unavailable / unrecoverable:** missing profiles, disconnected sessions or
  insufficient evidence prevent resumption. Reconnect through the normal network
  dialog and refresh. Host-key and TLS checks are unchanged.

Discarding a record requires confirmation and forgets only metadata; it is not a
filesystem cleanup. Reveal opens the affected destination directory in a pane.
Archives restart from the beginning, including the ordinary password prompt;
passwords are not journaled. The separate MCP service does not use the desktop
transfer queue or desktop journal.

## Schema and durability

Schema version 1 stores an operation ID, source/destination references, queue
order, timestamps, settings, item phases, private staging/backup references and
local verification evidence. Remote references contain a saved profile ID, a digest
of its endpoint identity and a provider path. Credential values, signed URLs and
raw provider error messages are not stored. Terminal reasons are stable error
codes. Paths are sensitive metadata; the journal directory is private (0700).

Snapshots use exclusively created temporary files, file synchronization, atomic
rename and directory synchronization. An exclusive process lock prevents two
writers. Unknown schema versions, corrupt/truncated records and oversized records
fail closed and preserve evidence; version 1 is the first schema, so there is no
older supported schema to migrate.

The item sequence is planned → writing → prepared → publishing → committed →
source removed (moves only), or skipped after safe cancellation/rollback. Local
staged data is synchronized and hashed before publication; native same-filesystem
renames use object identity. Publication is journaled before deleting a copy-based
move source. Remote providers currently cannot prove durable publication, so
interrupted remote writes are never offered as verified item-boundary resumption.
A failed cleanup remains visible rather than silently reporting success.

## Bounds and retention

Each record is limited to 1 MiB and 20,000 planned items; the journal holds at most
128 records. Enqueuing prunes acknowledged terminal history older than 30 days and
keeps at most the newest 100 such existing records. Unresolved operations are never
evicted to make space: new work fails with an explicit journal-limit error.
Private interrupted snapshot temporaries are removed on journal opening. Transfer
staging is not removed on a timer: normal cancellation cleans owned unpublished
stages, while crash leftovers require explicit, identity-checked cleanup. Unknown
or replaced artifacts are retained. Local verification is bounded to 100,000 tree
nodes and depth 128; the live publication scan observes cancellation.

## Verification

Unit tests launch child processes and terminate them without destructors after
staging, writes, flushes, preparation, backup, destination swap, commit recording
and source removal. They cover sequential and parallel copy paths, copy-based and
native moves, pack and extract. Tests check old data, complete new data and retained
sources directly after reopening the journal. Additional tests cover unsafe direct
writes, queue order/settings, corrupt/future/oversized snapshots, missing sources
and profiles, ownership changes, cleanup refusal and journal capacity. Frontend
checks cover explicit confirmation, blocked actions, ordering, native reattachment
and lost completion events. Native Windows/Linux checks are deferred to 251.
