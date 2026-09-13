use super::super::recovery::types::{RecoveryKind, RecoveryLocation, RecoverySettings};
use crate::vfs::limits::ProviderLimits;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OperationMapping {
    pub source: RecoveryLocation,
    pub target: RecoveryLocation,
    pub source_generation: String,
    pub source_version: String,
    pub target_version: Option<String>,
    pub target_exists: bool,
    pub files: u32,
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub existing_bytes: u64,
    pub native_move: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SpaceEstimate {
    /// Additional bytes needed; existing backups already occupy disk space.
    #[ts(type = "number")]
    pub additional_bytes: u64,
    #[ts(type = "number")]
    pub staging_bytes: u64,
    #[ts(type = "number")]
    pub backup_bytes: u64,
    #[ts(type = "number")]
    pub peak_footprint_bytes: u64,
    /// Verification reads source and output; no second full output is allocated.
    #[ts(type = "number")]
    pub verification_read_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PlanIssue {
    pub severity: String,
    pub code: String,
    pub item: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct OperationPlan {
    pub version: u32,
    /// Hash of the reviewed semantics; not execution authority.
    pub digest: String,
    pub kind: RecoveryKind,
    pub destination: RecoveryLocation,
    pub destination_generation: String,
    pub destination_identity: Option<String>,
    pub settings: RecoverySettings,
    pub mappings: Vec<OperationMapping>,
    pub limits: ProviderLimits,
    pub space: SpaceEstimate,
    pub issues: Vec<PlanIssue>,
}
