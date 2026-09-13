# TICKET-220: Add redacted diagnostics and an exportable support bundle

## Backlog placement

Moved from 045 to 220 as support and diagnostic infrastructure. This is supporting
work in the 220+ backlog; the requirements and open status are unchanged.

## Goal

Provide enough structured local diagnostics to investigate failures across UI,
IPC, transfers, providers, and connections while keeping filesystem details and
credentials under the user's control.

## Background

The application has browser-demo `console.log` calls and provider errors, but no
coherent production logging, correlation IDs, retention policy, or user-facing
diagnostic export. Networking and cross-store transfers add asynchronous layers
where a final error string alone is insufficient. Future plugin and MCP surfaces
will further increase the need for traceability, but diagnostics in a file manager
can easily expose filenames, paths, server addresses, usernames, tokens, or keys.

TICKET-036 defines safe structured errors. This ticket owns event correlation,
local retention, redaction, and explicit user-controlled export rather than remote
telemetry.

## Requirements

- Adopt structured logging with levels, timestamps, component names, operation/
  connection correlation IDs, and stable event names across Rust and TypeScript.
- Define default production and development levels and allow temporary per-session
  verbosity without requiring a rebuild.
- Centralize redaction before persistence. Cover credentials, headers, URL user
  information, query secrets, key material, clipboard content, terminal input/
  output, file contents, and sensitive path segments.
- Do not log file contents, terminal streams, passwords, passphrases, tokens, or
  private keys at any level.
- Use bounded rotating local logs with documented storage location, size, age, and
  deletion behavior.
- Correlate frontend actions, IPC commands, backend work, provider requests, retry,
  cancellation, and final structured errors without logging high-frequency byte
  progress individually.
- Provide a diagnostics screen that shows effective app/build versions, platform,
  provider capability summaries, sanitized configuration, recent error codes, and
  log location.
- Let the user preview categories and explicitly export a support bundle containing
  redacted logs and non-secret metadata. Default to excluding full paths and host
  names; require opt-in for less-redacted identifiers.
- Provide a one-action delete/reset for diagnostic data.
- Keep all diagnostics local unless a future, separately approved feature adds an
  explicit upload destination and consent flow.

## Acceptance Criteria

- [ ] One correlation ID traces a failed transfer or connection across frontend,
      IPC, backend, and provider layers.
- [ ] Production logs are bounded and exclude high-volume progress and content.
- [ ] Seeded credentials, tokens, terminal input/output, and file contents never
      appear in logs or the default support bundle.
- [ ] Users can inspect bundle categories before export and delete stored logs.
- [ ] Default export pseudonymizes paths, hosts, profile names, and usernames
      consistently enough to correlate repeated events.
- [ ] Diagnostics remain local and no network telemetry is introduced.
- [ ] Logging failures cannot interrupt file operations or application startup.

## Tests

- Unit-test redaction with credentials embedded in URLs, headers, nested errors,
  filenames, Unicode, encoded strings, and provider-specific messages.
- Seed canary secrets through every authentication method, terminal I/O, clipboard,
  and file content, then scan logs and exports byte-for-byte.
- Test correlation across successful, failed, cancelled, and retried operations.
- Test rotation, age cleanup, full disk, permission denial, corrupt log files, and
  concurrent writers.
- Snapshot the default and opt-in support-bundle manifests and verify deterministic
  pseudonyms within one bundle.
- Verify diagnostics generate no outbound network requests.

## Definition of Done

- Structured, correlated, bounded diagnostics cover all major application layers.
- Central redaction and canary tests protect sensitive data.
- Users can inspect, export, and delete local diagnostic information.
- Support-bundle schema and privacy behavior are documented.
- No telemetry or upload behavior is added by this ticket.
- Rename this file to `220-redacted-diagnostics-and-support-bundle.done.md`.
