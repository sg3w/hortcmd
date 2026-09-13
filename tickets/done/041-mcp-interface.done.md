# TICKET-041: MCP interface (hortcmd as MCP server)

## Goal
hortcmd exposes its file-management capabilities over the **Model Context Protocol**,
so an MCP client (Claude Desktop, Claude Code, any other MCP host) can browse, read
and — after explicit opt-in — modify files **through hortcmd**, including its remote
locations (SFTP/SMB/WebDAV/FTP) and the panel context the user is currently looking at.

## Background
Every MCP host today ships some flavour of a local-filesystem server. What hortcmd
adds is the part those cannot do: since tickets 020–026 the app owns a **VFS** with
live, authenticated connections (`src-tauri/src/vfs/`), credentials in the OS keychain
(`net/credentials.rs`) and a session registry (`net/session.rs`). An MCP client talking
to a *running* hortcmd therefore reaches `sftp://…` or `smb://…` addresses with the
**already open** connection — no second login, no credentials handed to the AI host —
and it can see which folder and selection the user has in front of them.

This is a **separate axis from the plugin system** (tickets 201–208): there, foreign
code runs *inside* hortcmd behind a broker; here, hortcmd is the *server* and foreign
code stays outside. The permission thinking is deliberately borrowed from 201
(capability strings, default-deny, user confirmation) — the mechanism is not shared,
and 041 does not depend on the plugin tickets.

**Direction assumption:** "MCP interface" is implemented as hortcmd **serving** MCP.
The opposite direction — hortcmd as an MCP *client* that consumes foreign MCP servers
— is a different feature and is explicitly out of scope (see below).

## Requirements

### Transport & process model
- **Primary: in-process server in the running app.** The MCP server lives inside the
  Tauri backend, bound to **`127.0.0.1` only**, on a configurable port, speaking
  **streamable HTTP**. Reason: the live VFS connections, the keychain access and the
  panel context exist only in the GUI process; a standalone process would have to
  re-authenticate every remote and could not see the panes.
- **Bridge for stdio-only clients.** Most MCP hosts spawn a command and speak stdio.
  Ship a small second binary (`hortcmd-mcp`) that is nothing but a stdio ↔ localhost
  bridge to the running app. It must fail with a clear, actionable message when
  hortcmd is not running or the interface is disabled — not hang.
- Server **off by default**; started/stopped at runtime from the settings without an
  app restart. The bound port and the connection state are reported to the frontend.

### Authentication
- A **bearer token** generated on first activation, stored in the OS keychain via the
  existing `CredentialStore`, shown/copyable in the settings, regeneratable.
  Requests without a valid token are rejected — a localhost bind alone is not a
  security boundary (any local process can reach it).
- Validate `Origin`/`Host` on incoming HTTP requests (a browser on the machine must
  not be able to drive the interface via DNS rebinding).

### Capability tiers (default-deny)
Three tiers, each separately switchable, `read` is the only default:
- **`read`** — `list_dir`, `stat`, `read_file`, `search`, `checksums`,
  `compare_dirs`, `list_archive`, `volume_info`, `git_status`.
- **`write`** — `make_dir`, `write_file`, `rename`, `copy`, `move`, `extract`,
  `create_archive`.
- **`delete`** — `trash` (default) and, only when separately allowed,
  permanent `delete`.

### Path scoping
- An **allowlist of roots** (local paths and/or VFS addresses); empty list = nothing
  is reachable. Every tool argument is resolved through `vfs::parse_address`/`resolve`
  and checked against the roots **after** normalisation, so `..`, symlinks and
  case-folding cannot escape.
- Convenience roots the user can tick instead of typing: *home*, *the current panel
  paths*, *the connected remotes*.
- Remote addresses are only reachable while their connection is active; the interface
  never triggers a credential prompt on behalf of an MCP client and never returns
  passwords, tokens or key material in any payload.

### Confirmation
- Every `write`/`delete` call is **confirmed by the user in the app** by default
  (dialog with tool name, resolved target(s), client name), with a per-session
  "allow this tool for this client" option and a settings switch for users who want
  unattended operation. A pending confirmation must not block the UI thread or the
  read tier.

### Tools, resources, context
- Tools are named `<verb>_<noun>` with an explicit JSON schema and typed errors
  (not found · denied by scope · not permitted · needs confirmation · transport).
  Errors are returned as MCP tool errors, never as panics.
- `read_file` respects a **max byte size** (settings, default like the preview cap)
  and returns text or base64 with the detected encoding, so a client cannot pull a
  10 GB file into its context.
- Long operations (copy/move/search) report **progress notifications** and honour
  **cancellation**, reusing the existing transfer/cancel plumbing
  (`cancel_transfer`, `cancel_dir_size`).
- **Resources** expose the panel context read-only: `hortcmd://panes/active` and
  `hortcmd://panes/{left,right}` → current path, selection, sort/filter; plus
  `hortcmd://connections` → *names and schemes only* of the active connections.
  This is the piece that makes the interface worth more than a generic FS server:
  "compress what I have selected" works without the user typing any path.
- No tool may open windows, focus the app, or change the user's panel state — the
  interface observes the UI, it does not drive it.

### Code layout
- `src-tauri/src/mcp/{mod.rs, server.rs, auth.rs, scope.rs, tools/…}`, started from
  `lib.rs` setup; the stdio bridge as a second `[[bin]]`.
- MCP tool handlers call the **same service functions** as the Tauri commands. Where
  logic still sits inside a command body (`commands/fs/…`), it moves into a service
  first — no copy of file-system logic in `mcp/`, and no MCP handler calling a Tauri
  command.
- Settings type + status/audit events exported via **ts-rs**; settings UI under
  `src/features/settings/` in the new category, strings in `src/i18n/dictionaries.ts`.
- Candidate crate: the official Rust SDK **`rmcp`** (streamable-HTTP server feature);
  verify MSRV 1.85 and dependency weight before committing to it.

### Visibility & audit
- Status indicator in the app while the server runs / a client is connected.
- An **audit log** (ring buffer, viewable in the settings, optionally to file):
  timestamp, client, tool, resolved target, allowed/denied/confirmed, result.
  Denied calls are logged too — that is what makes a misconfiguration visible.

## Out of scope
- **hortcmd as MCP client** (consuming foreign MCP servers, e.g. a chat panel or
  AI-assisted rename inside the app) — separate feature, own ticket if wanted.
- Remote/non-localhost exposure, multi-user access, OAuth — localhost + token only.
- Any dependency on, or integration with, the plugin broker (201–208); if both exist
  later, a plugin exposing MCP tools is a follow-up, not this ticket.

## Acceptance criteria
- [x] The MCP server can be enabled/disabled at runtime; it is **off** after a fresh
      install and binds `127.0.0.1` exclusively.
- [x] A standard MCP client connects (directly or through the stdio bridge),
      completes the handshake and lists tools and resources.
- [x] Without a valid token every request is rejected; the token lives in the OS
      keychain and can be regenerated, which invalidates the old one.
- [x] Requests with a foreign `Origin`/`Host` are rejected.
- [x] `list_dir`/`read_file` work for a local path **and** for an address of an
      active remote connection, with no additional credential prompt.
- [x] A path outside the configured roots is refused — including via `..`, a symlink
      pointing out, and a differently-cased spelling on a case-insensitive volume.
- [x] With only the `read` tier enabled, every write/delete tool is refused and the
      refusal is distinguishable from "not found".
- [x] A write/delete call raises the in-app confirmation and is executed only after
      approval; declining returns a clean error and changes nothing on disk.
- [x] `read_file` on an oversized file returns a size error instead of the content.
- [x] A running copy/search reports progress and can be cancelled by the client.
- [x] `hortcmd://panes/active` reflects the pane the user is actually on, including
      the current selection; no resource or tool response contains secrets.
- [x] The audit log records allowed, denied and confirmed calls.
- [x] With the interface disabled, the app behaves exactly as before (no port bound,
      no extra thread, no measurable startup cost).

## Tests
- Unit: scope resolution (inside/outside a root, `..`, symlink escape, case folding,
  local vs. each VFS scheme), tier gating per tool, token check, `Origin` check.
- Integration against the running server with a test client: handshake, tool listing,
  `list_dir`/`read_file`/`stat`, an error path per typed error, cancellation of a
  long-running call.
- Remote: against the protocol harness's SFTP/WebDAV/FTP/SMB servers
  (`npm run test:harness`, TICKET-038) — `list_dir` and `read_file` on an active
  connection, and the correct refusal when that connection is closed.
- Bridge: `hortcmd-mcp` with the app running (data flows both ways) and with the app
  not running (clear error, exits, does not hang).
- Manual: connect a real MCP host, run a read-only session, then a write session with
  the confirmation dialog.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] `npm run gen:types` regenerates the bindings without errors
- [x] Existing functionality unchanged
- [x] Documentation updated (Concept.md; a short "connecting an MCP client" section
      in README.md incl. the security model and the default-deny defaults)
- [x] Rename the ticket to *.done.md

## Integration evidence — 2026-09-13
- Official `@modelcontextprotocol/sdk` 1.30.0 connected over Streamable HTTP and
  through the compiled `hortcmd-mcp` stdio bridge. Handshake, list/call tools,
  structured write refusal and list/read resources passed. Wrong-token and
  unreachable-app startup errors also passed. Reproduction: `docs/mcp.md`.
- `harness_mcp_reads_live_remote_and_refuses_closed_connection` passed against
  SFTP, FTP, explicit/implicit FTPS, SMB, WebDAV HTTP/HTTPS in run 6f9f5b. It
  registers actual providers, lists/reads through MCP, rejects an out-of-scope
  request and refuses reads after unregistering the connection.
- This exposed and corrected 066's SMB share-root check: the share selector is
  a namespace boundary, so optional srvsvc enumeration is not required; link
  validation still applies below it.
- The broader run subsequently hit an intermittent WebDAV→SMB large-copy failure
  tracked in 071. It is not recorded as a passing complete protocol suite.
- `npm run check` passed after these code changes: 366 Rust tests passed,
  24 ignored external tests, 371 frontend tests passed, all remaining gates passed.
- Official SDK verification now also cancels real copy and move requests on the
  first progress notification over both transports, waits for the terminal error,
  and verifies preservation of the source and existing destination.
- Token-generation admission prevents requests authenticated before rotation from
  recreating a session after revocation; a deterministic transport regression
  passes. Local canonical aliases are accepted without weakening containment.
- Confirmation queue, approval/decline, session revocation and listener cleanup
  have automated Rust/React coverage. An interactive native MCP-host/window smoke
  test was not performed; the real standard client is the official SDK.
- Reported staging leftovers after cancellation are tracked separately in 072.
- Follow-up: after the 071 compatibility backport, remote run 599e21 passed all
  19 harness tests, including the three previously failing SMB paths. The harness
  completed teardown successfully. Rust 1.85.1 remained supported.
