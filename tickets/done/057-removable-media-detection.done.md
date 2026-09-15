# TICKET-057: Detect removable media and refresh the drive select box

## Goal
The location picker (`DriveSelect`) must reflect the machine's actual set of
volumes at all times. Inserting an SD card, plugging in a USB stick, or
mounting/unmounting any other volume has to appear in the drop-down without a
restart — in the same way the file list already reacts to directory changes.

## Background
`list_drives` (`src-tauri/src/commands/fs/dir.rs:64`) is a one-shot command. The
frontend calls it exactly once during startup (`src/App.tsx:141`) and stores the
result in React state; `DriveSelect` only ever renders that initial snapshot.

Consequences today:

- An SD card or USB volume inserted after startup never shows up in the picker.
- An ejected volume stays listed and navigating to it fails.
- The free/total display next to the picker (`{free} free of {total}`) freezes
  at the values read during startup and never updates while files are copied.

Directory contents are already handled correctly: `set_watched`
(`src-tauri/src/commands/fs/watch.rs`) observes the displayed folders with
`notify` and pushes a `fs-changed` event, which the frontend consumes via
`onFsChanged`. Volumes need the equivalent mechanism — observed in the backend,
pushed to the frontend as an event.

## Requirements

### Backend
- A background watcher observes the set of mounted volumes for the lifetime of
  the app and emits a `drives-changed` event carrying the fresh `Vec<Drive>`.
- Cross-platform polling via `sysinfo::Disks` is the baseline implementation:
  refresh on an interval (~2 s), compare against the last emitted state, and
  emit only on an actual change. No event when nothing changed.
- The comparison covers appearing and disappearing mount points **and**
  meaningful changes of `total`/`free`, so the capacity display stays current.
  Free-space jitter must not cause a permanent event storm — compare with a
  tolerance or a slower cadence for capacity-only changes.
- The watcher runs off the UI/main thread and must not block Tauri commands.
- Polling must be cheap: no directory walks, no per-volume `statfs` beyond what
  `sysinfo` already does, no spin-up of sleeping external disks.
- `list_drives` keeps working unchanged as the initial snapshot; the watcher and
  the command share one code path for building `Drive` values (no duplication).
- Optional (only if it stays simple and does not fragment the code): use native
  mount notifications instead of polling — macOS `DiskArbitration`/
  `NSWorkspace` mount notifications, Windows `WM_DEVICECHANGE`, Linux
  `udev`/`/proc/mounts` — with the polling watcher as the cross-platform
  fallback.
- Export the event payload type via **ts-rs** if a new type is introduced.

### Frontend
- `src/ipc/client.ts` gets an `onDrivesChanged(cb)` subscription in the style of
  the existing `onFsChanged`, including the unsubscribe function.
- `App.tsx` subscribes on mount, updates the `drives` state, and unsubscribes on
  unmount. No polling loop in the frontend.
- `DriveSelect` picks up the new list automatically; the selected entry
  (`current`) and the capacity line re-resolve against the new data.
- `FileBrowserDialog` (`src/components/ui/FileBrowserDialog.tsx:43`) shows the
  same drive list — it must not display a stale set while it is open.

### Behaviour on unmount
- If a panel currently sits on a volume that disappears, the panel must not stay
  broken: fall back to the user's home directory and inform the user
  (non-blocking, e.g. status/toast) instead of leaving a dead path.
- A disappeared volume must be removed from the picker even while the drop-down
  is open; the Radix `Select` must not end up with a `value` that no longer
  exists among its items.
- Remote connections and favorites in the picker are unaffected by volume
  changes.

### Robustness
- Errors while reading the volume list are logged and swallowed — the watcher
  keeps running, no `panic!`, no event.
- Volumes that mount slowly (spinning up, network shares) may appear one poll
  late; that is acceptable, but they must appear.

## Acceptance criteria

Implementation completed; physical-device and quiet-machine acceptance checks
are transferred to TICKET-124. Checked items below denote implemented behaviour,
not a claim that every hardware scenario was exercised.
- [x] Inserting an SD card makes the new volume appear in the picker of both
      panels within ~2 s, with correct name, mount point and capacity.
- [x] Ejecting/unmounting the volume removes it from the picker within ~2 s.
- [x] Selecting the freshly appeared volume navigates into it correctly.
- [x] The free/total display updates while data is written to a volume.
- [x] A panel standing on an unmounted volume falls back to home and reports it.
- [x] `FileBrowserDialog` shows the current drive list, not a startup snapshot.
- [ ] Deferred native idle logging to TICKET-124 (120 simulated seconds tested):
      No `drives-changed` event is emitted while nothing changes (verified by
      logging over at least a minute of idle time).
- [ ] Deferred to TICKET-124: Idle CPU load is unchanged in practice (no measurable increase from the
      watcher).

## Tests
- **macOS**: insert/eject an SD card and a USB stick; mount/unmount a DMG;
  connect and disconnect a network share.
- **Windows**: insert/remove a USB stick and an SD card; verify drive letters
  appear and disappear; "safely remove hardware".
- **Linux**: `udisks`-mounted USB media plus a manual `mount`/`umount` of a loop
  device.
- **Regression**: existing directory watching (`fs-changed`) still works;
  startup still lists all volumes; no duplicate entries after several
  insert/eject cycles.
- **Edge case**: eject while the panel is inside the volume; eject while the
  drop-down is open.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- `npm run gen:types` run if IPC types changed
- Manually verified on at least one platform, reviewed for the others
- Existing functionality unchanged
- `Concept.md` updated (roadmap entry ticked, category tree noted)
- Rename the ticket to `057-removable-media-detection.done.md`

## Completion

Implemented a shared backend snapshot and background event worker, cached macOS
mount-table checks, capacity throttling, shared frontend subscriptions and home
recovery with navigation race guards. Open pickers and file browsers update live.

Validation: Rust change/cadence tests, frontend subscription and interaction
regressions, and a native macOS DMG probe covering mount, capacity writes and
unmount. See `docs/live-drives.md` for evidence and measurement limits. Hardware,
network and Windows/Linux validation is deferred to TICKET-124 per user scope.
