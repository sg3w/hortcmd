# TICKET-382: Let a case-only rename through on case-insensitive filesystems

Priority: **P1**. Status: **open**.
Discovered during a full code review (2026-09-28).

## Problem

Renaming `Readme.txt` to `readme.txt` is refused on macOS and Windows. The
frontend offers it, the backend rejects it, and the user gets a message saying
the name is already taken — by the very file being renamed.

Both rename commands decide "the target is taken" with a plain existence check
against a filesystem that folds case:

- `src-tauri/src/commands/fs/file.rs:73-75` (`rename_entry`):

  ```rust
  if provider.exists(to_p) {
      return Err(AppError::from("Zielname existiert bereits"));
  }
  ```

  `LocalProvider::exists` is `Path::exists` (`src-tauri/src/vfs/local.rs:127-129`),
  so on APFS/NTFS `exists("…/readme.txt")` is `true` while `Readme.txt` is on
  disk. `fs::rename` would perform the rename happily; the guard never lets it.

- `src-tauri/src/commands/fs/file.rs` in `rename_batch_on`, the pre-check
  around lines 137-141 and 176-183:

  ```rust
  let sources: HashSet<&str> = renames.iter().map(|(f, _)| f.as_str()).collect();
  …
  if provider.exists(&target) && !sources.contains(to.as_str()) {
      errors.push(ItemError::new(from, AppError::new(code::FS_EXISTS).with("name", to)));
      continue;
  }
  ```

  `sources` is matched by exact case, so for `("Readme.txt", "readme.txt")` the
  set lookup misses, `exists` hits, and the pair is dropped with `FS_EXISTS`
  before phase 1 ever runs. Phase 2 would have been safe — by then every source
  already carries a `.rcren-tmp-…` name — so the rejection is purely an
  artefact of the pre-check.

This contradicts what the frontend promises. Ticket 359 folded case in
`buildPreview` and its resolution states that "case-only renames of entries
inside the batch keep working: the folded source set still absorbs them"
(`src/features/commander/renameRules.ts:167-170`,
`renameRules.test.ts:222` — *"still allows a case-only rename of an entry in the
batch"*). The preview marks the row `ok`, the dialog submits it, and the backend
answers `fs.exists`. The single-file rename has the same split: `fileOps.ts:610-617`
checks the conflict with `e.name === name`, exact case, so a case-only rename
passes the dialog and fails in `rename_entry`.

Secondary defect in the same place: `rename_entry` reports both of its refusals
as free-text German (`"Quelle existiert nicht"`, `"Zielname existiert bereits"`)
instead of `code::FS_NOT_FOUND` / `code::FS_EXISTS`. The frontend cannot
localize or key off either, and `rename_batch_on` already uses the structured
codes for the same two conditions.

## Required change

- Treat "the target exists" as a collision only when the existing entry is not
  the source itself. Compare identity, not spelling: on a local store the
  cheapest correct test is the one the preflight walker already uses
  (`recovery::durability::rename_identity`, dev/inode or the Windows file id) —
  if `from` and `to` resolve to the same object, it is a rename, not a
  collision.
- Apply the same rule in `rename_batch_on`'s pre-check, or fold the `sources`
  lookup the way `buildPreview` folds it (`ProviderLimits.case_sensitive` says
  when folding is wrong). Phase 2's `provider.exists(target)` check stays as
  the authority; it is already correct because the sources are staged by then.
- Give `rename_entry` the structured codes `FS_NOT_FOUND` and `FS_EXISTS`.

## Verification

- Rust test: create `Readme.txt`, call `rename_entry` with `readme.txt`, assert
  `Ok` and that the directory afterwards holds exactly `readme.txt`. Guard it
  so it is meaningful on a case-sensitive filesystem too (skip, or assert the
  same outcome).
- Rust test: `rename_batch_on` with a single case-only pair succeeds, and a
  real foreign collision (`notes.txt` → `readme.txt` next to an untouched
  `Readme.txt`) still reports `FS_EXISTS`.
- Frontend: the single-rename conflict check in `fileOps.ts` must not diverge
  from `buildPreview` — a case-only rename is submitted, not blocked locally.
- `npm run check`.

## Implementation

Identity decides the collision now, not spelling.

`taken_by_other` (`src-tauri/src/commands/fs/file.rs`) answers "the target is
occupied by something else": it is a collision only if the target exists *and*
is not the same object as the source. `same_object` compares
`Provider::planning_identity` — on the local store that is
`recovery::durability::rename_identity`, the dev/inode token the preflight
walker already uses. Where no identity is available it falls back to a
case-folded comparison of the last components, but only for the local store on
a platform that reports no identity: those are exactly the platforms that fold
case. A remote store keeps the strict answer, because its case rules belong to
the server, not to us.

`rename_entry` uses it, and now answers `code::FS_NOT_FOUND` and
`code::FS_EXISTS` (with the `name` parameter) instead of free-text German, so
the frontend can localize both.

`rename_batch_on` keeps its exact-spelling `sources` set as the cheap path and
adds `taken_by_batch_foreigner`: an existing target that is the same object as
*any* source of the batch is a chain or a case-only rename, not a collision.
That covers the folded cases the exact set misses without having to decide
whether the store folds. Phase 2's `provider.exists(target)` check is unchanged
and remains the authority — by then every source carries a `.rcren-tmp-…` name.

Frontend: `runRename` (`src/features/commander/fileOps.ts`) now folds its local
conflict check the way `buildPreview` does and excludes the entry being
renamed, so a case-only rename is submitted rather than blocked, and a name
held by a *different* entry is reported locally in both spellings.

Not in scope, noticed here: `rename_batch_on` counts duplicate targets by exact
spelling, so a batch renaming two entries to names that differ only in case
(`a.txt`→`X.txt`, `b.txt`→`x.txt`) would publish both on a case-folding store
and the second would replace the first. The frontend preview already rejects
that pair, and this ticket does not change the behaviour either way.

### Verification

- `commands::fs::file::tests::case_only_rename_is_not_a_collision`: `Readme.txt`
  → `readme.txt` through `rename_batch` succeeds and the directory afterwards
  holds exactly `readme.txt` with the original content. Meaningful on a
  case-sensitive filesystem too — there the target simply does not exist.
- `foreign_collision_is_still_reported`: `notes.txt` → `Readme.txt` next to an
  untouched `Readme.txt` reports `fs.exists` and leaves both files alone.
- `rename_entry_allows_case_only_and_codes_its_refusals`: the single-file
  command performs the case-only rename and answers `fs.exists` /
  `fs.not_found` for the two refusals.
- Frontend (`fileOpsErrors.test.ts`): a case-only rename reaches
  `rename_entry`; `README.TXT` while another entry is `Readme.txt` is reported
  locally and never submitted.
- `npm run check` passed on macOS (arm64).
