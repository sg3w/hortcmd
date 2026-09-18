use super::manifest::{matches_view, PluginManifest, PluginViewKind};
use serde::Serialize;
use std::{io::Read, path::Path};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct PluginRecord {
    #[ts(optional)]
    pub fingerprint: Option<String>,
    pub active: bool,
    pub granted: Vec<super::manifest::PluginPermission>,
    pub id: String,
    pub manifest: Option<PluginManifest>,
    pub error: Option<String>,
}

pub fn read_manifest(root: &Path) -> Result<PluginManifest, String> {
    let path = super::manifest::package_file(root, "plugin.json")?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Manifest exceeds 64 KiB".into());
    }
    let manifest: PluginManifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid manifest: {e}"))?;
    manifest.validate(root)?;
    Ok(manifest)
}
pub fn discover(root: &Path) -> Result<Vec<PluginRecord>, String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut records = Vec::new();
    for entry in entries.take(1024) {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.starts_with('.') {
            continue;
        }
        let result = if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            read_manifest(&entry.path()).and_then(|m| {
                if m.id == id {
                    Ok(m)
                } else {
                    Err("Directory and manifest id differ".into())
                }
            })
        } else {
            Err("Plugin package must be a directory, not a link or file".into())
        };
        records.push(match result {
            Ok(manifest) => PluginRecord {
                fingerprint: None,
                active: false,
                granted: vec![],
                id,
                manifest: Some(manifest),
                error: None,
            },
            Err(error) => PluginRecord {
                fingerprint: None,
                active: false,
                granted: vec![],
                id,
                manifest: None,
                error: Some(error),
            },
        });
    }
    records.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(records)
}
/// Deterministic ordering: descending priority, then plugin/view id.
pub fn preview_match<'a>(
    manifests: &'a [PluginManifest],
    kind: &PluginViewKind,
    extension: Option<&str>,
    mime: Option<&str>,
) -> Option<(
    &'a PluginManifest,
    &'a super::manifest::PluginPreviewContribution,
)> {
    let mut candidates: Vec<_> = manifests
        .iter()
        .flat_map(|m| m.contributes.preview.iter().map(move |v| (m, v)))
        .filter(|(_, v)| matches_view(&v.r#match, kind, extension, mime))
        .collect();
    candidates.sort_by(|(a, av), (b, bv)| {
        bv.priority
            .cmp(&av.priority)
            .then(a.id.cmp(&b.id))
            .then(av.id.cmp(&bv.id))
    });
    candidates.into_iter().next()
}
#[cfg(test)]
mod tests;
