# TICKET-334: Keep KeePass unlock state consistent with a failed keychain write

Discovered during a full-codebase review (2026-09-22).

## Problem

`Store::unlock` (`src-tauri/src/net/credentials.rs`, around lines 212-225)
calls `KeepassStore::unlock` and mutates the in-memory unlocked state before
the subsequent `self.keychain.set(MASTER_ID, password)?`; if that keychain
write fails, `Store::unlock` returns `Err` to the caller while the KeePass
vault is actually left unlocked.

With `backend = "keepass"` and `master_in_keychain = true`, if the OS
keychain is temporarily unavailable (locked, Secret Service down, permission
dialog dismissed): `self.keepass.unlock(&cfg, password, remember, create)`
succeeds and stores an unlocked `Session`. The following
`self.keychain.set(MASTER_ID, password)?` fails and propagates `Err` from
`unlock()`. The UI, seeing an error, may keep prompting for the master
password or show a failure state, while `store().status().unlocked` is
actually already `true` and any subsequent credential access proceeds
without a fresh prompt — the caller's belief about lock state and the actual
state have diverged.

## Required change

- On the keychain-write failure, re-lock the just-opened session
  (`self.keepass.lock()`) before returning `Err`, or perform the keychain
  write first / treat it as best-effort (log and continue) so unlock
  success/failure is reported consistently with the vault's actual state.

## Verification

- Test: simulate a keychain write failure during KeePass unlock; assert
  `store().status().unlocked` matches the reported `Err`/`Ok` result.
- `npm run check`.
