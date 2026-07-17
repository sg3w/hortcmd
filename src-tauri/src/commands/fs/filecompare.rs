// ============================================================
// File/binary comparison of two individual files.
//   compare_files – detects text vs. binary and returns either a
//   line-by-line diff (similar/Myers) or a hex comparison.
// Runs on a background thread (spawn_blocking) so large
// files don't block the UI; results are hard-limited.
// ============================================================

use serde::Serialize;
use similar::{ChangeTag, TextDiff};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use ts_rs::TS;

/// One line of the text comparison (side-by-side).
/// `tag`: "equal" | "replace" | "delete" | "insert".
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct DiffLine {
    pub tag: String,
    #[ts(type = "number | null")]
    pub left_no: Option<u32>,
    pub left: Option<String>,
    #[ts(type = "number | null")]
    pub right_no: Option<u32>,
    pub right: Option<String>,
}

/// One 16-byte line of the hex comparison.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct HexRow {
    #[ts(type = "number")]
    pub offset: u32,
    pub left_hex: String,
    pub left_ascii: String,
    pub right_hex: String,
    pub right_ascii: String,
    /// Whether the two sides differ in this line.
    pub differs: bool,
}

/// Overall result of a file comparison.
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct FileDiff {
    /// "text" | "binary"
    pub mode: String,
    /// Whether both files are byte-identical.
    pub identical: bool,
    /// Whether the result was truncated due to the size limits.
    pub truncated: bool,
    #[ts(type = "number")]
    pub left_size: u64,
    #[ts(type = "number")]
    pub right_size: u64,
    /// Lines in text mode (otherwise empty).
    pub lines: Vec<DiffLine>,
    /// Lines in binary mode (otherwise empty).
    pub hex: Vec<HexRow>,
}

// Upper limits: protect memory/frontend for very large files.
const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024; // je Seite für den Textmodus
const MAX_LINES: usize = 200_000; // Diff-Zeilen gesamt
const MAX_HEX_BYTES: usize = 1024 * 1024; // je Seite für die Hex-Ansicht

/// Checks in a streaming way whether two files are byte-identical (without keeping
/// both fully in memory).
fn files_identical(a: &Path, b: &Path) -> std::io::Result<bool> {
    let mut ra = BufReader::new(File::open(a)?);
    let mut rb = BufReader::new(File::open(b)?);
    let mut ba = [0u8; 64 * 1024];
    let mut bb = [0u8; 64 * 1024];
    loop {
        let na = read_full(&mut ra, &mut ba)?;
        let nb = read_full(&mut rb, &mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

/// Fills the buffer as much as possible (up to EOF) so it can be compared
/// block by block without short reads distorting the comparison.
fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Reads at most `cap` bytes; also reports whether the file is larger.
fn read_capped(path: &Path, cap: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.by_ref().take(cap as u64).read_to_end(&mut buf)?;
    // Read one more byte to detect "larger than cap".
    let mut extra = [0u8; 1];
    let more = f.read(&mut extra)? > 0;
    Ok((buf, more))
}

/// Heuristic: NUL byte or invalid UTF-8 ⇒ binary file.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err()
}

/// Builds the side-by-side lines from a line diff.
fn build_text_diff(left: &str, right: &str) -> (Vec<DiffLine>, bool) {
    let diff = TextDiff::from_lines(left, right);
    let changes: Vec<_> = diff.iter_all_changes().collect();
    let mut rows = Vec::new();
    let mut truncated = false;

    let clean = |v: &str| v.trim_end_matches(['\n', '\r']).to_string();
    let no = |i: Option<usize>| i.map(|n| n as u32 + 1);

    let mut i = 0;
    while i < changes.len() {
        if rows.len() >= MAX_LINES {
            truncated = true;
            break;
        }
        let c = &changes[i];
        match c.tag() {
            ChangeTag::Equal => {
                rows.push(DiffLine {
                    tag: "equal".into(),
                    left_no: no(c.old_index()),
                    left: Some(clean(c.value())),
                    right_no: no(c.new_index()),
                    right: Some(clean(c.value())),
                });
                i += 1;
            }
            // A block of deletions (+ directly following insertions) is
            // paired line by line as "replace", overhangs as delete/insert.
            ChangeTag::Delete | ChangeTag::Insert => {
                let mut dels = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Delete {
                    dels.push(&changes[i]);
                    i += 1;
                }
                let mut inss = Vec::new();
                while i < changes.len() && changes[i].tag() == ChangeTag::Insert {
                    inss.push(&changes[i]);
                    i += 1;
                }
                let n = dels.len().max(inss.len());
                for k in 0..n {
                    let d = dels.get(k);
                    let s = inss.get(k);
                    let tag = match (d, s) {
                        (Some(_), Some(_)) => "replace",
                        (Some(_), None) => "delete",
                        _ => "insert",
                    };
                    rows.push(DiffLine {
                        tag: tag.into(),
                        left_no: d.and_then(|x| no(x.old_index())),
                        left: d.map(|x| clean(x.value())),
                        right_no: s.and_then(|x| no(x.new_index())),
                        right: s.map(|x| clean(x.value())),
                    });
                }
            }
        }
    }
    (rows, truncated)
}

/// Formats 16 bytes as hex ("48 65 …") and printable ASCII text.
fn hex_and_ascii(chunk: &[u8]) -> (String, String) {
    let hex = chunk
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ");
    let ascii = chunk
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                '.'
            }
        })
        .collect();
    (hex, ascii)
}

/// Builds the hex comparison (16 bytes per line).
fn build_hex_diff(left: &[u8], right: &[u8]) -> Vec<HexRow> {
    let rows = left.len().max(right.len()).div_ceil(16);
    (0..rows)
        .map(|row| {
            let start = row * 16;
            let l = &left[start.min(left.len())..(start + 16).min(left.len())];
            let r = &right[start.min(right.len())..(start + 16).min(right.len())];
            let (left_hex, left_ascii) = hex_and_ascii(l);
            let (right_hex, right_ascii) = hex_and_ascii(r);
            HexRow {
                offset: start as u32,
                left_hex,
                left_ascii,
                right_hex,
                right_ascii,
                differs: l != r,
            }
        })
        .collect()
}

fn compare_impl(left: String, right: String) -> Result<FileDiff, String> {
    let lp = Path::new(&left);
    let rp = Path::new(&right);
    let lmeta = std::fs::metadata(lp).map_err(|e| format!("{}: {}", left, e))?;
    let rmeta = std::fs::metadata(rp).map_err(|e| format!("{}: {}", right, e))?;
    if lmeta.is_dir() || rmeta.is_dir() {
        return Err("Beide Seiten müssen Dateien sein".into());
    }
    let left_size = lmeta.len();
    let right_size = rmeta.len();

    let identical =
        left_size == right_size && files_identical(lp, rp).map_err(|e| e.to_string())?;

    let (lbytes, lmore) = read_capped(lp, MAX_TEXT_BYTES).map_err(|e| e.to_string())?;
    let (rbytes, rmore) = read_capped(rp, MAX_TEXT_BYTES).map_err(|e| e.to_string())?;
    let binary = looks_binary(&lbytes) || looks_binary(&rbytes);

    if binary {
        // Limit to a smaller window for the hex view.
        let lhex = &lbytes[..lbytes.len().min(MAX_HEX_BYTES)];
        let rhex = &rbytes[..rbytes.len().min(MAX_HEX_BYTES)];
        let truncated = lmore || rmore || lbytes.len() > MAX_HEX_BYTES || rbytes.len() > MAX_HEX_BYTES;
        Ok(FileDiff {
            mode: "binary".into(),
            identical,
            truncated,
            left_size,
            right_size,
            lines: Vec::new(),
            hex: build_hex_diff(lhex, rhex),
        })
    } else {
        // Safe, since looks_binary already checked UTF-8.
        let ltext = String::from_utf8_lossy(&lbytes);
        let rtext = String::from_utf8_lossy(&rbytes);
        let (lines, line_trunc) = build_text_diff(&ltext, &rtext);
        Ok(FileDiff {
            mode: "text".into(),
            identical,
            truncated: lmore || rmore || line_trunc,
            left_size,
            right_size,
            lines,
            hex: Vec::new(),
        })
    }
}

/// Compares two files by content (text diff or hex comparison).
#[tauri::command]
pub async fn compare_files(left: String, right: String) -> Result<FileDiff, String> {
    tauri::async_runtime::spawn_blocking(move || compare_impl(left, right))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::compare_impl;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn write(name: &str, bytes: &[u8]) -> String {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rc-fcmp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p: PathBuf = dir.join(format!("{}-{}", n, name));
        fs::write(&p, bytes).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn identical_text() {
        let a = write("a", b"line1\nline2\n");
        let b = write("b", b"line1\nline2\n");
        let d = compare_impl(a, b).unwrap();
        assert_eq!(d.mode, "text");
        assert!(d.identical);
        assert!(d.lines.iter().all(|l| l.tag == "equal"));
    }

    #[test]
    fn changed_line_is_replace() {
        let a = write("a", b"same\nold\ntail\n");
        let b = write("b", b"same\nnew\ntail\n");
        let d = compare_impl(a, b).unwrap();
        assert!(!d.identical);
        let repl = d.lines.iter().find(|l| l.tag == "replace").unwrap();
        assert_eq!(repl.left.as_deref(), Some("old"));
        assert_eq!(repl.right.as_deref(), Some("new"));
        // Pure insertion/deletion as delete/insert.
        let c = write("c", b"same\ntail\n");
        let d2 = compare_impl(write("a2", b"same\nextra\ntail\n"), c).unwrap();
        assert!(d2.lines.iter().any(|l| l.tag == "delete"));
    }

    #[test]
    fn binary_mode_and_diff() {
        let a = write("a", &[0u8, 1, 2, 3, 0xff]);
        let b = write("b", &[0u8, 1, 9, 3, 0xff]);
        let d = compare_impl(a, b).unwrap();
        assert_eq!(d.mode, "binary");
        assert!(!d.identical);
        assert_eq!(d.hex.len(), 1);
        assert!(d.hex[0].differs);
        assert_eq!(d.hex[0].left_hex, "00 01 02 03 ff");
    }
}
