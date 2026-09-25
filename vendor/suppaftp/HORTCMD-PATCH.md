# hortcmd patch: socket timeouts (ticket 320)

Source: crates.io suppaftp 6.3.0, unchanged version, license and features.
`connect_secure_implicit_with_stream` accepts a caller-configured TCP socket;
the existing implicit API delegates to it. TLS verification and PROT behavior
are unchanged. This lets the VFS set timeouts before TLS or the FTP greeting.
No dependency upgrades or advisory exceptions. Existing suppaftp advisories
remain tracked by ticket 374. Replace the local addition when an upstream API
provides equivalent behavior. Validated by stalled-peer and protocol tests.
