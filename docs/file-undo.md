# File undo

## Beta, and off by default

File undo is a beta feature and ships **off** — for new installations and for
existing ones alike (TICKET-391). It is switched on in **Einstellungen →
Dateioperationen → Datei-Undo**, where it carries a BETA badge, and the page
states what being on costs: every rename, directory creation, metadata change
and transfer writes a journal entry, and undoing a copy retains the copied bytes
instead of deleting them.

The backend holds the same switch and also defaults to off, so nothing is
recorded before the frontend has pushed the setting — which matters for a
browser build, a renderer that died early, and for plugin or MCP clients driving
a file command. While the switch is off, `prepare`, `prepare_group` and `seal`
record nothing, `undo_classify` answers `Irreversible`, `undo_preview` is empty
and `undo_execute`/`undo_forget` refuse; the menu command and its ⌘⌥Z shortcut
are gone and the operation dialogs print no reversibility sentence at all.

Switching it back off never deletes an existing journal or a retained copy. Both
stay on disk and become reachable again when the switch is turned back on.

## Where it lives

File undo lives in **Dateien → Dateiaktion rückgängig machen**. The shortcut is
**Cmd+Option+Z** on macOS; native Edit-menu Cmd+Z continues editing text. File
shortcuts never operate through an input, contenteditable element, dialog or menu.
Opening the history reads a fresh preview; it does not change files. The next
eligible item, original/destination paths, group membership and retained copies
are displayed before execution.

## Supported contract

The current adapter supports local macOS operations. Every operation remains
**conditionally reversible** because another process can change its result.
`undo_classify` is a read-only assessment, not execution permission. Copy/move
confirmation and rename/create-directory prompts display the assessment before
submission; metadata and multi-rename dialogs describe the conditional contract.

- Rename and same-volume move reverse the namespace operation. Atomic Darwin
  `RENAME_EXCL` refuses occupied names, including a racing destination creator.
- Copy undo removes the destination name by moving the verified result to a
  `.hortcmd-undo-*` sibling. It deliberately **retains the bytes**: another process
  may still hold a writable descriptor. The history shows this path; inspect and
  remove retained data explicitly through ordinary file operations when desired.
- A newly created directory can be removed only while still empty and matching
  its recorded identity, contents and modification time. Recursive creation with
  a missing parent is not eligible.
- Permissions, ownership and Finder tags capture previous metadata. Inverses act
  on an opened descriptor whose device/inode match the committed object.
- Multi-renames and transfers have a shared group. Inverse swaps first stage all
  participants. Nothing is published if staging fails. Per-item results identify
  partial restoration and retained paths; a group is not advertised as atomic.
  Running groups cannot be undone before their original operation finishes.

Permanent deletion, replacement without a retained backup, cross-volume moves,
archives and unsupported providers receive no undo entry. The macOS trash API
currently used by hortcmd does not expose a stable restore token, so trash uses
system restoration and is explicitly excluded from file undo. Remote providers
lack the required stable identity/no-replace inverse contract and are classified
irreversible; no reconnect or changed trust identity can revive a local plan.
Windows/Linux adapters are tracked in [TICKET-119](../tickets/119-file-undo-windows-linux-adapters.md).
Redo is not offered.

## Durability, bounds and privacy

`history.json` in the application configuration directory’s `undo-v1` subdirectory stores a
versioned `UndoPlan` before the original mutation. Only a completed, verified plan
is eligible. Plans capture inverse kind/paths, store device and inode, expected
content digest and modification time, previous permissions/ownership/tags,
group, creation time, state and any retained inverse staging path. No credentials
or file contents are stored. Local filenames and tag values are private metadata.
The directory is 0700 and files 0600, with a process-exclusive lock, atomic
replacement and fsync. Corrupt/unknown journals grant no execution authority.

The limit is 100 entries, 4 MiB of journal metadata, and seven days of eligibility.
Tree verification is capped at 100,000 nodes and depth 128. Incomplete operations
and inverse staging locations are never guessed safe. On restart they are shown
as interrupted, requiring manual inspection. Retained data is never automatically
deleted, even after expiry. Protected records may exhaust the limit; the original
operation then stops before its affected item's mutation. A history-only remove
button deletes no files and refuses active entries or existing retained data.
After checking/removing a retained copy, its history entry can be removed.

Undo rechecks identities, content and destination occupancy at execution. Names
and paths can still change concurrently; failed rollback retains and reports data
instead of overwriting a conflicting name. No application can stop another
process from writing through an already-open descriptor; retained copy data is
therefore intentional. File changes after a successful inverse remain ordinary
external changes, not an undo guarantee.

## Validation

Local tests cover restart, corruption/locking, identity replacement, changed
content, collisions, grouped swaps, empty/nonempty directories, permissions,
owner/tags, unsupported/overwriting operations, read-only assessment, retention
and interrupted state. A gate test covers the off state across platforms: no
pending entry, an empty preview, refused execution and an `Irreversible`
assessment. UI tests cover DE/EN preview and confirmation, rejected changed
entries, the actual selected operation, text-focus shortcut isolation, and the
beta gate — no shortcut, no menu entry, no classification IPC while off.
The full project gate is `npm run check`.
