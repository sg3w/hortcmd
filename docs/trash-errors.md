# System-trash errors (ticket 086)

The pinned trash 5.2.6 source exposes structured filesystem causes on freedesktop
platforms. Those retain the existing permission, missing-entry and other I/O error
codes and recovery actions. Before disposal, hortcmd also checks the local entry
with symlink_metadata, preserving the OS error and accepting dangling symlinks.
This check describes access at that instant; it does not guess why a later Finder
request failed.

On macOS the default Finder method invokes osascript. Its Error::Os code is the
**process exit status**, not errno and not an AppleEvent error number. Consequently,
exit 1 (or text mentioning -1743) does not prove automation was denied. The UI
identifies a Finder failure and suggests checking Finder and, if appropriate,
System Settings → Privacy & Security → Automation, while stating the uncertainty.
There is no localized diagnostic-string matching and no change of trash method.

CouldNotAccess, CanonicalizePath and Unknown do not carry enough evidence to
separate missing entries from denied access. These receive an explicit trash
failure with guidance to check existence/access. Other opaque platform failures
use the same conservative fallback. Original diagnostics pass through the existing
redaction builder and are not included in the displayed per-item messages.

Each entry is attempted independently. Failed entries are reported with their
paths; there is no retry using permanent deletion. A failed OS request does not
constitute a transactional guarantee that the OS made no change.

Regression tests cover error mapping, diagnostic redaction, missing entries,
dangling symlinks, retained source contents after failure, and the actual trash
confirmation/error dialog in German and English. Controlled native automation
refusal needs a disposable signed-app/TCC test setup; it was not exercised here.
Ticket 275 records that smoke-test procedure and the deferred platform checks.
