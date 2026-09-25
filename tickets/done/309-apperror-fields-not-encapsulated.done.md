# TICKET-309: Encapsulate AppError's detail/params fields

Discovered during a full-codebase review (2026-09-22).

## Problem

Every field of `AppError` in `src-tauri/src/errors/mod.rs` (around line 74),
including `detail` and `params`, is `pub`. Nothing in the type system enforces
that `detail`/`params` can only be populated through the redacting
`.with()`/`.detail()` builders that the module's header comment promises
("centralized… not at call sites").

No current constructor bypasses the builders (verified by grepping every
`AppError`-returning constructor in the crate), but because the fields are
public, a future call site can write
`AppError { detail: Some(raw_provider_reply.into()), ..AppError::new(code) }`
or `err.params.insert("msg".into(), raw)` — this compiles without warning and
ships an unredacted secret straight to the frontend or an MCP client.

## Required change

- Make `detail` and `params` (at minimum) private or `pub(crate)`, keeping
  `#[serde]` working via the existing builder pattern, so the only way to
  populate them from outside `errors::mod` is through `.with()`/`.detail()`,
  which redact.
- Fix any resulting compile errors at call sites by switching to the builders.

## Verification

- `cargo build`/`cargo clippy` after the visibility change confirms no
  remaining direct field construction outside the module.
- `npm run check`.

## Implementation

Parameters and diagnostic details are private, with immutable accessors. External
callers must use the redacting builders; Serde and ts-rs retain the same wire
shape. This enforces construction discipline, not generic sanitization of every
possible secret format. Existing error-contract tests exercise serialization.
