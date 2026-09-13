use crate::commands::fs::transfer::recovery::types::RecoveryLocation;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use ts_rs::TS;

#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct IndexPolicy {
    pub content: bool,
    pub hidden: bool,
    pub ignore: Vec<String>,
    pub allow_battery: bool,
    pub offline: bool,
    pub max_content_bytes: u32,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Row {
    pub name: String,
    pub directory: bool,
    pub size: u64,
    pub modified: u64,
    pub identity: String,
    pub content: Option<String>,
    pub seen: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub id: String,
    pub location: RecoveryLocation,
    pub policy: IndexPolicy,
    pub generation: String,
    pub rows: BTreeMap<String, Row>,
    pub queue: VecDeque<String>,
    pub cycle: u64,
    pub revision: u64,
    pub completed: u64,
    pub paused: bool,
    pub limited: bool,
    pub unreadable: u32,
}
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct IndexInfo {
    pub id: String,
    pub root: Option<String>,
    pub policy: Option<IndexPolicy>,
    pub state: String,
    pub entries: u32,
    pub bytes: u32,
    pub pending: u32,
    pub unreadable: u32,
    pub persistent: bool,
}
#[derive(Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct IndexQuery {
    pub name: String,
    pub path: String,
    pub content: String,
    pub case_sensitive: bool,
    pub kind: String,
    #[ts(type = "number | null")]
    pub min_size: Option<u64>,
    #[ts(type = "number | null")]
    pub max_size: Option<u64>,
    #[ts(type = "number | null")]
    pub modified_after: Option<u64>,
    pub after: Option<String>,
    #[ts(type = "number | null")]
    pub revision: Option<u64>,
    pub limit: u32,
    pub adapter: String,
}
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct IndexHit {
    pub id: String,
    pub relative: String,
    pub address: Option<String>,
    pub directory: bool,
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number")]
    pub modified: u64,
    pub provenance: String,
}
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct IndexPage {
    pub hits: Vec<IndexHit>,
    pub next: Option<String>,
    #[ts(type = "number")]
    pub revision: u64,
    pub freshness: String,
    pub source: String,
    pub fallback: bool,
}
