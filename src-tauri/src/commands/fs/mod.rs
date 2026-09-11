// ============================================================
// File-system commands, split by topic:
//   dir     – read directories/drives
//   size    – recursive folder size (background + cancellable)
//   volume  – capacity/usage of a drive or mount point
//   file    – copy/move/delete/folders + transfer infra
//   preview – file preview (F3) + open with default program (F4)
//   archive – ZIP: browse, extract, pack
//   walk    – reusable local directory traversal (search, sizes)
// ============================================================

pub mod archive;
pub mod attrs;
pub mod cancel;
pub mod compare;
pub mod dir;
pub mod filecompare;
pub mod file;
pub mod git;
pub mod preview;
pub mod props;
pub mod search;
pub mod size;
pub mod tags;
pub mod volume;
pub mod walk;
pub mod watch;
