// ============================================================
// The payloads a transfer reports with (ticket 035).
//
// Names, field names and the events they travel on are the frontend's
// contract — `OpProgress`/`OpDone`/`Leftovers`/`CollisionReq` are
// generated into TypeScript by ts-rs — so they live in one module of
// their own and nothing else in the engine defines a wire type.
// ============================================================

use crate::errors::AppError;
use serde::Serialize;
use ts_rs::TS;

/// Ongoing progress of a transfer – event "fs-progress".
/// Two levels: current file (file_done/file_total) and
/// overall operation (bytes_done/bytes_total, files_done/files_total).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OpProgress {
    /// Unique operation ID (assigned by the frontend).
    pub id: String,
    /// "copy" | "move" | "extract" | "pack"
    pub op: String,
    /// currently processed file (full path)
    pub file_name: String,
    #[ts(type = "number")]
    pub file_done: u64,
    #[ts(type = "number")]
    pub file_total: u64,
    pub files_done: u32,
    pub files_total: u32,
    #[ts(type = "number")]
    pub bytes_done: u64,
    #[ts(type = "number")]
    pub bytes_total: u64,
    /// Whether the operation is currently paused (waiting to resume).
    pub paused: bool,
}

/// Completion of a transfer – event "fs-done".
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OpDone {
    pub id: String,
    pub op: String,
    pub ok: u32,
    /// What failed, per item (ticket 036). A batch keeps both halves:
    /// `ok` says how much succeeded, and each entry names the item it
    /// belongs to — a flat list of sentences loses the first and makes
    /// the second guesswork.
    pub errors: Vec<ItemError>,
    pub cancelled: bool,
    /// Which transfer safety policy actually ran: `"safe"` (staged and
    /// committed) or `"direct"` (written straight to the final name).
    /// Empty for archive operations, which have no such choice. Recorded
    /// so a failure can be read against the mode that produced it
    /// (ticket 029).
    pub mode: String,
}

/// Recognizable remains of an interrupted failure-safe transfer –
/// event "fs-leftovers". The engine only *reports* them: staging and
/// backup entries may hold the only copy of the user's data, so nothing
/// is deleted without an explicit answer (ticket 029).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Leftovers {
    /// The destination folder as an address, for the message.
    pub dir: String,
    /// Full addresses of the leftover entries.
    pub paths: Vec<String>,
}

/// Name-conflict request – event "fs-collision".
/// The transfer thread waits until the frontend calls `resolve_collision`.
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CollisionReq {
    pub transfer_id: String,
    pub req_id: String,
    /// target path that already exists
    pub path: String,
    pub is_dir: bool,
}

/// Result of a synchronous operation (e.g. delete).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OpResult {
    pub ok: u32,
    /// Per-item failures; `ok` is the count that succeeded anyway.
    pub errors: Vec<ItemError>,
}

/// One item of a batch that failed, and why (ticket 036).
#[derive(Serialize, Clone, Debug, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ItemError {
    /// The entry this is about — a path, or an archive member.
    pub item: String,
    pub error: AppError,
}

impl ItemError {
    pub fn new(item: impl Into<String>, error: AppError) -> Self {
        ItemError {
            item: item.into(),
            error,
        }
    }
}
