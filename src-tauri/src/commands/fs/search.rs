// ============================================================
// Suche über einen Verzeichnisbaum. Vier Modi:
//   files        – Name- und/oder Inhaltssuche (Glob oder Regex)
//   duplicates   – gleiche Dateien finden (Größe, dann SHA-256)
//   empty_dirs   – leere Ordner
//   large_files  – Dateien ab einer Mindestgröße (nach Größe sortiert)
//
// Läuft asynchron über `spawn_blocking` und streamt Treffer in Chargen
// über einen Tauri-`Channel`. Ignorierte Ordner werden übersprungen; ein
// hartes Trefferlimit schützt Speicher/Frontend.
// ============================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use tauri::ipc::Channel;
use ts_rs::TS;

/// Ein Suchtreffer (Datei oder Ordner).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct SearchHit {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    #[ts(type = "number")]
    pub size: u64,
    #[ts(type = "number")]
    pub modified: u64,
    /// Zusatzinfo: Trefferzeile (Inhalt), Größe oder Gruppenhinweis.
    pub detail: String,
    /// Duplikatgruppe (>0 im Modus „duplicates"; sonst 0).
    pub group: u32,
}

/// Suchoptionen vom Frontend.
#[derive(Deserialize)]
pub struct SearchOptions {
    /// "files" | "duplicates" | "empty_dirs" | "large_files"
    pub mode: String,
    /// Namensmuster (Glob oder Regex); leer = alle Namen.
    pub name: String,
    pub name_regex: bool,
    /// Inhaltssuche (nur Modus „files"); leer = keine Inhaltssuche.
    pub content: String,
    pub content_regex: bool,
    pub case_sensitive: bool,
    /// Ordnernamen, die (auf jeder Ebene) übersprungen werden.
    pub ignore_dirs: Vec<String>,
    /// Mindestgröße in Bytes (nur Modus „large_files").
    #[serde(default)]
    pub min_size: u64,
}

const MAX_HITS: usize = 50_000;
const MAX_CONTENT_BYTES: u64 = 8 * 1024 * 1024; // Inhaltssuche nur bis 8 MB/Datei

type Meta = (u64, u64); // (Größe, mtime)

fn meta_of(m: &fs::Metadata) -> Meta {
    let mtime = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (m.len(), mtime)
}

/// Wandelt ein Glob-Muster (`*`, `?`, mehrere durch Leerzeichen/`;`) in Regex.
fn glob_to_regex(pattern: &str, case_sensitive: bool) -> Result<regex::Regex, String> {
    let parts: Vec<String> = pattern
        .split([';', ' ', '\t'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|g| {
            let mut out = String::new();
            for ch in g.chars() {
                match ch {
                    '*' => out.push_str(".*"),
                    '?' => out.push('.'),
                    c if ".+^${}()|[]\\".contains(c) => {
                        out.push('\\');
                        out.push(c);
                    }
                    c => out.push(c),
                }
            }
            out
        })
        .collect();
    let body = if parts.is_empty() {
        ".*".to_string()
    } else {
        parts.join("|")
    };
    build_regex(&format!("^(?:{})$", body), case_sensitive)
}

fn build_regex(pattern: &str, case_sensitive: bool) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Ungültiger Ausdruck: {}", e))
}

/// Ein Name-Matcher (Regex oder Glob) bzw. „alles" bei leerem Muster.
enum NameMatch {
    Any,
    Re(regex::Regex),
}

impl NameMatch {
    fn build(opts: &SearchOptions) -> Result<Self, String> {
        if opts.name.trim().is_empty() {
            return Ok(NameMatch::Any);
        }
        let re = if opts.name_regex {
            build_regex(&opts.name, opts.case_sensitive)?
        } else {
            glob_to_regex(&opts.name, opts.case_sensitive)?
        };
        Ok(NameMatch::Re(re))
    }
    fn matches(&self, name: &str) -> bool {
        match self {
            NameMatch::Any => true,
            NameMatch::Re(re) => re.is_match(name),
        }
    }
}

/// Sammelt rekursiv alle Dateien und Ordner (ignorierte Ordner übersprungen,
/// Symlinks nicht verfolgt). Reihenfolge ist die des Dateisystems.
fn walk(root: &Path, ignore: &[String], files: &mut Vec<(PathBuf, Meta)>, dirs: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let Ok(m) = entry.metadata() else { continue };
        if m.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        if m.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if ignore.iter().any(|ig| ig == &name) {
                continue;
            }
            dirs.push(path.clone());
            walk(&path, ignore, files, dirs);
        } else {
            files.push((path, meta_of(&m)));
        }
    }
}

fn hit(path: &Path, size: u64, modified: u64, detail: String, group: u32) -> SearchHit {
    SearchHit {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        is_dir: false,
        size,
        modified,
        detail,
        group,
    }
}

/// Sucht in einer Datei nach der ersten passenden Zeile (Inhaltssuche).
/// Gibt None zurück, wenn die Datei binär/zu groß ist oder nichts passt.
fn content_match(path: &Path, size: u64, needle: &Needle) -> Option<String> {
    if size > MAX_CONTENT_BYTES {
        return None;
    }
    let mut buf = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut buf).ok()?;
    if buf.contains(&0) {
        return None; // binär
    }
    let text = std::str::from_utf8(&buf).ok()?;
    for (i, line) in text.lines().enumerate() {
        if needle.is_match(line) {
            let trimmed: String = line.trim().chars().take(200).collect();
            return Some(format!("{}: {}", i + 1, trimmed));
        }
    }
    None
}

/// Inhalts-Matcher: Regex oder (case-sensitiv/-insensitiv) Teilstring.
enum Needle {
    Re(regex::Regex),
    Plain { text: String, case_sensitive: bool },
}

impl Needle {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Needle::Re(re) => re.is_match(line),
            Needle::Plain { text, case_sensitive } => {
                if *case_sensitive {
                    line.contains(text)
                } else {
                    line.to_lowercase().contains(&text.to_lowercase())
                }
            }
        }
    }
}

/// SHA-256 einer Datei (für die Duplikaterkennung).
fn hash_file(path: &Path) -> Option<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().to_vec())
}

fn run_search(
    root: PathBuf,
    opts: SearchOptions,
    channel: &Channel<Vec<SearchHit>>,
) -> Result<bool, String> {
    let name_match = NameMatch::build(&opts)?;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    walk(&root, &opts.ignore_dirs, &mut files, &mut dirs);

    match opts.mode.as_str() {
        "empty_dirs" => {
            let mut batch = Vec::new();
            for d in &dirs {
                let empty = fs::read_dir(d).map(|mut r| r.next().is_none()).unwrap_or(false);
                if empty {
                    let (size, modified) = fs::metadata(d).map(|m| meta_of(&m)).unwrap_or((0, 0));
                    let mut h = hit(d, size, modified, String::new(), 0);
                    h.is_dir = true;
                    batch.push(h);
                }
            }
            let truncated = batch.len() > MAX_HITS;
            batch.truncate(MAX_HITS);
            if !batch.is_empty() {
                let _ = channel.send(batch);
            }
            Ok(truncated)
        }

        "large_files" => {
            let mut hits: Vec<(u64, u64, PathBuf)> = files
                .iter()
                .filter(|(_, (sz, _))| *sz >= opts.min_size)
                .map(|(p, (sz, mt))| (*sz, *mt, p.clone()))
                .collect();
            hits.sort_by(|a, b| b.0.cmp(&a.0)); // größte zuerst
            let truncated = hits.len() > MAX_HITS;
            hits.truncate(MAX_HITS);
            let batch: Vec<SearchHit> = hits
                .into_iter()
                .map(|(sz, mt, p)| hit(&p, sz, mt, String::new(), 0))
                .collect();
            if !batch.is_empty() {
                let _ = channel.send(batch);
            }
            Ok(truncated)
        }

        "duplicates" => Ok(find_duplicates(files, channel)),

        // Standard: Name- und/oder Inhaltssuche.
        _ => {
            let needle = if opts.content.trim().is_empty() {
                None
            } else if opts.content_regex {
                Some(Needle::Re(build_regex(&opts.content, opts.case_sensitive)?))
            } else {
                Some(Needle::Plain {
                    text: opts.content.clone(),
                    case_sensitive: opts.case_sensitive,
                })
            };

            let mut sent = 0usize;
            let mut batch = Vec::new();
            for (path, (size, modified)) in &files {
                if sent + batch.len() >= MAX_HITS {
                    if !batch.is_empty() {
                        let _ = channel.send(std::mem::take(&mut batch));
                    }
                    return Ok(true);
                }
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                if !name.map(|n| name_match.matches(&n)).unwrap_or(false) {
                    continue;
                }
                let detail = match &needle {
                    Some(nd) => match content_match(path, *size, nd) {
                        Some(line) => line,
                        None => continue, // Inhaltssuche ohne Treffer
                    },
                    None => String::new(),
                };
                batch.push(hit(path, *size, *modified, detail, 0));
                if batch.len() >= 200 {
                    sent += batch.len();
                    let _ = channel.send(std::mem::take(&mut batch));
                }
            }
            if !batch.is_empty() {
                let _ = channel.send(batch);
            }
            Ok(false)
        }
    }
}

/// Findet Duplikate: nach Größe vorgruppieren, dann innerhalb gleicher Größe
/// per SHA-256. Nur Gruppen mit ≥2 identischen Dateien werden gemeldet.
fn find_duplicates(files: Vec<(PathBuf, Meta)>, channel: &Channel<Vec<SearchHit>>) -> bool {
    let mut by_size: HashMap<u64, Vec<(PathBuf, u64)>> = HashMap::new();
    for (p, (sz, mt)) in files {
        if sz > 0 {
            by_size.entry(sz).or_default().push((p, mt));
        }
    }

    let mut group_id = 0u32;
    let mut total = 0usize;
    let mut truncated = false;
    for (size, group) in by_size {
        if group.len() < 2 {
            continue;
        }
        // Innerhalb gleicher Größe per Hash weiter gruppieren.
        let mut by_hash: HashMap<Vec<u8>, Vec<(PathBuf, u64)>> = HashMap::new();
        for (p, mt) in group {
            if let Some(h) = hash_file(&p) {
                by_hash.entry(h).or_default().push((p, mt));
            }
        }
        for (_, dups) in by_hash {
            if dups.len() < 2 {
                continue;
            }
            group_id += 1;
            let detail = format!("{} × {}", dups.len(), human_size(size));
            let batch: Vec<SearchHit> = dups
                .iter()
                .map(|(p, mt)| hit(p, size, *mt, detail.clone(), group_id))
                .collect();
            total += batch.len();
            let _ = channel.send(batch);
            if total >= MAX_HITS {
                truncated = true;
                return truncated;
            }
        }
    }
    truncated
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

/// Startet die Suche und streamt die Treffer über `on_batch`.
/// Rückgabe `true` = Ergebnis wurde beim Trefferlimit abgeschnitten.
#[tauri::command]
pub async fn search(
    root: String,
    options: SearchOptions,
    on_batch: Channel<Vec<SearchHit>>,
) -> Result<bool, String> {
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(format!("{}: kein Verzeichnis", root));
    }
    tauri::async_runtime::spawn_blocking(move || run_search(root_path, options, &on_batch))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rc-search-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn opts(mode: &str) -> SearchOptions {
        SearchOptions {
            mode: mode.into(),
            name: String::new(),
            name_regex: false,
            content: String::new(),
            content_regex: false,
            case_sensitive: false,
            ignore_dirs: vec![],
            min_size: 0,
        }
    }

    #[test]
    fn glob_matches_extension() {
        let re = glob_to_regex("*.txt *.md", false).unwrap();
        assert!(re.is_match("a.TXT"));
        assert!(re.is_match("readme.md"));
        assert!(!re.is_match("a.rs"));
    }

    #[test]
    fn name_and_content_needle() {
        let plain = Needle::Plain { text: "Fehler".into(), case_sensitive: false };
        assert!(plain.is_match("ein FEHLER hier"));
        assert!(!plain.is_match("alles gut"));
        let re = Needle::Re(build_regex(r"TODO:\s*\w+", true).unwrap());
        assert!(re.is_match("  TODO: fixme"));
    }

    #[test]
    fn empty_dir_and_large_file_walk() {
        let d = tmp();
        fs::create_dir_all(d.join("leer")).unwrap();
        fs::create_dir_all(d.join("voll")).unwrap();
        fs::write(d.join("voll/big.bin"), vec![0u8; 2048]).unwrap();
        fs::write(d.join("small.txt"), b"hi").unwrap();

        let mut files = Vec::new();
        let mut dirs = Vec::new();
        walk(&d, &[], &mut files, &mut dirs);
        assert_eq!(files.len(), 2);
        assert_eq!(dirs.len(), 2);

        // „leer" ist leer, „voll" nicht.
        let empty: Vec<_> = dirs
            .iter()
            .filter(|p| fs::read_dir(p).unwrap().next().is_none())
            .collect();
        assert_eq!(empty.len(), 1);
        assert!(empty[0].ends_with("leer"));

        // Große Datei ≥ 1 KB: nur big.bin.
        let large: Vec<_> = files.iter().filter(|(_, (sz, _))| *sz >= 1024).collect();
        assert_eq!(large.len(), 1);

        let _ = opts("files"); // Options-Konstruktion prüfen
    }

    #[test]
    fn ignore_dirs_are_skipped() {
        let d = tmp();
        fs::create_dir_all(d.join("node_modules")).unwrap();
        fs::write(d.join("node_modules/x.js"), b"x").unwrap();
        fs::write(d.join("keep.js"), b"y").unwrap();

        let mut files = Vec::new();
        let mut dirs = Vec::new();
        walk(&d, &["node_modules".to_string()], &mut files, &mut dirs);
        assert_eq!(files.len(), 1);
        assert!(files[0].0.ends_with("keep.js"));
        assert!(dirs.is_empty());
    }
}
