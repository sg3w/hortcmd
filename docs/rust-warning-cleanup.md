# Rust warning cleanup (TICKET-271)

The application keeps strict Clippy warnings enabled. No `allow(warnings)` or
`no-serde-warnings` feature is used. Repeated build steps previously repeated the
same dependency and derive diagnostics; their count did not represent distinct
failures.

## Binding generator

`vendor/ts-rs-macros` contains the published 10.1.0 sources, MIT licensed, from
Cargo's registry cache. Original crate SHA-256:
`0e9d8656589772eeec2cf7a8264d9cda40fb28b9bc53118ceb9e8c07f8f38730`.
Upstream revision: `0ba56be43af1e438053af2b422a82fcff252f7ef`, directory `macros`.
The MIT license was copied from upstream's v10.1.0 LICENSE. Packaging markers and
Cargo.lock are omitted. `vendor/ts-rs-macros/upstream.patch` records source changes.

The narrow compatibility patch recognizes:

- Enum `serde(deny_unknown_fields)`: no TypeScript shape change; Serde's runtime
  validation remains intact. Struct support already exists upstream.
- Field `serde(skip_serializing_if = "Option::is_none")`: generates an optional,
  non-nullable field, matching omission during serialization. The two production
  transport fields already declare `ts(optional)`, so their bindings stay identical.

Other omission predicates and unknown attributes still take the existing warning
path. The patch does not claim support for arbitrary Serde predicates. Existing
explicit TS attributes remain supported.

`ipc_contract_tests` checks actual transport-field omission and declarations,
optional inference, and enum runtime rejection of unknown fields. The macro's
parser rejection test can be run with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked -p ts-rs-macros --lib
```

Remove the local override when adopting a compatible upstream version supporting
these attributes, retaining the contract tests and checking generated bindings.

## Other dependencies

- sevenz-rust: remove the unused SeedRead marker and its implementation; spell the
  iterator's borrowed lifetime explicitly.
- zip: spell four borrowed return lifetimes explicitly.
- smb: gate the sync_helpers import on std-fs-impls, which uses it.

The archive source patch is regenerated in vendor/archive-budget/upstream.patch;
SMB changes are described in [smb-backport.md](smb-backport.md). These edits change
neither decoder budgets nor transfer/authentication behavior.

Validation: targeted contract and parser tests, followed by `npm run check`.
Frontend tooling diagnostics are separate; the known Vitest mock-hoisting warning
is tracked in TICKET-224.
