//! Versioned transfer intent and commit records. No credential-bearing fields.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum RecoveryLocation {
    Local {
        path: String,
    },
    Remote {
        profile: String,
        identity: String,
        path: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RecoverySettings {
    #[ts(type = "number")]
    pub limit: u64,
    pub verify: bool,
    #[ts(type = "number")]
    pub buf_kb: u64,
    pub threads: u32,
    pub safe: bool,
}
impl Default for RecoverySettings {
    fn default() -> Self {
        Self {
            limit: 0,
            verify: false,
            buf_kb: 256,
            threads: 1,
            safe: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum RecoveryKind {
    Copy,
    Move,
    Extract,
    Pack,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub kind: RecoveryKind,
    pub sources: Vec<RecoveryLocation>,
    pub destination: RecoveryLocation,
    pub settings: RecoverySettings,
    pub base: String,
    pub names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Planned,
    Writing,
    Prepared,
    Publishing,
    Committed,
    SourceRemoved,
    Skipped,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub source: Option<RecoveryLocation>,
    pub target: RecoveryLocation,
    pub staging: Option<RecoveryLocation>,
    pub backup: Option<RecoveryLocation>,
    pub phase: Phase,
    /// SHA-256 over a bounded, deterministic tree manifest and file bytes.
    pub digest: Option<String>,
    /// Set only after the provider confirms durable data and namespace updates.
    pub durable: bool,
    pub owner: Option<OwnedArtifact>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedArtifact {
    pub location: RecoveryLocation,
    pub identity: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub version: u32,
    pub id: String,
    pub intent: Intent,
    #[serde(default)]
    pub preflight: Option<super::super::preflight::types::OperationPlan>,
    pub order: u64,
    pub created: u64,
    pub updated: u64,
    pub started: bool,
    pub terminal: bool,
    pub acknowledged: bool,
    #[serde(default)]
    pub retry_of: Option<String>,
    /// Stable error codes only; never provider messages, parameters or content.
    pub reason: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum RecoveryState {
    Queued,
    Running,
    Resumable,
    Retryable,
    Completed,
    CleanupRequired,
    UnsafeInterrupted,
    Unavailable,
    Unrecoverable,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RecoveryEntry {
    pub id: String,
    pub kind: RecoveryKind,
    pub state: RecoveryState,
    #[ts(type = "number")]
    pub order: u64,
    pub sources: Vec<RecoveryLocation>,
    pub destination: RecoveryLocation,
    pub committed: u32,
    pub total: u32,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct RecoveryLaunch {
    pub id: String,
    pub kind: RecoveryKind,
    pub sources: Vec<String>,
    pub destination: String,
    pub base: String,
    pub names: Vec<String>,
    pub settings: RecoverySettings,
}
