# TICKET-112: Establish performance benchmarks and regression budgets

## Backlog placement

Moved from 052 to 223 (now 112) as benchmark and regression infrastructure. This is supporting
work in the support backlog; the requirements and open status are unchanged.

## Goal

Measure and protect startup, navigation, rendering, search, preview, transfer, and
memory performance with representative local and remote workloads.

## Background

The frontend virtualizes file, search, and comparison result sets, and the transfer
engine supports configurable buffers and parallel threads. The build reports an
oversized JavaScript chunk, but there is no benchmark suite, stable fixture corpus,
performance budget, or profiling procedure. Optimizations such as bundle splitting,
caching, indexed search, and parallel copies cannot be evaluated reliably without
shared measurements and correctness checks.

TICKET-034 owns frontend chunking. This ticket owns system-level runtime budgets and
repeatable measurement rather than prescribing a specific optimization.

## Requirements

- Define representative workload tiers for small, medium, large, and pathological
  directory trees, including Unicode names, deep paths, mixed file sizes, Git state,
  archives, and high-latency remote providers.
- Measure at least cold/warm startup, first usable pane, directory load, sort/filter,
  selection/navigation latency, scroll frame stability, search time, preview time,
  transfer throughput, pause/cancel responsiveness, and peak memory.
- Separate frontend rendering, IPC serialization, Rust planning, provider I/O,
  checksum, and UI event overhead where practical.
- Add deterministic Rust microbenchmarks for path parsing, sorting/planning,
  checksums, archive listing, and transfer-buffer behavior.
- Add browser/WebView performance scenarios for 100, 10,000, and 100,000 visible
  entries without relying solely on synthetic DOM snapshots.
- Use controlled local storage and shaped loopback network profiles for remote
  latency/bandwidth/loss; never publish benchmarks from uncontrolled Internet hosts.
- Define per-scenario budgets with noise tolerance, baseline hardware/runner class,
  warm-up, repetitions, and statistical comparison.
- Run a stable smoke subset in pull-request CI and the full suite on scheduled
  dedicated runners. Flag regressions for review instead of hiding them by updating
  baselines automatically.
- Record profiling instructions for Rust CPU/heap and WebView/React performance.
- Require correctness checks alongside throughput so fast but incomplete operations
  cannot pass.

## Acceptance Criteria

- [ ] Benchmarks cover all named workloads and publish machine-readable results.
- [ ] Pull requests detect material regressions in a stable smoke subset.
- [ ] Full scheduled results are comparable over time on documented runners.
- [ ] Every budget includes statistical tolerance and an owner.
- [ ] Results separate major frontend, backend, and provider costs sufficiently to
      guide investigation.
- [ ] Benchmark outputs contain no developer filenames, credentials, or real hosts.
- [ ] Performance optimizations must retain functional and data-safety checks.

## Tests

- Run fixtures repeatedly to establish variance and tune thresholds before enabling
  blocking gates.
- Inject known delays, extra renders, smaller buffers, redundant hashing, and memory
  retention to confirm the relevant benchmark detects each regression.
- Run controlled local, high-latency, bandwidth-limited, and packet-loss profiles.
- Compare sequential and parallel transfers with checksum and safe staging on/off.
- Verify cancellation latency under CPU-bound hashing and blocked provider I/O.
- Validate all benchmark fixture outputs against expected entry counts and hashes.

## Definition of Done

- A documented benchmark corpus and runner produce repeatable cross-layer metrics.
- Key workflows have agreed budgets and regression ownership.
- CI and scheduled jobs publish actionable comparisons without leaking user data.
- Profiling procedures identify whether regressions originate in UI, IPC, backend,
  or provider work.
- Optimization tickets reference baseline and final measurements.
- Rename this file to `112-performance-benchmarks-and-budgets.done.md`.
