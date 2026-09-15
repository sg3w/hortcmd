# TICKET-118: Expose negotiated TLS details from client APIs

## Problem

TICKET-047 exposes protection state, verification and certificate fingerprint.
The currently used FTPS/WebDAV clients do not expose their active TLS connection's
negotiated version and cipher through their public APIs. The UI explicitly marks
those values unavailable. This diagnostics/API work belongs in the later backlog.

## Requirements

- Obtain version/cipher from the actual established FTPS/WebDAV connection through
  a supported client API or a small maintainable upstream contribution.
- Do not substitute configured minimums, infer a cipher or open an unrelated probe
  connection and present its results as the active session.
- Cover control and data-channel differences where the client exposes them.
- Never use key logging or persist traffic secrets to obtain telemetry.
- Keep the shared verifier, profile pins, secure defaults and no-downgrade behavior.
- Include SFTP negotiated cipher telemetry if its client API exposes it safely.

## Acceptance Criteria

- [ ] Real negotiated values are shown where available and unknown remains explicit.
- [ ] TLS/security regression tests and `npm run check` pass.

## Definition of Done

Document API sources and validation; rename with `.done.md` after completion.
