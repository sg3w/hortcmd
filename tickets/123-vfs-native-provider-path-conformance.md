# TICKET-123: Verify native Windows/Linux VFS provider paths

## Scope

Follow-up to TICKET-055, deferred under the user's instruction to postpone
Windows/Linux work. The versioned address codec and lexical local-path model
have shared Rust/TypeScript fixtures; native provider execution needs separate
platform evidence.

- Replace the remaining streaming-provider `Path`/`PathBuf` compatibility
  adapters with native-local versus provider-component path interfaces before
  enabling remote names whose meaning differs under Windows separators.
- Verify SFTP, FTP, SMB and WebDAV against the same seeded objects on Windows
  and Linux, including IPv6, percent signs, Unicode normalization variants,
  reserved characters, repeated separators and case-sensitive stores.
- Verify native drive-relative, UNC and extended-length paths without crossing
  a share root, plus non-UTF-8 provider names: support them losslessly or fail
  explicitly, never substitute replacement characters.
- Exercise migrated sessions, profiles, MCP roots and transfer journals on
  both platforms; unknown future versions must not be overwritten.

## Acceptance

- Native tests demonstrate identical object identity across clients.
- Remote joins and basenames do not inherit Windows `PathBuf` semantics.
- Record the tested provider/server versions and any refused capabilities.
- Pass `npm run check` plus the explicit native protocol test matrix.
