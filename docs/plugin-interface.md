# Plugin interface

## Status and scope

API version 1 defines one broker protocol for JavaScript views and native plugins.
The contract and discovery are implemented in TICKET-100. Execution, integration
and management follow in TICKET-101 through TICKET-106. The reference plugins and
storage analyzer remain separate work (TICKET-107 and TICKET-108).

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
