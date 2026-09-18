# Plugin interface

## Status and scope

API version 1 defines one broker protocol for JavaScript views and native plugins.
Tickets 100–106 implement discovery, isolated JavaScript views, native helper
processes, command buttons, context, mediated actions/events and local plugin
management. Installable reference plugins are provided by TICKET-107; the storage
analyzer remains separate work (TICKET-108).

PHP is source text, never executable server code. A future Apple Photos provider
will be read-only: browse and copy out, with no import, delete or mutation.

## Manifest

Each package has a `plugin.json` at its root. Packages live in the platform app
data directory under plugins/<reverse-DNS-id>. Discovery validates without
executing code and reports invalid packages alongside valid ones.

```json
{
  "id": "org.example.source-view",
  "name": "Source view",
  "version": "1.0.0",
  "description": "Display source text",
  "author": "Example",
  "apiVersion": 1,
  "runtime": "js",
  "entry": "index.html",
  "icon": "🌈",
  "permissions": ["read:context", "read:fs", "ui:preview"],
  "contributes": {
    "preview": [{
      "id": "source", "title": "Source", "icon": "🌈",
      "match": {"kinds": ["file"], "extensions": ["php", "html"]},
      "priority": 0
    }]
  }
}
```

Versions are three unsigned integer components. Unknown fields, capabilities,
runtimes, kinds and surfaces are rejected. Entries must exist within the package;
absolute paths, traversal and symbolic links are rejected. Icons are emoji or
base64 PNG, WebP or SVG data URLs, rendered as images, never injected markup.

A native entry is a platform map with macos, windows and linux library filenames.
An unsupported platform is reported without loading anything. Native packages
cannot contribute HTML views. They may contribute command buttons.

Command-bar items have id, title, icon and action. Action is either
`{"type":"openWindow"}` (JS, requiring ui:window) or
`{"type":"command","command":"inspect"}`. Contribution IDs are unique across
all surfaces within one plugin.

Preview items optionally override entry. Matches require a matching kind and,
when specified, extension and MIME type. Extensions omit the dot and compare
case-insensitively. Highest priority wins; ties use plugin ID then contribution
ID. No match, failure or missing readiness falls back to the built-in preview.

Tab items have id, title, icon, optional entry, surface "pane", path "active" or
"none", closable (default true), and singleton (default false). Active tabs
receive pane navigation; visual-only tabs receive no path. Singleton means one
instance of the contribution across panes.

## Permissions and protocol

All capabilities are default-deny, declared in the manifest and explicitly
granted by the user. Declaring a capability does not grant it. Revocation removes
access and closes owned instances. Paths are opaque VFS addresses, including
remote addresses; plugins never interpret them as necessarily local paths.

| Request type | Payload | Required permission |
| --- | --- | --- |
| ping | none | active instance |
| context:get | none | read:context |
| view:ready | none | active view instance |
| view:title | title | active view instance |
| view:requestClose | none | active view instance |
| fs:read | path, maxBytes, encoding (utf8/base64) | read:fs |
| fs:list, fs:preview, op:open | path | read:fs |
| fs:write | path, content | fs:write |
| op:copy | sources, destination | op:copy |
| op:move | sources, destination | op:copy and fs:write |
| op:limit | value (null to read) | op:limit |
| notify | message | notify |
| events:subscribe / events:unsubscribe | event (selection/transfer) | events:selection plus read:context, or events:transfer |

Surface contributions additionally require ui:window, ui:commandbar, ui:preview
or ui:tab as appropriate. The same permission table applies to native callbacks.

Core events are view:open (viewId, instanceId, optional path/context), view:path
(instanceId and path), view:close, context:changed, command and transfer.
The view must send view:ready before the host stops showing its loading state.
The broker associates identity with the running instance, never with an ID
asserted by a request payload.

PluginContext includes path, folderPath, filePath (nullable), selection, side,
copyLimit and an optional copyOperation containing sources and destination.
No context is delivered without read:context. Transfer context does not authorize
any operation on its own. Generated TypeScript definitions in
`src/ipc/bindings/` are exported from the Rust types through ts-rs.

## Threat model

JS code is untrusted. It runs inside an opaque-origin sandboxed frame, including
inside a dedicated plugin window. It has no Tauri invoke access, application DOM
access or direct file/network access. The trusted bridge validates the sending
frame and passes only typed messages to the Rust broker. HTML/JS libraries must
be packaged locally. The core owns all file and system operations.

Native code is trusted system code. A library loads only inside a dedicated
helper process, never the commander process. The helper prevents a library crash
from taking down the commander but is **not an operating-system sandbox**. Native
code has the user's OS privileges and can bypass broker permissions using OS
APIs. Approval must disclose this distinction. Broker callbacks themselves still
require declared and granted permissions. A malicious native plugin is outside
the protection boundary; install only trusted native code.

Neither host treats a view, context snapshot, cached result or plugin-provided
path as authorization for a destructive operation. Existing core validation,
provider capabilities and operation controls remain authoritative.

## JavaScript host and SDK

JavaScript plugins use dedicated plugin windows or the shared embedded-frame
host for previews and pane tabs. Opening an instance requires an active grant;
discovery alone never starts a plugin.

A JS entry is an HTML document. Package JavaScript/CSS libraries alongside it (no
CDN), using paths relative to the package root. The asset server rejects links and
traversal and limits each asset to 2 MiB. Frames use `sandbox="allow-scripts"`
without same-origin, forms, popups or top navigation. The asset response has its
own restrictive CSP; the main document keeps its existing script policy.

The host injects the following API before plugin scripts run:

```js
const unsubscribe = hortcmd.onEvent(event => {
  if (event.type === "view:open") {
    document.querySelector("#path").textContent = event.data.path ?? "";
  }
});
await hortcmd.request({type: "view:ready"});
console.log(await hortcmd.request({type: "ping"})); // "pong"
```

Use textContent for source displays, including PHP. A request returns a promise;
rejected requests throw an Error. The SDK allows at most 32 outstanding requests
and times them out after 15 seconds. Send view:ready within 10 seconds. A load
failure, readiness timeout, revoked/closed instance or broken bridge removes the
frame; callers retain the built-in view as fallback. Cleanup removes listeners,
queued events and package access. Event queues retain at most 128 events.

Only messages from the exact opaque-origin child frame reach the broker. Plugin
window wrappers can invoke only instance-scoped bridge commands; they cannot call
ordinary file, network, settings or administrative commands. The native broker
also rejects an instance owned by a different window. A separate window is reused
on another open request for the same plugin. Up to 32 instances may coexist.

SDK typings are in `src/features/plugins/pluginApi.ts`; payload types are generated
from `src-tauri/src/plugins/protocol.rs`.

## Native helper and C ABI

TICKET-102 loads native libraries in a dedicated child process. The installed
commander executable is started with an internal helper flag before any Tauri
initialization; no separate sidecar installation is required. The main process
validates the manifest and grants before launching it. Native code retains the
user's OS privileges: only the crash boundary is isolated.

Use [hortcmd_plugin.h](plugins/hortcmd_plugin.h) or the equivalent Rust layouts in
`src-tauri/src/plugins/native_abi.rs`. Export the C symbol hortcmd_plugin_v1,
returning a stable table with ABI version 1 and its exact struct size. The table
provides initialize, event and shutdown. Wrong versions, missing symbols and
failed initialization reject the instance. Library names use .dylib, .so or .dll
in the appropriate manifest platform entry.

Initialize receives a host table with a request callback. Send a UTF-8 JSON
PluginRequest and receive UTF-8 JSON containing either value or error. All
buffers are caller-owned; no Rust String, Vec, trait object or allocator ownership
crosses the boundary. Allocate 1 MiB for the result. A negative return indicates
transport failure; -2 means insufficient output capacity. Never retry a mutation
just because its response did not fit. Host calls must run synchronously on the
initialize/event thread; background-thread callbacks are rejected. Do not retain
input event pointers after the event function returns.

The helper and core exchange bounded newline-delimited JSON over private pipes.
Only one event is in flight at a time. Initialization and event handling have a
10-second deadline; closing an instance permits 300 ms for cooperative shutdown
before termination. Every helper is reaped. Application exit also terminates
registered helpers. A library panic/abort, invalid ABI pointer, invalid wire
message or pipe failure can terminate the helper without taking down the core.
Long work should be split into short event handlers; this ABI does not support
blocking callbacks or background calls into the broker.

Tests compile a real C library, verify allowed and denied callbacks, reject an
incompatible ABI, crash a helper deliberately, and close a hung handler. These
process tests run on Unix without a GUI. Windows requires native verification of
DLL packaging and process behavior; the platform entry and transport are shared.

## Command bar

TICKET-103 adds a separate, horizontally scrollable plugin bar below the core
command bar. It is absent when no active plugin contributes permitted buttons.
Buttons show both plugin ownership and a colorful icon. SVG icons are rendered
only as image sources, never as application DOM markup.

A click sends the plugin ID and declared button ID to the backend. The backend
reloads and validates the manifest, checks active grants and resolves the declared
action itself. The frontend cannot substitute an arbitrary action. openWindow
opens or focuses the JS window. A command queues a command event for an existing
JS instance; if none exists, a JS plugin with ui:window may open its window first.
A JS plugin without ui:window must have an open view before a command is sent.
Native command buttons start or reuse the helper. Events wait for readiness.

Optional plugin and MCP controls load behind React lazy boundaries. The existing
325 KiB entry-chunk budget remains unchanged.

## Context, previews and pane tabs

TICKET-104 publishes typed snapshots from the main UI and canonicalizes all paths
in Rust. Window/native instances follow the active pane on context:get. Path-bound
tabs follow the pane that opened them, even when the other pane becomes active.
Preview instances retain their requested file/folder path. Visual-only tabs
receive no path or context, including from context:get. Without read:context,
view:open omits both path and context and context:get is denied.

Context includes the cursor file, selected full VFS locations, pane side and the
copy limit in KiB/s (zero means unlimited). Optional copyOperation is a handoff,
not an instruction to perform a transfer. The trusted UI can publish one through
the publishContext helper before launching a plugin. This snapshot never grants
filesystem permissions. Archive members do not currently have VFS addresses in
this contract: while inside an archive the context describes its container and
has an empty member selection, rather than inventing potentially dangerous paths.

Each new instance queues view:open; the SDK receives it after view:ready. Pane
navigation queues view:path for path-bound tabs. Queued path changes are coalesced.
Workspace snapshots are limited to 256 KiB and 4096 selection/copy entries.
view:close is advisory on teardown; plugins cannot veto host cleanup.

The pane tab bar offers **Plugin views** for active tab contributions. A singleton
contribution reuses its existing instance and owning pane. Inactive tabs stay
mounted, so returning to a folder tab does not destroy plugin state. A failed view
is removed and the folder remains usable. Dynamic view:title updates its caption.

F3 resolves active preview contributions by kind, extension/MIME and priority in
Rust. The embedded renderer uses the same bridge as a separate window. The core
retains its preview for a missing match, failure or readiness timeout; for folders
without a renderer it explains that the folder remains available in the pane.
PHP is displayed as source text and is never executed.

## Mediated actions and events

Both runtimes use the same permission-checked broker. `fs:list`, `fs:read`, and
`fs:preview` use the commander's filesystem services. Reads are limited to
256 KiB per request; `fs:write` accepts at most 256 KiB and uses the core
create-only write operation. `op:open` navigates a directory and never launches
an executable. Paths must be absolute local paths or supported VFS locations.
PHP files are read as source text; the host never executes PHP.

`op:copy` and `op:move` return `{operationId, status: "awaiting-confirmation"}`.
The normal transfer dialog performs preflight review before the backend starts
the operation. Cancellation, queuing, progress, recovery and verification reuse
the core transfer system. Moving additionally requires `fs:write`. Each request
captures the current speed limit in KiB/s (zero means unlimited); the engine
receives bytes/s. `op:limit` reads or changes that limit. Closing or deactivating
the originating instance cancels its pending and running transfers.

Subscribe with `events:subscribe` and `event: "selection"` or `"transfer"`;
unsubscribe with `events:unsubscribe`. Selection additionally requires
`read:context`. Transfer events expose operation ID, status and byte count,
without source paths. Context updates and intermediate progress are coalesced
in the bounded event queue. `notify` displays a message in the commander.

```js
await hortcmd.request({type: "events:subscribe", event: "transfer"});
hortcmd.onEvent(event => {
  if (event.type === "transfer") console.log(event.operationId, event.status);
});
const pending = await hortcmd.request({
  type: "op:copy", sources: ["/absolute/source.txt"],
  destination: "/absolute/destination"
});
// The user reviews the transfer in the commander before any copy starts.
```


## Installation, updates and permission management

Open **Configuration → Settings → Plugins**. Enter an absolute local folder or
ZIP path and choose **Install**. `plugin.json` must be directly at the package
root (without an enclosing folder in a ZIP). No marketplace, downloads or
package scripts run during installation. Packages are staged and validated
before publication; links, special files, traversal and platform-incompatible
entries are rejected. Limits are 4,096 entries, 16 directory levels and 256 MiB
of extracted file content. Keep assets below the separate 2 MiB serving limit.

New installations are inactive. **Activate** displays the declared permissions;
**Approve and activate** grants all listed permissions. Cancel leaves the plugin
inactive. Native approval explicitly explains that a helper is not a security
sandbox. The UI presents name, version, author, runtime, status and validation
errors. Active plugins contribute their declared buttons, previews and tabs.

Approvals persist in the hidden .state.json file inside the plugins folder in
the application data directory.
They are bound to a SHA-256 digest of the package paths and contents. The host
loads approvals on discovery and revalidates them before launching views or
commands. Changed packages require a fresh review. This detects changes between
uses; it is not protection against an attacker already controlling the user's
account or native plugins modifying files while a view is running.

For an **Update**, enter the new package path and choose Update beside the
installed plugin. Its ID must match and its numeric `major.minor.patch` version
must increase. Updates stop instances, revoke old grants and require activation
again. If publication fails, the old directory is restored but remains inactive.
**Deactivate** preserves package files and revokes grants; **Uninstall** confirms
removal of the installed package. Both stop instances and their owned transfers.
Uninstall does not remove unrelated files a plugin created through approved
filesystem operations.

## Author workflow and supported extensions

1. Create a folder containing `plugin.json`, an HTML entry and local assets.
2. Declare only the permissions needed by the contribution. For a source preview,
   request `read:context`, `read:fs` and `ui:preview` as in the manifest above.
3. Register event listeners, then send `view:ready`. Render source with
   `textContent`, never by injecting the file's markup into the plugin document.
4. Install the folder in Settings and approve its permissions. Open a matching
   file preview, plugin tab or declared command button.
5. To distribute a change, increment the manifest version and install it with
   Update. No build-time dependency on the commander's frontend is needed.

Example source reader inside a packaged HTML document:

```html
<pre id="source"></pre>
<script>
hortcmd.onEvent(async event => {
  if (event.type !== "view:open" || !event.data.path) return;
  try {
    const result = await hortcmd.request({
      type: "fs:read", path: event.data.path,
      maxBytes: 262144, encoding: "utf8"
    });
    document.querySelector("#source").textContent = result.content;
  } catch (error) {
    document.querySelector("#source").textContent = String(error);
  }
});
hortcmd.request({type: "view:ready"});
</script>
```

Native authors implement the versioned C ABI in
[hortcmd_plugin.h](plugins/hortcmd_plugin.h) and supply a library per supported
platform. See the native helper section for buffer ownership and threading.
The integration fixture under `src-tauri/src/plugins/fixtures/` illustrates a
small compiled library and is exercised as an actual helper subprocess in tests.

Current extension surfaces are command buttons, separate JS windows, previews,
pane tabs and broker actions/events. A Swift/PhotoKit filesystem provider, archive
providers, columns and context-menu extensions are not part of this API release.
Future Apple Photos access must remain read-only: browse and copy out only.

## Validation

Each of tickets 100–106 is checked with `npm run check`: formatting, strict
Clippy, Rust tests, generated IPC bindings, architecture/documentation/UI checks,
TypeScript, Vitest, production build and bundle budget. Tests cover denied
permissions, manifest validation, bridge ownership, native ABI rejection and
crash/timeout isolation, context/view lifecycle, transfer review, package import,
permission persistence, changed-package revocation and settings dialogs.
Native integration tests run locally on the host platform. Interactive desktop
rendering and Windows/Linux native integration still need platform smoke tests;
a passing browser component test does not substitute for those checks.

## Installable reference packages

[Reference plugin instructions](../plugins/README.md) describe Path tools, a
JavaScript context/copy example, and Native hello, a dependency-free Rust library.
Both use the normal installer and explicit activation workflow. Their source,
package-level tests and native subprocess coverage serve as author templates.
