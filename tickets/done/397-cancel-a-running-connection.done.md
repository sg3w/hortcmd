# TICKET-397: A running connection attempt cannot be cancelled

Priority: **P1**. Status: **done**.
Reported on 2026-09-30: "it is not possible to cancel an old connection".

## Problem

The connection manager offers a Cancel button for every running attempt, and
TICKET-032 built the whole mechanism behind it — an attempt id, a cancel flag
checked between phases, `net_cancel_connect`, idempotent and forgiving. It was
still not possible to use it, for two independent reasons.

**1. The cancel was never delivered.** `net_connect` was a *synchronous* Tauri
command. A synchronous command runs inline on the thread that dispatches IPC
(`tauri-macros`' blocking execution context), and `connect_inner` waits on its
worker's channel for up to `OVERALL_TIMEOUT` — 45 seconds. For as long as there
was anything to cancel, no further command reached the backend at all:
`net_cancel_connect` was queued behind the very attempt it was meant to stop, and
so was `net_disconnect`. The button did nothing, which is exactly what was
reported. `net_disconnect` and `net_trust_host_key` had the same shape and the
same effect — teardown closes shell channels, which is network I/O, and trusting
a host key runs a full second attempt.

**2. A row without an attempt id had no way out.** `cancelConnect` looked up
`status.attempt` and **returned silently** when there was none
(`netStore.ts:264`). A profile the backend reports as `connecting` without naming
an attempt is then a row whose only control does nothing at all.

On top of both: `lifecycle::cancel` deliberately does *not* move the phase — the
worker may be deep in a handshake and claiming "disconnected" while its sockets
are open would be a lie. Correct, but it means a successful cancel looked
identical to no cancel at all: same spinner, same "Connecting …", same button.

## Required change

- The blocking net commands must not occupy the IPC dispatch thread. A cancel
  issued while an attempt runs has to arrive while it still matters.
- A row in `connecting` must always have a way out, including when no attempt id
  is available.
- A cancel that has been taken must be visible as such, without claiming a state
  the backend has not reached.
- Cancelling must stay a cancel: a **connected** profile is disconnected by its
  own button and must never be torn down by the cancel path.

## Verification

- Rust: a cancel reaches an attempt that is still running in its worker, which
  then settles to `disconnected` and installs nothing.
- Frontend: a connecting row without an attempt id falls back to a disconnect; a
  cancelled row is marked until the backend reports the attempt settled; a
  connected profile still triggers no IPC at all; `rowControls` answers `waiting`
  plus `net.cancelling` for a cancelled attempt.
- `npm run check`.

## Implementation

**Backend** (`net/session.rs`). `net_connect`, `net_trust_host_key`,
`net_disconnect` and `net_cancel_connect` became `async` commands whose blocking
body runs through one helper, `off_thread`. `connect_inner` and the new
`disconnect_inner` keep their synchronous shape — the monitor calls
`connect_inner` from its own supervisor thread and needed no change.

`off_thread` carries the **correlation id** across the thread boundary. It lives
in a thread-local that the IPC middleware sets on the dispatch thread
(`lib.rs:261`), and `net_connect` is on the traced-command allowlist, so without
this the connection's own diagnostic events would no longer be matched to the
request that asked for them (ticket 045). A `JoinError` — the blocking task
panicked — is reported as `internal.unexpected` with the operation named.

`spawn_blocking` rather than `#[tauri::command(async)]`: the latter would run the
45-second wait inside an async task and block a runtime worker. The blocking pool
is what the rest of the codebase already uses for work of this shape
(`compare.rs`, `icons.rs`, `commands/plugins.rs`).

**Frontend.** `cancelConnect` now reads the whole status rather than just the
attempt id:

- `phase !== "connecting"` → nothing happens, as before. This is the guard that
  keeps a connected profile out of the cancel path; it used to be implied by
  "there is no attempt id", which was true but for the wrong reason.
- an attempt id → `net_cancel_connect`, the precise handle.
- no attempt id → `net_disconnect`, which cancels whatever is running for the
  profile and forces it back into a state the user can act on. It is the blunter
  instrument, and it is only reached where the precise one does not exist.

The visible half is a `cancelling: string[]` in the store, set when the cancel is
issued and pruned by `stillCancelling` from every status snapshot — both the
polled refresh and the pushed `net-state` — as soon as the profile is no longer
`connecting`. `rowControls` takes it as a second argument and answers `waiting`
with `net.cancelling` instead of `cancel`, so the row shows a spinner and says
what it is waiting for. The backend stays the authority on the phase; this only
records what the *user* has asked for, which is information the backend does not
have and should not invent. The bound is the attempt's own 45-second cap.

### Verification

- `net::lifecycle::tests::a_cancel_reaches_an_attempt_that_is_still_running`:
  the attempt runs in a worker that polls `check()` the way `run_attempt` does
  between its phases, the cancel arrives from another thread, the worker returns
  `FailureKind::Cancelled`, `settle` installs nothing and the phase lands on
  `Disconnected`.
- `lifecycle.test.ts`: "waits, and says so, once the cancel has been asked for";
  "falls back to a disconnect for a connecting row without an attempt id";
  "marks the row as cancelling until the attempt has settled"; and the existing
  "does nothing when there is no attempt to cancel" (a *connected* profile) now
  also asserts that no cancel mark is left behind.
- `npm run check`: green except `check:bundle`, which is **already red on `main`**
  — the entry chunk is 325.1 kB against a 325 kB budget; ticket 396 took it to
  326.3 kB and this one to 326.8 kB (one status string plus the store field).
- **Not verified natively:** that the button now responds while a real attempt to
  an unreachable host is in flight was not clicked through in a running build.
  The IPC-threading half rests on the Tauri dispatch contract above and on the
  Rust test; the two frontend halves are covered by unit tests.
