# TICKET-395: Packing is bounded by the extraction limits, and says so unhelpfully

Priority: **P1**. Status: **done**.
Reported from use on 2026-09-29, packing seven project folders from
`/Volumes/Samsung_T5/PhpStormProjectsArchiv` into one ZIP: the operation fails
immediately at `0 B / 0 B` with "Sicherheitsgrenze für Archive überschritten",
and nothing says which grenze, what the value was, or what to do about it.

## Problem

### 1. The hostile-archive limits are applied to the user's own files

`safety::Limits` (`src-tauri/src/commands/fs/archive/safety.rs:19-29`) exists to
defend extraction against an archive somebody else wrote: an entry count, a
total size, a per-entry size and a compression ratio that a zip bomb would blow
through. `pack::run_with_recovery` is handed the *same* `Limits::load()`
(`src-tauri/src/commands/fs/archive.rs:394`, `src-tauri/src/mcp/transfers.rs:177`)
and runs every collected entry through `Manifest::add`, so creating an archive
is bounded as though the user's own directory tree were an attack on the app.

The reported selection is an ordinary one:

| bound | limit | selection |
| --- | --- | --- |
| `entries` | 100,000 | 264,689 |
| `total_bytes` | 20 GiB | ~44 GiB |

Two limits exceeded, neither of them a defence against anything: the bytes are
read from a local disk and written to a local disk, where the real bound is free
space — and that is already checked, separately, by
`transfer::preflight::archive_capacity`. A per-entry cap of 4 GiB likewise
refuses a single large file the filesystem holds and ZIP64 can store. The effect
is that the ZIP command is unusable for exactly what a file manager packs:
project archives, photo libraries, backups.

What packing *does* need a bound for is memory, not safety: the collection pass
holds one `entries` tuple and one `Manifest` node per path component before a
byte is written. That is a reason for a large ceiling, not for 100,000.

### 2. The refusal does not name the limit

`safety::limit()` builds a bare `ARCHIVE_LIMIT` with no parameters, and
`error.archive.limit` renders one sentence for all six bounds. A user who hits
it learns nothing: not which limit, not the value that exceeded it, not that
`archive-limits.json` can raise it. Diagnostics get no more — the error carries
no `detail` either.

### 3. The pack dialog buries the field the user came for

In `PackDialog` the name input sits below the base select and the date/time
checkboxes (`src/components/ui/PackDialog.tsx:117-160`). The field is what the
dialog focuses and pre-selects on opening, and in the common case the only thing
touched — so the eye starts at the bottom of the dialog and the options sit
above the thing they compose.

## Required change

- Packing gets its own limits, separate from the extraction ones and configurable
  the same way: entry count and byte ceilings sized for a real local tree rather
  than for a hostile archive, while the per-name rules that keep a ZIP member
  portable — depth, path length, reserved and ambiguous names — stay exactly as
  they are and stay shared.
- The pack ceilings are a memory bound, and the comment at them says so: they
  exist because collection is held in RAM, not because a local directory is
  suspect. Free space stays the business of `archive_capacity`.
- `archive-limits.json` keeps working unchanged. A file written before this
  ticket still parses, and the extraction limits in it keep their meaning.
- An exceeded limit names itself: which bound, and the value it was measured
  against, in the message the user reads and in the diagnostics. Extraction
  refusals gain the same naming — it is the same error. The numeric value is
  what the user reads; the exact bound stays in the diagnostics.
- The archive-name field is the first control in the pack dialog, above the
  switches that compose it, with focus and the pre-selected stem unchanged.

## Verification

- Backend test: the pack limits admit a tree that the extraction limits refuse,
  and a pack still refuses past its own ceiling.
- Backend test: `archive-limits.json` without the new keys loads with the
  extraction limits it names and the default pack ceilings.
- Backend test: the limit error carries which bound was exceeded and its value.
- Frontend test: the limit message renders the named bound; the pack dialog
  renders the name field before the option controls.
- `npm run check`.

## Implementation

`Limits` grew three packing ceilings (`pack_entries`, `pack_total_bytes`,
`pack_entry_bytes`) and a `for_packing()` that swaps them in for the extraction
three. The two pack call sites — `archive.rs` and `mcp/transfers.rs` — apply it;
`pack::run` still takes whatever limits it is handed, so the tests that bind a
pack against a deliberately tiny ceiling keep working.

- **Three fields, not a second struct.** The per-name rules are the same rules
  in both directions and there is nothing to gain from duplicating them; only
  the three ceilings differ, and `..self` says so in one line. A file written
  before this ticket parses unchanged, because the container already carries
  `serde(default)`.
- **A million entries, not "no limit".** The collection pass holds every entry
  and one manifest node per path component in memory before the first byte is
  written, so the ceiling is real — it is just a memory bound rather than a
  defence, and the comment at it says which. The reported selection needs a
  quarter of it.
- **Two codes rather than a parameter on `archive.limit`.** The error contract
  renders one sentence per code and leaves a missing placeholder visible, so a
  parameterized shared message would read as `{limit}` for the decoder, ratio
  and path refusals that have no number. `archive.tooManyEntries` and
  `archive.tooLarge` carry one; `archive.limit` keeps the cases that are about
  a hostile input rather than a ceiling the user chose (a lying member size,
  the compression ratio, a name too deep or too long, the decoder budget).
- **Sizes are formatted where the number is produced.** `human_size` moved from
  `search.rs` up to `commands/fs`, so "4.0 TB" reaches the message rather than
  4398046511104.
- `pack::fatal` recognizes both new codes — a ceiling still ends the archive
  instead of being skipped past, which is what ticket 388 left it for.

## Verification

- Backend: `packing_uses_its_own_ceilings_and_names_the_one_it_refuses` packs a
  tree the extraction limits refuse and then hits the pack ceiling, which names
  itself; `a_limits_file_without_the_pack_ceilings_keeps_its_meaning` and
  `count_and_size_refusals_carry_their_bound` in `safety.rs`.
- Frontend: the limit messages render their bound (`errorText.test.ts`), and the
  name field precedes the options (`PackDialog.test.tsx`).
- `npm run check`: green except `check:bundle`, which is **already red on
  `main`** — the entry chunk is 325.1 kB against a 325 kB budget, unchanged by
  this ticket (verified by running it on a stashed tree).
