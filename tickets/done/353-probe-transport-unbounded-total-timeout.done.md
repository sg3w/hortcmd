# TICKET-353: Bound probe_transport's total connect time across all addresses

Discovered during a full-codebase review (2026-09-22).

## Problem

In `src-tauri/src/net/lifecycle.rs` (around lines 454-461), the per-address
connect budget (`TRANSPORT_TIMEOUT / min(addrs.len(), 4)`) bounds each
individual `TcpStream::connect_timeout`, but the loop still iterates every
resolved address sequentially, so total probe time scales with
`addrs.len()` and is not actually capped at the documented `TRANSPORT_TIMEOUT`
(15s) once a host resolves to more than 4 addresses.

A profile's host resolving (via large round-robin DNS) to, say, 20 addresses
that are all unreachable (dropped/filtered, not RST) makes each connect
attempt consume close to its full ~3.75s share before failing, so the loop
can run for ~75s — well past the documented 15s transport budget and past the
45s `OVERALL_TIMEOUT` the caller (`session.rs::connect_inner`) waits on via
`recv_timeout`. The visible command already times out at 45s, but the
spawned worker thread keeps running the stale probe in the background for
tens of seconds beyond that, so repeated connect attempts against such a
host accumulate lingering threads.

## Required change

- Bound the whole probe with a single deadline (e.g. `Instant::now() +
  TRANSPORT_TIMEOUT`) shared across all addresses, shrinking each subsequent
  attempt's timeout by what's left, rather than giving every address its own
  fixed share independent of how many addresses there are.

## Verification

- Test: a host resolving to many unreachable addresses; assert the probe
  completes within `TRANSPORT_TIMEOUT` regardless of address count.
- `npm run check`.

## Resolution (2026-09-25)

TCP connection probes now share one deadline across every resolved address, with each attempt capped by the remaining time. Deterministic tests cover 20 dropped addresses, partially or fully consumed resolution time, and success after fast refusals. Existing loopback transport tests pass. Synchronous OS DNS resolution cannot be interrupted; this limitation is now stated explicitly in the timeout documentation.

Validation: `RUST_TEST_THREADS=1 npm run check` passed on macOS.
