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

- [ ] One documented command provisions services, runs the full suite, and tears
      everything down even after a test failure.
- [ ] The suite requires no external account, persistent credential, or public
      network access after prerequisite images/packages are available.
- [ ] Every remote provider passes the shared conformance contract or records an
      explicit capability difference.
- [ ] FTP, explicit FTPS, and implicit FTPS are exercised separately.
- [ ] Network interruption and server-key/certificate mismatch cases are covered.
- [ ] Parallel tests use isolated users, shares, ports, and paths.
- [ ] Failure artifacts contain no reusable secrets.

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
