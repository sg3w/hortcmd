use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use ts_rs::TS;

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginPermission {
    #[serde(rename = "read:context")]
    ReadContext,
    #[serde(rename = "read:fs")]
    ReadFs,
    #[serde(rename = "fs:write")]
    FsWrite,
    #[serde(rename = "op:copy")]
    Copy,
    #[serde(rename = "op:limit")]
    Limit,
    #[serde(rename = "events:selection")]
    Selection,
    #[serde(rename = "events:transfer")]
    Transfer,
    #[serde(rename = "ui:window")]
    Window,
    #[serde(rename = "ui:commandbar")]
    Commandbar,
    #[serde(rename = "ui:preview")]
    Preview,
    #[serde(rename = "ui:tab")]
    Tab,
    #[serde(rename = "notify")]
    Notify,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginRuntime {
    Js,
    Native,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginEntry {
    Js(String),
    Native {
        macos: Option<String>,
        windows: Option<String>,
        linux: Option<String>,
    },
}
impl PluginEntry {
    pub fn current(&self) -> Option<&str> {
        match self {
            Self::Js(entry) => Some(entry),
            Self::Native {
                macos,
                windows,
                linux,
            } => {
                if cfg!(target_os = "macos") {
                    macos.as_deref()
                } else if cfg!(windows) {
                    windows.as_deref()
                } else {
                    linux.as_deref()
                }
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginAction {
    OpenWindow,
    Command { command: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginButton {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub action: PluginAction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginViewKind {
    File,
    Folder,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginViewMatch {
    pub kinds: Vec<PluginViewKind>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mime_types: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginPreviewContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub entry: Option<String>,
    pub r#match: PluginViewMatch,
    #[serde(default)]
    pub priority: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginTabSurface {
    Pane,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum PluginTabPath {
    Active,
    None,
}
fn yes() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginTabContribution {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub entry: Option<String>,
    pub surface: PluginTabSurface,
    pub path: PluginTabPath,
    #[serde(default = "yes")]
    pub closable: bool,
    #[serde(default)]
    pub singleton: bool,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginContribution {
    #[serde(default)]
    pub commandbar: Vec<PluginButton>,
    #[serde(default)]
    pub preview: Vec<PluginPreviewContribution>,
    #[serde(default)]
    pub tabs: Vec<PluginTabContribution>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub api_version: u32,
    pub runtime: PluginRuntime,
    pub entry: PluginEntry,
    pub icon: String,
    pub permissions: Vec<PluginPermission>,
    #[serde(default)]
    pub contributes: PluginContribution,
}

/// No absolute paths, parent components or links, even links inside the package.
pub fn package_file(root: &Path, entry: &str) -> Result<PathBuf, String> {
    if entry.is_empty()
        || entry.contains(['\\', ':', '\0'])
        || Path::new(entry)
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("Entry must be a relative package path".into());
    }
    let mut path = root.to_path_buf();
    for component in Path::new(entry).components() {
        path.push(component);
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| format!("Missing entry: {entry}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Package links are forbidden".into());
        }
    }
    if !path.is_file() {
        return Err("Entry is not a file".into());
    }
    Ok(path)
}
pub fn valid_id(id: &str) -> bool {
    id.len() <= 160
        && id.split('.').count() >= 2
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
}
fn icon_valid(icon: &str) -> bool {
    !icon.is_empty()
        && icon.len() <= 32768
        && (!icon.starts_with("data:")
            && !icon.contains(['<', '>', ':', '/', '\\'])
            && icon.len() <= 64
            || [
                "data:image/png;base64,",
                "data:image/svg+xml;base64,",
                "data:image/webp;base64,",
            ]
            .iter()
            .any(|p| icon.starts_with(p)))
}
impl PluginManifest {
    pub fn validate(&self, root: &Path) -> Result<(), String> {
        use PluginPermission as P;
        if !valid_id(&self.id) {
            return Err("Invalid reverse-DNS plugin id".into());
        }
        if self.api_version != API_VERSION {
            return Err("Incompatible apiVersion".into());
        }
        if [&self.name, &self.description, &self.author]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 4096)
        {
            return Err("Missing or oversized metadata".into());
        }
        if self.version.split('.').count() != 3
            || !self
                .version
                .split('.')
                .all(|p| !p.is_empty() && p.parse::<u32>().is_ok())
        {
            return Err("Version must be major.minor.patch".into());
        }
        if !icon_valid(&self.icon) {
            return Err("Invalid icon (use emoji or base64 image data)".into());
        }
        if self.permissions.iter().collect::<BTreeSet<_>>().len() != self.permissions.len() {
            return Err("Duplicate permission".into());
        }
        match (&self.runtime, &self.entry) {
            (PluginRuntime::Js, PluginEntry::Js(_))
            | (PluginRuntime::Native, PluginEntry::Native { .. }) => (),
            _ => return Err("Entry does not match runtime".into()),
        }
        package_file(root, self.entry.current().ok_or("Unsupported platform")?)?;
        if self.runtime == PluginRuntime::Native
            && (!self.contributes.preview.is_empty() || !self.contributes.tabs.is_empty())
        {
            return Err("Native plugins cannot contribute HTML views".into());
        }
        let mut ids = BTreeSet::new();
        let mut validate = |id: &str,
                            title: &str,
                            icon: &str,
                            entry: Option<&str>,
                            permission: P|
         -> Result<(), String> {
            if id.is_empty() || id.len() > 128 || !ids.insert(id.to_string()) {
                return Err("Empty or duplicate contribution id".into());
            }
            if title.trim().is_empty() || title.len() > 256 || !icon_valid(icon) {
                return Err("Invalid contribution title or icon".into());
            }
            if !self.permissions.contains(&permission) {
                return Err(format!("Missing permission: {permission:?}"));
            }
            if let Some(entry) = entry {
                package_file(root, entry)?;
            }
            Ok(())
        };
        for b in &self.contributes.commandbar {
            validate(&b.id, &b.title, &b.icon, None, P::Commandbar)?;
            if matches!(b.action, PluginAction::OpenWindow)
                && (self.runtime != PluginRuntime::Js || !self.permissions.contains(&P::Window))
            {
                return Err("openWindow requires JS and ui:window".into());
            }
            if let PluginAction::Command { command } = &b.action {
                if command.is_empty() || command.len() > 128 {
                    return Err("Invalid command id".into());
                }
            }
        }
        for v in &self.contributes.preview {
            validate(&v.id, &v.title, &v.icon, v.entry.as_deref(), P::Preview)?;
            if v.r#match.kinds.is_empty()
                || v.r#match
                    .extensions
                    .iter()
                    .any(|e| e.is_empty() || e.contains(['/', '\\', '.']))
            {
                return Err("Invalid view match".into());
            }
        }
        for v in &self.contributes.tabs {
            validate(&v.id, &v.title, &v.icon, v.entry.as_deref(), P::Tab)?;
        }
        Ok(())
    }
}

pub fn matches_view(
    rule: &PluginViewMatch,
    kind: &PluginViewKind,
    extension: Option<&str>,
    mime: Option<&str>,
) -> bool {
    rule.kinds.contains(kind)
        && (rule.extensions.is_empty()
            || extension
                .is_some_and(|ext| rule.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))))
        && (rule.mime_types.is_empty()
            || mime
                .is_some_and(|mime| rule.mime_types.iter().any(|m| m.eq_ignore_ascii_case(mime))))
}
