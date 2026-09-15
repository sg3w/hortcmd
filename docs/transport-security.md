# Remote transport security

The backend policy in `net/security.rs` applies to desktop connection attempts and
direct provider creation. Desktop checks run before retrieving credentials and
before the transport probe. FTPS, HTTPS and SFTP never downgrade after an error.
WebDAV redirects are disabled, including HTTPS-to-HTTP and cross-host redirects.

## Defaults and explicit compatibility

- New profiles default to SFTP. WebDAV uses HTTPS for a bare hostname regardless
  of port; use `http://host` explicitly for a deliberately unencrypted server.
- FTP and HTTP WebDAV require `allowPlaintext` in `transport-policy.json` and
  `plaintextAcknowledged` in that profile. Neither defaults to true. Existing
  plaintext profiles therefore require review before their next connection.
- Profile editing displays the warning and keeps global enablement separate from
  the profile acknowledgment. Changing host, user, port or protocol resets the
  acknowledgment and certificate approval.
- Global enablement is saved immediately and governs new connection attempts.
  Already active operations are not represented as retroactively encrypted.
- SMB requires encryption; unsigned guest access is disabled. Servers without the
  required protection fail rather than falling back to older/unprotected sessions.
  Integrity-protected encrypted SMB frames need no additional unencrypted signature.

## Certificates

FTPS and HTTPS share Rustls configuration with platform certificate verification,
TLS 1.2/1.3 and the maintained Rustls cipher suites. The verifier delegates chain,
hostname, signature and platform revocation decisions; explicit end-entity date
checks also apply when the approved anchor is the server certificate itself.

In a TLS profile's editor, enter the absolute path of one PEM or DER certificate,
inspect it, review subject, issuer, validity, hostnames, signature algorithm and
SHA-256 fingerprint, then approve it and save the profile. Inspection alone does
not grant trust. A CA certificate adds a private trust anchor for this profile;
a server certificate also pins the presented leaf's SHA-256 digest. Private keys,
certificate bundles with trailing material and oversized files are rejected.

Saved approval contains a path, public digest, profile ID and endpoint identity,
not credentials. The backend verifies these again before saving/using approval.
A different profile cannot borrow it. Changing the approved file requires a fresh
review; a different pinned server certificate fails. Removing or replacing trust
happens in profile editing, outside the failed connection attempt. Profile updates
cancel pending attempts and remove the old registered session. An existing worker
may finish work on a connection it already owned; new FTPS connections recheck the
current profile and global policy.

`HORTCMD_EXTRA_CA` is compiled only into the test harness. Production private CAs
must be explicitly scoped through profile editing. There is no accept-all switch.

Errors distinguish plaintext policy, invalid approval material, changed approval,
expiry, future validity, hostname mismatch, known revocation, other trust failure
and TLS negotiation failure. No credential is included in transport metadata.

## Connection details

Connection status and location capabilities expose typed protection state and
public certificate information. A live session's profile ID prevents a different
saved profile at the same address from appearing to own its certificate approval.
SFTP reports SSH-2.0; SMB reports its negotiated dialect and cipher when exposed.
FTPS/WebDAV cipher and version telemetry are not exposed by their current client
APIs (follow-up [TICKET-118](../tickets/118-negotiated-tls-diagnostics.md)), so the UI explicitly displays “Not exposed by the provider”, rather than
substituting the minimum supported version or an inferred cipher. TLS policy still
enforces version 1.2 or newer. SMB's integrity field describes required signing
or authenticated encryption, not the header flag of every individual frame.

## Verification and remaining native matrix

Local Rustls servers exercise trusted and untrusted peers, changed pins, expiry,
future validity, wrong hostnames, private CA approval/removal and profile isolation.
Invalid peers receive no seeded application credential. The actual FTPS adapter
is tested against rejected AUTH TLS, unknown certificates, pin rotation and a valid
TLS login with PROT P. The actual WebDAV adapter rejects a wrong certificate host
and never connects to the plaintext target of an HTTPS redirect.

Frontend tests verify the two independent plaintext opt-ins, certificate preview
before approval, localized details and rejection of stale previews after endpoint
changes. Existing connection/lifecycle tests remain in the normal suite.

The isolated protocol harness explicitly enables its plaintext fixtures only in
its own configuration directory. Native Windows/Linux runs and the live SMB server
matrix are deferred by user instruction to [TICKET-117](../tickets/117-native-transport-security-verification.md).
