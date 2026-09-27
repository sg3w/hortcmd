# hortcmd patch: socket timeouts (ticket 320) and CRLF validation (ticket 374)

Source: crates.io suppaftp 6.3.0, unchanged version, license and features.

## Socket timeouts (TICKET-320)

`connect_secure_implicit_with_stream` accepts a caller-configured TCP socket;
the existing implicit API delegates to it. TLS verification and PROT behavior
are unchanged. This lets the VFS set timeouts before TLS or the FTP greeting.
No dependency upgrades or advisory exceptions. Replace the local addition when
an upstream API provides equivalent behavior. Validated by stalled-peer and
protocol tests.

## Control-channel CRLF validation (TICKET-374)

Backport of the RUSTSEC-2026-0271 fix (upstream commit `194bdd1`, released in
suppaftp 10.0.2). `command::render` renders a `Command` and rejects the line if
a CR or LF appears anywhere but in the trailing terminator, returning
`FtpError::ConnectionError(InvalidInput)` — the same error upstream returns. Both
`perform` implementations, sync and async, call it instead of `Command::to_string`,
so every control-channel argument passes through one gate: credentials, paths,
rename, listing, transfer, `SITE` and custom commands alike. As upstream, a
`Custom` command can no longer carry several CRLF-joined commands.

Migrating to upstream 10.0.2 was not chosen because it would mean re-vendoring
against a version whose API no longer offers the ticket-320 socket entry point,
and because 10.x raises its own MSRV expectations independently of this fix; the
backport is one function and two call sites, with identical observable behavior.
Replace it when the vendored base moves to 10.0.2 or later.

Verified by `vfs::ftp::injection_tests` in hortcmd, which drives a real client
against a scripted FTP server and asserts the bytes the server received, and by
`command::test` inside this crate. Existing advisories for this crate remain
tracked by ticket 374 until the scanner sees a non-affected version; note that
cargo-deny does not scan path sources, so this patch's correctness is what the
hortcmd tests establish.
