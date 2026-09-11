# TICKET-203: Native Rust plugin host (dynamic library)

## Goal
Native Rust plugins are loaded as a dynamic library and connected via the same
broker mediation as JS plugins. The contract from TICKET-201 applies
unchanged; only the transport is native.

## Background
Concept.md explicitly names Rust/JS plugins. Native plugins enable performance
and system proximity, but bring ABI and security questions that must be cleanly
regulated.

## Requirements
- **Stable C-ABI interface**: `#[repr(C)]` vtable (or `abi_stable`) with a clearly
  versioned entry symbol; the plugin receives a **host callback table**
  (broker calls) and reports back its contributions/handlers.
- **Version/ABI check** on load: `apiVersion` and ABI signature must match,
  otherwise the plugin is rejected (no crash of the main process).
- **Lifecycle**: load, initialize, unload; an error in the plugin must not take down the
  core (defensive call boundaries, catch panics at the FFI boundary).
- **Permission enforcement via the same `PluginBroker`** as 202: every host callback checks
  the plugin's granted permissions.
- **Security documentation (binding)**: native plugins run with **full
  process privileges**; there is **no hard sandbox**. The "permission model" is here
  trust-based (origin/signature + explicit user approval). This boundary
  is clearly named in the ticket result and in the plugin docs.
- Account for platform-specific library names (`.dylib`/`.so`/`.dll`).
- Location: `src-tauri/src/plugins/native_host.rs` (+ shared `broker.rs`).

## Acceptance criteria
- [ ] A minimal native Rust plugin is loaded, initialized, and unloaded again.
- [ ] The plugin calls an allowed broker function via the host callback table.
- [ ] A call without a matching permission is rejected.
- [ ] An incompatible `apiVersion`/ABI is rejected cleanly, without terminating the core.
- [ ] A panic in the plugin at the FFI boundary is caught (no process crash).
- [ ] The sandbox/trust boundaries are documented.

## Tests
- Load/unload a minimal plugin; allowed and denied broker call.
- Negative test: wrong `apiVersion` → defined rejection.

## Definition of Done
- Code compiles (`cargo check`, `npm run typecheck`, `npm run build`)
- Tests pass
- Existing functionality unchanged
- Documentation updated (incl. security boundaries)
- Rename the ticket to *.done.md
