# ZIP-LZMA method-14 decoding

The adapter follows [PKWARE APPNOTE 6.3.10, section 5.8](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT).
It reads the SDK-version bytes and the little-endian property length, requires
the supported five-byte property layout, and validates the packed properties and
dictionary bound before decoding. SDK-version bytes are informational; a different
version with the same supported layout remains readable.

ZIP stores the decoded size in its file metadata. The adapter supplies that size
to the already pinned xz2/liblzma decoder through a synthetic LZMA-alone header.
When general-purpose bit 1 is set, the decoder instead receives an unknown size
and requires the stream's end marker. An independent output counter always checks
the ZIP-declared size, including empty members. CRC verification remains in the
existing ZIP reader. Both seekable and non-seekable ZIP readers pass the correct
size and flag. Raw LZMA-alone payloads incorrectly labelled as ZIP method 14 are
not accepted as an alternate format.

Decoder memory remains limited to 64 MiB and queued decoded output to 8 MiB.
Input is consumed once, at most 8 KiB per feed. Scope checks remain before input
and output work; no worker thread bypasses the thread-local budget. Memory-limit
failures map to the existing archive-limit error. Truncation, unsupported properties,
trailing compressed bytes, missing required end markers and decoded-size mismatch
fail instead of producing a successful partial result.

The ZIP feature now uses the project's existing xz2 dependency instead of lzma-rs;
no new crate or system executable is required at runtime. The auditable patch in
`vendor/archive-budget/upstream.patch` includes the framing and flag changes and
applies to the original ZIP 2.4.2 sources.

## Verification

[Independent fixtures](../src-tauri/src/commands/fs/archive/fixtures/zip-lzma/README.md)
come from Python zipfile and a standalone liblzma encoder. The latter explicitly
omits end markers, so tests do not simulate that case by merely truncating an
EOS stream. Fixtures and their generators are checked in; ordinary tests do not
invoke external tools.

Tests cover normal and empty members, with and without EOS, repeated small reads,
local-header streaming, version bytes, property lengths and values, CRC failures,
wrong decoded sizes, truncated headers/payloads, trailing data, dictionary refusal,
cancellation and high-expansion buffering. The existing input-consumption test now
uses a conforming ZIP header. Run `npm run check` for the complete regression gate.
