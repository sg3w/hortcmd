# Local diagnostics and support bundles (schema 1)

Settings → Diagnostics shows version, OS/architecture, development/production
build, diagnostic configuration, provider capability summaries, recent error codes
and the actual log directory. Production defaults to Info, development to Debug.
Error/Warn/Info/Debug may be changed for this session only. Restart restores the
default. No level enables payload logging. The service has no HTTP client, upload,
telemetry endpoint or automatic export.

The record schema accepts only timestamp, enum level/component/event, a 128-bit
correlation ID, an allowlisted error code and an optional session-keyed identifier
pseudonym. There is deliberately no free-form message/attribute field. Paths,
hosts, profile names and usernames are never persisted verbatim. URLs (including
credentials and encoded query strings), nested errors, auth methods, headers,
private keys, clipboard, terminal I/O and file contents cannot enter a record.
Protocol errors contribute only stable codes. Identifier hashing happens before
persistence, uses a random process key, and is consistent within a session/bundle.
Keys are never persisted or exported. Restart/deletion breaks identifier linkage.

UI action lifecycle calls carry a random correlation in the Tauri header; the IPC
boundary installs it for synchronous commands, file-operation workers and their
parallel copy workers inherit it, and connection workers/monitor retries retain
it. AppError IDs created within the scope use that same ID. Provider resolutions
and connection attempts record the same context. Transfer cancel/pause requests
also emit an event under the original active operation's ID. A finished lifecycle
means the worker/IPC call returned, not that every item succeeded: consult Failed
records. Uninstrumented asynchronous commands still get independent error IDs;
there is no claim that arbitrary third-party threads inherit thread-local state.
Byte progress, polling, terminal and clipboard commands are excluded from the
lifecycle allowlist. Native plugin internals are outside this host-side logging.

Logs live in `<config>/hortcmd/diagnostics`, or
`$HORTCMD_CONFIG_DIR/diagnostics`, as four JSONL files of at most 512 KiB each.
Files older than seven days are removed on startup/write/read; records older than
seven days are excluded on read/export. Unix directories/files are 0700/0600;
Windows uses the user's config-directory ACL. Log writes are serialized; busy,
unavailable, full or denied storage drops diagnostic events without propagating
errors to startup or file operations. Corrupt/truncated/unknown records are skipped
on read and revalidated before export. Logging is best effort, not an audit trail.

The screen previews three categories before export: lifecycle records, build and
platform metadata, and capability/diagnostic configuration. Choose a new absolute
local `.json` filename and explicitly export. No existing file is overwritten.
The default bundle omits the log path as well as all raw identifiers. The optional
checkbox includes **only the log directory**, which may reveal a username; event
identifiers stay pseudonymized. The manifest states that choice. No saved profiles,
application config file, tokens, environment variables or other logs are bundled.
Deletion removes all four owned log files and rotates the in-memory pseudonym key;
future operations may create fresh logs. Previously exported files remain under
the user's control and are not deleted by this action.
