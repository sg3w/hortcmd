# TICKET-384: Keep a recursive compare alive when one subdirectory cannot be opened

Priority: **P2**. Status: **open**.
Discovered during a full code review (2026-09-28).

## Problem

A single unreadable subdirectory throws away the entire directory comparison,
including everything already streamed to the dialog.

`read_level` tolerates exactly one failure — `NotFound`, and only below the
root (`src-tauri/src/commands/fs/compare.rs:53-62`):

```rust
let rd = match fs::read_dir(dir) {
    Ok(rd) => rd,
    Err(e) if missing_ok && e.kind() == std::io::ErrorKind::NotFound => {
        return Ok((BTreeMap::new(), BTreeSet::new()))
    }
    Err(e) => return Err(e.into()),
};
```

`walk_compare` calls it per level with `?` (`compare.rs:130-131`), so a
`PermissionDenied` on any subdirectory — `~/Library`, a root-owned build
output, a mounted share that went away mid-walk — propagates out of
`walk_compare`, out of `compare_dirs`, and lands in the dialog's `catch`,
which clears the list it has been filling (`src/components/ui/CompareDialog.tsx:112-118`):

```ts
} catch (e) {
  if (runToken.current === token) {
    setEntries([]);
    setSelected(new Set());
    setError(describeErrorFull(e));
  }
}
```

The user sees an error and an empty list after a walk that may have taken
minutes and was correct for every other directory.

Ticket 326 fixed the neighbouring case — a single *entry* whose `metadata()`
fails no longer aborts the walk — and recorded that "root-level open errors
still propagate", which is right. A *subdirectory* open error is neither
root-level nor per-entry, and it was not covered. The sibling local walker
already treats unreadable directories as counted, never fatal
(`src-tauri/src/commands/fs/walk.rs:19`).

## Required change

- In `walk_compare`, treat a failure to open a non-root level the way
  `read_entries` treats a failed entry: skip that level and carry on, rather
  than propagating. Only the two roots keep hard-failing — those are what the
  user explicitly asked to compare.
- Make the skip visible instead of silent: report it so the result is not
  mistaken for "these directories are identical". The streaming channel
  already carries per-item data; an `unknown`-status entry for the directory,
  or a counter surfaced alongside the existing `truncated` flag, both fit the
  dialog.
- While in `CompareDialog.run`: a late failure should not discard batches that
  already arrived. Keep the entries and show the error beside them.

## Verification

- Rust test (unix): compare two trees where one subdirectory is `chmod 000`;
  assert `walk_compare` returns `Ok`, that the readable siblings are in the
  result, and that the unreadable level is reported rather than silently
  absent.
- Frontend test: a rejected `compareDirs` after batches have been delivered
  keeps the delivered rows and shows the error.
- `npm run check`.
