// ============================================================
// File preview (F3): text, image (data URL), or hex (binary).
// Additionally: open with the default program (F4).
// ============================================================

use crate::vfs::{self, Provider};
use crate::vfs::caps::{self, Capability};
use base64::Engine as _;
use serde::Serialize;
use std::io::{self, Cursor, Read};
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

/// Reads the whole file through the provider (for images).
fn read_all(provider: &dyn Provider, p: &Path) -> io::Result<Vec<u8>> {
    let mut r = provider.open_read(p)?;
    let mut buf = Vec::new();
    r.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Reads up to `cap` bytes through the provider, looping over short
/// reads (a single `read` may return a partial chunk, especially over
/// SFTP).
fn read_upto(provider: &dyn Provider, p: &Path, cap: usize) -> io::Result<Vec<u8>> {
    let mut r = provider.open_read(p)?;
    let mut buf = Vec::with_capacity(cap.min(1 << 20));
    let mut chunk = vec![0u8; 64 * 1024];
    while buf.len() < cap {
        let want = (cap - buf.len()).min(chunk.len());
        let n = r.read(&mut chunk[..want])?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

/// Reads up to `max_bytes` of a file for the preview. Works for local
/// and remote (VFS-resolved) paths alike.
#[tauri::command]
pub fn read_preview(path: String, max_bytes: u32) -> Result<Preview, String> {
    let (provider, rpath) = vfs::resolve(&path)?;
    let p = Path::new(&rpath);
    let st = provider.stat(p).map_err(|e| e.to_string())?;
    if st.is_dir {
        return Err("Verzeichnis kann nicht angezeigt werden".into());
    }
    let size = st.len;
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
        let bytes = read_all(&*provider, p).map_err(|e| e.to_string())?;
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
    let buf = read_upto(&*provider, p, cap).map_err(|e| e.to_string())?;
    let truncated = (buf.len() as u64) < size;

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
///
/// This is the *file* entry point (F4 / double-click) and takes whatever
/// path the pane shows. It is deliberately **not** used for links out of
/// rendered content — see `open_external`, which validates the scheme so
/// a preview can never turn this into a general launcher (TICKET-030).
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    // The OS launcher only understands a local path; `sftp://host/x`
    // would reach it as an opaque string (TICKET-031).
    caps::require(&path, Capability::SystemOpen)?;
    opener::open(&path).map_err(|e| e.to_string())
}

/// Whether a link from rendered content may be handed to the system
/// browser. Only absolute `http`/`https` URLs with a host qualify —
/// everything else (`file:`, `javascript:`, `data:`, custom schemes,
/// scheme-relative or malformed input, embedded control characters) is
/// refused, so markup in a previewed file cannot reach the launcher.
///
/// The check is deliberately a whole-string one rather than a prefix
/// test: a leading space, a newline or a NUL would otherwise let
/// `\njavascript:…` pass a naive `starts_with`.
pub fn is_safe_external_url(url: &str) -> bool {
    // Control characters (and whitespace) are never part of a URL a
    // browser should be handed — they are how scheme checks get fooled.
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    // Everything up to the first `/`, `?` or `#` is the authority.
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // Credentials in the URL are a phishing aid, not something a file
    // preview needs — and the host after `@` is easy to misread.
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    // A bare port or an authority that is only punctuation is not a host.
    authority
        .split(':')
        .next()
        .is_some_and(|host| !host.is_empty())
}

/// Opens a validated `http(s)` URL in the system browser. The single
/// way out of the WebView for links in rendered content (TICKET-030):
/// the main window itself never navigates away from the app document.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !is_safe_external_url(&url) {
        return Err(format!("Nicht unterstützte Adresse: {}", url));
    }
    opener::open(&url).map_err(|e| e.to_string())
}

/// Opens the native macOS Quick Look preview (Space). macOS only.
#[tauri::command]
pub fn quick_look(path: String) -> Result<(), String> {
    // macOS-only and local-only — both gates live in the capability.
    caps::require(&path, Capability::QuickLook)?;
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

    caps::require(&path, Capability::OpenWith)?;
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

    // A local terminal has no notion of a remote location. SFTP folders
    // get a shell through `ssh_open_shell` on the live connection (026),
    // which is the `sshTerminal` capability — not this one.
    caps::require(&path, Capability::LocalTerminal)?;
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

// ------------------------------------------------------------
// Native system properties (TICKET-015)
//
// Hand-off to the operating system's own information window —
// Finder "Get Info", the Explorer properties sheet, the desktop
// file manager's properties dialog. hortcmd's own dialog stays
// as it is; this only opens the OS view next to it.
// ------------------------------------------------------------

/// How long we wait for a helper process (osascript, dbus-send) before
/// we consider the window "opened" and detach. Long enough to catch an
/// immediate failure, short enough never to block the dialog.
#[cfg(not(target_os = "windows"))]
const HELPER_WAIT: std::time::Duration = std::time::Duration::from_millis(4000);

/// Waits briefly for a spawned helper and maps a non-zero exit to an
/// error. A process still running when the budget is used up counts as
/// success (the native window is up, its lifetime is not ours).
#[cfg(not(target_os = "windows"))]
fn wait_briefly(mut child: std::process::Child) -> Result<(), String> {
    use std::time::Instant;
    let deadline = Instant::now() + HELPER_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                let mut msg = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut msg);
                }
                let msg = msg.trim();
                return Err(if msg.is_empty() {
                    "Systeminformationen konnten nicht geöffnet werden".into()
                } else {
                    msg.to_string()
                });
            }
            // Still running: the window is open, we do not wait for it.
            Ok(None) if Instant::now() >= deadline => return Ok(()),
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(25)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Percent-encodes a local path into a `file://` URI (freedesktop
/// interfaces take URIs, not paths). Everything outside the unreserved
/// set is escaped byte-wise, so non-ASCII names survive unchanged.
#[cfg(all(unix, not(target_os = "macos")))]
fn file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(b as char)
            }
            _ => uri.push_str(&format!("%{:02X}", b)),
        }
    }
    uri
}

/// Opens the operating system's own properties/information window for
/// `path`. Local file-system paths only — remote VFS locations and
/// entries inside an archive have no OS-level counterpart.
///
/// Returns as soon as the window has been handed to the system; its
/// lifetime is independent of this call and of hortcmd's own dialog.
#[tauri::command]
pub fn open_system_properties(path: String) -> Result<(), String> {
    // Remote addresses (sftp://, smb://, …) are rejected here as well,
    // not only hidden in the UI (TICKET-031).
    caps::require(&path, Capability::SystemOpen)?;
    let addr = vfs::parse_address(&path)?;

    // `symlink_metadata`, so a symlink (even a broken one) is its own
    // entry — the info window is about the link, not its target.
    let target = Path::new(&addr.path);
    if target.symlink_metadata().is_err() {
        return Err("Eintrag existiert nicht".into());
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        // Every path travels as an argv entry into the `run` handler, never
        // as text inside the script — quotes, ampersands, newlines and
        // umlauts in a name can therefore not change what the script does.
        //
        // The entry is addressed as "item <name> of folder <parent>", not
        // via `POSIX file <path> as alias`: the alias coercion follows a
        // symlink, so a link would open the info window of its target and
        // a broken link would fail outright. Addressing it inside its
        // container gives Finder the link itself.
        let mut cmd = Command::new("osascript");
        match (target.parent(), target.file_name()) {
            (Some(parent), Some(name)) => {
                cmd.args([
                    "-e",
                    "on run argv",
                    "-e",
                    "set parent_ to POSIX file (item 1 of argv) as alias",
                    "-e",
                    "tell application \"Finder\"",
                    "-e",
                    "activate",
                    "-e",
                    "open information window of (item (item 2 of argv) of folder parent_)",
                    "-e",
                    "end tell",
                    "-e",
                    "end run",
                    "--",
                ])
                .arg(parent)
                .arg(name);
            }
            // A volume root has no container of its own.
            _ => {
                cmd.args([
                    "-e",
                    "on run argv",
                    "-e",
                    "set item_ to POSIX file (item 1 of argv) as alias",
                    "-e",
                    "tell application \"Finder\"",
                    "-e",
                    "activate",
                    "-e",
                    "open information window of item_",
                    "-e",
                    "end tell",
                    "-e",
                    "end run",
                    "--",
                ])
                .arg(target);
            }
        }
        let child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        wait_briefly(child)
    }

    #[cfg(target_os = "windows")]
    {
        // `ShellExecuteEx` with the "properties" verb needs an
        // initialised apartment and a thread that stays alive until the
        // sheet is up (SEE_MASK_NOASYNC guarantees the latter). Both
        // happen on a dedicated thread, so the command itself returns
        // immediately and hortcmd stays responsive.
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{
            ShellExecuteExW, SEE_MASK_INVOKEIDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
        };
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

        let wide: Vec<u16> = target
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "properties\0".encode_utf16().collect();

        std::thread::spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_INVOKEIDLIST | SEE_MASK_NOASYNC,
                lpVerb: PCWSTR(verb.as_ptr()),
                lpFile: PCWSTR(wide.as_ptr()),
                nShow: SW_SHOW.0,
                ..Default::default()
            };
            let _ = ShellExecuteExW(&mut info);
            CoUninitialize();
        });
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::process::{Command, Stdio};

        let uri = file_uri(target);

        // `ShowItemProperties` is the exact counterpart; where a desktop
        // does not implement it, `ShowItems` at least reveals the entry
        // in its file manager. Both go over the session bus; `dbus-send`
        // and `gdbus` take every argument as its own argv entry, so the
        // URI needs no further quoting.
        let attempts: [(&str, Vec<String>); 4] = [
            (
                "dbus-send",
                dbus_send_args("ShowItemProperties", &uri),
            ),
            ("gdbus", gdbus_args("ShowItemProperties", &uri)),
            ("dbus-send", dbus_send_args("ShowItems", &uri)),
            ("gdbus", gdbus_args("ShowItems", &uri)),
        ];

        let mut last: Option<String> = None;
        for (bin, args) in attempts {
            match Command::new(bin)
                .args(&args)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => match wait_briefly(child) {
                    Ok(()) => return Ok(()),
                    Err(e) => last = Some(e),
                },
                Err(e) => last = Some(e.to_string()),
            }
        }
        Err(match last {
            Some(e) => format!(
                "Kein Dateimanager mit org.freedesktop.FileManager1 gefunden: {}",
                e
            ),
            None => "Kein Dateimanager mit org.freedesktop.FileManager1 gefunden".into(),
        })
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn dbus_send_args(method: &str, uri: &str) -> Vec<String> {
    vec![
        "--session".into(),
        "--dest=org.freedesktop.FileManager1".into(),
        "--type=method_call".into(),
        "/org/freedesktop/FileManager1".into(),
        format!("org.freedesktop.FileManager1.{}", method),
        format!("array:string:{}", uri),
        "string:".into(),
    ]
}

#[cfg(all(unix, not(target_os = "macos")))]
fn gdbus_args(method: &str, uri: &str) -> Vec<String> {
    vec![
        "call".into(),
        "--session".into(),
        "--dest".into(),
        "org.freedesktop.FileManager1".into(),
        "--object-path".into(),
        "/org/freedesktop/FileManager1".into(),
        "--method".into(),
        format!("org.freedesktop.FileManager1.{}", method),
        // GVariant literal. `file_uri` percent-encodes everything outside
        // the unreserved set, so the URI cannot contain a quote that would
        // close the literal early.
        format!("['{}']", uri),
        "".into(),
    ]
}

#[cfg(test)]
mod system_properties_tests {
    use super::*;

    #[test]
    fn rejects_remote_addresses() {
        for addr in [
            "sftp://user@host/tmp/a.txt",
            "smb://host/share/a.txt",
            "ftp://host/a.txt",
            "webdav://host/a.txt",
        ] {
            let err = open_system_properties(addr.to_string()).unwrap_err();
            // Since TICKET-031 the refusal is the typed capability
            // error, so the frontend recognizes it without reading prose.
            assert!(
                err.starts_with(crate::vfs::caps::UNSUPPORTED)
                    && err.contains(Capability::SystemOpen.id()),
                "unerwartete Meldung für {addr}: {err}"
            );
        }
    }

    #[test]
    fn rejects_missing_entries() {
        let missing = std::env::temp_dir().join("hortcmd-does-not-exist-015");
        let err = open_system_properties(missing.to_string_lossy().into_owned()).unwrap_err();
        assert!(err.contains("existiert nicht"), "{err}");
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn file_uri_escapes_everything_outside_the_unreserved_set() {
        assert_eq!(file_uri(Path::new("/tmp/a.txt")), "file:///tmp/a.txt");
        assert_eq!(
            file_uri(Path::new("/tmp/Grüße \"q\" & 'a'.txt")),
            "file:///tmp/Gr%C3%BC%C3%9Fe%20%22q%22%20%26%20%27a%27.txt"
        );
    }
}

// ============================================================
// External links from rendered preview content (TICKET-030).
// The WebView never navigates away from the app document, so a link
// only ever reaches the system browser — and only after this check.
// ============================================================
#[cfg(test)]
mod external_link_tests {
    use super::{is_safe_external_url, open_external};

    // ----- External links (TICKET-030) -----

    #[test]
    fn plain_http_and_https_urls_are_allowed() {
        assert!(is_safe_external_url("https://example.com"));
        assert!(is_safe_external_url("http://example.com/"));
        assert!(is_safe_external_url("https://example.com/a/b?q=1#frag"));
        assert!(is_safe_external_url("https://example.com:8443/docs"));
        assert!(is_safe_external_url("HTTPS://Example.COM"));
        assert!(is_safe_external_url("http://192.168.0.1:8080/x"));
    }

    #[test]
    fn other_schemes_never_reach_the_launcher() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "sftp://host/path",
            "hortcmd://internal",
            "mailto:someone@example.com",
        ] {
            assert!(!is_safe_external_url(url), "must be refused: {}", url);
        }
    }

    #[test]
    fn malformed_input_is_refused() {
        for url in [
            "",
            "example.com",
            "//example.com",
            "https://",
            "https:///path",
            "https://:8080/",
            "http:/example.com",
        ] {
            assert!(!is_safe_external_url(url), "must be refused: {:?}", url);
        }
    }

    #[test]
    fn whitespace_and_control_characters_defeat_no_check() {
        // A naive `starts_with` would let these through.
        for url in [
            " javascript:alert(1)",
            "\njavascript:alert(1)",
            "https://example.com\u{0}.evil.test",
            "https://exa mple.com",
            "\thttps://example.com",
            "https://example.com/a\rb",
        ] {
            assert!(!is_safe_external_url(url), "must be refused: {:?}", url);
        }
    }

    #[test]
    fn credentials_in_the_url_are_refused() {
        assert!(!is_safe_external_url("https://user:pass@evil.test/"));
        assert!(!is_safe_external_url("https://example.com@evil.test/"));
    }

    #[test]
    fn the_command_refuses_before_it_launches_anything() {
        assert!(open_external("javascript:alert(1)".into()).is_err());
        assert!(open_external("file:///etc/passwd".into()).is_err());
        assert!(open_external("".into()).is_err());
    }
}
