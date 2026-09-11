# TICKET-026: SSH terminal

## Goal
An interactive SSH shell in the commander that uses the same SSH connection as SFTP and
fits into the existing terminal feature.

## Background
Concept.md lists an SSH terminal as part of the network subsystem. Technically it depends
only on the SSH connection from SFTP (TICKET-022), not on SMB/WebDAV/FTP — so it can be
implemented directly after 022 (mentioned last by the user).

## Requirements
- Open an **interactive shell** over the existing SSH connection (TICKET-021/022)
  (PTY, bidirectional data stream, size/resize).
- Integrate with the existing terminal feature or the context **"Open in terminal"**
  (today `open_terminal`, `src/ipc/client.ts`) — for a remote folder it opens
  an SSH shell **in the corresponding remote directory**.
- Terminal UI (its own window via the TICKET-002 infra or an embedded panel),
  incl. input/output, scrollback, close.
- Handle session end/connection loss cleanly.
- Location: backend wired into `src-tauri/src/net/`; frontend terminal component under
  `src/features/net/`.

## Acceptance criteria
- [x] An interactive shell can be opened from an active SSH connection.
- [x] "Open in terminal" on a remote folder starts the shell in the correct
      remote directory. *(live verified: `cd '/config'` on the opened shell)*
- [x] Input and output work; resize is reported to the remote side.
- [x] Connection loss/session end are reported cleanly and the terminal is closed.

## Tests
- Against an SSH server: open a shell, run commands, resize, disconnect.

## Definition of Done
- [x] Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- [x] Tests pass
- [x] Existing functionality unchanged
- [x] Documentation updated
- [x] Rename the ticket to *.done.md

---

## Implementation

Builds directly on the SSH connection of [[022-sftp-provider]] and the connection
layer of [[021-connections-credentials]] — the shell rides on the **same** SSH
connection that already serves SFTP, so opening a terminal costs no second login and
no second authentication prompt.

### Backend

**`net/ssh.rs` (new).** The russh client handler, `connect` and the whole
authentication block (password / ssh-agent / key / none) moved out of
`vfs/sftp.rs` into an `SshSession` in `net/`. `vfs/sftp.rs` now only opens the
`sftp` subsystem on it; the terminal opens a second channel on the same session.
This is a pure move — the auth logic is unchanged.

**`net/terminal.rs` (new).**
- `open_shell_channel(ssh, path, cols, rows)` opens a session channel, requests a
  **PTY** (`xterm-256color`, initial size) and a **shell**, then sends
  `cd '<path>'` — shell-quoted (`'` → `'\''`), so paths with spaces or quotes are
  safe. The line is echoed by the shell exactly like a typed `cd`, which is the
  intended feedback.
- Commands `ssh_open_shell` (resolves the address through the VFS, returns a session
  id) · `ssh_send` · `ssh_resize` (`window_change`) · `ssh_close` (idempotent).
- The channel is `split()`: the **write half** lives in a session registry keyed by
  the id; a task on the shared Tokio runtime pumps the **read half** and emits
  `ssh-term-data` events until the channel ends, then emits `ssh-term-closed` with
  the reason (`Sitzung beendet` / `Shell beendet (Status n)`) and drops the session.
- **Output is base64-encoded** rather than sent as a string: a shell emits arbitrary
  bytes and multi-byte characters routinely straddle packet boundaries — decoding
  belongs in the terminal emulator, not in the transport. Stderr (`ExtendedData`) is
  merged into the same stream, as a PTY would.

**Reaching the SSH session from an address.** A default trait method
`Provider::ssh() -> Option<Arc<SshSession>>` (only SFTP overrides it) keeps the live
provider registry as the single source of truth — no second registry to keep in sync,
and `net_disconnect`/`net_delete_profile` additionally call
`terminal::close_for_store()` so no shell channel outlives the connection it rides on
(a channel write half holds a sender into the session and would otherwise keep it
alive).

### Frontend
- **xterm.js** (`@xterm/xterm` 6 + `@xterm/addon-fit`) — a real interactive shell
  needs VT/ANSI emulation (cursor addressing, colors, alternate screen); hand-rolling
  that was out of the question.
- `features/net/terminalStore.ts` holds **one** session (the window is a modal
  dialog, so a second one is unreachable by construction). The backend id is attached
  *after* opening, because the shell needs cols/rows and those only exist once the
  terminal has been laid out.
- `features/net/SshTerminal.tsx`: `AppDialog` + xterm; `onData` → `ssh_send`,
  `ssh-term-data` → `term.write(bytes)`, `ResizeObserver` → `fit()` → `ssh_resize`.
  Errors and the end-of-session reason are written **into** the terminal (red/amber)
  and shown in the footer.
- `AppDialog` gained one prop, `keepEscape`: without it Radix would close the dialog
  on `Esc` — fatal for a terminal (vim). No other dialog is affected.
- The context menu entry **"Open in terminal"** now branches: an `sftp://` folder
  opens the SSH shell, a local folder keeps the previous behavior, and the entry is
  **disabled** for FTP/WebDAV/SMB folders, which have no shell.

### Verified
- `cargo check` clean; `cargo test` → **83 passed / 6 ignored** (incl. new tests for
  shell quoting, id uniqueness and the "session no longer active" error).
- `npm run typecheck` / `npm run build` clean.
- **Live-tested against a real SSH server** (`vfs::live_tests::live_ssh_shell`, uses
  the production `open_shell_channel`):

  ```bash
  docker run -d --name hortcmd-ssh -p 2222:2222 -e PASSWORD_ACCESS=true \
    -e USER_NAME=demo -e USER_PASSWORD=demo123 -e SUDO_ACCESS=false \
    lscr.io/linuxserver/openssh-server
  cargo test --lib live_ssh_shell -- --ignored --nocapture
  ```

  Passing on the first run: SFTP connect → shell channel on the *same* connection →
  `cd '/config'` → command executed and its output received (`HORTCMD-42`) →
  `window_change(120×40)` accepted mid-session → `exit` → clean EOF/close.
  Note: the sftp-only image used by `live_sftp` grants no shell, hence the separate
  server for this test.
- **UI live-verified** in the browser (vite, demo mode) with a pane on an `sftp://`
  address: the context entry is enabled, the terminal window opens with the right
  title (`demo@127.0.0.1:/config`), xterm mounts and fits the dialog (27 rows),
  `Esc` does **not** close the window, the failing `invoke` (no Tauri in the browser)
  is reported in red inside the terminal and in the footer, and closing leaves the
  app fully interactive (no Radix pointer-events leak). Zero console errors.
- **Not covered:** the browser pane's environment never delivers `ResizeObserver`
  callbacks, so the *browser-side* `fit()`-on-resize link could not be exercised
  there; the remote half of the resize (`window_change`) is proven by the live test,
  and the same ResizeObserver pattern is already used by `FileTable`/`TabBar`.

### Deferred
- **Host-key verification** is still trust-on-first-use (inherited from 022, not
  widened here) — a `known_hosts` check belongs to the SSH layer as a whole.
- Multiple simultaneous shells (would need a non-modal terminal window).
- Local shells inside the app: "Open in terminal" still hands local folders to the
  configured external terminal.
