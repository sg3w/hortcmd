// ============================================================
// File preview (F3): text, image (data URL), or hex (binary).
// Additionally: open with the default program (F4).
// ============================================================

use base64::Engine as _;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use ts_rs::TS;

/// An EXIF field (display name + formatted value).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ExifTag {
    pub name: String,
    pub value: String,
}

/// Preview content of a file: text, image (data URL), or hex (binary).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Preview {
    /// "text" | "image" | "binary"
    pub kind: String,
    pub name: String,
    #[ts(type = "number")]
    pub size: u64,
    /// Text version of the read excerpt (lossy UTF-8; for binary files too).
    pub text: Option<String>,
    /// Hex dump (only for kind = binary).
    pub hex: Option<String>,
    /// data URL (for kind = image)
    pub data_url: Option<String>,
    /// EXIF metadata (only for images; otherwise empty).
    pub exif: Vec<ExifTag>,
    /// true if only an initial excerpt was read
    pub truncated: bool,
}

const IMAGE_EXTS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"];

/// Reads EXIF metadata from the image bytes (empty if none/not readable).
fn read_exif(bytes: &[u8]) -> Vec<ExifTag> {
    let exif = match exif::Reader::new().read_from_container(&mut Cursor::new(bytes)) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    exif.fields()
        .map(|f| ExifTag {
            name: f.tag.to_string(),
            value: f.display_value().with_unit(&exif).to_string(),
        })
        .collect()
}

fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!("{:08x}  {:<47}  {}\n", i * 16, hex.join(" "), ascii));
    }
    out
}

/// Reads up to `max_bytes` of a file for the preview.
#[tauri::command]
pub fn read_preview(path: String, max_bytes: u32) -> Result<Preview, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        return Err("Verzeichnis kann nicht angezeigt werden".into());
    }
    let size = meta.len();
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Return images as a data URL.
    if IMAGE_EXTS.contains(&ext.as_str()) {
        let bytes = fs::read(p).map_err(|e| e.to_string())?;
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "ico" => "image/x-icon",
            _ => "image/png",
        };
        let exif = read_exif(&bytes);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        return Ok(Preview {
            kind: "image".into(),
            name,
            size,
            text: None,
            hex: None,
            data_url: Some(format!("data:{};base64,{}", mime, b64)),
            exif,
            truncated: false,
        });
    }

    // Read a text excerpt.
    let cap = (max_bytes as u64).min(size) as usize;
    let mut f = File::open(p).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; cap];
    let n = f.read(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(n);
    let truncated = (n as u64) < size;

    // Null bytes in the initial region → treat as binary (hex view).
    let is_binary = buf.iter().take(8000).any(|&b| b == 0);
    let text = Some(String::from_utf8_lossy(&buf).into_owned());
    Ok(Preview {
        kind: if is_binary { "binary" } else { "text" }.into(),
        name,
        size,
        // Text is always available; hex only additionally for binary files.
        text,
        hex: is_binary.then(|| hex_dump(&buf)),
        data_url: None,
        exif: Vec::new(),
        truncated,
    })
}

/// Opens a file/URL with the default program registered in the system.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    opener::open(&path).map_err(|e| e.to_string())
}

/// Opens the native macOS Quick Look preview (Space). macOS only.
#[tauri::command]
pub fn quick_look(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        // `qlmanage -p` opens the Quick Look panel; suppress the output.
        Command::new("qlmanage")
            .args(["-p", &path])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err("Quick Look ist nur unter macOS verfügbar".into())
    }
}

/// Opens a file with a specific program.
/// Empty `program` → system default (like `open_path`).
#[tauri::command]
pub fn open_with(path: String, program: String) -> Result<(), String> {
    use std::process::Command;

    let program = program.trim();
    if program.is_empty() {
        return open_path(path);
    }

    #[cfg(target_os = "macos")]
    {
        // App bundle (….app) or a bare app name → via `open -a`.
        // A path to a binary (contains "/") is started directly.
        if program.ends_with(".app") || !program.contains('/') {
            Command::new("open")
                .args(["-a", program, &path])
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        } else {
            Command::new(program)
                .arg(&path)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Command::new(program)
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Opens a terminal in the given folder.
/// `program` overrides the default terminal (empty/None = platform default).
#[tauri::command]
pub fn open_terminal(path: String, program: Option<String>) -> Result<(), String> {
    use std::process::Command;

    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Err("Kein Verzeichnis".into());
    }
    let prog = program
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());

    #[cfg(target_os = "macos")]
    {
        // Default: Terminal.app; otherwise the given app (e.g. "iTerm").
        let app = prog.unwrap_or_else(|| "Terminal".to_string());
        Command::new("open")
            .args(["-a", &app, &path])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "windows")]
    {
        // Default: cmd; starts with the working directory = folder.
        let app = prog.unwrap_or_else(|| "cmd".to_string());
        Command::new(&app)
            .current_dir(dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Try the given terminal or common candidates in order.
        let candidates: Vec<String> = match prog {
            Some(p) => vec![p],
            None => [
                "x-terminal-emulator",
                "gnome-terminal",
                "konsole",
                "xfce4-terminal",
                "xterm",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        };
        for term in candidates {
            if Command::new(&term).current_dir(dir).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("Kein Terminal gefunden".into())
    }
}
