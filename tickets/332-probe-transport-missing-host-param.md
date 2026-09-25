# TICKET-332: Include the host parameter in the final unreachable error

Discovered during a full-codebase review (2026-09-22).

## Problem

The final "unreachable after all TCP attempts failed" branch in
`probe_transport` (`src-tauri/src/net/lifecycle.rs`, around lines 462-465)
omits the `host` parameter that every other `NET_UNREACHABLE` error carries,
so the frontend renders the literal placeholder instead of the host.

A user connecting to a host whose port is closed/filtered or that refuses the
TCP connection (by far the most common transport failure — wrong port,
firewall, server down) triggers `to_socket_addrs()` succeeding (DNS resolves
fine) but every `TcpStream::connect_timeout` in the loop erroring, hitting
`Err(ConnFailure::new(FailureKind::Unreachable, format!(...)))` at line 462,
which — unlike the DNS-failure branch above it (line 436) — never calls
`.with("host", bare)`. The frontend's `error.net.unreachable` template is
`"{host} ist nicht erreichbar"` / `"{host} is not reachable"`, and
`fillParams` deliberately leaves an unmatched `{host}` placeholder visible
rather than blanking it, so the user sees the literal string `{host} is not
reachable`.

## Required change

- Add `.with("host", bare)` to the final `ConnFailure::new(...)` call,
  matching the two branches above it in the same function.

## Verification

- Test: connect to a host with a closed/filtered port; assert the surfaced
  error message includes the actual host, not the literal placeholder.
- `npm run check`.
