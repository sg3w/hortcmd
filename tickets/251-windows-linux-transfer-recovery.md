# TICKET-251: Enable and verify transfer recovery on Windows and Linux

## Goal

Extend the desktop recovery journal from TICKET-044 to Windows and Linux after
the current macOS feature batch. Platform work is deferred by user instruction.

## Requirements

- Implement and verify private journal/staging permissions, exclusive process
  ownership, atomic snapshot replacement and durable file/directory publication.
- Verify safe object identity and cleanup refusal when staging paths are replaced.
- Enable the recovery adapter only after native durability tests pass.
- Run sequential/parallel copy, copy-based/native move, pack and extract process
  termination tests at every journal and publication boundary.
- Exercise local and remote destinations with safe/direct mode and checksum on/off;
  remote sessions must reconnect through the existing verified credential flow.
- Test the recovery dialog, retained queue order, cancellation and webview reload
  on both native platforms. Record environments and commands.

## Acceptance Criteria

- [ ] Windows adapter and native fault-injection tests pass.
- [ ] Linux adapter and native fault-injection tests pass.
- [ ] Neither platform deletes an uncommitted source or unverified backup.
- [ ] Documentation identifies the verified durability guarantees and limitations.

## Definition of Done

Run `npm run check`, record native evidence, and rename this ticket with `.done.md`.
