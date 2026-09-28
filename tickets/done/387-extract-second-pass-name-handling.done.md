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

## Implementation

Both passes of every format now derive the relative name the same way, through
`match_target`.

**ZIP.** `extract_zip_one` takes `all` and `targets` and runs the name it read
from the *reopened* archive through `match_target` instead of slicing it at
`base.len()`. A member that no longer belongs to the chosen level — because the
file on disk changed between the two opens — becomes
`ARCHIVE_CORRUPT`/`extract-changed`, the same answer `manifest.verify` gives for
the neighbouring case, and is reported as an item error rather than panicking.
The unchecked slice could panic two ways, out of range for a name shorter than
`base` and on a char boundary for one sharing a multi-byte prefix; neither is
reachable any more. The function is `pub(super)` so the mismatch can be
exercised directly, which is the only way to reach it: the swap happens between
two opens inside `extract_zip`.

**7z.** Pass 1 no longer normalizes separators. `safety::path` refuses any
member whose name contains a backslash during preflight, so such a member never
reaches either pass — normalizing in pass 1 could only make the two disagree,
and the dead `replace` suggested backslash names were handled. The comment
records where they are actually validated.

**tar** was already consistent — it re-derives the name per entry in both
passes — and is now covered by the same test as the other formats.

### Verification

- `archive::tests::a_zip_member_outside_the_chosen_level_is_an_error_not_a_panic`:
  `extract_zip_one` is called with `base = "approved/"` and the indices of
  `other/outside.txt` and `ä.txt` — a member in another level and one shorter
  than `base` beginning with a multi-byte character, both of which the old
  slice panicked on. Both answer `archive.corrupt` / `extract-changed`, the
  member that does belong to the level still extracts, and the destination ends
  up holding exactly that one file.
- `both_extraction_passes_select_the_same_members`: for zip, tar, tar.gz and
  7z, with the whole archive and with a single selected subfolder, pass 1's
  `files_total` equals the number of members pass 2 wrote.
- `a_backslash_member_is_refused_during_preflight`: a zip member named
  `dir\file.txt` is rejected with `archive.unsafeEntry` before extraction —
  the reason the normalization could be dropped rather than duplicated.
- `npm run check` passed on macOS (arm64).
