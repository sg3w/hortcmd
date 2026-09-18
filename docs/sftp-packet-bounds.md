# Incoming SFTP packet bounds

Every SFTP subsystem opened by the application passes through
`src-tauri/src/vfs/sftp_frames.rs` before reaching `russh-sftp`. The adapter holds
at most the four-byte length prefix and an 8 KiB copy buffer. It exposes the
prefix only after checking that the announced packet length is between 1 byte
and 1 MiB inclusive. The limit includes packet type and payload, and excludes
the four-byte prefix itself.

This compensates for the locked russh-sftp 2.4.0 client reading incoming frames
with a `u32::MAX` bound despite its configured outbound packet sizing. Without
the guard, a remote length prefix could trigger a large allocation before the
body arrived. The guard does not collect complete responses or directory lists.
It does not bound the application's total memory across multiple packets or
replace separate directory-listing budgets.

Oversized/zero-length packets and truncated frames are terminal for that SFTP
stream: the first read fails and subsequent reads return EOF. Later writes fail.
This prevents the dependency's read loop from repeatedly processing the same
framing failure. Its request caller may still wait for the existing request
timeout before receiving an error; transport closure is separate from request
completion. Outgoing bytes, SSH authentication, host-key verification and other
SSH channels are unchanged.

Servers that send directory listings or extension payloads in individual packets
over 1 MiB will have that SFTP stream closed. Such servers need smaller response
packets; the client does not silently raise the allocation ceiling. Ordinary
256 KiB transfer packets fit within the limit. Reconnection uses the normal
verified connection workflow.

Tests cover fragmented prefixes and bodies with pending reads, exact limits,
multiple consecutive frames, rejected prefixes without any body consumption,
truncation and outgoing writes. An actual SFTP client handshake over an in-memory
duplex stream checks a valid version response and stream closure after an
oversized prefix without allocating a large body.
