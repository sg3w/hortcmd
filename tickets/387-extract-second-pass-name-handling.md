# TICKET-387: Derive extraction names the same way in both passes

Priority: **P3**. Status: **open**.
Discovered during a full code review (2026-09-28).

## Problem

Extraction reads every archive twice — pass 1 selects the members and sums
the totals, pass 2 writes them. The two passes derive the relative target
name differently, in two formats.

### ZIP: an unchecked slice can panic

`match_target` is the one function that decides whether a member is inside
the chosen level, and it checks the prefix before slicing
(`src-tauri/src/commands/fs/archive/extract.rs:15-34`):

```rust
if !name.starts_with(base) { return None; }
let rel = &name[base.len()..];
```

`extract_zip_one` does the slice without the check
(`extract.rs:237`):

```rust
let rel = &name[base.len()..];
```

`name` there comes from a **different** `File` handle than the one pass 1
read: `extract_zip` opens the archive, collects `indices`
(`extract.rs:162-188`), then reopens it and indexes the new handle with those
same numbers (`extract.rs:195-212`). If the file on disk is replaced between
the two opens, index `i` can name a member that does not start with `base`,
or one shorter than `base`, and the slice panics — out of range, or on a
non-UTF-8 char boundary for a member whose name shares a multi-byte prefix
with `base`. `manifest.verify` (`extract.rs:229-234`) does not prevent this:
it only asserts the member exists in the manifest with the same size, and the
manifest holds every member, including ones outside `base`.

A panic in the extraction thread is not a data-loss risk — staging means the
destination is untouched — but it is an unhandled crash on input the code
already assumes can change (that assumption is exactly why `manifest.verify`
exists).

### 7z: pass 1 normalizes separators, pass 2 does not

`extract_7z` pass 1 matches against a normalized name
(`extract.rs:344-348`):

```rust
let name = f.name.replace('\\', "/");
if match_target(&name, base, all, targets).is_some() { … }
```

Pass 2 matches against the raw one (`extract.rs:369-373`):

```rust
let name = entry.name.clone();
let rel = match match_target(&name, base, all, targets) { … };
```

The totals and the extraction therefore disagree for any member using
backslash separators, which would leave `prog.files_done != prog.files_total`
and surface as `ARCHIVE_CORRUPT`/`extract-incomplete` (`extract.rs:475-480`)
rather than as the real cause. Today the mismatch is unreachable — such a
member is rejected in `preflight` by `safety::path`, which refuses any name
containing `\` (`archive/safety.rs:80-86`) — so this is a latent
inconsistency, not a live failure. It should not be left as a trap for
whoever relaxes that rule, and the dead `replace` should not suggest
backslash names are handled.

## Required change

- Have `extract_zip_one` derive `rel` through `match_target` (or at minimum
  `name.get(base.len()..)` with a structured error), so a member that no
  longer matches the selection becomes an item error instead of a panic.
  `extract_zip` already has `all` and `targets` to hand them down.
- Make `extract_7z` use one spelling in both passes. Either normalize in
  both, or — matching what `safety::path` actually permits — drop the
  `replace` in pass 1 and note that separators are validated during
  preflight.
- Check `extract_tar` for the same split; it re-derives the name per entry in
  both passes and looks consistent, but it should be covered by the same
  test.

## Verification

- Rust test: an archive whose central directory is swapped between the two
  opens (or a direct `extract_zip_one` call with an index whose name is
  outside `base`) produces an item error, not a panic.
- Rust test: 7z pass 1 and pass 2 select the same member set for the same
  `base` and `targets`.
- `npm run check`.
