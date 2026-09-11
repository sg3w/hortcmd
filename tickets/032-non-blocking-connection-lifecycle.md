# TICKET-032: Make remote connection lifecycle bounded and cancellable

## Goal

Connecting, reconnecting, and disconnecting remote profiles must have explicit
progress, time limits, cancellation, and deterministic cleanup. A slow or dead
server must not leave the interface in an ambiguous pending state or allow
duplicate attempts for the same profile.

## Background

`net_connect` currently performs keychain access and protocol connection setup in
one synchronous command body. Protocol implementations use a mixture of blocking
clients and calls bridged through a shared Tokio runtime. The frontend awaits the
command but keeps no per-profile pending state, exposes the connect button during
the attempt, and provides no cancellation action.

Several providers also lack an explicit lifecycle contract for connection loss.
An operation can discover a broken session before the connection list reflects
it, while terminal sessions and provider handles must be cleaned up consistently.
These concerns become more important as four network protocols share the same UI.

## Requirements

### Attempt model

- Give each connect/reconnect request a unique attempt ID and associate it with a
  profile ID and a monotonically increasing generation.
- At most one current attempt may exist per profile. A duplicate connect action
  focuses/reuses the current state instead of starting another socket/session.
- Add a cancellation command keyed by attempt ID. Cancellation is idempotent and
  must stop pending work where the underlying client permits it.
- A late completion from a cancelled or superseded generation must be discarded
  and its newly created provider/session closed; it must never replace the current
  registry entry.
- Connection setup must run away from the Tauri/UI thread: async clients remain
  async and blocking protocol clients run in bounded blocking tasks.

### Timeouts

- Apply a 15-second connection/transport timeout and a 45-second overall timeout
  covering handshake plus authentication. These are initial constants, not new
  user settings in this ticket.
- Apply bounded timeouts to protocol probes used to declare a connection ready.
- Distinguish timeout, cancellation, authentication failure, host-key failure,
  unreachable endpoint, protocol error, and keychain error as typed outcomes.
- A timeout or cancellation closes all partially created sockets, sessions,
  channels, and temporary registry state.

### Frontend state

- Store connection lifecycle per profile: `disconnected`, `connecting`,
  `connected`, `disconnecting`, or `failed`, plus attempt ID and structured error.
- While connecting, replace the connect action with a spinner/progress label and a
  cancel action. Disable edit/delete/disconnect actions that would race with the
  attempt unless they explicitly cancel it first.
- While disconnecting, prevent navigation into the connection and disable another
  disconnect request.
- Keep the connection manager open on failure or cancellation and show a concise,
  localized result. Successful connection retains the current behavior of closing
  the manager and navigating the active pane.
- Refreshing profiles or reopening the dialog must preserve authoritative pending
  state from the backend rather than resetting it to disconnected.

### Registry and connection loss

- Insert a provider into the live registry only after transport, authentication,
  host verification, and the provider readiness probe all succeed.
- Reconnecting must not discard a working existing session until the replacement
  is ready. If replacement fails, leave the old session connected.
- Disconnect cancels a pending attempt, removes the live provider, closes all SSH
  terminal sessions associated with the store, and emits the final disconnected
  state.
- When a provider reports a definitive broken connection, mark it failed or
  disconnected once and notify the frontend. Do not leave a connection marked live
  indefinitely after terminal/provider loss.
- Operations that race with disconnect fail cleanly; no mutex is held while doing
  network I/O or waiting for task completion.

### Protocol coverage

- Apply the same lifecycle state machine to SFTP, FTP/FTPS, SMB, and WebDAV.
- Protocol adapters may implement cancellation differently, but all must obey the
  same attempt/generation and cleanup semantics.
- Integrate TICKET-028 host-key challenges as an explicit connection state rather
  than encoding them in generic errors. If TICKET-028 lands first, preserve its
  public behavior while adopting this state machine.

## Acceptance criteria

- [ ] Starting a connection immediately shows a pending state and disables
      duplicate attempts.
- [ ] A user can cancel a pending connection and the profile returns to a stable
      disconnected state.
- [ ] Transport setup times out after 15 seconds and the complete attempt after 45
      seconds, with a distinguishable timeout result.
- [ ] A cancelled or superseded late result cannot enter the live registry.
- [ ] A failed reconnect leaves the previously working session active.
- [ ] Disconnect closes associated SSH terminals and invalidates the provider.
- [ ] Connection loss updates both the manager and location pickers.
- [ ] No registry mutex is held during keychain prompts or network I/O.
- [ ] SFTP, FTP/FTPS, SMB, and WebDAV follow the same visible lifecycle.

## Tests

- Unit-test every valid and invalid lifecycle transition, attempt generation, and
  idempotent cancellation/disconnect behavior.
- Use fake connectors for success, timeout, cancellation, authentication failure,
  host-key challenge, late success, and cleanup failure.
- Test duplicate clicks and simultaneous connect/disconnect requests for one
  profile and for different profiles.
- Integration-test each protocol against a responsive server, a refused port, and
  a server that accepts TCP but never completes its handshake.
- Drop an active test server and verify connection status plus terminal cleanup.
- Frontend-test spinner, cancel action, disabled controls, localized failures, and
  navigation after success.
- Regression-test application close while attempts and terminals are active.

## Definition of Done

- All four protocols use the shared lifecycle contract and bounded execution.
- Pending work is visible, cancellable, and cleaned up deterministically.
- IPC status/error types and generated TypeScript bindings are documented.
- No duplicate or stale provider can replace the authoritative session.
- German and English lifecycle messages are present.
- Concept.md documents timeout and reconnect semantics.
- Rename this file to `032-non-blocking-connection-lifecycle.done.md`.
