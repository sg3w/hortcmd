# VFS locations, version 1

TICKET-055 defines `VfsLocation` as a versioned local/remote union. Rust owns
provider resolution and authorization. TypeScript uses generated types and a
presentation codec checked against the same [golden corpus](../fixtures/vfs-locations.json).

## Boundaries and representation

String-valued Tauri/MCP commands retain their public signatures. `parse_address`
is their compatibility input boundary and returns the typed location;
`resolve_location` performs typed provider lookup. `vfs_location` exposes the
typed value, canonical URI, location/store identities and parent to IPC clients.
Streaming interfaces carry `StreamPath::Local(PathBuf)` or
`StreamPath::Remote(ProviderPath)` through planning, traversal, staging, publication
and rollback. Only native-local boundaries can obtain an OS path; remote
providers reject local variants. Windows/Linux native verification is tracked
in [TICKET-259](../tickets/259-vfs-native-provider-path-conformance.md).

Local paths retain their original data: no URI unescaping, Unicode normalization,
case folding or remote separator rules. POSIX backslashes are literal. Windows
drive, UNC and extended-length forms have an explicit local flavor; lexical
parents stop at their drive/share root. Filesystem authorization still resolves
native paths and symlinks; lexical identity is not evidence of object ownership.
For compatibility, `file:///path` and `local:///path` wrap literal local paths.

Remote paths contain decoded Unicode components, including empty components.
Only `/` separates components. The parser decodes percent escapes exactly once,
rejects encoded `/`, controls, invalid UTF-8, and `.`/`..` components. SMB also
rejects backslashes because the protocol treats them as separators. Other
providers preserve them. URI output escapes reserved delimiters and literal `%`;
spaces and Unicode remain readable. `%2520` names the literal component `%20`.
Repeated separators are preserved; no Unicode normalization or filename case
folding is performed. Conservative equality does not equate two differently
spelled names merely because some servers do.

[TICKET-095](../tickets/done/095-provider-component-streaming-paths.done.md)
removes the generic repeated-separator refusal. `/a//b` retains parent `/a//`
and its staging and backup files stay in that exact directory. FTP, SFTP and
WebDAV forward the preserved path; SMB still rejects repeated separators and
backslashes because its native namespace cannot safely represent them.
Recovery records preserve the same identity across journal reopening. Remote
cleanup remains conservative when ownership cannot be proven.

Authorities accept hostnames, IPv4 and bracketed IPv6. Hostnames are lowercased,
IPv6 compressed, ports validated in 1–65535, and missing ports receive the
scheme's default in identity keys. Username delimiters are escaped; password
userinfo, query/fragment credentials and malformed authorities are refused.
Locations are limited to 32 KiB at the input boundary. Invalid input reports
`validation.badInput` with operation `vfs-location` and a reason, without echoing
secret-bearing input. Deserialization rejects unknown versions and fields.

`StoreIdentity` contains scheme plus the canonical authority key.
`LocationIdentity` additionally includes the exact decoded path, separated by
a control character forbidden in input. Display URIs are not authorization
keys. Cache entries also retain provider-generation/object-version information;
MCP containment compares typed provider components and checks native identity
for local paths. Remote lexical containment does not prove server-side symlink
containment; the existing provider capability restrictions still apply.

## Stored data

- Session, history and preferences advance to schema 2. Their old remote paths
  had literal percent signs; migration escapes those once. Local paths remain
  unchanged. Current-schema loads do not apply the migration again.
- Profile files accept the old raw-field array and write a version-1 envelope.
  Profile IDs, secret-store references and literal start paths are retained.
- MCP configuration accepts the old object and migrates its literal remote
  roots before writing a version-1 envelope. Failed migration disables the
  configuration rather than broadening access.
- Recovery journals and encrypted search snapshots already store a profile ID
  and decoded path separately. They retain that representation; URI output now
  escapes it, and lookup accepts the previous endpoint digest alongside the
  version-prefixed canonical digest. A changed endpoint is still refused.
- Presentation caches are memory-only and restart empty. Unknown future profile
  and MCP file versions are refused on write to preserve the original file.

Ambiguous legacy remote dot paths are refused rather than silently normalized
across possible server symlinks. Their saved data is retained for correction.

## Verification

The shared corpus covers all schemes, IPv6 (including IPv4-mapped addresses),
Unicode spelling variants, default ports, reserved characters, empty components,
local drive/UNC forms, malformed input and legacy migration. Additional tests
cover component round trips, root containment, serialization validation,
Zustand migration, profile/MCP migration and both IPC adapters.

Streaming regression tests use a fake store where `/a/b` and `/a//b` name
different resources. They exercise preflight traversal, verified copy, failed
publication and rollback, reopened journals and MCP containment. No live remote
server or Windows/Linux native matrix was run for this ticket.
