# Archive extraction safety

The extraction pipeline applies the same entry validation to ZIP, tar, tar.gz,
tar.xz and 7z. It rejects absolute paths, parent traversal, alternate separators,
control characters, Windows device names, link records, duplicate paths and
case/Unicode normalization collisions. Valid names retain their spelling.

Metadata is checked before extraction. The writer also counts actual bytes and
rejects a stream that exceeds its declared size or the configured limits. A
shorter stream is treated as corrupt. Metadata is compared again during extraction
because an archive may change between its metadata and data passes.

Output is written to a random private staging directory inside the selected
existing destination. File operations use directory handles through cap-std. Unix staging directories
use mode 0700. On Windows a protected, inheritable owner/LocalSystem DACL is
applied through a directory-relative handle before any archive data is created.
No archived path is passed directly to an ambient filesystem write. Before
publication, targets are checked for links and incompatible file types. Old files
are moved into staging backups before replacement. A publication error rolls back
completed replacements. A failed rollback preserves recovery data and reports its
location; a cleanup failure is reported rather than hidden.

## Limits

An optional archive-limits.json file in the application configuration directory
can override these defaults. Missing fields retain defaults; unknown fields and
zero limits are rejected. Changes apply to subsequent extractions.

```json
{
  "entries": 100000,
  "totalBytes": 21474836480,
  "entryBytes": 4294967296,
  "depth": 64,
  "pathBytes": 4096,
  "ratio": 1000
}
```

The ratio compares total declared/actual expanded bytes with the input archive's
size, using overflow-safe arithmetic. It is an aggregate expansion limit.
Extraction is transactional within the running operation; recovery after an
application or machine crash is a separate concern from rollback on a reported
error. Decoder-internal allocation budgets are tracked in TICKET-067.

## Regression corpus

The adjacent Rust tests generate deterministic hostile fixtures, so no downloaded
archive is needed. They cover traversal, absolute/drive paths, alternate
separators, duplicates, case collisions, Windows device names and links in tar,
gzip/xz-wrapped tar and 7z, plus ZIP traversal/collisions and symbolic links.
Tests also exercise a destination directory replaced by a link, rollback after a
publication failure, abandonment/cancellation and round trips for all five
formats. The shared validator tests integer boundaries, actual bytes, implicit
parent conflicts and Unicode-equivalent names.

Run the focused suite with:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked commands::fs::archive::
```

The complete project gate remains `npm run check`.
