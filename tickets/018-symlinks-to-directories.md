# TICKET-018: Symlinks to directories are not treated as folders

## Goal
A symlink that points to a directory must behave like a directory in the file
list: sorted with the folders, no bogus size, and navigable with `Enter` — while
still being recognisable as a link.

## Analysis
Symlinks are **not** missing from the listing — they are listed, but classified
as files. Verified against the real `std::fs` behaviour used by
`LocalProvider::list` (`src-tauri/src/vfs/local.rs:35-63`):

```
file.txt       is_dir=false  is_symlink=false  len=3
broken_link    is_dir=false  is_symlink=true   len=7
link_to_dir    is_dir=false  is_symlink=true   len=4
link_to_file   is_dir=false  is_symlink=true   len=8
real           is_dir=true   is_symlink=false  len=64
```

`std::fs::DirEntry::metadata()` does not traverse symlinks — it is `lstat`. The
provider fills `is_dir: meta.is_dir()` (`local.rs:55`) from that, so **every**
symlink gets `is_dir = false`, including `link_to_dir`. `size` comes from
`meta.len()` on the same metadata, which is the byte length of the link target
string (4, 8, 7 above) — a meaningless number shown in the size column.

Three visible consequences:

1. **Not in the folder list.** `sortEntries` groups by `is_dir`
   (`src/store/panesStore.ts:148`), so with "folders first" a symlinked folder
   sorts among the files, away from the directories it belongs with.
2. **Not navigable.** `openEntry` branches on `entry.is_dir`
   (`src/features/commander/navigate.ts:37`); with it false, `Enter` on a
   symlinked folder falls through to `openPath` and hands the link to the
   default application instead of entering it.
3. **Wrong size.** The size column shows the link-target string length.

Broken links are listed fine — `DirEntry::metadata()` succeeds via `lstat`, so
the `Err(_) => continue` arm (`local.rs:38`) does not drop them.

The same applies to the SFTP provider: `attrs.is_dir()` on SFTP `READDIR`
attributes is likewise link-level (`src-tauri/src/vfs/sftp.rs:113`). FTP reports
`is_symlink` (`ftp.rs:165`); SMB and WebDAV hardcode `is_symlink: false`
(`smb.rs:191,247`, `webdav.rs:187`).

## Requirements

### Backend — local provider
- Resolve the link target for entries where `file_type().is_symlink()` is true,
  and report `is_dir` from the **target**: one additional `fs::metadata()` per
  symlink, not per entry.
- `is_symlink` keeps reporting the link itself — the frontend needs both facts
  (folder behaviour *and* link appearance).
- `size`: 0 for a symlink to a directory, the target's size for a symlink to a
  file. Never the link-target string length.
- **Broken links stay listed.** When the target cannot be stat'd, fall back to
  the link's own metadata (`is_dir = false`, `size = 0`) — a dangling symlink
  must remain visible in the list rather than disappearing or erroring.
- No extra syscall for non-symlink entries; large directories must not get
  measurably slower.
- Idiomatic `Result` handling, no `panic!`, per CLAUDE.md.

### Backend — remote providers
- SFTP: same treatment where the protocol allows it — resolve via `stat` (which
  follows links) for entries flagged as symlinks, with the same broken-link
  fallback. Weigh the extra round trip: if it is too costly for large remote
  directories, resolve lazily or document the limitation in the ticket's
  implementation notes rather than silently shipping the wrong `is_dir`.
- FTP/SMB/WebDAV: no change required; note in the code why (`is_symlink` is not
  available or not meaningful for those protocols).

### Frontend
- A symlink to a directory sorts with the folders and is entered by `Enter`,
  `openEntry` needing no change once `is_dir` is correct.
- It stays visually distinguishable as a link: the existing symlink colour rule
  (`src/lib/fileColors.ts:128`) and the `l` mode character
  (`src/lib/format.ts:56`) must still apply.
- The size column shows nothing (or `<DIR>`-equivalent) for a symlinked folder,
  consistent with real folders.
- No file-system logic moves into the frontend.

### Navigation semantics — decide and document
`LocalProvider::list` canonicalises the directory path
(`local.rs:29`), so entering a symlinked folder shows the **resolved** path, and
`..` then leads to the target's real parent rather than back through the link.
That is loop-safe and matches how the rest of the app resolves paths.

- Keep this behaviour and state it explicitly in the code comment, **or**
  preserve the link path in the breadcrumb — but do not leave it undecided.
- Whichever is chosen, navigating a symlink cycle (`a/link → a`) must not hang
  or recurse.

### Related, deliberately unchanged
- Folder-size calculation skips symlinks (`src-tauri/src/commands/fs/size.rs:141`)
  and directory comparison does the same (`compare.rs:58`) — both prevent
  infinite recursion and stay as they are. Confirm the changed `is_dir` does not
  push symlinks into either traversal.

## Acceptance criteria
- [ ] A symlink to a directory sorts with the folders under "folders first".
- [ ] `Enter` on it navigates into the target instead of launching an app.
- [ ] Its size column shows no byte count.
- [ ] It is still visibly a link (colour rule, `l` in the permissions column).
- [ ] A symlink to a file keeps `is_dir = false` and shows the **target's** size.
- [ ] A broken symlink is still listed, shows as a file, size 0, and produces a
      readable error when opened — it never vanishes from the list.
- [ ] A symlink cycle does not hang the app or the folder-size calculation.
- [ ] Listing a large directory (10k+ entries, no symlinks) is no slower than
      before.
- [ ] SFTP: a symlinked remote directory behaves like the local case, or the
      limitation is documented.

## Tests
- **macOS/Linux**: directory with a symlink to a folder, to a file, a broken
  link, a cycle, and a relative vs. absolute link — checked in both panels, with
  "folders first" on and off, and with each sort column.
- **Windows**: directory symlinks and junctions — verify `is_dir` and that
  navigation works; note in the implementation which of the two Windows reports
  as a symlink.
- **Regression**: folder size (TICKET-010) on a tree containing symlinks still
  terminates and still skips them; directory comparison unchanged; copy/move of
  a symlink behaves as before.
- **Regression**: `..`, breadcrumbs and the history behave sanely after entering
  a symlinked folder.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- `npm run gen:types` run if `DirEntry` changed
- Existing functionality unchanged
- `Concept.md` updated (roadmap entry ticked, category tree noted)
- Rename the ticket to `018-symlinks-to-directories.done.md`
