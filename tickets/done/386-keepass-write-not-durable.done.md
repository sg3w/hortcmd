# TICKET-386: Make the KeePass database write durable

Priority: **P2**. Status: **open**.
Discovered during a full code review (2026-09-28).

## Problem

`write_atomic` in `src-tauri/src/net/credentials.rs:603-635` promises that
"an abort (crash, full disk) leaves the original intact". It writes a
temporary sibling, syncs *that file*, and renames:

```rust
let write = |tmp: &Path| -> std::io::Result<()> {
    let mut file = std::fs::File::create(tmp)?;
    file.write_all(&buffer)?;
    file.sync_all()
};
…
std::fs::rename(&tmp, path)
```

The directory entry created by the rename is never synced. On a crash or
power loss shortly after `set()` returns, the rename can be lost even though
the file data was flushed, so the `.kdbx` reverts to its previous contents —
the newly stored connection password is gone with no indication that anything
failed. On a filesystem that reorders more aggressively the directory can
instead come back holding only `secrets.kdbx.hortcmd-tmp`, leaving no
`.kdbx` at the configured path at all, which reads to the user as a lost
vault.

The rest of the codebase already holds the opposite standard for exactly this
sequence. `transfer/recovery/durability::sync_parent` exists for it and is
called after every publishing rename in the transfer engine
(`transfer/plan.rs:418-460`) and in archive staging
(`archive/staging.rs:245-254`, `306-312`). Credentials are the one place that
renames into a user-visible final name without it — and they are the payload
the project ranks under "Data safety" (AGENTS.md, priority 2).

Two smaller points in the same function:

- The temporary file is created with the process umask (0644 on Unix), unlike
  every other private artefact the app writes (`archive/staging.rs:68-75` and
  `transfer/plan.rs:233-240` both force `0700`; the WebDAV upload spool forces
  `0600`, `vfs/webdav.rs:729-735`). The content is encrypted, so this is
  hardening rather than a leak, but it is a gratuitous inconsistency.
- `std::fs::File::create` truncates an existing temp file rather than failing.
  `create_spool` in `webdav.rs:724-746` shows the pattern the project prefers:
  `create_new(true)` with bounded retries, so a stale or hostile sibling is
  never silently reused.

## Required change

- Sync the containing directory after the rename, reusing
  `recovery::durability::sync_parent` rather than adding a second
  implementation. Report a failure — the caller must not be told the secret
  was stored when it may not have been.
- Create the temporary file with `create_new(true)` and restrictive
  permissions (0600 on Unix), as `create_spool` does.

## Verification

- Rust test: after `KeepassStore::set`, the `.kdbx` at the configured path
  holds the new entry and no `*.hortcmd-tmp` sibling remains.
- Rust test (unix): the temporary file's mode during the write is 0600, and a
  pre-existing file at the temporary name is not truncated.
- `npm run check`.

## Implementation

`write_atomic` now finishes the sequence it started.

- After the rename it calls `recovery::durability::sync_parent(path)` — the same
  helper the transfer engine and archive staging use after every publishing
  rename, rather than a second implementation of it. Its failure is reported
  with `CRED_STORE`: the caller must not be told the secret was stored when the
  directory entry may still be lost.
- The temporary sibling is created by `create_temp`: `create_new(true)` so a
  stale or hostile file at the name is never truncated or reused, a random
  suffix with 16 bounded retries so a collision is a retry rather than a
  failure, and mode 0600 on Unix so the vault is not briefly readable by other
  users. That is the shape `create_spool` and the recovery journal already use.
- The two error identities are unchanged: a failure to write the temporary file
  still reads "KeePass-Datei nicht schreibbar", a failure to publish is still
  `CRED_STORE`, and every failure path removes the temporary file.

`create_temp_with` takes the candidate generator as a parameter, which is what
lets the collision behaviour be tested without depending on a random name.

### Verification

- `net::credentials::tests::keepass_write_publishes_the_vault_and_leaves_no_temporary`:
  after `KeepassStore::set` the `.kdbx` is a file at the configured path, no
  `*.hortcmd-tmp*` sibling remains in the directory, and a freshly opened store
  reads the new entry back.
- `keepass_temporary_is_private_and_never_truncated` (mode assertion under
  unix): an occupied candidate is retried around with its content intact, the
  file that is created has mode 0600, and a generator that only ever offers the
  occupied name fails with `AlreadyExists` instead of truncating it.
- The existing KeePass round-trip and persistence tests still pass, so the
  publishing path itself is unchanged in behaviour.
- `npm run check` passed on macOS (arm64).
