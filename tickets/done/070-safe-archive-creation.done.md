# TICKET-070: Preserve existing archives when creation fails

## Finding
While connecting 041's create_archive tool, the existing packer was found to
truncate the destination before packing, remove it on cancellation, ignore
unreadable directory entries, and follow directory symlinks recursively. A failed
or cancelled replacement can lose the previous archive.

## Acceptance
- Share one packing service between the GUI and MCP.
- Collect entries without following links or ignoring errors; preserve empty
  directories and reject ambiguous/duplicate archive names.
- Build in private staging and publish only after ZIP finalization succeeds;
  keep the existing destination on error or cancellation.
- Honor cancellation inside large files; MCP checks every source and publication.
- Regression tests and npm run check pass; local ticket-only commit, no push.

Urgent prerequisite for 041's archive creation tool.

## Completed — 2026-09-13
Shared `archive::pack::run` now collects validated entries, includes empty
directories, honors pause/cancel inside files, and writes a private staged ZIP.
It finishes and syncs the archive before publication through the existing
backup/rollback service. The GUI delegates to it; the 041 MCP adapter supplies
its path-policy callback. Unreadable entries and links are explicit failures.

Verification: `npm run check` passed on macOS (364 Rust tests passed, 22 ignored;
366 frontend tests passed; all remaining gates passed). Four new packing tests
cover round trip/empty directories, mid-file cancellation retaining the old
archive, duplicate names, and a directory-link cycle. MCP integration additionally
checks archive creation and denial of an out-of-scope source. Native Windows and
Linux runtime checks were not available locally; staging uses the already shared
platform implementation from 046. The commit isolates the shared service/GUI fix;
MCP adapter changes remain with 041.
