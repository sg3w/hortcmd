//! The wire protocol is identical for JS frames and native helper processes.
use super::manifest::PluginPermission;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginSide {
    Left,
    Right,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginCopyOperation {
    pub sources: Vec<String>,
    pub destination: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginContext {
    pub path: String,
    pub folder_path: String,
    pub file_path: Option<String>,
    pub selection: Vec<String>,
    pub side: PluginSide,
    pub copy_limit: u32,
    pub copy_operation: Option<PluginCopyOperation>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginViewOpen {
    pub view_id: String,
    pub instance_id: String,
    pub path: Option<String>,
    pub context: Option<PluginContext>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginViewPath {
    pub instance_id: String,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginEncoding {
    Utf8,
    Base64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginRequest {
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "context:get")]
    Context,
    #[serde(rename = "view:ready")]
    Ready,
    #[serde(rename = "view:title")]
    Title { title: String },
    #[serde(rename = "view:requestClose")]
    Close,
    #[serde(rename = "fs:read", rename_all = "camelCase")]
    Read {
        path: String,
        max_bytes: u32,
        encoding: PluginEncoding,
    },
    #[serde(rename = "fs:list")]
    List { path: String },
    #[serde(rename = "fs:preview")]
    Preview { path: String },
    #[serde(rename = "fs:write")]
    Write { path: String, content: String },
    #[serde(rename = "op:open")]
    Open { path: String },
    #[serde(rename = "op:copy")]
    Copy {
        sources: Vec<String>,
        destination: String,
    },
    #[serde(rename = "op:move")]
    Move {
        sources: Vec<String>,
        destination: String,
    },
    #[serde(rename = "op:limit")]
    Limit { value: Option<u32> },
    #[serde(rename = "notify")]
    Notify { message: String },
    #[serde(rename = "events:subscribe")]
    Subscribe { event: PluginSubscription },
    #[serde(rename = "events:unsubscribe")]
    Unsubscribe { event: PluginSubscription },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginSubscription {
    Selection,
    Transfer,
}
impl PluginRequest {
    pub fn permissions(&self) -> &'static [PluginPermission] {
        use PluginPermission as P;
        match self {
            Self::Ping | Self::Ready | Self::Title { .. } | Self::Close => &[],
            Self::Context => &[P::ReadContext],
            Self::Read { .. } | Self::List { .. } | Self::Preview { .. } | Self::Open { .. } => {
                &[P::ReadFs]
            }
            Self::Write { .. } => &[P::FsWrite],
            Self::Copy { .. } => &[P::Copy],
            Self::Move { .. } => &[P::Copy, P::FsWrite],
            Self::Limit { .. } => &[P::Limit],
            Self::Notify { .. } => &[P::Notify],
            Self::Subscribe { event } | Self::Unsubscribe { event } => match event {
                PluginSubscription::Selection => &[P::Selection, P::ReadContext],
                PluginSubscription::Transfer => &[P::Transfer],
            },
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginEvent {
    #[serde(rename = "view:open")]
    Open { data: PluginViewOpen },
    #[serde(rename = "view:path")]
    Path { data: PluginViewPath },
    #[serde(rename = "view:close", rename_all = "camelCase")]
    Close { instance_id: String },
    #[serde(rename = "context:changed")]
    Context { data: PluginContext },
    #[serde(rename = "command")]
    Command { command: String },
    #[serde(rename = "transfer", rename_all = "camelCase")]
    Transfer {
        operation_id: String,
        status: String,
        bytes: f64,
    },
}
