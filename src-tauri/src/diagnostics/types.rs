use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum DiagnosticLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum DiagnosticEvent {
    Started,
    Finished,
    Failed,
    Cancelled,
    Retried,
    Resolved,
    Paused,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum DiagnosticComponent {
    Ui,
    Ipc,
    Backend,
    Provider,
    Connection,
    Transfer,
    Plugin,
}
/// Intentionally no message, detail, payload, headers or arbitrary attribute map.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DiagnosticRecord {
    pub at: f64,
    pub level: DiagnosticLevel,
    pub component: DiagnosticComponent,
    pub event: DiagnosticEvent,
    pub correlation: String,
    pub code: Option<String>,
    pub identifier: Option<String>,
}
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DiagnosticStatus {
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub build: String,
    pub level: DiagnosticLevel,
    pub location: String,
    pub available: bool,
    pub recent: Vec<DiagnosticRecord>,
    pub providers: Vec<String>,
    pub categories: Vec<String>,
    pub configuration: Vec<String>,
}
