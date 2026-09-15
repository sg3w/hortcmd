# TICKET-117: Verify transport security on native platforms and SMB servers

## Goal

Complete the native Windows/Linux and live SMB verification matrix for TICKET-047.
This platform/support work is deferred by user instruction; macOS loopback TLS
and frontend checks belong to the implementation ticket.

## Requirements

- Run FTPS/HTTPS certificate tests on native Windows and Linux trust stores.
- Exercise known-revoked certificates, trust-store changes, proxy infrastructure
  and expired/future/wrong-host/rotated certificates without sending credentials
  to a rejected peer.
- Run the existing isolated protocol harness, including explicit and implicit
  FTPS, private-CA WebDAV, and the deliberately enabled plaintext fixtures.
- Test SMB servers with encryption/signing available, unavailable, optional and
  required. Confirm policy refusal and the reported negotiated state.
- Record native UI behavior, environments and packet-level encryption evidence.
- Keep trust exceptions profile-scoped; never weaken validation to pass a test.

## Acceptance Criteria

- [ ] Windows native matrix recorded.
- [ ] Linux native matrix recorded.
- [ ] SMB interoperability and required encryption/integrity matrix recorded.
- [ ] Known-revocation and packet-level evidence recorded.

## Definition of Done

Run `npm run check`, attach reproducible native evidence, and rename with `.done.md`.
