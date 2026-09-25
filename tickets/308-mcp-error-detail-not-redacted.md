# TICKET-308: Stop sending raw AppError detail to MCP clients

Discovered during a full-codebase review (2026-09-22).

## Problem

`call_session`'s error arm in `src-tauri/src/mcp/tools.rs` (around line 843)
sends `AppError::to_string()` — code, correlation id, and the `detail` field —
verbatim to the external MCP client. `AppError`'s own doc comment
(`errors/mod.rs:183-192`) and `Concept.md § The error boundary` both state
that `detail` is diagnostics-only and that "nothing display-ready crosses the
boundary"; CONTRIBUTING.md's security section says the same for the planned
MCP interface explicitly.

A failed `copy`/`move`/`extract` produces an `AppError` whose `.detail` is a
JSON blob of `Vec<ItemError>`, each embedding a full nested `AppError` built
from raw `io::Error`/provider error text. `redact()` only strips four specific
secret shapes (URL userinfo, `password=`/`token=` assignments, bearer tokens,
PEM blocks) — it does not generically scrub provider wording, internal paths,
or protocol banners. Any other technical detail an SFTP/SMB/FTP/WebDAV backend
or local `io::Error` produces reaches the untrusted MCP client as the tool's
error text.

## Required change

- Build the MCP-facing error message from `e.code` (+ safe `e.params`) only —
  e.g. a small code→safe-sentence map analogous to the frontend's i18n layer.
- Keep `e.to_string()` (with `detail`) solely for the local audit log, which
  is already local-only.
- At minimum, drop `.detail` before calling `typed_tool_error`.

## Verification

- Test that a tool failure whose underlying `AppError` carries a
  provider/path string in `detail` does not leak that string in the MCP
  response — only `code` + safe `params` appear.
- `npm run check`.
