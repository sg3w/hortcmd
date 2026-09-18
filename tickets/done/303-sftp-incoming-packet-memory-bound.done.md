# TICKET-303: Bound incoming SFTP packet allocation

## Problem

The locked `russh-sftp` 2.4.0 client does not apply `Config::max_packet_len` to
incoming packets. Its `client::process_handler` calls `read_packet(stream,
u32::MAX)`. `utils::read_packet` reads a four-byte peer-controlled length and
allocates `vec![0; length as usize]` before reading the body. A configured 256 KiB
packet limit therefore does not bound incoming allocation; a peer can announce a
much larger packet without sending that body.

The application's `vfs/sftp.rs::connect_async` passes the SSH channel directly to
`SftpSession::new`, so the existing SFTP connection has no application-owned
inbound frame guard. This is distinct from TICKET-302: even a single response can
allocate beyond a scan or transfer memory budget before directory aggregation.
The finding comes from inspection of the locked dependency's client and utility
implementations, not an attempted large allocation or external-server test.

## Required change

- Enforce an explicit inbound SFTP packet ceiling before dependency allocation,
  using a bounded stream adapter or a verified dependency correction.
- Keep framing correct across fragmented prefixes/bodies and multiple packets.
- Reject oversized and malformed frames without allocating or consuming their
  claimed bodies. Terminate the affected SFTP stream after a framing error.
- Preserve ordinary reads, writes, verified SSH connection ownership and unrelated
  channels; do not weaken authentication or trust checks.
- Document the ceiling and compatibility consequences for legitimate servers.

## Verification

- Use bounded in-memory async streams to exercise fragmented four-byte lengths,
  exact boundaries, over-limit and truncated frames, and back-to-back packets.
- Verify the guard does not expose an oversized length to the downstream client.
- Exercise an actual `SftpSession` handshake through the adapter with a modest
  intentionally over-limit fixture; do not allocate multi-gigabyte test buffers.
- Run `npm run check`; implement on its own ticket branch and commit.

## Completion

Implemented and verified; this file is retained in the completed-ticket archive.

## Completed

Implemented `BoundedSftpStream` at the authenticated SFTP subsystem boundary.
Incoming frames are limited to 1 MiB before their prefix reaches russh-sftp;
zero/oversized/truncated frames terminate that stream. Five focused tests pass,
including a real SFTP handshake over an in-memory duplex stream. The complete
`npm run check` passes; the entry bundle is 317.7 KiB against a 325 KiB budget.
Compatibility and request-timeout behavior are documented in
[Incoming SFTP packet bounds](../../docs/sftp-packet-bounds.md).
