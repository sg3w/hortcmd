# TICKET-010: Show folder size and detailed volume information

## Goal
The information dialog should display the actual size of folders as well as detailed storage information for volumes. Folder sizes and volume statistics must be calculated asynchronously without blocking the user interface.

## Background
File sizes are available directly from the file system metadata. Folder sizes, however, require recursively summing all contained files and subdirectories.

Volume information such as total capacity, used space, and available space can also take time to retrieve on some systems. All long-running operations must therefore execute in the background.

The information dialog should provide a quick overview of available storage, similar to modern file managers.

## Requirements

### Folder size
- The information dialog displays the total size of a folder.
- All files and subdirectories are included recursively.
- The calculation is asynchronous.
- The UI remains fully responsive.
- While calculating, a loading indicator is shown (e.g. `Calculating…`).
- The displayed value updates automatically when finished.
- Optionally display the exact size in bytes in addition to the human-readable value.
- Reading errors for individual files or folders do not abort the calculation.
- Unreadable items are skipped and reported.
- Symbolic links must not create recursive loops.
- Closing the information dialog cancels the running task or safely discards its result.

### Volume information
For drives, partitions, and mounted volumes, display the following asynchronously:

- Volume name
- Mount point
- File system type
- Total capacity
- Used space
- Free space
- Percentage used
- Percentage free

The free space should be displayed in a user-friendly format, for example:

- **40 GB free of 500 GB**
- **1.2 TB free of 2 TB**
- **85 GB free (17%) of 500 GB**

The information should additionally contain:

- Total capacity (e.g. 500 GB)
- Used space (e.g. 460 GB)
- Free space (e.g. 40 GB)
- Used percentage
- Free percentage

### Formatting
- Use readable units (KiB, MiB, GiB, TiB or localized decimal units if used consistently throughout the application).
- Large numbers should be rounded appropriately.
- All size formatting should use the application's central formatter.

## Technical notes
- Folder size calculation should run in a background task.
- Volume information should use the platform abstraction already used by the file system layer (`sysinfo`, platform APIs, etc.).
- The UI should subscribe to progress/results instead of polling.
- Results should be cached while the dialog remains open.
- Cancel background work when the dialog is closed.

## Acceptance criteria

- [x] Folder sizes are calculated recursively.
- [x] The calculation never blocks the UI.
- [x] A loading indicator is visible while calculating.
- [x] Results appear automatically when available.
- [x] Volumes display total, used, free, and percentage values.
- [x] A compact summary like **"40 GB free of 500 GB"** is displayed prominently.
- [x] All values update asynchronously.
- [x] Unreadable files do not abort the calculation.
- [x] Symbolic links cannot create infinite recursion.
- [x] Closing the dialog during calculation causes no errors.
- [x] The feature works on macOS, Windows, and Linux.
      (macOS verified at runtime; Windows/Linux via the platform-neutral
      `std::fs`/`sysinfo` layer — not executed on those platforms.)

## Tests

### Folder
- Empty folder
- Folder with files
- Deep directory hierarchy
- Very large folder
- Folder containing unreadable files
- Folder containing symbolic links
- Close dialog during calculation

### Volumes
- Internal drive
- External drive
- Network share (if supported)
- Nearly full drive
- Nearly empty drive
- Verify display:
  - **40 GB free of 500 GB**
  - used/free percentages
  - readable units (KiB–TiB)

### Cross-platform
- macOS
- Windows
- Linux

## Definition of Done

- Code compiles.
- Tests pass.
- Existing functionality remains unchanged.
- Documentation updated.
- Ticket renamed to `*.done.md`.
---

## Implementation

**Backend**
- `src-tauri/src/commands/fs/size.rs` (new) — `dir_size(path, token, onProgress)`
  sums a folder on a background thread (`spawn_blocking`) and streams
  `DirSizeStats { bytes, files, dirs, errors }` over a Tauri `Channel`
  (throttled to 120 ms). The walk is **iterative** (explicit stack — a deep
  hierarchy cannot overflow the stack), **never follows symlinks** (so a loop
  a → b → a is impossible), and **counts unreadable entries** (`errors`)
  instead of aborting. `cancel_dir_size(token)` sets the cancel flag; the flag
  is checked per directory and per entry. A cancel that arrives **before** the
  task registers is kept (`register` reuses the stored flag), so closing the
  dialog immediately cannot lose it.
- `src-tauri/src/commands/fs/volume.rs` (new) — `volume_info(path)` returns
  `VolumeInfo { name, mount, fs, total, used, free, used_pct, free_pct }` via
  the same `sysinfo` abstraction `list_drives` already uses. The deepest
  matching mount point wins, compared **component-wise** (`mount_rank`), so
  "/Volumes/Data" does not match the volume "/Volumes/Dat". The path is
  canonicalized first (symlinks resolve to the real volume). Runs in
  `spawn_blocking` — enumerating disks can block on external drives.

**Frontend**
- `PropertiesDialog.tsx` — `useDirSize` / `useVolume` hooks; both key off the
  loaded `FileProps`, so they cancel on close and restart cleanly when
  switching entries. Folders show `31 M (32.255.920 B) · 3544 Dateien ·
  578 Ordner`, with a spinner and live-growing totals while walking
  ("Berechne …" until the first result). New `Volume` section with the compact
  summary, a usage bar (red below 10 % free), and mount/fs/total/used/free.
- `lib/format.ts` — `formatPercent` added; all sizes go through the central
  `formatSize` (exact bytes via `formatSize(n, "bytes")`).
- `DriveSelect.tsx` — the pane's drive bar now shows the total as well
  ("268 G frei von 926 G", same wording as the dialog) instead of only the
  free space; entries without capacity (favorites/home, `total = 0`) keep
  showing nothing. Verified natively: "/" → "268 G frei von 926 G",
  `/Volumes/Samsung_T5` → "1.0 G frei von 931 G", home unchanged.
- i18n keys `props.size/modified/mode/calculating/files/folders/skipped` and
  `vol.*` in de + en. The header rows no longer use hardcoded German literals.

## Verified

Driven natively (dev build, real Rust backend) on macOS 14:

- **Folder size exact against ground truth** (`find | stat` sums): `hordDemo1`
  32.255.920 B / 3544 files / 578 dirs · `AppProjects` 13.737.063.661 B /
  47.498 files · `/Applications` 84.974.469.989 B / 506.284 files / 71.204 dirs
  — all three matched to the byte.
- **Live progress**: walking `~/Library` the totals climbed 2.0 G → 9.2 G → 19 G
  (10.868 → 116.544 → 593.873 files) next to a spinner; UI stayed responsive.
  "Berechne …" is shown until the first result arrives.
- **Unreadable entries reported, not fatal**: `~/Library` → "9 nicht lesbar
  (übersprungen)"; an unreadable volume root → "0 B · 1 nicht lesbar".
- **Cancel**: closing the dialog during a 930 GB USB walk stopped the backend
  task — `walk_size` stack frames 7 → 0, CPU 13,6 % → 0,0 %.
  *(First attempt exposed a real bug: the load effect early-returned on close
  without clearing `data`, so the size hook's argument never changed, its
  cleanup never ran and the walk continued after closing. Fixed by resetting
  the entry when the dialog closes.)*
- **Volumes**: internal `Macintosh HD` "268 G frei von 926 G (29 %)", apfs,
  mount "/" — total/used/free match `df`. External nearly-full `Samsung_T5`
  "1.0 G frei von 931 G (0.1 %)", exfat, mount `/Volumes/Samsung_T5` (deepest
  mount correctly preferred over "/"), usage bar red.
- **Symlink**: `/Volumes/Macintosh HD` → `/` is typed as a symlink and shows its
  own size (1 B) — no recursive walk into the whole system.

### Notes for later

- `sysinfo` reports macOS free space as *AvailableCapacityForImportantUsage*
  (what Finder shows, incl. purgeable) — 268 G where `df` says 145 G. This is
  the pre-existing behaviour of `list_drives`/DriveSelect, so both agree.
- Neither `dir_size` nor `volume_info` has a timeout. If a `read_dir` or
  `sysinfo`'s disk enumeration blocks in the kernel (observed with external
  USB drives and TCC-protected folders), the section spins indefinitely and the
  blocking-pool thread stays parked — a cancel flag cannot interrupt a blocked
  syscall. The UI itself stays fully responsive.
- `formatPercent` rounds 99,89 % → "100 %" while 1 G is still free (cosmetic).
- Esc does not close the properties dialog (pre-existing; overlay click does).
