# Archive decoder budgets (TICKET-067)

The expansion/path/staging checks tracked by TICKET-113 (originally 046) remain in place. They cannot
prevent a library from allocating an advertised dictionary or encoded header before
returning an entry. The decoder boundary now has a separate, non-configurable set
of limits, enforced before those allocations. Raising archive-limits.json values
does not disable them. ZIP, tar, tar.gz, tar.xz and 7z remain enabled.

## Policy

| Resource | Limit |
| --- | --- |
| One raw/decoded 7z header, ZIP64 extension, or tar extension body | 16 MiB |
| Files / pack streams / folders / total 7z substreams | 100,000 |
| Coders in one 7z folder | 16, with checked acyclic stream graph |
| One LZMA/LZMA2 dictionary | 64 MiB |
| XZ decoder memory / ZIP-LZMA decoder buffer memory | 64 MiB |
| Zstd window in ZIP | 64 MiB (window log 26) |
| ZIP-LZMA pending output | 8 MiB |
| Charged metadata plus live 7z decoder reservations per scope | 512 MiB |
| Work checkpoints plus declared AES derivation rounds per scope | 32,000,000 |
| AES derivation exponent | at most 20; specification's direct-key value 63 allowed |
| Password UTF-8 input | 4,096 bytes |
| Listing / extraction preflight wall deadline | 30 seconds |
| Extraction decoding wall deadline | 30 minutes |

These are algorithmic/allocation limits, **not a process RSS quota or an OS CPU
quota**. Metadata charges are cumulative, conservatively covering copies; live 7z
decoder reservations are released between streams. Each decoder reserves its
advertised dictionary plus 16 MiB for fixed/probability buffers. Native ZIP/XZ
buffers, the application's validated manifest, staging buffers, and unrelated app
memory are outside that accounting. Concurrent operations have independent scopes.

CPU protection consists of bounded structural counts and AES work factors, a shared
work counter, and cooperative deadline/cancellation checks. Input and 7z decoder
output reads are capped at 64 KiB, buffered-header parsing checks the same scope,
and AES checks cancellation every 1,024 rounds. This does not preempt a blocked
filesystem syscall. Failed budgets stay failed for the remainder of a scope, even
if an upstream parser retries or wraps an error as a password failure.

Listing and preflight normalize a tripped scope to the existing localized
`archive.limit` or `cancelled` code. Extraction checks failure before
publication and cleans its private stage. Typed liblzma memory refusals are also
mapped to the limit code. Other decoder failures keep their existing structured
error mapping. Parser panics are contained around decoding, so the caller can still
run its staging cleanup; this is not a substitute for parser validation.

## Dependency decision and maintenance

The pinned sevenz-rust API has no usable caller-provided metadata allocation budget;
its original LZMA1 path does not enforce the LZMA2 memory parameter. Inspection of
the newer [sevenz-rust2 ArchiveReader API](https://docs.rs/sevenz-rust2/0.22.2/sevenz_rust2/struct.ArchiveReader.html)
also did not reveal a complete header/allocation budget interface. The project
therefore maintains small patches against the already locked sources, using one
shared `archive-budget` API, instead of claiming that a version bump alone solves it.
No system binaries, password-bearing command lines, or platform worker are involved.

Original registry releases/checksums (before the path patches):

| Crate | Version | Registry SHA-256 |
| --- | --- | --- |
| sevenz-rust | 0.6.1 | 26482cf1ecce4540dc782fc70019eba89ffc4d87b3717eb5ec524b5db6fdefef |
| zip | 2.4.2 | fabe6324e908f85a1c52063ce7aa26b68dcb7eb6dbc83a2d148403c9bc3eba50 |
| tar | 0.4.46 | 3f6221d9a6003c78398e3b239969f352578258df48c8eb051caadae0015bc840 |

Sources and licenses are retained under vendor. The exact upstream diff is retained
in vendor/archive-budget/upstream.patch for review and rebasing. All three manifests depend on
vendor/archive-budget. Source changes are limited to:

- sevenz-rust: reader.rs (headers/counts/stream graphs/checkpoints), decoders.rs
  (dictionary reservations), aes256sha256.rs (bounded KDF, cancellation and correct
  direct-key salt copying).
- zip: read.rs (central metadata), spec.rs (ZIP64 counts/extensions), compression.rs
  (XZ/Zstd windows and checks), read/lzma.rs (memory/output bounds, consume input once,
  finish buffered output at EOF).
- tar: archive.rs (entry/sparse-record accounting), entry.rs (extension-body limits).

The local API belongs to the backend's dependency boundary, contains no UI/IPC
business logic and stores no file names or passwords. Scopes and reservations must
stay on their decoder thread; decoders are dropped before their scope ends.
Future upgrades must carry or replace these checks, then rerun the malicious
fixtures. TICKET-132 tracks upstream migration/patch retirement.

Two existing defects surfaced during this work. The root dependency did not enable
7z AES support despite advertising passwords; the feature is now enabled and tested.
The ZIP-LZMA adapter neither consumed buffered input nor finished the decoder's
pending output; both are corrected to make the bounded adapter usable. TICKET-131
tracks independent ZIP-LZMA framing/conformance with externally generated archives;
the adapter's legacy LZMA-alone framing is not claimed to cover all standard ZIP
method-14 inputs.

## Tests

`cargo test --manifest-path src-tauri/Cargo.toml commands::fs::archive --locked`
covers all archive adapters plus the added allocation fixtures. The new tests use
small inputs advertising huge resources, not huge actual allocations:

- 1 TiB 7z next-header size, excessive file count and expanded encoded header;
- 4 GiB dictionary property in an encoded 7z header, before any output;
- cyclic coder graph and excessive cumulative work;
- excessive ZIP64 central-directory count;
- oversized PAX body behind gzip and oversized XZ dictionary;
- cancellation while reading a header into its parser buffer, before an entry;
- cancelled AES derivation, oversized work factor, expired deadline, live memory
  reservation exhaustion and release;
- encrypted 7z header/payload round trip (20 files), missing and wrong passwords;
- legacy ZIP-LZMA stream consumption, final output and CRC;
- existing all-format listing/extraction, rollback, path and expansion checks.

`npm run check` remains the release gate. No Windows/Linux-specific implementation
or manual platform campaign was added for this ticket.
