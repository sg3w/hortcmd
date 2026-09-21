# TICKET-131: Correct ZIP-LZMA framing with independent conformance fixtures

## Implementation

- Parse method-14 version/property headers and retain the ZIP EOS flag.
- Use ZIP metadata for the decoded size and require EOS when flagged.
- Adapt to the existing pinned xz2/liblzma backend; remove unused lzma-rs.
- Preserve 64 MiB decoder memory, 8 MiB queued-output and cancellation limits.
- Reject malformed, truncated, oversized and CRC-invalid inputs.
- Check in independent Python and liblzma fixtures plus reproducible generators.
- Update the vendor patch and verify it applies to original ZIP 2.4.2 sources.

## Verification

Nine targeted conformance/budget tests pass, covering normal and empty files,
EOS/no-EOS, repeated reads, streaming local headers, SDK versions, property sizes,
CRC, declared sizes, truncation, trailing bytes, dictionary limits, cancellation
and bounded expansion. The old input-consumption regression now uses correct ZIP
framing. Validation passed: `npm run check` with 611 Rust unit tests, four native
plugin integration tests, 746 frontend tests and the production bundle within
budget. Fixture regeneration is byte-for-byte reproducible and independently
verified by Python.

See [ZIP-LZMA conformance](../../docs/zip-lzma.md).
