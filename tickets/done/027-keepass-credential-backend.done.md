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
- [x] KeePass can be chosen as the backend in the settings and a `.kdbx`
      (password and/or key file) attached.
- [x] Credentials are stored in the `.kdbx` and read from it; nothing ends up
      in plain text.
- [x] A file created with KeePassXC is readable/writable and vice versa (interop).
- [x] The master password is not stored permanently without being asked.
- [x] Without an explicit selection the OS keychain stays the default (regression-free).
- [x] All remote providers (SFTP/SMB/WebDAV/FTP) work with the KeePass backend
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

## Implementation

### Backend (`src-tauri/src/net/credentials.rs`)
- **`KeepassStore`** as a second `CredentialStore` implementation, on top of the
  `keepass` crate with the `save_kdbx4` feature (reads **and** writes `.kdbx`).
  Entries are addressed by **title = connection id**; a lookup walks the whole
  group tree, so entries the user filed anywhere in a shared database are found.
  New entries land in a group `hortcmd` and carry a `Notes` line saying where
  they come from.
- **`Store`** routes every `get/set/delete` to the configured backend. The
  callers are untouched: `session.rs` just asks `credentials::store()` instead of
  holding its own `Box<dyn CredentialStore>`, so **all providers (SFTP/SMB/
  WebDAV/FTP) inherit the backend switch for free**.
- **Configuration** lives in `credentials.json` next to `connections.json`
  (`backend`, `kdbx_path`, `key_file`, `master_in_keychain`). It is `#[serde(default)]`,
  and both a missing and an unreadable file fall back to `"keychain"` — **the
  default cannot be lost**.
- **Unlock** (`net_cred_unlock`): master password and/or key file. The opened
  database is cached in memory because Argon2 makes opening deliberately
  expensive; the master password never touches disk. "Remember for this session"
  off ⇒ the session is **one-shot** and re-locks after the single next access.
  `create` starts a new KDBX 4 database at the configured path.
- **Locked ≠ failed.** Every access while locked returns an error prefixed with
  the stable marker `KEEPASS_LOCKED`. The frontend parks the action that hit it,
  shows the unlock prompt and **replays the action** afterwards — including the
  pane navigation that a connect would have done.
- **Opt-in persistence**: with `master_in_keychain` the master password is put
  into the OS keychain under `__keepass_master__`, and the store unlocks itself
  silently on first access. Switching backends or clearing the option deletes
  that entry again. Without the option nothing is ever written.
- **Crypto / integrity**: an opened file keeps its own `DatabaseConfig`, so its
  **KDF parameters (Argon2) are preserved on write**. Writes go to a temporary
  sibling, `sync_all()`, then `rename` over the target — an abort leaves the
  original file intact, and a failed write removes the temp file.
- **New databases** fill `Meta` explicitly (memory protection, history limits,
  recycle-bin/template UUIDs, icon ids). The crate writes `None` fields as *empty*
  XML elements, and KeePassXC refuses to open such a file — this cost a debugging
  round and is now pinned by `new_database_fills_the_metadata_keepassxc_requires`.
- **Crate version**: `keepass` is pinned to `=0.10.5`. Newer versions require
  `hmac ^0.13` (stable), while `smb`/`sspi` pin `hmac =0.13.0-rc.2`; Cargo cannot
  unify those, so 0.10.5 is the newest resolvable release. Revisit when `sspi`
  moves off the pre-release.

### Frontend
- `netStore`: `cred` (backend + lock state), `panel` (`list` | `store` | `unlock`)
  and `pending` (the parked action). `park()` wraps save/remove/connect.
- `ConnectionsDialog`: a **"Speicher …"** button opens the credential-store panel
  (backend select, `.kdbx` path, key file, the clearly labeled keychain opt-in
  with its consequence spelled out, plus lock state and a *Sperren* button), and
  a separate unlock panel (master password, "remember for this session", "create
  the file").
- `cred.*` keys in both dictionaries (de/en).

### Verified
- `cargo check` / `cargo clippy` clean (no new warnings); `cargo test` →
  **150 passed / 9 ignored**, including 9 new KeePass tests: round-trip and
  persistence across a re-open, *the secret never appears in plain text in the
  file*, wrong master password → defined error and still locked, the
  `KEEPASS_LOCKED` marker on all three operations, missing file needs `create`,
  one-shot re-lock after exactly one access, key-file-only unlock, empty key
  rejected, entries found outside our own group and updated in place.
- `npm run typecheck` / `npm run build` clean.
- **Live interop against KeePassXC 2.7.6** (`keepassxc_interop_live`, `#[ignore]`d,
  run with `cargo test -- --ignored`): hortcmd creates a KDBX 4 database and
  writes an entry → `keepassxc-cli` opens it and reads the entry → `keepassxc-cli`
  adds an entry and edits ours → hortcmd reads both changes back. Both directions
  pass.
- **KDBX 3 caveat, deliberately explicit**: `keepassxc-cli db-create` still emits
  the legacy KDBX 3.1 format, which the crate can read but not write. Such a file
  stays usable read-only and a write reports "… nur gelesen werden. Bitte in
  KeePassXC auf KDBX 4 aktualisieren" instead of a raw format error
  (`keepassxc_kdbx3_is_readable_but_not_writable`). Databases created by the
  KeePassXC **GUI** are KDBX 4 and are fully read/write.
- **UI verified** in the browser pane (vite, demo mode): the manager's *Speicher …*
  button opens the panel, switching to *KeePass-Datei* reveals path/key-file/opt-in
  with the hint texts, *Übernehmen* stays disabled until a path is entered, and
  applying with a path leads straight into the unlock prompt (master password
  focused, "remember" pre-checked). Zero console errors.

### Deferred
- No file picker for the `.kdbx`/key file — the paths are typed. A native picker
  would be a nice follow-up but is not part of this ticket.
- Challenge-response (YubiKey) unlocking: the crate supports it behind a feature
  flag; not wired up.
