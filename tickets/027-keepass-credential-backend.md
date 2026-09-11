# TICKET-027: KeePass backend for credentials (optional)

## Goal
As an alternative to the OS keychain, credentials can be stored in a portable, encrypted
**KeePass file (`.kdbx`)** — for users who want to control their secrets in their
own, syncable file or reuse an existing KeePass DB.

## Background
TICKET-021 sets the OS keychain as the **default** and introduces the abstraction
`CredentialStore` (trait `get/set/delete`) for it. KeePass is deliberately **optional**: it
brings no security gain over the keychain, but **portability/control**
(a single file, syncable, interoperable with KeePassXC & co.). It therefore requires the
`CredentialStore` abstraction from 021 — this ticket only adds another backend,
without changing the callers.

## Requirements
- **`KeepassStore`** as a second `CredentialStore` implementation (read **and**
  write `.kdbx`), e.g. via the `keepass` crate.
- **Unlock**: master password and/or key file; keep the password only in memory,
  never persist it.
- **Backend selection** in the settings: OS keychain (default) ↔ KeePass file;
  path to the `.kdbx` configurable, KeePassXC-compatible.
- **Unlock UX**: prompt for the master password on first access per session; optionally
  "remember for this session". Do **not** persist automatically and permanently (otherwise
  chicken-and-egg — then the keychain would be the better choice); if still wanted, only the
  unlock key in the OS keychain, a clearly labeled option.
- **Interoperability**: a file created with KeePassXC is readable; entries written by us
  are readable in KeePassXC.
- **Correct crypto usage**: adopt/preserve current KDF parameters (Argon2);
  file integrity on write (no data loss on abort).
- No change of the default: without an explicit selection the OS keychain stays active.
- Location: `src-tauri/src/net/credentials.rs` (next to `KeychainStore`).

## Acceptance criteria
- [ ] KeePass can be chosen as the backend in the settings and a `.kdbx`
      (password and/or key file) attached.
- [ ] Credentials are stored in the `.kdbx` and read from it; nothing ends up
      in plain text.
- [ ] A file created with KeePassXC is readable/writable and vice versa (interop).
- [ ] The master password is not stored permanently without being asked.
- [ ] Without an explicit selection the OS keychain stays the default (regression-free).
- [ ] All remote providers (SFTP/SMB/WebDAV/FTP) work with the KeePass backend
      identically to the keychain (thanks to `CredentialStore`).

## Tests
- Create a `.kdbx` with KeePassXC → open it in hortcmd, read an entry; write an entry in
  hortcmd → check it in KeePassXC.
- Wrong master password → defined error, no crash.
- Backend switch keychain ↔ KeePass without changing the provider callers.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged (OS keychain stays the default)
- Documentation updated
- Rename the ticket to *.done.md
