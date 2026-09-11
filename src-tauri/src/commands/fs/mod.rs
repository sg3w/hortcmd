// ============================================================
// File-system commands, split by topic:
//   dir     – read directories/drives
//   size    – recursive folder size (background + cancellable)
//   volume  – capacity/usage of a drive or mount point
//   file    – the copy/move/delete/folder *commands* (thin adapters)
//   transfer – the engine behind them: events, operation registries,
//             progress, planning/commit, sequential and parallel copy
//   preview – file preview (F3) + open with default program (F4)
//   archive – ZIP: browse, extract, pack
//   walk    – reusable local directory traversal (search, sizes)
//   selection – save/load a marked-entry list (TICKET-059)
// ============================================================

pub mod archive;
pub mod attrs;
pub mod cancel;
pub mod compare;
pub mod dir;
pub mod file;
pub mod filecompare;
pub mod git;
pub mod preview;
pub mod props;
pub mod search;
pub mod selection;
pub mod size;
pub mod tags;
pub mod transfer;
pub mod volume;
pub mod walk;
pub mod watch;

/// Narrow window onto the transfer engine's commit primitive so the
/// live provider tests can check each protocol's staging contract
/// (ticket 029) without making the engine's internals public.
#[cfg(test)]
pub mod staging_test_support {
    pub(crate) use super::transfer::plan::commit;
}
