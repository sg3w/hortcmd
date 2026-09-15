# TICKET-047: Enforce a clear remote transport security policy

## Goal

Make encryption and peer verification explicit for every remote provider so users
cannot unknowingly send credentials or file data over an insecure or ambiguously
configured connection.

## Background

SFTP uses SSH, FTPS uses the system TLS stack, and WebDAV validates HTTPS
certificates through reqwest. The application also supports plain FTP and permits
WebDAV hosts with an explicit `http://` scheme or port 80. SMB security behavior
is inherited from a default client configuration and is not surfaced to the user.
Profiles do not currently expose a normalized transport-security state or a
central policy for insecure protocols, certificate exceptions, minimum versions,
or downgrade prevention.

TICKET-028 owns SSH host keys. This ticket covers TLS, plain-text transports, and
SMB signing/encryption without weakening platform certificate validation.

## Requirements

- Define typed transport states for encrypted-and-verified, encrypted-with-user-
  approved exception, encrypted-but-unverified, and plaintext.
- Default new profiles to secure transports: SFTP, FTPS, HTTPS WebDAV, and SMB with
  signing required where supported.
- Disable plaintext FTP and HTTP WebDAV by default. Enabling either requires a
  persisted advanced setting and a per-profile warning that credentials and data
  may be exposed.
- Never silently fall back from FTPS to FTP, HTTPS to HTTP, verified TLS to
  unverified TLS, or signed/encrypted SMB to a weaker mode.
- Require hostname verification and the platform trust store for TLS by default;
  reject expired, not-yet-valid, mismatched, revoked-when-known, and untrusted
  certificates with structured errors.
- If private certificate authorities are supported, trust only an explicitly
  selected CA or pinned certificate for the specific profile. Do not add a global
  “accept all certificates” option.
- Persist certificate pins by cryptographic digest and show subject, issuer,
  validity, hostnames, algorithm, and fingerprint before trust is granted.
- Treat a changed pinned certificate as a hard failure requiring explicit trust
  replacement outside the failed connection attempt.
- Surface negotiated protocol, cipher, certificate status, and SMB signing/
  encryption state in connection details without exposing secrets.
- Define minimum TLS versions and reject obsolete protocols and ciphers supported
  only for legacy compatibility.
- Include effective transport state in provider capabilities and diagnostics.

## Acceptance Criteria

- [x] New remote profiles use encrypted, peer-verified transport by default.
- [x] Plain FTP and HTTP WebDAV cannot connect without both global enablement and a
      clear per-profile acknowledgement.
- [x] No provider silently downgrades after a secure negotiation failure.
- [x] Invalid TLS certificates and changed pins fail before credentials or file
      content are transmitted where the protocol permits.
- [x] SMB signing/encryption requirements and negotiated state are visible.
- [x] Certificate exceptions are scoped to one profile and are revocable.
- [x] Structured errors distinguish trust, hostname, expiry, protocol, and
      plaintext-policy failures.

## Tests

- Use the harness from TICKET-038 to test trusted, self-signed, expired, future,
  wrong-host, rotated, and weak-protocol TLS endpoints.
- Attempt downgrade paths for FTPS, WebDAV, and SMB and verify hard failure.
- Capture loopback traffic to confirm credentials and payloads are encrypted in
  every secure mode and to demonstrate the explicit warning for plaintext modes.
- Test certificate pin first use, repeat use, change, removal, and profile isolation.
- Test SMB servers with signing optional/required and encryption available/
  unavailable against each policy combination.
- Verify diagnostic and error payloads contain fingerprints but no credentials.

## Definition of Done

- A single documented security policy governs every remote transport.
- Secure defaults, downgrade prevention, and peer verification are enforced in
  code and represented in the UI.
- Insecure compatibility modes require deliberate, revocable user choice.
- Protocol integration tests cover negotiation and trust failures.
- Connection details accurately expose effective transport protection.
- Rename this file to `047-remote-transport-security-policy.done.md`.

## Implementation evidence

Central backend policy requires both persisted plaintext opt-ins, including direct
provider callers. Bare WebDAV hosts use HTTPS; redirects are disabled. FTPS and
WebDAV share a Rustls platform verifier with TLS 1.2/1.3 only, optional profile-bound
CA approval/server pins, and no accept-all path. Certificate inspection displays
public metadata before explicit approval; changed profile/file/server pins fail.
SMB requires encryption and disallows unsigned guest sessions.

Status/capability metadata and localized connection details expose effective
protection and available negotiation information. Unavailable FTPS/WebDAV cipher
and version fields are explicitly labeled, with API diagnostics follow-up in
[TICKET-118](../118-negotiated-tls-diagnostics.md).

Local TLS tests exercise the actual FTPS and WebDAV adapters, unknown/expired/
future/wrong-host/rotated certificates, private CA removal and profile isolation.
Rejected peers receive no seeded credential; AUTH TLS refusal never causes a plain
login and HTTPS redirects never reach an HTTP target. Frontend tests cover separate
plaintext gates, preview-before-approval and stale-preview rejection. The complete
`npm run check` passed: 429 Rust tests (25 ignored protocol/keychain tests), 526
frontend tests, strict Clippy, bindings, documentation, types and production build.

Native Windows/Linux, live SMB server combinations and known-revocation/packet
capture evidence are deferred by user instruction to
[TICKET-117](../117-native-transport-security-verification.md). No such native
matrix is claimed as executed here. Policy and migration details are documented in
[transport security](../../docs/transport-security.md). TICKET-093 was implemented
and merged separately to correct WebDAV navigation addresses needed by this policy.
