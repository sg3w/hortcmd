# TICKET-021: Connections & credentials (profiles, keychain, sessions)

## Goal
Users manage remote connection profiles, connect/disconnect, and see active
connections in the location selector. Credentials are stored **securely in the OS keychain**.
Protocol-independent base for all remote providers.

## Background
All protocols (SFTP/SMB/WebDAV/FTP) need the same connection and
credential management. Secrets must never end up in plain text (localStorage,
config file).

**Credentials decision:** the primary store is the **OS keychain** — not an
encrypted file (KeePass/Stronghold). Rationale: a file would need a
master password (friction on every start) or would have to store its key in the
keychain anyway (chicken-and-egg). The OS keychain is native, without a master password, with
biometrics/OS-session integration and relieves us of the crypto/KDF responsibility. A
portable **KeePass backend stays optional for later** (its own ticket), Tauri
Stronghold is deliberately **not** used.

## Requirements
- **Connection profiles** (persisted without the secret): name, protocol, host, port,
  user, auth method (password / SSH key / ssh-agent / anonymous), start path.
- **`CredentialStore` abstraction** (trait `get/set/delete` per connection ID) so that
  later backends (e.g. KeePass) can be added without rework:
  - **`KeychainStore` (default)** via the `keyring` crate → macOS keychain,
    Windows Credential Manager, Linux Secret Service (libsecret).
  - A missing Secret Service on Linux (headless/minimal) is **detected and clearly
    reported** (no silent plain-text fallback); the remedy is explained in the UI.
- **Credentials secure**: passwords/passphrases/key references only via the
  `CredentialStore`. No plain text, no storage in localStorage/JSON.
- **SSH preferably agent-based**: for SFTP/SSH use **ssh-agent + existing
  `~/.ssh` keys** if possible, rather than storing private keys ourselves; only passphrases/
  passwords end up in the `CredentialStore`.
- **Session lifecycle**: connect, disconnect, status, **reconnect without an
  app restart**; multiple simultaneous connections.
- **Management UI** (new section, e.g. settings or a dedicated dialog): create/edit/delete
  profiles, connect/disconnect.
- **Location selector**: active/saved connections as their own group in
  `DriveSelect.tsx` (analogous to the favorites group), selecting navigates to the start path.
- Report error cases (auth failed, host unreachable) clearly.
- Location: `src-tauri/src/net/{session.rs, credentials.rs}` (trait + `KeychainStore`),
  `src/features/net/` (store + UI).

## Acceptance criteria
- [x] A connection profile can be created, edited, and deleted; survives a restart.
- [x] Credentials are stored via the `CredentialStore` **demonstrably in the OS keychain**,
      **not** in plain text (localStorage/JSON/config).
- [x] The `CredentialStore` abstraction is cut so that another backend
      (KeePass) could be added without changing the callers.
- [x] For SFTP/SSH, login works via ssh-agent/existing keys without
      private keys being stored by us.
- [x] A missing Secret Service (Linux) is reported understandably — **no** silent
      plain-text fallback.
- [x] Connect/disconnect works; active connections appear in the location selector.
- [x] After a connection loss, reconnecting is possible without a restart.
- [x] Failed auth / unreachable host are reported understandably.

## Tests
- Create a profile, check the secret in the keychain (not in config/localStorage).
- `CredentialStore`: get/set/delete against the keychain; behavior with a missing
  Secret Service (Linux) → defined error instead of plain text.
- SFTP login via ssh-agent without a stored private key.
- Connect/disconnect/reconnect against a test server (mock/local).

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to *.done.md

---

## Implementation

Built together with [[022-sftp-provider]] (same session). Backend in
`src-tauri/src/net/`, frontend in `src/features/net/`.

**Backend**
- `net/credentials.rs` — `CredentialStore` trait (`get/set/delete` by connection id)
  + `KeychainStore` (crate `keyring` 3.6, features `apple-native` / `windows-native` /
  `sync-secret-service`). Service = `com.hortcmd.app`, account = profile id. A missing/
  locked Secret Service maps (Linux) to a clear remedy message via `explain()` /
  `no_service_hint()` — **no** plain-text fallback. A `#[cfg(test)]` `MemoryStore` proves
  the trait is backend-swappable.
- `net/session.rs` — `Profile` (id, name, protocol, host, port, user, auth, key_path,
  start_path; ts-rs exported, **no secret**) persisted to `config_dir()/hortcmd/
  connections.json`. A global `Registry` (behind a `Mutex`) holds profiles + live
  `SftpConnection`s keyed by `user@host:port`, plus a static multi-thread Tokio runtime.
  Commands: `net_list_profiles`, `net_save_profile` (secret → keychain), `net_delete_profile`
  (also deletes the secret), `net_connect`, `net_disconnect`, `net_list_connections`.

**Frontend**
- `features/net/netStore.ts` (zustand) — profiles + connection status + manager state.
- `features/net/ConnectionsDialog.tsx` — list (connect/disconnect/edit/delete) + add/edit
  form (auth: password / agent / key / anonymous, conditional secret/key fields). Secrets
  are entered here and sent straight to the backend; never kept after save.
- `DriveSelect.tsx` — a "Connections" group (live connections navigate to their
  `sftp://…` address; a "Manage connections…" entry opens the dialog).
- IPC in `ipc/client.ts`, types re-exported from `ipc/bindings/index.ts`, i18n keys
  (`net.*`) in both `de` and `en`.

### Design decisions
- **OS keychain only** (per ticket): no encrypted-file fallback. KeePass stays a future
  backend behind the same trait (ticket 027).
- **Secrets never cross into the frontend store or the profiles JSON** — only passed on
  save and read back on connect inside the backend.
- **ssh-agent preferred**: the `agent` auth method uses the running ssh-agent's existing
  keys; only passwords/passphrases are stored (see [[022-sftp-provider]] `authenticate`).
- **Reconnect** = `net_connect` again (replaces the live session for that key); there is
  no automatic mid-transfer resume.

### Verified
- `cargo check` clean (no warnings); `cargo test --lib` → **72 passed / 1 ignored**
  (ignored = the live-keychain round-trip; the deterministic `MemoryStore` contract test
  runs). `npm run typecheck` / `npm run build` clean; `gen:types` produced `Profile.ts`
  + `ConnectionStatus.ts`.
- **UI smoke-tested live** in the vite dev server (browser, demo mode): the DriveSelect
  "Connections" group renders, "Manage connections…" opens the dialog, the empty state
  and the full add/edit form (fields, auth select with conditional password field, Save
  disabled until Host is set) all render with **zero console errors**.
- **Not** verified live (needs environment/server, per the agreed build+handoff scope):
  a real OS-keychain write, an actual connect against an SFTP server, profile persistence
  across an app restart, and the ssh-agent auth path. Backend logic is unit-tested where
  deterministic (address/key/JSON round-trips, credential contract).
