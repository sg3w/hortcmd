# Paired directory comparison (064)

The existing dialog now renders one shared virtual grid: selection, left relative
path/size/date, status, right date/size/relative path. Its two sticky header rows
hold copyable root addresses and side-specific column names. One horizontal and
vertical table viewport keeps pairs aligned; narrow/short windows can also scroll
the form. Sorting operates on whole relative paths. Identity includes run + path.
Arrow keys/Home/End move the pair cursor; Space toggles the whole pair.

This compares local files by size and modification time, not file contents.
Missing sides show dashes. Missing timestamps and unrecognized statuses show an
unknown-metadata label, never equality. Read errors now propagate through both
the Tauri and MCP entry points instead of silently looking like empty directories.
A genuinely missing subtree on one side remains a supported one-sided comparison.
Symlinks remain outside this comparison method.

Filters remove hidden selection. Copy counts exclude absent source files and the
buttons also check current capabilities. Copying is explicit in either direction;
no mirroring/deletion is introduced. The existing overwrite behavior remains.
Actions recheck capabilities and run identity immediately before dispatch, prevent
duplicate dispatch, and compare again after completion. Structured batch errors
use the central error formatter. Same-root actions are disabled; the backend also
rejects canonical aliases and Unix hardlinks to avoid truncating a source file.

Tests cover pair cells, missing metadata, filters/selection, both directions,
errors/limits, capability loss, stale streams, keyboard access, and self-copy.
Browser verification used the existing demo's 1,507 pairs (1,131 visible with the
default filters), including scrolling to the final pair, narrow/wide windows,
reopening, and dark/German plus light/English rendering. At 640×360 effective
viewport size the form scrolls and the grid remains reachable. Actual native
200% zoom and Windows/Linux verification remain in TICKET-263.
