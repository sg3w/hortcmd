// ============================================================
// Interactive SSH shell (ticket 026).
//
// Opens a PTY-backed shell channel on the SSH connection that already
// serves the SFTP provider (`net::ssh`), so no second login happens.
// Output is pushed to the frontend as base64 chunks (`ssh-term-data`),
// because a shell emits arbitrary bytes and multi-byte characters may
// straddle packet boundaries — the terminal emulator decodes them.
// Input, resize and close come back as commands.
// ============================================================

use super::session::runtime;
use super::ssh::SshSession;
use base64::Engine;
use russh::client::Msg;
use russh::{Channel, ChannelMsg, ChannelWriteHalf};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

/// A chunk of shell output for one terminal session.
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TermData {
    pub id: String,
    /// Base64-encoded raw bytes as received from the remote shell.
    pub data: String,
}

/// End of a terminal session (remote exit, disconnect or error).
#[derive(Serialize, Clone, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct TermClosed {
    pub id: String,
    /// Human-readable reason, shown in the terminal before it closes.
    pub reason: String,
}

/// A live shell session: the write half of its channel plus the
/// connection it belongs to (so `net_disconnect` can close it).
struct Session {
    store_id: String,
    write: ChannelWriteHalf<Msg>,
}

fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Quotes a path for a POSIX shell (`'` → `'\''`).
fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', r"'\''"))
}

/// Opens a PTY-backed shell channel on `ssh` and moves it to `path`.
/// Shared by the command and the live test.
pub async fn open_shell_channel(
    ssh: &SshSession,
    path: &str,
    cols: u32,
    rows: u32,
) -> Result<Channel<Msg>, String> {
    let channel = ssh.open_channel().await?;
    channel
        .request_pty(true, "xterm-256color", cols.max(1), rows.max(1), 0, 0, &[])
        .await
        .map_err(|e| format!("PTY-Anforderung fehlgeschlagen: {}", e))?;
    channel
        .request_shell(true)
        .await
        .map_err(|e| format!("Shell-Start fehlgeschlagen: {}", e))?;
    // Start in the folder the pane shows. The shell echoes the line,
    // exactly like a manually typed `cd`.
    if !path.is_empty() && path != "/" {
        let cd = format!("cd {}\n", shell_quote(path));
        channel
            .data(cd.as_bytes())
            .await
            .map_err(|e| format!("Wechsel in {} fehlgeschlagen: {}", path, e))?;
    }
    Ok(channel)
}

// ---------- Commands ----------

/// Opens an interactive shell on the SSH connection behind `address`
/// and starts it in that address' remote directory. Returns the id the
/// other terminal commands and the events use.
#[tauri::command]
pub fn ssh_open_shell(
    app: AppHandle,
    address: String,
    cols: u32,
    rows: u32,
) -> Result<String, String> {
    let (provider, path) = crate::vfs::resolve(&address)?;
    let ssh = provider.ssh().ok_or_else(|| {
        "Für diesen Ort gibt es keine SSH-Verbindung — bitte eine SFTP-Verbindung öffnen"
            .to_string()
    })?;
    let store_id = provider.store_id();
    let store_id_for_task = store_id.clone();

    let channel = runtime().block_on(open_shell_channel(&ssh, &path, cols, rows))?;

    let id = format!("term-{}", super::unique_id());
    let (mut read, write) = channel.split();
    sessions()
        .lock()
        .unwrap()
        .insert(id.clone(), Session { store_id, write });

    // Pump the shell output to the frontend until the channel ends.
    let task_id = id.clone();
    let task_store = store_id_for_task;
    let task_ssh = ssh.clone();
    runtime().spawn(async move {
        let mut reason = String::from("Sitzung beendet");
        let mut clean_exit = false;
        while let Some(msg) = read.wait().await {
            match msg {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    let payload = TermData {
                        id: task_id.clone(),
                        data: base64::engine::general_purpose::STANDARD.encode(&data[..]),
                    };
                    if app.emit("ssh-term-data", payload).is_err() {
                        break;
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    // The remote shell exited by itself — the connection
                    // underneath it is fine.
                    clean_exit = true;
                    reason = format!("Shell beendet (Status {})", exit_status);
                }
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }
        sessions().lock().unwrap().remove(&task_id);
        // A shell that ended because the transport under it died means
        // the whole connection is gone, not just this channel. Reported
        // once, which removes the session and tells the frontend
        // (ticket 032); a normal `exit` reports nothing.
        if !clean_exit && task_ssh.is_closed() {
            reason = "Verbindung zum Server verloren".to_string();
            let store = task_store.clone();
            // A plain thread, not a runtime task: the report closes the
            // remaining shells, which blocks on the runtime this pump is
            // itself running on.
            std::thread::spawn(move || {
                super::session::report_connection_lost(&store, "Verbindung zum Server verloren");
            });
        }
        let _ = app.emit(
            "ssh-term-closed",
            TermClosed {
                id: task_id,
                reason,
            },
        );
    });

    Ok(id)
}

/// Sends keyboard input to a shell session.
#[tauri::command]
pub fn ssh_send(id: String, data: String) -> Result<(), String> {
    with_session(&id, |s| {
        runtime()
            .block_on(s.write.data(data.as_bytes()))
            .map_err(|e| format!("Senden fehlgeschlagen: {}", e))
    })
}

/// Reports a new terminal size to the remote side.
#[tauri::command]
pub fn ssh_resize(id: String, cols: u32, rows: u32) -> Result<(), String> {
    with_session(&id, |s| {
        runtime()
            .block_on(s.write.window_change(cols.max(1), rows.max(1), 0, 0))
            .map_err(|e| format!("Größenänderung fehlgeschlagen: {}", e))
    })
}

/// Closes a shell session (idempotent — an already ended session is fine).
#[tauri::command]
pub fn ssh_close(id: String) -> Result<(), String> {
    let Some(session) = sessions().lock().unwrap().remove(&id) else {
        return Ok(());
    };
    let _ = runtime().block_on(session.write.close());
    Ok(())
}

fn with_session<F>(id: &str, f: F) -> Result<(), String>
where
    F: FnOnce(&Session) -> Result<(), String>,
{
    let guard = sessions().lock().unwrap();
    let session = guard
        .get(id)
        .ok_or_else(|| "Terminal-Sitzung ist nicht mehr aktiv".to_string())?;
    f(session)
}

/// Closes every shell session of a connection. Called when that
/// connection is disconnected so no channel outlives it.
pub fn close_for_store(store_id: &str) {
    let closing: Vec<Session> = {
        let mut guard = sessions().lock().unwrap();
        let ids: Vec<String> = guard
            .iter()
            .filter(|(_, s)| s.store_id == store_id)
            .map(|(id, _)| id.clone())
            .collect();
        ids.iter().filter_map(|id| guard.remove(id)).collect()
    };
    for session in closing {
        let _ = runtime().block_on(session.write.close());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_for_the_shell() {
        assert_eq!(shell_quote("/srv/data"), "'/srv/data'");
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        // A single quote inside the path must not end the quoting.
        assert_eq!(shell_quote("/tmp/it's"), r"'/tmp/it'\''s'");
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(crate::net::unique_id(), crate::net::unique_id());
    }

    #[test]
    fn unknown_session_is_an_error() {
        assert!(ssh_send("nope".into(), "ls\n".into()).is_err());
        assert!(ssh_resize("nope".into(), 80, 24).is_err());
        // Closing an unknown session stays silent.
        assert!(ssh_close("nope".into()).is_ok());
    }
}
