# Connection monitoring

In Connections, edit a profile and expand **Connection monitoring**. Monitoring
and automatic reconnection are enabled by default for existing and new profiles.
The defaults are a 30-second check interval, a 10-second response timeout and
three reconnect attempts. Both monitoring and automatic reconnect can be disabled
per profile. Settings are saved with the profile, without credentials.

Three failed checks mark a session interrupted; a transport known to be closed
is removed immediately. FTP closes a failed control socket immediately so a late
NOOP reply cannot be consumed by a later file operation. A busy FTP control
channel postpones the check. Retries wait 1, 2, 4 seconds (up to 60 seconds when
more attempts are configured). Suspend/resume never triggers catch-up bursts.

The connection manager and screen-reader announcements distinguish interruption,
reconnection and final failure. **Disconnect** also cancels a scheduled retry
before it has opened a socket. Editing/deleting a profile, manual disconnect and
application exit stop its supervisor. Only one supervisor and connection attempt
may own a canonical store; late results cannot replace or remove a new session.

Reconnection uses the normal saved profile, credentials, host-key and TLS checks.
Authentication, keychain, host-key, certificate and unclassified protocol failures
stop automatic retries. Use the normal Connect action for the existing trust or
unlock interaction. Only unreachable/timeout connect failures are retried.

Visible panes refresh their current path when their connection returns. A missing
path remains an explicit listing error. Transfers and shell commands are never
replayed. Interrupted transfer remnants remain subject to the existing recovery
and ownership checks. Closed SSH terminals require a new shell; old input is not
sent to it.

## Protocol methods and verification

| Provider | Authenticated read-only check | Coordination / timeout | Local evidence |
| --- | --- | --- | --- |
| SFTP/SSH | SSH keepalive plus SFTP REALPATH round trip | Existing SSH message routing; cancellable Tokio timeout | Passed |
| FTP | NOOP on the existing control channel | Nonblocking mutex acquisition; socket timeout; close on failure | Passed |
| Explicit FTPS | NOOP inside the TLS control channel | Same mutex and socket timeout | Passed |
| Implicit FTPS | NOOP inside the TLS control channel | Same mutex and socket timeout | Passed |
| SMB | SMB2 ECHO on an already authenticated session | Existing signed/encrypted session and message-ID routing; configured connection timeout | Passed |
| WebDAV HTTP | Authenticated depth-0 PROPFIND on the profile start path | Separate HTTP request; cancellable Tokio timeout | Passed |
| WebDAV HTTPS | Same PROPFIND with normal TLS validation | Same timeout; no redirect or trust bypass | Passed |

The local Docker test `harness_monitor_roundtrips_and_recovers_after_silence`
passed for all seven servers: successful probe, paused server and bounded failure,
unpause, fresh authenticated connection and successful listing. SMB's connection
captures its response timeout at login. New adapters without a health-check
implementation explicitly report unsupported monitoring.

Deterministic supervisor tests cover response/busy/timeout decisions, bounded
retry delays, successful and exhausted retries, disabled reconnect, cancellation,
security failures and stale session results. Existing TLS/host-key, transfer
recovery and shell tests remain part of the regression suite. This evidence is
from local macOS execution with disposable Linux protocol servers; it does not
claim native Windows/Linux GUI verification.
