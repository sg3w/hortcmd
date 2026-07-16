// ============================================================
// Verzeichnisvergleich & -synchronisation.
//   compare_dirs – zwei Bäume rekursiv vergleichen (nach Größe + Zeit)
//   sync_copy    – ausgewählte Dateien von einer Seite zur anderen kopieren
// Der Vergleich liefert je Datei einen Status; das Kopieren erledigt
// das Backend (Zielordner werden bei Bedarf angelegt).
// ============================================================

use super::file::OpResult;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::ipc::Channel;
use ts_rs::TS;

/// Ein Vergleichsergebnis für eine Datei (relativ zu den beiden Wurzeln).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DiffEntry {
    /// Relativer Pfad (mit "/" getrennt), identisch auf beiden Seiten.
    pub rel: String,
    /// Basisname der Datei.
    pub name: String,
    /// Existiert links / rechts.
    pub left: bool,
    pub right: bool,
    #[ts(type = "number")]
    pub left_size: u64,
    #[ts(type = "number")]
    pub right_size: u64,
    #[ts(type = "number")]
    pub left_modified: u64,
    #[ts(type = "number")]
    pub right_modified: u64,
    /// "same" | "left_only" | "right_only" | "newer_left" | "newer_right" | "different"
    pub status: String,
}

/// Obergrenze der Vergleichseinträge. Verhindert, dass sehr große Bäume
/// den Speicher / das Frontend überlasten; darüber wird abgeschnitten und
/// das Frontend zeigt einen Warnhinweis.
const MAX_ENTRIES: usize = 200_000;

type Meta = (u64, u64); // (Größe, mtime in Unix-Sekunden)

/// Liest eine Verzeichnisebene: Dateien (Name → Meta) und Unterordner-Namen.
/// Symlinks werden übersprungen; sortierte Sammlungen für stabile Reihenfolge.
fn read_level(dir: &Path) -> (BTreeMap<String, Meta>, BTreeSet<String>) {
    let mut files = BTreeMap::new();
    let mut subdirs = BTreeSet::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return (files, subdirs);
    };
    for entry in rd.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if meta.is_dir() {
            subdirs.insert(name);
        } else {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            files.insert(name, (meta.len(), mtime));
        }
    }
    (files, subdirs)
}

fn status_of(l: Option<&Meta>, r: Option<&Meta>) -> &'static str {
    match (l, r) {
        (Some(_), None) => "left_only",
        (None, Some(_)) => "right_only",
        (Some((ls, lm)), Some((rs, rm))) => {
            if ls == rs && lm == rm {
                "same"
            } else if lm > rm {
                "newer_left"
            } else if rm > lm {
                "newer_right"
            } else {
                "different"
            }
        }
        (None, None) => "same",
    }
}

/// Vergleicht Ebene für Ebene und schickt jede Verzeichnis-Charge sofort
/// über den Channel ans Frontend (Streaming), bevor in Unterordner
/// abgestiegen wird. `count` zählt die bereits gesendeten Einträge; bei
/// Erreichen von `MAX_ENTRIES` wird abgebrochen. Rückgabe `true` = Limit
/// erreicht (Ergebnis gekürzt).
fn walk_compare(
    lroot: &Path,
    rroot: &Path,
    rel: &str,
    recursive: bool,
    channel: &Channel<Vec<DiffEntry>>,
    count: &mut usize,
) -> bool {
    let ljoin = if rel.is_empty() {
        lroot.to_path_buf()
    } else {
        lroot.join(rel)
    };
    let rjoin = if rel.is_empty() {
        rroot.to_path_buf()
    } else {
        rroot.join(rel)
    };
    let (lfiles, lsubs) = read_level(&ljoin);
    let (rfiles, rsubs) = read_level(&rjoin);

    // Dateien dieser Ebene vergleichen (Namen beider Seiten, sortiert).
    let mut names: BTreeSet<&String> = BTreeSet::new();
    names.extend(lfiles.keys());
    names.extend(rfiles.keys());

    let mut batch = Vec::new();
    let mut limit_hit = false;
    for name in names {
        if *count >= MAX_ENTRIES {
            limit_hit = true;
            break;
        }
        let l = lfiles.get(name);
        let r = rfiles.get(name);
        let (left_size, left_modified) = l.copied().unwrap_or((0, 0));
        let (right_size, right_modified) = r.copied().unwrap_or((0, 0));
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", rel, name)
        };
        batch.push(DiffEntry {
            rel: child_rel,
            name: name.clone(),
            left: l.is_some(),
            right: r.is_some(),
            left_size,
            right_size,
            left_modified,
            right_modified,
            status: status_of(l, r).to_string(),
        });
        *count += 1;
    }
    if !batch.is_empty() {
        let _ = channel.send(batch);
    }
    if limit_hit || *count >= MAX_ENTRIES {
        return true;
    }

    if recursive {
        let mut subs: BTreeSet<&String> = BTreeSet::new();
        subs.extend(lsubs.iter());
        subs.extend(rsubs.iter());
        for sub in subs {
            let child_rel = if rel.is_empty() {
                sub.clone()
            } else {
                format!("{}/{}", rel, sub)
            };
            if walk_compare(lroot, rroot, &child_rel, recursive, channel, count) {
                return true;
            }
        }
    }
    false
}

/// Vergleicht zwei Verzeichnisbäume und streamt die Ergebnisse pro
/// Verzeichnis über `on_batch`. Läuft auf einem Hintergrund-Thread
/// (`spawn_blocking`), damit große Bäume die UI nicht blockieren.
/// Rückgabe `true` = Ergebnis wurde bei `MAX_ENTRIES` abgeschnitten.
#[tauri::command]
pub async fn compare_dirs(
    left: String,
    right: String,
    recursive: bool,
    on_batch: Channel<Vec<DiffEntry>>,
) -> Result<bool, String> {
    let lp = PathBuf::from(&left);
    let rp = PathBuf::from(&right);
    if !lp.is_dir() {
        return Err(format!("{}: kein Verzeichnis", left));
    }
    if !rp.is_dir() {
        return Err(format!("{}: kein Verzeichnis", right));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let mut count = 0usize;
        walk_compare(&lp, &rp, "", recursive, &on_batch, &mut count)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Kopiert Dateien von Quelle → Ziel (blockierend).
fn sync_copy_impl(items: Vec<(String, String)>) -> OpResult {
    let mut ok = 0u32;
    let mut errors = Vec::new();
    for (src, dst) in &items {
        let dpath = Path::new(dst);
        if let Some(parent) = dpath.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                errors.push(format!("{}: {}", dst, e));
                continue;
            }
        }
        match fs::copy(src, dst) {
            Ok(_) => ok += 1,
            Err(e) => errors.push(format!("{} → {}: {}", src, dst, e)),
        }
    }
    OpResult { ok, errors }
}

/// Kopiert Dateien von Quelle → Ziel (Paare absoluter Pfade). Fehlende
/// Zielordner werden angelegt; vorhandene Zieldateien werden überschrieben.
/// Läuft auf einem Hintergrund-Thread, um die UI nicht zu blockieren.
#[tauri::command]
pub async fn sync_copy(items: Vec<(String, String)>) -> OpResult {
    match tauri::async_runtime::spawn_blocking(move || sync_copy_impl(items)).await {
        Ok(res) => res,
        Err(e) => OpResult {
            ok: 0,
            errors: vec![e.to_string()],
        },
    }
}
