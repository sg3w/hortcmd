// ============================================================
// Finder-Tags (macOS) anzeigen und bearbeiten.
//
// Finder speichert Tags im Extended Attribute
// `com.apple.metadata:_kMDItemUserTags` als **Binär-plist**: ein Array
// von Strings, jeweils "Name" oder "Name\nFarbindex" (Farbe 0–7, 0 = keine).
// Auf Nicht-macOS-Plattformen liefern die Commands leere Werte bzw. einen
// klaren Fehler.
// ============================================================

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Ein Finder-Tag: Name plus Farbindex (0 = keine, 1–7 = Finder-Farben).
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Tag {
    pub name: String,
    #[ts(type = "number")]
    pub color: u8,
}

#[cfg(target_os = "macos")]
const TAGS_ATTR: &str = "com.apple.metadata:_kMDItemUserTags";

/// Liest die Finder-Tags eines Eintrags.
#[tauri::command]
pub fn get_tags(path: String) -> Result<Vec<Tag>, String> {
    #[cfg(target_os = "macos")]
    {
        macos::read_tags(&path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Ok(Vec::new())
    }
}

/// Schreibt die Finder-Tags eines Eintrags (leere Liste entfernt das Attribut).
#[tauri::command]
pub fn set_tags(path: String, tags: Vec<Tag>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::write_tags(&path, &tags)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, tags);
        Err("Finder-Tags gibt es nur unter macOS".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Tag, TAGS_ATTR};
    use plist::Value;
    use std::io::Cursor;

    /// Zerlegt einen Tag-String "Name" bzw. "Name\nFarbe" in Name + Farbindex.
    fn parse_tag(s: &str) -> Tag {
        let mut parts = s.splitn(2, '\n');
        let name = parts.next().unwrap_or("").to_string();
        let color = parts
            .next()
            .and_then(|c| c.trim().parse::<u8>().ok())
            .filter(|&c| c <= 7)
            .unwrap_or(0);
        Tag { name, color }
    }

    /// Setzt einen Tag wieder in die Finder-Kodierung zusammen.
    fn encode_tag(t: &Tag) -> String {
        if t.color == 0 {
            t.name.clone()
        } else {
            format!("{}\n{}", t.name, t.color)
        }
    }

    pub fn read_tags(path: &str) -> Result<Vec<Tag>, String> {
        let raw = match xattr::get(path, TAGS_ATTR).map_err(|e| e.to_string())? {
            Some(bytes) if !bytes.is_empty() => bytes,
            _ => return Ok(Vec::new()),
        };
        let value = Value::from_reader(Cursor::new(raw)).map_err(|e| e.to_string())?;
        let arr = value
            .as_array()
            .ok_or("Tag-Attribut ist kein Array")?;
        Ok(arr
            .iter()
            .filter_map(|v| v.as_string())
            .map(parse_tag)
            .filter(|t| !t.name.is_empty())
            .collect())
    }

    pub fn write_tags(path: &str, tags: &[Tag]) -> Result<(), String> {
        if tags.is_empty() {
            // Attribut entfernen; „nicht vorhanden" ist kein Fehler.
            return match xattr::remove(path, TAGS_ATTR) {
                Ok(_) => Ok(()),
                Err(e) if e.raw_os_error() == Some(libc_enoattr()) => Ok(()),
                Err(e) => Err(e.to_string()),
            };
        }
        let arr: Vec<Value> = tags
            .iter()
            .map(|t| Value::String(encode_tag(t)))
            .collect();
        let mut buf = Vec::new();
        Value::Array(arr)
            .to_writer_binary(&mut buf)
            .map_err(|e| e.to_string())?;
        xattr::set(path, TAGS_ATTR, &buf).map_err(|e| e.to_string())
    }

    /// ENOATTR ist auf macOS 93 (kein direkter std-Konstantenzugriff ohne libc).
    fn libc_enoattr() -> i32 {
        93
    }

    #[cfg(test)]
    mod tests {
        use super::super::{get_tags, set_tags, Tag};
        use std::fs;

        fn tmp(name: &str) -> String {
            let dir = std::env::temp_dir().join(format!("rc-tags-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let p = dir.join(name);
            fs::write(&p, b"x").unwrap();
            p.to_string_lossy().into_owned()
        }

        #[test]
        fn roundtrip_tags() {
            let path = tmp("a.txt");
            // Anfangs keine Tags.
            assert!(get_tags(path.clone()).unwrap().is_empty());

            let tags = vec![
                Tag { name: "Wichtig".into(), color: 6 },
                Tag { name: "Projekt".into(), color: 0 },
            ];
            set_tags(path.clone(), tags).unwrap();

            let back = get_tags(path.clone()).unwrap();
            assert_eq!(back.len(), 2);
            assert_eq!(back[0].name, "Wichtig");
            assert_eq!(back[0].color, 6);
            assert_eq!(back[1].name, "Projekt");
            assert_eq!(back[1].color, 0);

            // Leere Liste entfernt das Attribut wieder.
            set_tags(path.clone(), Vec::new()).unwrap();
            assert!(get_tags(path).unwrap().is_empty());
        }
    }
}
