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

- [ ] A new contributor can identify the frontend, backend, IPC, VFS, transfer,
      network-session, and credential components from the README.
- [ ] Setup and validation instructions are complete for all supported platforms.
- [ ] Documentation distinguishes implemented, ticketed, and aspirational features.
- [ ] The remote-provider matrix matches executable capability tests.
- [ ] Security documentation matches the effective Tauri configuration and code.
- [ ] Every documented command succeeds from a clean supported checkout.
- [ ] Internal links and referenced paths are automatically checked.

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
