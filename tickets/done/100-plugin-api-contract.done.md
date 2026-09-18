# TICKET-100: Plugin API & manifest (contract first)

## Goal
The binding contract of the plugin system is in place: manifest schema, permission model,
broker protocol, and the shared types. No execution of plugins yet — only the
API that all following tickets (101–108) build on.

## Background
Concept.md (§10 P4, §Vision) explicitly requires **defining the plugin API early**,
before hosts or UI are built. A clean, runtime-agnostic
contract prevents later breaks between the JS and the native Rust host.

## Requirements
- **Manifest `plugin.json`** per plugin with the fields:
  - `id` (reverse-DNS, unique), `name`, `version`, `description`, `author`
  - `apiVersion` (contract version; incompatible plugins are rejected)
  - `runtime`: `"js" | "native"`
  - `entry`: JS → HTML/JS entry file; native → library name per platform
  - `icon`: **colorful** — emoji **or** data URI/SVG
  - `permissions[]`: declared capabilities (see below)
  - `contributes.commandbar[]`: `{ id, title, icon, action }`
  - `contributes.preview[]` and `contributes.tabs[]`: **view contributions**
    (see "Views" below)
- **Permission model, default-deny** — capability strings at least:
  `read:context`, `read:fs`, `fs:write`, `op:copy`, `op:limit`,
  `events:selection`, `events:transfer`, `ui:window`, `ui:commandbar`,
  `ui:preview`, `ui:tab`, `notify`.
  Undeclared permissions are forbidden; declared ones must be confirmed by the
  user at install time.
- Define the **broker protocol** (runtime-agnostic): messages/commands
  (plugin → core) and events (core → plugin), each with a payload type and
  the required permission. The transport stays host-specific (101/102), the protocol is not.
  For views this includes at least:
  - core → plugin: `view:open` (`{ viewId, instanceId, path? , context? }`),
    `view:path` (`{ instanceId, path }` — the pane navigated; only for path-bound
    views), `view:close`.
  - plugin → core: `view:ready` (handshake — until it arrives the core shows its own
    loading state and can fall back), `view:title` (dynamic tab/preview caption),
    `view:requestClose`, `fs:read` (`{ path, maxBytes, encoding }`, needs `read:fs`).
  - A view that never sends `view:ready`, or that errors, must not break the pane or
    the preview dialog — the core falls back to the built-in rendering.
- Export the **shared types** via **ts-rs** (`src/ipc/bindings/`): `PluginManifest`,
  `PluginPermission`, `PluginContext`, `PluginContribution`, broker messages/events,
  and for the views `PluginPreviewContribution`, `PluginTabContribution`,
  `PluginViewMatch`, `PluginViewOpen`, `PluginViewPath`.
- Define **`PluginContext`** (filled in 104): `path` (general/active),
  `folderPath`, `filePath`, `selection[]`, `side`, `copyLimit`, optional
  copy operation.
- **Directory convention**: plugins live in the app data folder under `plugins/<id>/`;
  a backend command **detects and validates** present manifests
  (`list_plugins` → list incl. validation errors), **without** executing plugins.
- **Document the threat model** (in this ticket + referenced in 102): JS plugins can be
  contained in the WebView; **native plugins run with full process privileges and are
  not hard-sandboxable** — their protection is trust-based.
- Location: `src-tauri/src/plugins/{manifest.rs, registry.rs}`,
  `src-tauri/src/commands/plugins.rs`; no host/broker logic in this ticket except
  detection/validation.

### Views: preview and tabs (frontend-only surfaces)
Besides its own window (101) a plugin can contribute **views that live inside the
main window** and are handed a **path**: a preview renderer for the F3 preview, and
tabs in the pane tab bar. Both are **pure frontend surfaces** — the plugin ships
HTML/JS, the core renders it embedded and feeds it a path; no backend part of the
plugin is involved and no extra native window is opened.

- **`contributes.preview[]`** — renders a path *visually* in place of the built-in
  preview: `{ id, title, icon, entry?, match, priority? }`
  - `match`: `{ kinds: ("file" | "folder")[], extensions?: string[],
    mimeTypes?: string[] }` — which paths this renderer claims.
  - `priority` (number, default `0`): highest match wins; the **built-in preview
    stays the fallback** and is used whenever no plugin matches or a plugin view
    fails to load.
- **`contributes.tabs[]`** — a tab in the pane tab bar next to the folder tabs:
  `{ id, title, icon, entry?, surface, path, closable?, singleton? }`
  - `surface`: `"pane"` (today the only value; the enum exists so a global/bottom
    tab area can be added without a contract break).
  - `path`: `"active"` → the tab is **path-bound**: it receives the pane's path on
    open and on every subsequent navigation. `"none"` → a **purely visual tab** that
    gets no path at all.
  - `singleton`: at most one instance per plugin; `closable` (default `true`).
- `entry` defaults to the manifest `entry`; a view may point at its own HTML file, so
  one plugin can ship several distinct views.
- **Runtime restriction:** view contributions require `runtime: "js"` — they render
  HTML. A native plugin (102) contributes command-bar entries and actions, not views;
  the manifest validation rejects the combination rather than silently ignoring it.
- **Paths can be remote.** Since tickets 020–026 a path may be a VFS address
  (`sftp://user@host:port/…`), so a view must never assume a local file-system path.
  Reading content goes through the broker (`fs:read`, permission `read:fs`), which
  resolves the address via the VFS — a plugin does not read files itself.
- **Containment:** an embedded view is not part of the main window's DOM — it is a
  sandboxed frame with no Tauri `invoke`, reaching the core only through the broker
  bridge from 101 and checked against the same permissions. The threat-model document
  states this explicitly, including that a *native* plugin has no such containment.

### Where the views get implemented (not in this ticket)
This ticket only fixes the contract. The surfaces themselves belong to:
**101** (JS host — the embedded frame variant of the bridge next to the separate
window), **104** (feeding `path`/context into a running view), **106** (granting
`ui:preview`/`ui:tab` at install time) and **107** (the demo plugin gains a preview
renderer and a path-bound tab, which is what proves the contract).
Embedded views remain part of 101. Number 108 is assigned to the first functional
plugin, the [storage-usage analyzer](../108-folder-storage-usage-visualizer.md).
If embedded views need a separate ticket, allocate an unused plugin-series number.

## Acceptance criteria
- [x] The manifest schema is typed in Rust and available in the frontend via ts-rs.
- [x] `list_plugins` finds plugin folders, parses/validates manifests, and reports
      errors (missing fields, unknown permissions, incompatible `apiVersion`) clearly.
- [x] The permission model (capability strings, default-deny) is fixed as a type + docs.
- [x] The broker protocol (commands/events + required permissions) is documented.
- [x] **View contributions** (`preview[]`, `tabs[]`) are part of the schema and the
      generated bindings, including match rules, `priority` and the
      path-bound/purely-visual distinction (`path: "active" | "none"`).
- [x] Validation rejects: a view contribution on a `native` plugin, a missing `entry`
      file, a duplicate view `id`, an unknown `kind`/`surface`, and a view
      contribution without `ui:preview`/`ui:tab` in `permissions[]`.
- [x] The docs state that the built-in preview stays the fallback and that a failing
      view never breaks the pane or the preview dialog.
- [x] A Markdown document describes the plugin manifest and the threat model,
      including the containment of embedded views.
- [x] **No** plugin is loaded or executed.

## Tests
- `list_plugins` against a folder with (a) a valid, (b) an invalid, and
  (c) an incompatible manifest → correct result/error list.
- `list_plugins` against manifests with view contributions: a valid preview + tab
  contribution parses with its match rules; the rejection cases from the acceptance
  criteria each produce a clear, distinguishable error.
- Match resolution is unit-tested without any plugin running: given a set of
  contributions, the correct renderer wins by `extensions`/`kinds` and `priority`,
  and an unmatched path falls back to the built-in preview.
- `npm run gen:types` generates the bindings without errors.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated (Concept.md + manifest/threat-model docs)
- Rename the ticket to *.done.md

## Implementation notes

API version 1 is implemented in the Rust plugin modules and generated TypeScript
bindings. See [Plugin interface](../../docs/plugin-interface.md). Native libraries
will run in a helper process (user decision); process isolation is not an OS sandbox.
PHP rendering means source display only. No plugin code executes in this ticket.
