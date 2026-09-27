# TICKET-374: Prevent FTP control-channel command injection

Priority: **P1** (backlog triage, not a CVSS score). Status: **open**.
Discovered by the dependency-governance audit on 2026-09-25.

## Evidence and scope

| Crate | Locked version | Advisory | Finding type |
| --- | --- | --- | --- |
| `suppaftp` | `6.3.0` | [RUSTSEC-2026-0271](https://rustsec.org/advisories/RUSTSEC-2026-0271.html) | vulnerability |

Snapshot: cargo-deny 0.19.5, RustSec revision
`593df8c1b5ed0bcde9dddadfeeead776fa514ff8`. This ticket owns **1 of the 18**
recorded findings. Fixed-version guidance below is from that snapshot; refresh the
advisory and dependency graph before implementation. Creating this ticket does
not suppress a finding or approve an exception.

## Problem

The direct suppaftp 6.3.0 dependency triggers a CRLF command-injection finding for FTP control-channel arguments. Ordinary filenames and credentials can become protocol arguments; TLS does not neutralize command injection.

## Required change

Migrate to a non-affected compatible suppaftp release (recorded minimum 10.0.2), or implement a fully scoped reviewed fix if migration is blocked. Inventory every control-channel argument, including authentication, paths, rename and listing operations. Preserve encoding and explicit/implicit FTPS trust verification.

- Follow [dependency governance](../../docs/dependency-governance.md): update exact
  source/script/direct/duplicate reviews for changed versions, inspect changelogs,
  security history, licenses, features, MSRV and platform support.
- Keep changes scoped to this remediation. Preserve a release block for any
  unresolved finding; a blanket crate/target/severity exclusion is not a fix.
- Coordinate overlapping lockfile changes from the other advisory tickets and
  re-evaluate the resolved graph after each local merge.

## Verification and acceptance

Use a local scripted FTP server to verify that CR/LF-bearing arguments cannot produce a second command. Exercise affected argument classes, valid Unicode/spaces, FTP and both FTPS modes, rename/list/transfer, and cancellation. Never record canary credentials or raw authentication commands in logs.

- [ ] Every advisory/version pair in the table is removed or has a documented,
      policy-valid, narrowly scoped mitigation; no pair is silently dropped.
- [ ] Run `npm run audit:dependencies` and compare findings by crate, version and
      advisory ID. This ticket need not clear unrelated tickets, but must not
      introduce new unaddressed findings. All 18 require disposition before release.
- [ ] Pass `npm run check`; run affected protocol tests explicitly because they
      are ignored in ordinary CI, and obtain relevant Windows/macOS/Linux results.
- [ ] Regenerate and verify release evidence with `npm run supply:bundle` and
      `npm run supply:verify`; check licenses, notices, optional/target coverage
      and security-sensitive duplicate versions.
- [ ] Follow the per-ticket branch, local commit and merge workflow. Move this
      ticket to `done/` only after its remediation and verification are complete.

## Resolution (2026-09-27)

Refreshing the audit first turned up something the ticket's snapshot could not
show: `npm run audit:dependencies` no longer reports `suppaftp` **at all**.
`RUSTSEC-2026-0271` is still in the refreshed database (cargo-deny 0.19.5,
RustSec revision `e2111519ba6d14a5da59a7b2e5c8083ae8a37c01`, patched from
10.0.2), and the locked version is still the affected 6.3.0 — but suppaftp
resolves through `[patch.crates-io]` to `vendor/suppaftp`, and cargo-deny does
not scan path sources. A green gate here would have been a scanner blind spot,
not a fix, which is exactly the "silently dropped" case the acceptance criteria
warn about. That is recorded in ticket 369's notes and again here.

### Change

The fix is backported into the vendored crate rather than migrated, because
migration would mean re-vendoring against a 10.x API that no longer offers the
`connect_secure_implicit_with_stream` entry point ticket 320 depends on, while
the fix itself is one function and two call sites.

`vendor/suppaftp/src/command.rs` gains `render(&Command) -> FtpResult<String>`:
it renders the command and rejects the line when a CR or LF appears anywhere but
in the trailing `\r\n`, returning `FtpError::ConnectionError` with
`io::ErrorKind::InvalidInput` — the same error upstream 10.0.2 returns from
commit `194bdd1`. Both `perform` implementations, in `sync_ftp` and `async_ftp`,
now call it instead of `Command::to_string`.

Rendering and validating in the same place is the point of the inventory the
ticket asks for: **every** control-channel argument goes through `perform`, so
authentication (`USER`/`PASS`), paths (`CWD`, `MKD`, `RMD`, `DELE`, `SIZE`,
`MDTM`, `MLST`), rename (`RNFR`/`RNTO`), listing (`LIST`/`NLST`), transfers
(`RETR`/`STOR`/`APPE`), `SITE` and custom commands are covered by construction,
and a command type added later cannot reintroduce the hole. Encoding is
untouched — the line is still the crate's own UTF-8 rendering — and explicit and
implicit FTPS trust verification is not involved, since validation happens before
the bytes reach the (already established) stream. As upstream, `custom_command`
can no longer carry several CRLF-joined commands in one call.

`vendor/suppaftp/HORTCMD-PATCH.md` records provenance and the removal condition,
and the vendored source review in `policy/dependencies.json` is re-fingerprinted.

### Verification

`vfs::ftp::injection_tests` in `src-tauri/src/vfs/ftp.rs` drives a real
`RustlsFtpStream` against a scripted in-process FTP server that answers with real
status codes and records every line it receives. It asserts the thing that
matters — what reached the wire, not just that the call failed:

- CR/LF in `cwd`, `mkdir`, `rmdir`, `rm`, `rename` (source), `site`,
  `custom_command`, `size` and `mdtm` is refused with `InvalidInput`;
- a rename whose *destination* carries the injection sends the legitimate `RNFR`
  and refuses `RNTO` before writing it;
- a login whose user name carries the injection is refused before anything is
  written, so the password never leaves the process — the server sees zero lines;
- ordinary arguments with spaces and non-ASCII (`/a folder/Ümlaut-Grüße`,
  `neuer Ordner`) still go out unchanged;
- the recorded transcript is asserted exactly: `USER`, `PASS`, `RNFR a.txt`,
  the two valid commands and `QUIT` — no injected command anywhere.

Both tests were confirmed to fail with the gate disabled, so they test the fix
rather than the absence of a server. `command::test` inside the vendored crate
covers the same argument classes at the render level (those tests belong to the
dependency's own suite and do not run in hortcmd's).

No canary credential or raw authentication command is logged: the tests use
literals, and the crate's `trace!("CC OUT: …")` line is unchanged, so nothing new
is written anywhere.

- `npm run check` passed on macOS (arm64).
- Protocol harness (`npm run test:harness`, Docker): FTP, explicit FTPS and
  implicit FTPS pass listing, stat, mutation, staging, streaming, transfers,
  cancellation (`harness_survives_a_server_dying_mid_copy`), the bounded
  connect timeout and the stalled-operation timeouts from ticket 320. The run's
  remaining failures are the ones already failing on the `main` baseline
  (tickets 378 and 379).
- `npm run audit:dependencies`: unchanged at 2 findings (tickets 373 and 376);
  nothing new appeared. The scanner cannot confirm this one, for the reason
  stated above, so the tests are the evidence.
