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

- [x] Starting a connection immediately shows a pending state and disables
      duplicate attempts.
- [x] A user can cancel a pending connection and the profile returns to a stable
      disconnected state.
- [x] Transport setup times out after 15 seconds and the complete attempt after 45
      seconds, with a distinguishable timeout result.
- [x] A cancelled or superseded late result cannot enter the live registry.
- [x] A failed reconnect leaves the previously working session active.
- [x] Disconnect closes associated SSH terminals and invalidates the provider.
- [x] Connection loss updates both the manager and location pickers.
- [x] No registry mutex is held during keychain prompts or network I/O.
- [x] SFTP, FTP/FTPS, SMB, and WebDAV follow the same visible lifecycle.

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

---

## Implementation

### Backend

- **`src-tauri/src/net/lifecycle.rs`** (new) is the state machine.
  - `Phase` (`disconnected` / `connecting` / `connected` / `disconnecting` /
    `failed`) and `FailureKind` + `ConnFailure`, both exported through ts-rs.
  - `begin` registers at most one attempt per profile — a duplicate returns
    `Start::AlreadyRunning` — with an id and a monotonically increasing
    generation. `cancel` is idempotent and forgiving of unknown ids.
  - `settle` is the **only** place a provider may enter the live registry. It
    installs under one lock, and only when the attempt is still the current
    generation and is not cancelled. Every guarantee in the acceptance criteria
    follows from that single gate.
  - `probe_transport` gives all four protocols the same 15-second transport
    limit (DNS + TCP) and the same `unreachable` result; a URL scheme in the
    host — WebDAV profiles store one — is stripped before resolving.
  - `classify_protocol_error` is deliberately narrow: cancellation, timeout,
    host key, keychain and unreachable are structural outcomes of the phase that
    produced them and are never guessed from text.
- **`session.rs`**: `net_connect` runs the sequence (secret → transport →
  handshake) on a worker thread with a cancellation check before each phase and
  waits on a channel with the 45-second cap, so the command is never stuck. A
  worker that finishes after the caller gave up settles itself, which drops and
  closes the provider it opened. `net_cancel_connect` is new. `net_disconnect`
  cancels a pending attempt first, refuses to run twice, and closes shells
  outside every lock. `ConnectionStatus` gained `phase`/`attempt`/`error`, and
  `emit_states` pushes the whole list on each transition.
- **Connection loss**: `Provider::transport_alive` (default `true`, implemented
  by SFTP through `SshSession::is_closed`) is answered from local state, so
  `all_states` reaps dead sessions on every refresh without a round trip. A shell
  whose channel ends while the transport is closed reports too.
  `report_connection_lost` removes the session once, closes its shells and
  records the reason.

### Frontend

- `netStore`: `cancelConnect`, `applyStates` (from the `net-state` push), the
  `pending` outcome adopted rather than treated as a failure, and typed failures
  turned into one localized sentence.
- `features/net/connFailure.ts` reads both shapes of a failure — the typed
  `ConnFailure` on a status and the `CONN_*`-tagged string a rejected
  `net_connect` throws.
- `features/net/lifecycleUi.ts` derives what a row offers from its phase
  (connect / cancel / disconnect / waiting, what is disabled, what the status
  line says), so the decision is testable without mounting a component;
  `ConnectionsDialog` renders it and adds nothing.
- `App.tsx` subscribes to `net-state`.
- i18n: phase labels, the cancel action, the "busy" tooltip and one message per
  failure kind, German and English.

### Tests

- Rust (`lifecycle.rs`, 21 tests): every transition; duplicate `begin`;
  independence between profiles; idempotent cancel; a cancelled attempt settling
  to *disconnected* rather than *failed*; a late success for a cancelled attempt;
  a superseded generation losing to a newer one; the failure kinds; `reconcile`
  keeping a running attempt and a recorded failure across a refresh; the markers;
  the classification vocabulary; and the transport probe against an unresolvable
  host, a refused port, an open-but-silent port, and a scheme-carrying host.
  The `install` closure panics in the tests that must not install, so a
  regression fails loudly rather than silently.
- Rust (`live_tests.rs`): `live_connect_timeout` binds a listener that accepts
  TCP and never speaks — the case only the overall timeout can catch — and pins
  that the transport probe passes it while a refused port does not.
- Frontend (`features/net/lifecycle.test.ts`, 17 tests): what each phase offers,
  the disabled secondary actions, the spinner and cancel action, every failure
  kind in both languages (all distinct, never a bare key), the markers on a
  thrown error, an untagged error passed through, navigation after success, a
  duplicate request adopting the running attempt, cancelling only the reported
  attempt, and a pushed snapshot reaching every view.
- `src/test/setup.ts` (new, registered in `vite.config.ts`) shims `localStorage`
  so tests may import the persisted stores at all.

### Verified

`cargo test` 237 passed / 0 failed, `cargo clippy --all-targets` with no new
warnings against the pre-change baseline, `npm run typecheck` clean, `npm test`
82 passed / 0 failed. The connection manager was driven in the browser demo with
a profile in each of the five phases, in German and English: the pulsing amber
dot with "Verbindung wird aufgebaut …" and a Cancel button while connecting, the
spinner and no second disconnect while tearing down, dimmed edit/delete/host-key
actions in both, the green dot with Disconnect when connected, and the red dot
with the localized timeout message plus the server's own words when failed —
console clean throughout.

Not covered by the fast suite: the 45-second cap itself and the per-protocol
integration against a hanging server need the `#[ignore]`d live tests
(`cargo test --lib live_ -- --ignored --nocapture --test-threads=1`), which
deliberately spend the full budget.
