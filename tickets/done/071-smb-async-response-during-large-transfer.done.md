# TICKET-071: Investigate SMB async response during large transfers

## Finding
Remote harness run 6f9f5b failed while copying `large.bin` from WebDAV to SMB:
`Invalid argument: Async command is not allowed in this context.` The preceding
run 84f08d passed this cross-provider transfer, so the failure is intermittent.
Both runs used the locked smb 0.11.1 dependency selected for Rust 1.85.1 in 068.
The new MCP live-remote read/scope/disconnection test passed all seven services
before this transfer failed. No claim of data loss has been established.

## Investigation and acceptance
- Reproduce with the existing Docker harness, retaining redacted failure logs.
- Identify the command receiving an async SMB reply and handle it or select a
  compatible dependency fix; do not silently raise the declared Rust MSRV.
- Preserve transfer staging and source data on failure.
- Verify large cross-provider and same-server SMB transfers, cancellation and
  the full npm run check. Commit separately, without pushing.

Follow-up discovered during 041's external integration verification. This does
not invalidate the successful MCP read test; the broader transport suite must
not be reported as fully passing while this failure remains unresolved.

## Diagnosis
Run 6f9f5b also failed the large-content stream and same-server SMB copy checks
(16 protocol tests passed, 3 failed). Inspection of the cached published sources
shows `smb 0.11.1` sends file reads with `ReceiveOptions::new()` (async replies
forbidden), while `smb 0.11.2` uses `.with_allow_async(true)` at the identical call
site in `src/resource/file.rs`. Version 0.11.2 declares Rust 1.89; 068 restored the
project's Rust 1.85.1 floor by selecting 0.11.1. This is therefore an identified
compatibility regression from that downgrade, requiring an urgent backport or
another validated compatible release before closing 041.

## Completed — 2026-09-13
- Backported the exact file-read receive-options correction from 0.11.2 onto
  the published 0.11.1 source in vendor/smb. Cargo uses a local patch override.
  MIT license, source commit, crate checksum and removal criteria are documented
  in docs/smb-backport.md. A byte comparison confirmed this is the only upstream
  source change. Transfer publication/source-removal behavior is unchanged.
- Rust 1.85.1 `cargo check --locked --all-targets` passed.
- `npm run check` passed: 366 Rust tests, 24 ignored external tests, 371 frontend
  tests; all other gates passed. An upstream unused-import warning remains in
  the dependency; project strict Clippy passes.
- Real Docker harness run 599e21 passed all 19 tests, including large SMB streams,
  cross-provider and same-server copies, interruption handling, staging and
  MCP reads on all seven services. The previously failing paths now pass.
- The documentation checker skips third-party vendor documents, as it already
  skips installed dependencies; project-owned provenance documentation remains
  checked normally.
