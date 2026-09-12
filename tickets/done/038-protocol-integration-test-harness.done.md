# TICKET-038: Build a reproducible remote-protocol integration test harness

## Goal

Exercise SFTP, SSH terminal, FTP, FTPS, SMB, and WebDAV behavior against controlled
services in a repeatable environment, including failure, cancellation, and
security cases that unit mocks cannot validate.

## Background

`src-tauri/src/vfs/live_tests.rs` contains useful ignored tests for remote
providers and SSH terminal behavior. They depend on manually prepared services
and environment variables, so ordinary CI runs 6 ignored tests and cannot detect
wire-protocol or server-compatibility regressions. TICKET-033 establishes the
general test runner and a manual live-test entry point. This ticket turns the
protocol suite into a reproducible, isolated harness with no dependency on a
developer's real servers or credentials.

## Requirements

- Provide pinned, disposable test services for:
  - SFTP plus SSH shell/PTY behavior;
  - plain FTP, explicit FTPS, and implicit FTPS;
  - SMB2/3 with signing coverage;
  - WebDAV over HTTP and HTTPS.
- Generate short-lived test credentials and certificates at harness startup; do
  not store real secrets or private keys in the repository.
- Bind services to loopback by default and document any container/runtime
  prerequisite.
- Seed deterministic trees containing empty files, large files, nested folders,
  Unicode, spaces, dotfiles, conflicting names, and provider-specific edge names.
- Reuse one provider conformance suite for listing, stat, stream read/write/flush,
  mkdir, rename, delete, traversal rejection, and error mapping.
- Add transfer-matrix cases for local-to-remote, remote-to-local, remote-to-same-
  provider, and remote-to-different-provider operations.
- Cover disconnection during connect, read, write, flush, checksum, rename, and
  terminal use; verify cleanup and retry behavior.
- Cover certificate rejection and SSH host-key first-use/change behavior after
  TICKET-028 and TICKET-030-related transport policy are implemented.
- Make artifacts and redacted service logs available on failure.
- Keep the suite opt-in or scheduled if runtime is unsuitable for every pull
  request, but make one command sufficient to run it locally.

## Acceptance Criteria

- [x] One documented command provisions services, runs the full suite, and tears
      everything down even after a test failure.
- [x] The suite requires no external account, persistent credential, or public
      network access after prerequisite images/packages are available.
- [x] Every remote provider passes the shared conformance contract or records an
      explicit capability difference.
- [x] FTP, explicit FTPS, and implicit FTPS are exercised separately.
- [x] Network interruption and server-key/certificate mismatch cases are covered.
- [x] Parallel tests use isolated users, shares, ports, and paths.
- [x] Failure artifacts contain no reusable secrets.

## Tests

- Run the harness twice consecutively and in parallel to prove isolation and
  deterministic cleanup.
- Force each service to stop during an active transfer and verify bounded failure,
  staging cleanup, and source/destination safety.
- Rotate the SSH key and TLS certificate between attempts and verify strict
  rejection behavior.
- Exercise files larger than copy buffers and enough concurrent operations to
  expose flush and locking defects.
- Verify path traversal attempts remain inside each seeded test root.
- Run the scheduled/manual workflow on every supported host capable of running the
  harness and publish redacted artifacts for deliberate failures.

## Definition of Done

- Remote protocol coverage is reproducible without developer-owned infrastructure.
- Providers share a conformance suite and document intentional differences.
- Security and interruption scenarios validate the guarantees from TICKET-028,
  TICKET-029, and TICKET-032.
- The harness is documented, version-pinned, isolated, and self-cleaning.
- CI owns a clearly defined scheduled or manual execution policy.
- Rename this file to `038-protocol-integration-test-harness.done.md`.

## Implementation

**The providers' most important code was the least tested code.** Six protocol
implementations, and the part of each that could actually go wrong — the wire
behaviour — had no test at all, because a mock provider agrees with whatever
the code believes. The existing `live_tests.rs` knew this and pointed at Docker
commands in doc comments, which meant the suite ran when somebody felt like
assembling four servers by hand. In practice: never.

So the servers come with the tests.

### The harness

`harness/harness.mjs` — one command, `npm run test:harness`:

1. allocates free ports, seeds a deterministic tree per service, and generates
   the run's password, a private CA, a server certificate and an SSH host key
   (plus a spare, for the rotation case);
2. builds and starts seven servers from base images pinned by digest —
   SFTP with a shell, plain FTP, explicit FTPS, implicit FTPS, SMB with signing
   *mandatory*, WebDAV over HTTP and over HTTPS;
3. runs the Rust suite against them;
4. tears everything down — on success, on failure, and on Ctrl+C.

Everything a run owns lives under `harness/.run/<id>/` and dies with the run.
Nothing is committed, nothing is published beyond `127.0.0.1`, and the run id
reaches the compose project name, so two runs sit side by side without meeting.
The only prerequisite is Docker. On failure the service logs are collected with
the run's own secrets scrubbed out — a failure artifact carrying a working
password is worse than no artifact.

### The suite

`src-tauri/src/vfs/harness/`, three files for three questions:

- **`conformance.rs`** — one contract, seven servers. The listing matches the
  manifest, `stat` agrees with it, a 3 MiB file round-trips byte for byte in one
  read and in 7919-byte chunks, create/rename/delete behave, the staging commit
  publishes with one rename and leaves nothing behind, failures come back as
  failures, traversal stays inside the tree, and the capability matrix is
  asserted immediately after the behaviour it claims to describe.
- **`transfers.rs`** — local→remote, remote→local, within one server (where a
  shared FTP control connection would deadlock), and between two *different*
  protocols, in both direct and failure-safe mode.
- **`failure.rs`** — the server dying mid-copy in both modes, an unreachable
  server, the certificate that must be refused, the host key that changes, the
  SSH shell, and the bounded connect timeout.

Without `HORTCMD_HARNESS` every test **skips** with the command to run. Run by
hand with `--ignored` and no servers they would otherwise be a wall of
connection errors that says nothing.

### What it found

Four provider bugs on the first complete run, none of which a unit test could
have seen, and two product gaps:

- **FTP transferred in ASCII mode.** FTP's default transfer type is ASCII
  (RFC 959) and a server that honours it rewrites every `0x0A` in passing: the
  3 MiB fixture came back 12 288 bytes longer — exactly the number of `0x0A`
  bytes in it. Every archive, image and executable ever copied over FTP was
  silently corrupted. `TYPE I` is now sent after login.
- **SFTP reported every directory as a file.** TICKET-018's symlink resolution
  read `is_dir` from the resolved target, and a plain entry has no target, so
  `target.is_some_and(…)` was `false` for every ordinary directory.
- **Implicit FTPS never protected its data connections.** `AUTH TLS` ends with
  `PBSZ 0` + `PROT P` inside suppaftp; the implicit path sends neither. A server
  that insists answers "522 Data connections must be encrypted"; one that does
  not would have carried the file in the clear over a connection the user chose
  *because* it was encrypted.
- **WebDAV could list files it could not touch.** Request paths went into the
  URI unescaped: `100%-done.txt` is an invalid escape sequence (HTTP 400),
  `a#b.txt` is cut at the fragment, `a?b.txt` becomes a query. All three were
  visible in the pane and unreachable by every operation.
- **FTP listings hid dotfiles**, because `LIST` is `ls` and not `ls -a`. Now
  `LIST -a` with a fallback to the plain form, because smuggling an argument
  through a path must never be the only attempt.
- **Implicit FTPS could only be chosen by using port 990.** An implicit server
  on any other port was silently spoken to in the clear. `ftps-implicit` is now
  its own protocol, with the port heuristic kept for profiles saved before it.

Two more fixes came out of building it: **SMB ignored the configured port**
(`UncPath` carries only the server name, so a non-445 profile quietly connected
to 445 anyway), and the three modules that spell out `<config>/hortcmd` now go
through `config::dir()`, which honours `HORTCMD_CONFIG_DIR` — a test run that
wrote its accepted host keys into the developer's real store would change the
machine it was meant to observe.

### One new feature, deliberately

`HORTCMD_EXTRA_CA` adds the certificates in a PEM bundle to the roots for FTPS
and WebDAV. The harness needs it to reach its own throwaway TLS servers without
installing anything into the machine's trust store, but it is not a test hook:
a self-hosted server with the operator's own CA is ordinary, and the established
answer is an *additional* anchor the user chooses, not a weakened check —
`CURL_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS`, `GIT_SSL_CAINFO` all work this way.
Hostname, expiry and chain are still verified, an unknown issuer still fails,
and a bundle that is named but unreadable is an error rather than a silent
fallback to fewer anchors than were asked for.

### Where protocols genuinely differ, the difference is named

- The seed manifest is read back **from the filesystem** rather than derived
  from the intended list. A case-insensitive host collapses `Case.txt` and
  `case.txt` into one file, and a manifest describing the wish would fail every
  provider for something no provider did.
- Names SMB cannot hold (`*`, `:`, a trailing dot) are marked in the seed, not
  excused in the assertions.
- `confined` records whether the *server* keeps the account inside the tree.
  Every service does except SSH, where a chroot would take the shell with it —
  and hortcmd cannot invent a confinement it does not have, so the traversal
  check asserts something different there and prints that it did.

### Notes on the servers themselves

Three of them had to be replaced while building this, and the reasons are in
the Dockerfiles so nobody repeats the search: vsftpd on musl segfaults the
moment a TLS session comes up; vsftpd on glibc segfaults a little later, under
this suite's load; and Alpine ships no DBM backend for apr-util, so Apache's
`mod_dav_fs` cannot open its lock database and every `MKCOL` fails. The health
checks are process checks rather than port probes for a related reason — an
earlier `nc -z` every second was killing the very server it was checking.

### Verified

`npm run check` passes end to end: `cargo fmt --check`, Clippy with
`-D warnings` across all targets and features, 278 Rust tests (22 `#[ignore]`d),
current bindings and error codes, no import cycles, `check:docs`, `tsc
--noEmit`, the frontend suite, the production build, and the bundle budget.

`npm run test:harness` — **18 tests, 7 servers, all passing**, in about 75
seconds after the images are built. The run tears itself down completely:
neither a container nor a run directory survives it.

- **Twice consecutively and twice in parallel**, all four runs green with no
  leftovers, which is what the isolation is for. The first parallel attempt
  failed exactly once, on a real flaw: ports were taken with `listen(0)`, out of
  the operating system's own ephemeral range, and something else claimed one in
  the seconds before Docker bound it. Ports now come from 20000–45000, below
  that range, and a collision is retried with new ports instead of failing the
  suite.
- **Every fix above was watched failing first.** The ASCII-mode corruption was a
  length assertion off by exactly the number of newlines; the SFTP directory bug
  was a kind assertion; the implicit-FTPS data channel was the server's own
  "522"; the WebDAV encoding was an HTTP 400 on a file the listing had just
  shown. Each was re-run against the same servers afterwards.
- **Deliberate interruption**: the SFTP server is stopped mid-write and the
  staged copy fails in microseconds with the final name absent and the staging
  entry reported, not deleted; the WebDAV server is stopped mid-write and the
  direct copy reports the failure rather than a short file.
- **Certificate rejection both ways**: the three TLS services answer with
  `HORTCMD_EXTRA_CA` pointing at the run's CA and are refused without it — the
  second half is what proves validation is really happening.
- **Host-key rotation end to end**: first contact prompts before authentication
  (proved with a deliberately wrong password), trusting persists, the reconnect
  is silent, and after the server's key is swapped and the container restarted
  the connection is refused as a mismatch with no bypass token.
- **`cargo test -- --ignored` without the harness** prints the skip notice and
  the command to run, rather than 18 connection failures.

Two things are recorded rather than fixed. FTP `LIST` of a directory that does
not exist answers with an empty listing on some servers rather than an error —
vsftpd did, ProFTPD does not — which would show a typo'd path as an empty pane;
no server in the harness reproduces it any more, and a fix nothing can exercise
is a fix nobody can trust. And successful HTTPS against a certificate from the
*machine's* trust store is still not covered, because installing a CA there is a
change to the developer's machine that a test suite has no business making.
