# TICKET-055: Define a canonical VFS address and path model

## Goal

Represent local and remote locations with one lossless, typed, canonical model so
navigation, caching, capabilities, transfers, diagnostics, and future external APIs
do not rely on ambiguous string parsing.

## Background

The backend currently parses addresses by splitting `scheme://`, authority, the
last `@`, and the last `:`. Frontend path helpers independently manipulate remote
strings with regular expressions. Provider paths are passed through `Path`/
`PathBuf` even though remote paths are POSIX-like, and WebDAV permits a scheme in
the profile host that is stripped in some identities and reused in others. This
model does not explicitly cover IPv6 literals, percent encoding, Unicode
normalization, case sensitivity, default ports, UNC paths, separator semantics, or
the distinction between display and identity.

Typed canonical addresses are foundational for TICKET-031 capabilities,
TICKET-039 persistence, TICKET-044 recovery, TICKET-053 caching, and the planned
plugin/MCP boundaries.

## Requirements

- Define serialized types for local paths, remote authorities, provider paths, and
  stable location/store identities instead of passing raw combined strings through
  internal APIs.
- Parse raw user input only at designated boundaries and serialize canonical display
  and identity forms through one shared specification.
- Support hostname and IPv4/IPv6 authorities, explicit/default ports, Unicode
  usernames/paths, percent encoding, spaces, reserved characters, roots, and empty
  path segments without ambiguity.
- Keep passwords, tokens, and other secrets invalid in VFS addresses.
- Model local Windows drive, UNC, extended-length, macOS/Linux absolute, and relative
  paths without applying remote separator rules to local data.
- Model remote paths as provider-defined byte/Unicode-safe components rather than
  host-platform `PathBuf` semantics where that would change separators or validity.
- Define equality, ancestry, join, parent, basename, case sensitivity, normalization,
  and display rules per provider. Do not use display strings as security identity.
- Preserve user-visible spelling where possible while maintaining a separate
  canonical key for connection, cache, journal, and capability lookup.
- Reject malformed authority, invalid port, embedded control character, unsupported
  scheme, traversal outside a provider root, and ambiguous normalization with
  structured errors.
- Generate or share conformance fixtures between Rust and TypeScript so frontend and
  backend operations cannot drift.
- Version serialized location types and provide migrations for profiles, sessions,
  histories, journals, and cached state.

## Acceptance Criteria

- [x] Internal VFS APIs use typed locations; raw address parsing is confined to
      documented input/compatibility boundaries.
- [x] Rust and TypeScript produce identical canonical forms and relationships for
      the shared fixture corpus.
- [x] IPv6, Unicode, spaces, percent escapes, default ports, Windows drives/UNC,
      roots, and reserved characters behave predictably.
- [x] Secrets cannot be embedded in or serialized as locations.
- [x] Display form is never used as connection, authorization, cache, or journal
      identity.
- [x] Existing saved profiles, tabs, history, and operations migrate without losing
      valid locations.
- [x] Provider root containment cannot be bypassed by alternate encodings.

## Tests

- Build a cross-language golden corpus covering every scheme/platform, IPv6,
  Unicode normalization forms, case behavior, escaped separators, malformed input,
  and traversal attempts.
- Property-test parse/serialize round trips, join/parent invariants, equality, and
  ancestry without lossy normalization.
- Test visually identical but byte-distinct names on providers that preserve them.
- Test profile/session/journal migrations and unknown future serialized versions.
- Fuzz parsers with control characters, overlong components, delimiter mixtures,
  invalid encodings, and secret-bearing URI forms.
- Run provider integration tests proving typed paths map to the same remote objects
  across Windows, macOS, and Linux clients.

## Definition of Done

- One versioned specification governs local paths, remote addresses, and identities.
- Rust and TypeScript share conformance fixtures and cannot silently diverge.
- Internal consumers use typed locations and separate display from identity.
- Existing persisted data migrates safely.
- Path ambiguity and alternate encoding cannot bypass provider-root policy.
- Rename this file to `055-canonical-vfs-address-and-path-model.done.md`.


## Implementation and verification

Implemented the version-1 typed local/remote model, canonical authority and
location identities, validated URI parsing/serialization, provider-component
containment, and generated TypeScript bindings. Existing string-valued commands
are documented compatibility boundaries returning typed locations internally.
Navigation and capability caching use the shared presentation specification.

Migrated literal remote paths in sessions/history/preferences (schema 2), profiles
and MCP configuration (version-1 envelopes). Recovery/search records keep decoded
paths and accept previous endpoint digests. Unknown future schemas are refused.

See [VFS locations](../../docs/vfs-locations.md) for exact rules and compatibility
limits. Repeated remote separators are represented losslessly but execution is
refused before I/O until [TICKET-095](../095-provider-component-streaming-paths.md)
replaces the remaining streaming adapters. Native Windows/Linux evidence and
adapter work are deferred to [TICKET-259](../259-vfs-native-provider-path-conformance.md)
under the user's scheduling instruction. This ticket does not claim those native
protocol tests have run.

Validation: `npm run check` passed. Shared golden cases, bounded parser round-trip
checks, malformed/secret input rejection, provider dispatch, typed identity,
MCP containment, persistence migrations and both IPC modes are covered. Existing
live protocol/keychain tests remain ignored in the ordinary check.
