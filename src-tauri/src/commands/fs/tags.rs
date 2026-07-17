// ============================================================
// Show and edit Finder tags (macOS).
//
// Finder stores tags in the extended attribute
// `com.apple.metadata:_kMDItemUserTags` as a **binary plist**: an array
// of strings, each "Name" or "Name\nColorIndex" (color 0–7, 0 = none).
// On non-macOS platforms the commands return empty values or a
// clear error.
// ============================================================

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A Finder tag: name plus color index (0 = none, 1–7 = Finder colors).
#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Tag {
    pub name: String,
    #[ts(type = "number")]
    pub color: u8,
}

#[cfg(target_os = "macos")]
const TAGS_ATTR: &str = "com.apple.metadata:_kMDItemUserTags";

/// Reads the Finder tags of an entry.
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

/// Writes the Finder tags of an entry (an empty list removes the attribute).
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

    /// Splits a tag string "Name" or "Name\nColor" into name + color index.
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

    /// Reassembles a tag into the Finder encoding.
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
            // Remove the attribute; "not present" is not an error.
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

    /// ENOATTR is 93 on macOS (no direct std constant access without libc).
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
            // No tags initially.
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

            // An empty list removes the attribute again.
            set_tags(path.clone(), Vec::new()).unwrap();
            assert!(get_tags(path).unwrap().is_empty());
        }
    }
}
