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

- [ ] A host whose key matches either trusted store connects without a prompt.
- [ ] A first connection shows host, port, key type, and SHA-256 fingerprint
      before any authentication attempt.
- [ ] Cancelling the first-use prompt sends no password, key signature, or agent
      signature to the server.
- [ ] Trusting the key persists it and the retried connection succeeds.
- [ ] A later connection with the same key succeeds without prompting.
- [ ] A changed server key is rejected with both old and new fingerprints shown.
- [ ] Changing a profile's host or port does not reuse the previous host's trust.
- [ ] Password, private-key, agent, and none authentication all use the same check.
- [ ] The integrated terminal only runs on the already verified SFTP connection.
- [ ] No code path in production returns unconditional acceptance from
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
