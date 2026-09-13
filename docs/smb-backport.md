# hortcmd compatibility patch (TICKET-071)

Base: published `smb` crate 0.11.1, MIT, upstream repository
https://github.com/afiffon/smb-rs, commit
`bc2024d866fde96c2eb1ab30c78d2f8a6b192fa9`, directory `crates/smb`.
Published `.crate` SHA-256: `502b7620c9f63f2aabccfeb6fe454d0ea3e86731ff76f961126d9a249f06edf4`.
Copied from Cargo's unpacked registry release; packaging markers and the
crate-local Cargo.lock are omitted. Cargo.toml and sources retain upstream text.

The sole source change is in `vendor/smb/src/resource/file.rs`, file-read response options:

```diff
- .sendo_recvo(request, ReceiveOptions::new())
+ .sendo_recvo(request, ReceiveOptions::new().with_allow_async(true))
```

This exact fix appears in published 0.11.2. Servers may return STATUS_PENDING
and an asynchronous completion for file reads; rejecting it causes intermittent
large-transfer failures. Version 0.11.2 requires Rust 1.89, while hortcmd supports
Rust 1.85.1. The local Cargo patch preserves the compatible release with this
specific correction. No transfer staging or source-deletion behavior is changed.

Remove this override when a maintained dependency release provides the fix with
a compatible MSRV, or when the project deliberately raises its supported MSRV.
Verify with `npm run check`, the declared-MSRV build and the real Docker protocol
harness. Do not edit the global Cargo cache to apply the fix.
