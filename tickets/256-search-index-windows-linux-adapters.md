# TICKET-256: Native Windows/Linux scope validation and resource controls for indexed search

Deferred from TICKET-054 under the user's instruction to move Windows/Linux work back.

## Goal

Enable local indexed search only after equivalent native no-follow root/component
opens, stable object/mount identity, hidden/system policies, restrictive persistent
storage and battery/memory-pressure controls are implemented and tested.

## Acceptance Criteria

- [ ] Windows junction/reparse points and Linux symlinks/mount changes cannot expand an approved scope.
- [ ] Native battery and memory-pressure behaviour is tested without silent opt-in.
- [ ] Checkpoint, permissions, corruption and search-equivalence tests run natively.
- [ ] The currently explicit unsupported response can safely be replaced.

Until completion, local index creation fails closed on those platforms. Existing
live search continues to work. No Windows/Linux implementation is included in 054.
