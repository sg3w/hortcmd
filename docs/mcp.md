# MCP server

hortcmd serves MCP from the running desktop app. Enable it in Settings → MCP,
choose allowed roots and apply. An empty root list allows no file access. Read
access is on by default; writes and deletion have separate switches. Permanent
deletion requires an additional opt-in. Changes disconnect existing MCP sessions
and cancel their work; reconnect the client afterwards.

The server binds only `127.0.0.1`. Port 8712 is the default; port 0 selects a free
port shown in settings. Copy the bearer token from settings. It is stored in the
OS credential store, never in the MCP configuration file. Regenerating it revokes
existing sessions. Treat the token as a credential; do not commit it.

## Clients

A Streamable HTTP client uses the displayed endpoint, normally
`http://127.0.0.1:8712/mcp`, with `Authorization: Bearer <token>`.

For clients supporting stdio, build the bridge:

```sh
cargo build --manifest-path src-tauri/Cargo.toml --locked --release --bin hortcmd-mcp
```

Use the absolute path to `src-tauri/target/release/hortcmd-mcp` (on Windows,
`hortcmd-mcp.exe`) in the client's server configuration:

```json
{
  "mcpServers": {
    "hortcmd": {
      "command": "/absolute/path/to/hortcmd-mcp",
      "env": {
        "HORTCMD_MCP_PORT": "8712",
        "HORTCMD_MCP_TOKEN": "<token from settings>"
      }
    }
  }
}
```

Start hortcmd and enable MCP before connecting. The bridge reports an unreachable
app or rejected token on stderr and exits. Stdout carries only MCP messages. The
bridge owns no remote credentials and does not open a second remote connection.

## Operations and safeguards

Read tools include directory listing, stat, bounded text/base64 reads, searches,
checksums, archive listing, properties, volume information, Git status and
comparison. Provider capabilities still apply: searches, archives and some system
metadata operations currently require local files. Local and connected-remote
paths are supported by the underlying VFS read and transfer operations.

Copy/move accept `from` and an exact `to` path with an existing parent. They stage
and verify data before publication; moves remove the source afterwards. Existing
targets require `overwrite: true`. Recursive transfers reject symbolic links.
ZIP creation and extraction share the application's validated archive services;
see [archive behavior and limits](archive-extraction.md).

Write/delete requests display the client label, operation and targets in the app.
Remembered approval applies to the server-issued session and that tool only.
Declining, cancellation, timeout or loss of the confirmation window refuses the
request. Unattended writes require explicitly disabling confirmation in settings.

Resources expose pane paths, selections, sorting/filtering and names/schemes of
active connections. Out-of-scope pane details and selected entries are filtered.
Read access also gates resources. Tool failures provide a readable message plus
`structuredContent.error.code`; scope denial, permissions, missing files,
confirmation and malformed arguments can be distinguished without parsing prose.
The in-memory audit log is visible and clearable in settings.

## Verification

`npm run check` covers unit tests, HTTP transport integration and the frontend.
The optional official SDK test requires Node and an isolated SDK installation:

```sh
npm install --prefix /tmp/hortcmd-mcp-sdk --ignore-scripts @modelcontextprotocol/sdk
cargo build --manifest-path src-tauri/Cargo.toml --locked --bin hortcmd-mcp
MCP_SDK_ROOT=/tmp/hortcmd-mcp-sdk \
MCP_BRIDGE_BIN="$PWD/src-tauri/target/debug/hortcmd-mcp" \
  cargo test --manifest-path src-tauri/Cargo.toml --locked \
  official_sdk_http_and_stdio -- --ignored
```

The Rust test starts the real transport with an isolated test token/configuration;
it invokes the official SDK through HTTP and the compiled bridge, checks tools
and resources, verifies rejected/offline bridge startup, and cancels real copy
and move requests after receiving progress notifications. Both transports preserve
the existing destination and move source. SDK 1.30.0 was used
for the local interoperability run. The SDK is not an application dependency.

Real protocol tests use the existing disposable Docker harness:

```sh
node harness/harness.mjs run
```

`harness_mcp_reads_live_remote_and_refuses_closed_connection` registers the real
providers, reads through MCP, checks scope refusal and unregisters the connection
to verify that subsequent reads are denied. Final completion evidence is recorded
in TICKET-041; an ignored test without its external fixture is not verification.

Cancelled copy and move requests retry cleanup of their registered staging entry
once readers and writers have closed (TICKET-072). This cleanup ignores cancellation
only; it still checks the current enabled state, write permission and roots. An
unregistered path or an entry not created by that transfer cannot use this exception.
Publication and source deletion remain cancellation-gated. If cleanup is denied or
fails, the error result includes a separate cleanup item with the staging address.

Staging allocation still uses the providers' existing create operations. Atomic
reservation and protection against external replacement races are tracked in
TICKET-270.
