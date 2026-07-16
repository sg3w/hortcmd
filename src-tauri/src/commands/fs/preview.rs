// ============================================================
// Datei-Vorschau (F3): Text, Bild (data-URL) oder Hex (binär).
// Zusätzlich: Öffnen mit dem Standardprogramm (F4).
// ============================================================

use base64::Engine as _;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use ts_rs::TS;

/// Ein EXIF-Feld (Anzeigename + formatierter Wert).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct ExifTag {
    pub name: String,
    pub value: String,
}

/// Vorschau-Inhalt einer Datei: Text, Bild (data-URL) oder Hex (binär).
#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct Preview {
    /// "text" | "image" | "binary"
    pub kind: String,
    pub name: String,
    #[ts(type = "number")]
    pub size: u64,
    /// Textfassung des gelesenen Ausschnitts (lossy UTF-8; auch bei Binärdateien).
    pub text: Option<String>,
    /// Hex-Dump (nur bei kind = binary).
    pub hex: Option<String>,
    /// data-URL (bei kind = image)
    pub data_url: Option<String>,
    /// EXIF-Metadaten (nur bei Bildern; sonst leer).
    pub exif: Vec<ExifTag>,
    /// true, wenn nur ein Anfangsausschnitt gelesen wurde
    pub truncated: bool,
}

const IMAGE_EXTS: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"];

/// Liest EXIF-Metadaten aus den Bildbytes (leer, wenn keine/nicht lesbar).
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

/// Liest bis zu `max_bytes` einer Datei für die Vorschau.
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

    // Bilder als data-URL zurückgeben.
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

    // Textausschnitt lesen.
    let cap = (max_bytes as u64).min(size) as usize;
    let mut f = File::open(p).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; cap];
    let n = f.read(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(n);
    let truncated = (n as u64) < size;

    // Nullbytes im Anfangsbereich → als binär behandeln (Hex-Ansicht).
    let is_binary = buf.iter().take(8000).any(|&b| b == 0);
    let text = Some(String::from_utf8_lossy(&buf).into_owned());
    Ok(Preview {
        kind: if is_binary { "binary" } else { "text" }.into(),
        name,
        size,
        // Text ist immer verfügbar; Hex nur zusätzlich bei Binärdateien.
        text,
        hex: is_binary.then(|| hex_dump(&buf)),
        data_url: None,
        exif: Vec::new(),
        truncated,
    })
}

/// Öffnet eine Datei/URL mit dem im System hinterlegten Standardprogramm.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    opener::open(&path).map_err(|e| e.to_string())
}

/// Öffnet die native macOS-Quick-Look-Vorschau (Leertaste). Nur macOS.
#[tauri::command]
pub fn quick_look(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        // `qlmanage -p` öffnet das Quick-Look-Panel; Ausgabe unterdrücken.
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

/// Öffnet eine Datei mit einem bestimmten Programm.
/// Leeres `program` → System-Standard (wie `open_path`).
#[tauri::command]
pub fn open_with(path: String, program: String) -> Result<(), String> {
    use std::process::Command;

    let program = program.trim();
    if program.is_empty() {
        return open_path(path);
    }

    #[cfg(target_os = "macos")]
    {
        // App-Bundle (…​.app) oder bloßer App-Name → über `open -a`.
        // Ein Pfad auf ein Binary (enthält "/") wird direkt gestartet.
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

/// Öffnet ein Terminal im angegebenen Ordner.
/// `program` überschreibt das Standard-Terminal (leer/None = Plattform-Standard).
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
        // Standard: Terminal.app; sonst die angegebene App (z. B. "iTerm").
        let app = prog.unwrap_or_else(|| "Terminal".to_string());
        Command::new("open")
            .args(["-a", &app, &path])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "windows")]
    {
        // Standard: cmd; startet mit Arbeitsverzeichnis = Ordner.
        let app = prog.unwrap_or_else(|| "cmd".to_string());
        Command::new(&app)
            .current_dir(dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Angegebenes Terminal oder gängige Kandidaten der Reihe nach probieren.
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
