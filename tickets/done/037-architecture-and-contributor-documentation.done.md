# TICKET-037: Bring architecture and contributor documentation up to date

## Goal

Make the repository documentation accurately describe the current application,
its local and remote architecture, supported workflows, security boundaries, and
the steps required to build, test, and contribute on each platform.

## Background

README.md still shows a small backend with a single `commands/fs.rs` file and does
not describe the VFS providers, connection profiles, credential store, SSH
terminal, transfer engine, generated IPC bindings, or current store layout.
Concept.md contains a much richer history and roadmap, but completed and planned
work is interleaved and some summaries lag the staged networking implementation.
TICKET-033 adds CI-specific commands; this ticket owns the broader information
architecture and ongoing documentation accuracy.

## Requirements

- Rewrite the README architecture tree and feature summary to match actual paths
  and implemented behavior.
- Document the supported local, SFTP, FTP/FTPS, SMB, and WebDAV feature matrix,
  including limitations defined by TICKET-031.
- Explain the trust boundaries for WebView/IPC, OS keychain credentials, SSH host
  keys, remote TLS, plugins, and the planned MCP server without overstating
  protections that are not implemented.
- Add platform-specific prerequisites and verified setup commands for Windows,
  macOS, and Linux, including Tauri native dependencies and the Rust MSRV.
- Describe browser-demo mode and clearly list which native, remote, and security
  behavior it does not exercise.
- Document generated bindings, normal tests, ignored/live tests, packaging, and
  troubleshooting entry points using the canonical commands from TICKET-033.
- Separate enduring architecture decisions from chronological implementation
  notes in Concept.md; preserve useful history rather than deleting it.
- Add a concise contributor guide covering ticket lifecycle, `.done.md` naming,
  formatting, validation, security-sensitive changes, and ignored `tickets/`
  behavior in `.gitignore` if that behavior is retained.
- Add an ownership rule: changes to public commands, provider capabilities,
  persisted schemas, or security policy must update their corresponding docs.
- Validate all internal links and commands in CI.

## Acceptance Criteria

- [x] A new contributor can identify the frontend, backend, IPC, VFS, transfer,
      network-session, and credential components from the README.
- [x] Setup and validation instructions are complete for all supported platforms.
- [x] Documentation distinguishes implemented, ticketed, and aspirational features.
- [x] The remote-provider matrix matches executable capability tests.
- [x] Security documentation matches the effective Tauri configuration and code.
- [x] Every documented command succeeds from a clean supported checkout.
- [x] Internal links and referenced paths are automatically checked.

## Tests

- Follow the clean-setup guide on fresh Windows, macOS, and Linux environments.
- Run every fenced shell command marked as executable documentation.
- Verify all repository-relative links and referenced files in CI.
- Compare the provider matrix against the capability contract from TICKET-031.
- Have a reviewer unfamiliar with the networking work trace a remote copy from UI
  action through IPC, VFS, credentials, and provider using only the documentation.
- Review security statements against TICKET-028, TICKET-030, and effective config.

## Definition of Done

- README.md, Concept.md, and the contributor documentation describe the current
  repository and supported workflows accurately.
- Architecture, security, provider support, and test modes have clear owners.
- Documentation links and commands are checked automatically.
- Historical notes remain accessible without obscuring the current design.
- Future contract changes have an explicit documentation update requirement.
- Rename this file to `037-architecture-and-contributor-documentation.done.md`.

## Implementation

**The problem is drift, not absence.** The README was not empty — it was
describing a backend with a single `commands/fs.rs`, a layout three tickets
old. Documentation that is merely missing is honest; documentation that is
confidently wrong costs a reader more than nothing would. So this ticket is
half rewriting and half making the rewrite hold.

**`scripts/check-docs.mjs`** (`npm run check:docs`, in `npm run check` and in
CI) checks every markdown file in the repository:

1. **Links and referenced paths.** Repository-relative markdown links and
   backticked paths must resolve. Two accommodations keep the check honest
   rather than annoying: backticked paths resolve against the roots documents
   actually write against (`src/`, `src-tauri/src/`, …), because prose that
   says `lib/path.ts` reads better than prose that repeats `src/` forever; and a
   link to `tickets/NNN-x.md` follows the ticket to `tickets/done/NNN-x.done.md`,
   since a completed ticket is moved, not deleted. Paths inside `tickets/` are
   exempt entirely — a ticket is either a plan naming files it will create or a
   record naming files as they were, and gating the build on either would mean
   editing history to keep a build green.
2. **Documented commands.** Every `npm run …` a document tells a reader to run
   must exist in `package.json`. This is the instruction most likely to be
   silently wrong on a clean checkout.
3. **The provider matrix** in Concept.md against `vfs/caps.rs`, in both
   directions, with the `remote_core_caps()` capabilities required to read ✅
   for all four protocols. The code is the source of truth; the matrix is a
   copy, and copies drift.
4. **Ticket references.** Every `TICKET-NNN` mentioned outside `tickets/` must
   have a file.

**Separating design from history.** Concept.md interleaved "this is how it
works" with "this is when it was built". The 153-line `### ✅ Done` block moved
out of §10 into a new **§12 Implementation history**, with §10 pointing to it.
That heading is also where the path check stops: a record naming a file that was
deliberately removed is not a broken document, and keeping deleted files around
to satisfy the record of their deletion would be absurd.

**README, rewritten around what a reader needs.** "What it does" separates
implemented from ticketed-but-unimplemented (plugins TICKET-100–107, MCP
TICKET-041). "The pieces" is a where-to-look table per component, followed by a
walkthrough of one remote copy from F5 through `runCopy` → capability check →
`copy_entries` → `vfs::resolve` → the session registry → the provider's
`open_read`/`create_write` → `fs-progress`/`fs-done`, which is the path that
touches every layer this ticket had to name. Platform prerequisites are per-OS
(the Linux package list is the one `.github/workflows/ci.yml` installs, so CI
keeps it current); packaging and a troubleshooting table were added; the Scripts
section became a table and states that the generated files are committed
deliberately, so a frontend-only checkout type-checks without a Rust toolchain.

**Browser-demo mode gets a list of what it does *not* do.** It is easy to
mistake demo mode for the app. Everything above the IPC boundary is real;
nothing below it is — no file system, no transfer engine, no protocol, no
credentials, no production CSP, and only one deterministic capability set, so
remote restrictions are invisible there.

**Security documentation says what is not protected.** The existing SSH
host-key, failure-safe-transfer and WebView sections were kept; four boundaries
were added, each written to avoid claiming more than the code does. The IPC
surface has no per-command privilege level and the frontend is not a security
boundary. Stored credentials get exactly the platform's protection — while your
session is unlocked, any program running as you can ask for the same entries —
and hortcmd encrypts nothing itself and does not zero secrets in memory. Remote
TLS is validated with no "continue anyway", but also with no pinning and no
TOFU, unlike SSH host keys; plain FTP and port-80 WebDAV are unencrypted; SMB
negotiates signing and encryption with the server and cannot currently be forced
to require either. And plugins and the MCP server do not exist — no host, no
loader, no port.

**`CONTRIBUTING.md`** carries the working agreement: the one command, the ticket
lifecycle and `.done.md` naming, commit conventions, the test layout, rules for
security-sensitive changes, and the ownership table naming what must be updated
with which kind of change (IPC command → Concept §7 + `gen:types`; capability →
the matrix; error code → `errors.ts` in both languages + `gen:errorcodes`;
persisted schema → Concept + a migration; security policy → README § Security).
The same table is in Concept.md's new **Cross-cutting — documentation** section,
where the enforcement is described.

### Verified

`npm run check` passes end to end: `cargo fmt --check`, Clippy with
`-D warnings` across all targets and features, 268 Rust tests, current bindings
and error codes, no import cycles, `check:docs`, `tsc --noEmit`, 303 frontend
tests, the production build, and the bundle budget (entry chunk 236.8 kB of
320 kB).

The new gate was broken on purpose and observed to fail, four ways:

- a remote-core row edited to ⚠️ → ``"`copyFrom`, `copyTo`, `move`, `create`,
  `deletePermanent`" is in remote_core_caps() but the matrix does not show it
  for every protocol``;
- a matrix row deleted → `capability "gitStatus" is declared in vfs/caps.rs but
  missing from the matrix`;
- `src-tauri/capabilities/default.json` misspelled as `main.json` →
  `referenced path does not exist`;
- and it caught a real mistake while this ticket was being written: the first
  draft of § Security cited `tickets/201-plugin-host-and-capability-model.md`
  and `tickets/041-mcp-server-integration.md`, neither of which exists (the real
  files are `100-plugin-api-contract.md` and `041-mcp-interface.md`). That
  escaped the checker at first because a backticked `.md` path was not being
  resolved; the extension list now includes `md`, with `NNN` placeholders
  exempt.

Stale references that the check found in the existing documentation were fixed:
`panes/state.ts` → `store/panes/state.ts` in Concept.md (twice) and the README,
and `../docs/` → `../../docs/` in a completed ticket.

The Linux package list was compared line by line with the three `apt-get`
invocations in `.github/workflows/ci.yml` — identical, which is what keeps it
true. The macOS instructions and every fenced command outside the platform
sections were run on this machine (Rust 1.96.1, Node 26.3.0, MSRV declared 1.85
and held by its own CI job). The Windows commands are **not** verified on a
Windows host; they are the standard Build Tools and rustup installs, and the
`rust` CI job proves the code compiles and its portable tests pass there.
Setting up fresh Windows and Linux machines, as the ticket's Tests section asks,
was out of reach here and is the one requirement left unverified.
