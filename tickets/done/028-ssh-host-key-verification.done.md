# TICKET-028: Verify SSH server host keys

## Goal

Every SSH connection must authenticate the remote server before credentials or
file data are exchanged. SFTP browsing and the integrated SSH terminal must use
the same verified host identity and must reject unexpected key changes.

## Background

`ClientHandler::check_server_key` in `src-tauri/src/net/ssh.rs` currently returns
`Ok(true)` for every key. This makes initial development convenient but defeats
SSH's server-authentication guarantee: a machine-in-the-middle server can present
any key and receive a password or an authentication attempt.

The completed SFTP and SSH terminal tickets explicitly deferred `known_hosts`
verification. The risk now applies to every SFTP authentication method because
host verification happens before password, key, or agent authentication. The
terminal shares the SFTP SSH session, so one correct implementation protects
both features.

## Requirements

### Trust sources and identity

- Replace unconditional acceptance with a verifier keyed by canonical host and
  port. Non-default ports must use the OpenSSH `[host]:port` identity form.
- Read the user's standard OpenSSH `known_hosts` files when present without
  rewriting them automatically.
- Maintain a separate hortcmd-managed known-hosts store in the application config
  directory for keys accepted through the connection manager.
- Match the presented public key using a maintained OpenSSH-compatible parser or
  library. Support the key algorithms accepted by the configured russh version.
- Display a SHA-256 fingerprint, key type, host, and port. Never display or persist
  private-key material.

### First connection

- An unknown host must interrupt the connection flow before user authentication
  and return a structured host-key challenge to the frontend.
- The connection manager shows a blocking confirmation with the host, port, key
  type, and SHA-256 fingerprint plus **Trust and connect** and **Cancel** actions.
- **Trust and connect** persists that exact key in the hortcmd trust store and
  restarts the connection. Cancelling changes nothing and does not authenticate.
- A profile edit that changes host or port is treated as a new host identity; it
  must not inherit trust from the previous endpoint.

### Known and changed keys

- A matching key proceeds without another prompt.
- A mismatching key is a hard failure. The error must clearly distinguish a host-
  key change from an unknown host and show both stored and presented fingerprints.
- Do not offer a one-click bypass in the failure dialog. Replacing a trusted key
  requires an explicit action in the connection manager that shows the warning
  and removes or replaces the stored entry.
- A mismatch in either the user's OpenSSH file or the hortcmd-managed store must
  block the connection. One matching source must not override a conflicting one.
- Duplicate matching entries are harmless and must not create duplicate prompts.

### Shared SSH session

- Host verification runs once while creating `SshSession`, before every supported
  authentication method: password, private key, agent, and none/anonymous.
- SFTP and all terminal channels opened from the session inherit the verified
  connection. The terminal must never open a second unverified connection.
- Reconnects perform verification again; trust is not inferred from an earlier
  in-memory session after the stored key changes.

### Errors and robustness

- Use typed outcomes for `trusted`, `unknown`, and `mismatch`; do not parse an
  English or German error string to decide which UI to show.
- Malformed or unreadable known-hosts entries must be reported with their source
  file and line where possible. Unrelated malformed entries must not silently
  disable verification for other hosts.
- Failure to persist a newly trusted key must abort authentication. Trust must not
  exist only in memory when the user selected a persistent action.
- Write the application trust store atomically with permissions appropriate for
  configuration data on the platform.

## Acceptance criteria

- [x] A host whose key matches either trusted store connects without a prompt.
- [x] A first connection shows host, port, key type, and SHA-256 fingerprint
      before any authentication attempt.
- [x] Cancelling the first-use prompt sends no password, key signature, or agent
      signature to the server.
- [x] Trusting the key persists it and the retried connection succeeds.
- [x] A later connection with the same key succeeds without prompting.
- [x] A changed server key is rejected with both old and new fingerprints shown.
- [x] Changing a profile's host or port does not reuse the previous host's trust.
- [x] Password, private-key, agent, and none authentication all use the same check.
- [x] The integrated terminal only runs on the already verified SFTP connection.
- [x] No code path in production returns unconditional acceptance from
      `check_server_key`.

## Tests

- Unit-test canonical host/port identities, default and non-default ports, IPv4,
  IPv6, hostnames, matching keys, unknown keys, conflicts, and malformed entries.
- Unit-test stable SHA-256 fingerprint formatting for representative key types.
- Integration-test an SSH server with a fixed key: first-use prompt, persisted
  trust, reconnect without prompt, then server-key rotation and hard rejection.
- Verify through instrumentation that authentication is not attempted before an
  unknown key is accepted.
- Run the same integration flow for password, private-key, and agent auth.
- Open an SSH terminal after SFTP connection and confirm it uses the same session.
- Regression-test profile deletion and recreation: deleting a profile does not
  silently delete shared host trust; trust is managed explicitly.

## Definition of Done

- Rust and frontend checks pass, including the new verification tests.
- No SSH connection can authenticate while the host identity is unknown or
  mismatched.
- The trust and mismatch UI is fully translated in German and English.
- The security behavior is documented in README.md and Concept.md.
- The completed SFTP and SSH terminal ticket notes link to this implementation.
- Rename this file to `028-ssh-host-key-verification.done.md`.

---

## Implementation

### Backend

- **`src-tauri/src/net/hostkey.rs`** (new) — the whole trust model:
  - `HostId::new(host, port)` canonicalizes what a profile stores (scheme
    prefix, trailing slash, bracketed IPv6, case) into the bare host;
    `identity()` renders the OpenSSH form — `host` on port 22, `[host]:port`
    otherwise. Host *and* port are part of the identity, so editing a profile's
    endpoint cannot inherit the previous host's trust.
  - Matching uses russh's maintained `known_hosts` reader
    (`known_host_keys_path`), so hashed (`|1|…`) hostnames, comments and
    multi-host lines behave exactly as OpenSSH defines them.
  - Two sources: the user's `~/.ssh/known_hosts` and `known_hosts2` (read only)
    and a hortcmd store at `<config>/hortcmd/known_hosts`. A same-algorithm,
    different-key entry in *either* source is a conflict and blocks; a match in
    one source never overrides a conflict in the other. A different *algorithm*
    is not a conflict — hosts legitimately offer several key types.
  - `check()` returns a typed `Decision` (`Trusted` / `Challenge`); a malformed
    file returns `Err` naming the file and, through a best-effort rescan, the
    offending line numbers. A broken entry for an unrelated host never disables
    verification (russh only parses lines whose host matches).
  - Trust is a **one-shot token**: the presented key stays in a backend map and
    the frontend only gets the token, so `net_trust_host_key` can confirm only
    the key hortcmd actually displayed. Writes go through a temp sibling +
    rename with `0600` on Unix, and a failed write propagates — trust never
    exists only in memory.
  - `ConnectError { HostKey(HostKeyChallenge), Message(String) }` carries the
    outcome through the provider layers as a type, not as text.

- **`net/ssh.rs`** — `ClientHandler` now holds the `HostId` and records the
  verdict; `check_server_key` returns `true` only for `Decision::Trusted`, which
  aborts the handshake before any authentication method runs. `connect` reads
  the recorded verdict rather than russh's generic error, and a handshake that
  never reached the host key is itself a failure — no production path accepts
  unconditionally. Password, key, agent and none auth all sit behind this one
  check; the SFTP subsystem and every terminal channel ride on the session it
  produced.

- **`net/session.rs`** — `net_connect` returns `ConnectOutcome`
  (`{kind:"connected"|"hostKey", data}`), plus `net_trust_host_key(id, token)`
  (persist, then retry) and `net_forget_host_key(id)` (clear hortcmd's store,
  *report* remaining entries in the user's files). `net/mod.rs` gained the
  shared `unique_id()` helper that `terminal.rs` used to keep privately.

### Frontend

- `netStore` branches on `outcome.kind`; a challenge opens the connection
  manager on the new `hostkey` panel. `trustHostKey`, `dismissHostKey`,
  `startForget` and `forgetHostKey` round it out.
- `ConnectionsDialog` gained `HostKeyPanel` (fingerprint confirmation; for a
  mismatch it shows both fingerprints with source file and line and offers
  **no** trust button) and `ForgetHostKeyPanel` (warning + explicit
  confirmation, reached from a shield button on SSH profile rows).
- 16 new `hostkey.*` keys in German and English.

## Verified

- `cargo test --lib`: **169 passed**, including 15 new `net::hostkey` unit tests
  covering identity forms (default/non-default port, IPv4, IPv6, hostnames,
  case/scheme normalization), stable SHA-256 fingerprints, match, duplicate,
  conflict, cross-source conflict-beats-match, port isolation, malformed entries
  (own host → error naming file and line; foreign host → verification intact),
  hashed and commented entries, removal, and one-shot token semantics.
- `npm run typecheck` and `npm run build` clean; `npm run gen:types` regenerated
  `ConnectOutcome`, `HostKeyChallenge`, `StoredHostKey`, `ForgetResult`.
- All three new panels were driven in the browser pane (demo mode, with the IPC
  calls temporarily stubbed and reverted afterwards): unknown-host confirmation,
  the mismatch dead end with both fingerprints and `source:line`, and the
  forget flow reporting `1 Eintrag entfernt` plus the untouched user file.
- `live_ssh_host_key` (`#[ignore]`, `cargo test --lib live_ -- --ignored`) walks
  the full server flow against a real sshd: first contact with a deliberately
  *wrong* password still ends in the challenge — proving no authentication is
  attempted before the key is accepted — then trust, silent reconnect, staged key
  rotation and hard rejection. `live_sftp` and `live_ssh_shell` answer the
  first-use prompt through a shared helper, and the shell test asserts it runs on
  the already-verified session.

**Not covered:** the live tests need a reachable sshd and are not part of the
normal run. `@cert-authority` / `@revoked` marker lines are ignored rather than
honoured (they never match a plain host field), and a mixed-case entry in the
user's own file will not match hortcmd's lowercased lookup.
