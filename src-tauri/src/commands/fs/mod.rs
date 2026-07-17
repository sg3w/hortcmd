// ============================================================
// File-system commands, split by topic:
//   dir     – read directories/drives
//   file    – copy/move/delete/folders + transfer infra
//   preview – file preview (F3) + open with default program (F4)
//   archive – ZIP: browse, extract, pack
// ============================================================

pub mod archive;
pub mod attrs;
pub mod compare;
pub mod dir;
pub mod filecompare;
pub mod file;
pub mod git;
pub mod preview;
pub mod props;
pub mod search;
pub mod tags;
pub mod watch;
