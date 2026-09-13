# TICKET-067: Bound memory used by archive decoders

## Finding
While implementing 046, source inspection found that sevenz-rust 0.6.1 uses
MAX_MEM_LIMIT_KB = usize::MAX / 1024 in its reader, including encoded headers.
Validating expanded bytes at the output writer therefore does not limit decoder
allocation before the first entry reaches the shared validator. ZIP central
metadata and compressed tar headers also need allocation-budget review.

## Goal
Enforce explicit memory and CPU budgets before decoding attacker-controlled
archive metadata or payload. Preserve password support and existing formats.

## Acceptance
- Select a maintained API exposing bounded decoder/header allocation, or isolate
  decoding in a worker with enforceable platform resource limits.
- Reject excessive dictionaries, encoded headers and metadata counts without
  allocating the advertised amount in the desktop process.
- Test malicious headers and cancellation before the first output byte.
- Keep errors structured and localized, and never log passwords.
- npm run check passes; commit separately and do not push.

## Relationship
046 adds entry validation, declared/actual expansion limits, staging and rollback.
This ticket covers allocations inside dependencies that occur before that layer.
Schedule before claiming broad resource-exhaustion protection for archives.
