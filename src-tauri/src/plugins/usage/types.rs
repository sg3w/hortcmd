use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum UsageMetric {
    #[default]
    Logical,
    Allocated,
    Deduplicated,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UsageOptions {
    pub root: String,
    #[serde(default)]
    pub size_mode: UsageMetric,
    pub max_depth: u16,
    pub item_limit: u32,
    pub node_limit: u32,
    pub seconds: u32,
    pub include_hidden: bool,
    pub stay_on_volume: bool,
}
impl UsageOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_depth > 256
            || !(1..=1_000_000).contains(&self.item_limit)
            || !(16..=5000).contains(&self.node_limit)
            || !(1..=300).contains(&self.seconds)
            || self.root.len() > 4096
        {
            return Err("Invalid scan limits (depth ≤256, entries ≤1000000, nodes 16–5000, duration 1–300 seconds)".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UsageNode {
    pub id: u32,
    pub parent: Option<u32>,
    pub location: String,
    pub name: String,
    pub kind: String,
    pub depth: u16,
    #[ts(type = "number")]
    pub direct_bytes: u64,
    #[ts(type = "number")]
    pub recursive_bytes: u64,
    #[ts(type = "number | null")]
    pub allocated_bytes: Option<u64>,
    #[ts(type = "number | null")]
    pub deduplicated_bytes: Option<u64>,
    #[ts(type = "number")]
    pub files: u64,
    #[ts(type = "number")]
    pub folders: u64,
    #[ts(type = "number")]
    pub unreadable: u64,
    #[ts(type = "number")]
    pub omitted_entries: u64,
    #[ts(type = "number")]
    pub omitted_bytes: u64,
    pub incomplete: bool,
    pub incomplete_below_depth: bool,
}
#[derive(Clone, Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UsageIssues {
    pub unreadable: u32,
    pub disappeared: u32,
    pub cyclic: u32,
    pub unsupported: u32,
    pub changed: u32,
    pub provider_errors: u32,
    pub overflow: u32,
}
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct UsageBatch {
    pub nodes: Vec<UsageNode>,
    pub pending: bool,
    pub status: String,
    pub elapsed_ms: u32,
    pub scanned: u32,
    pub current_path: String,
    pub issues: UsageIssues,
    pub stale: bool,
    pub options: UsageOptions,
    pub retained_nodes: u32,
    pub estimated_memory_bytes: u32,
    pub memory_budget_bytes: u32,
}
