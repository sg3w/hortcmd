# TICKET-102: Native plugin host (isolated helper process)

## Goal
Native plugins are loaded as a dynamic library in a dedicated helper process and connected via the same
broker mediation as JS plugins. The contract from TICKET-100 applies
unchanged; only the transport is native.

## Background
Concept.md explicitly names Rust/JS plugins. Native plugins enable performance
and system proximity, but bring ABI and security questions that must be cleanly
regulated.

## User decision

Native libraries must run in a helper process. A crash or abort terminates the
helper, not the commander. Process isolation is not an OS privilege sandbox.

## Requirements
- **Stable C-ABI interface**: `#[repr(C)]` vtable (or `abi_stable`) with a clearly
  versioned entry symbol; the plugin receives a **host callback table**
  (broker calls) and reports back its contributions/handlers.
- **Version/ABI check** on load: `apiVersion` and ABI signature must match,
  otherwise the plugin is rejected (no crash of the main process).
- **Lifecycle**: load, initialize, unload; an error in the plugin must not take down the
  core (defensive call boundaries, contain panics and crashes in the helper process).
- **Permission enforcement via the same `PluginBroker`** as 101: every host callback checks
  the plugin's granted permissions.
- **Security documentation (binding)**: native plugins run with **full
  user privileges in the helper process**; there is **no hard sandbox**. The "permission model" is here
  trust-based (origin/signature + explicit user approval). This boundary
  is clearly named in the ticket result and in the plugin docs.
- Account for platform-specific library names (`.dylib`/`.so`/`.dll`).
- Location: `src-tauri/src/plugins/native_host.rs` (+ shared `broker.rs`).

## Acceptance criteria
- [x] A minimal native Rust plugin is loaded, initialized, and unloaded again.
- [x] The plugin calls an allowed broker function via the host callback table.
- [x] A call without a matching permission is rejected.
- [x] An incompatible `apiVersion`/ABI is rejected cleanly, without terminating the core.
- [x] A panic or crash in the plugin cannot terminate the commander process.
- [x] The sandbox/trust boundaries are documented.

## Tests
- Load/unload a minimal plugin; allowed and denied broker call.
- Negative test: wrong `apiVersion` → defined rejection.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated (incl. security boundaries)
- Rename the ticket to *.done.md

## Implementation evidence

Implemented and verified with `npm run check`. See
[Plugin interface](../../docs/plugin-interface.md) for the contract, security
boundaries and author instructions. Native cross-platform GUI behavior requires
verification on the corresponding operating systems.
