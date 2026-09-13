//! Optional provider constraints. Unknown is never interpreted as unlimited.
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ProviderLimits {
    #[ts(type = "number | null")]
    pub available_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub quota_remaining_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub maximum_object_bytes: Option<u64>,
    pub maximum_name_bytes: Option<u32>,
    pub maximum_path_bytes: Option<u32>,
    pub case_sensitive: Option<bool>,
    pub writable: Option<bool>,
    pub staging: bool,
    /// None means the provider cannot report a complete reserved-name policy.
    pub reserved_names: Option<Vec<String>>,
    pub forbidden_characters: Option<String>,
    /// None means timestamp/permission/extended-attribute fidelity is unknown.
    pub preserves_metadata: Option<bool>,
}
impl ProviderLimits {
    pub fn available(&self) -> Option<u64> {
        match (self.available_bytes, self.quota_remaining_bytes) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}
