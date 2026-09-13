// ============================================================
// The transfer engine, split by responsibility (ticket 035).
//
// `file.rs` had grown to hold the wire types, the operation registries,
// two progress implementations, the staging/commit rules, both copy
// paths, the orchestration *and* the command adapters. Each of those is
// now its own module, with the dependency direction running strictly
// downwards:
//
//   events    ← nothing            the serialized payloads (ts-rs)
//   registry  ← events             cancel/pause/collision state
//   progress  ← events             Prog, SharedProg, ProgressSink
//   plan      ← nothing            naming, staging, commit, leftovers
//   copy      ← plan, progress     the sequential path
//   parallel  ← plan, progress,    the worker-pool path
//               copy
//   engine    ← all of the above   one transfer, start to finish
//
// Nothing points back up: `plan` knows nothing about progress, `copy`
// nothing about the engine's orchestration. That is what lets one of
// them be read — or changed — without the others.
//
// The command adapters stay in `file.rs`, which is what `lib.rs`
// registers; the names, events and payloads are unchanged.
// ============================================================

pub mod copy;
pub mod engine;
pub mod events;
pub mod parallel;
pub mod plan;
pub mod progress;
pub mod recovery;
pub mod registry;

// The surface the rest of the crate uses. Re-exported here rather than
// reached for by module path, so a later move inside the engine does not
// ripple into `archive.rs` or `lib.rs`.
pub use events::{OpDone, OpResult};
pub(crate) use progress::Prog;
pub(crate) use registry::spawn_op;

// The engine's own tests live next to it rather than inside one of the
// modules: they exercise planning, commit, both copy paths and the move
// ordering *together*, which is the behaviour that matters and is not
// any single module's.
#[cfg(test)]
mod tests;
