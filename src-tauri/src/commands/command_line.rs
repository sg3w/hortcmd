//! One explicitly submitted, non-interactive local shell command at a time.
use crate::errors::{code, AppError, AppResult};
use crate::vfs::caps::{self, Capability};
use serde::Serialize;
use std::{
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};
use ts_rs::TS;

const OUTPUT_LIMIT: usize = 128 * 1024; // Per stream; keep draining after the limit.

#[derive(Clone, Default, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct CommandOutput {
    pub id: u32,
    pub done: bool,
    pub cancelled: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub error: Option<AppError>,
}

struct Job {
    output: Mutex<CommandOutput>,
    cancel: AtomicBool,
}

#[derive(Default)]
pub struct CommandLine(Mutex<Option<Arc<Job>>>);

impl Drop for CommandLine {
    fn drop(&mut self) {
        if let Ok(slot) = self.0.lock() {
            if let Some(job) = slot.as_ref() {
                job.cancel.store(true, Ordering::Release);
            }
        }
    }
}

fn lock_error() -> AppError {
    AppError::new(code::INTERNAL_UNEXPECTED).op("command")
}

impl CommandLine {
    fn start(&self, path: &str, command: &str) -> AppResult<u32> {
        caps::require(path, Capability::LocalTerminal)?;
        if !Path::new(path).is_absolute()
            || command.trim().is_empty()
            || command.contains('\0')
            || command.len() > 32768
        {
            return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("command"));
        }
        if !Path::new(path).is_dir() {
            return Err(AppError::new(code::FS_NOT_A_DIRECTORY).with("path", path));
        }
        let mut slot = self.0.lock().map_err(|_| lock_error())?;
        if let Some(job) = slot.as_ref() {
            if !job.output.lock().map_err(|_| lock_error())?.done {
                return Err(AppError::new(code::FS_IN_USE).op("command"));
            }
        }
        let id = match slot.as_ref() {
            Some(job) => job
                .output
                .lock()
                .map_err(|_| lock_error())?
                .id
                .wrapping_add(1),
            None => 1,
        };
        #[cfg(unix)]
        let mut process = {
            // A predictable POSIX shell, without interactive startup files.
            let mut p = tokio::process::Command::new("/bin/sh");
            p.arg("-c").arg(command).process_group(0);
            p
        };
        #[cfg(windows)]
        let mut process = {
            let mut p = tokio::process::Command::new("cmd.exe");
            p.args(["/D", "/S", "/C", command]);
            p.creation_flags(0x08000000); // CREATE_NO_WINDOW
            p
        };
        let mut child = process
            .current_dir(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let job = Arc::new(Job {
            output: Mutex::new(CommandOutput {
                id,
                ..Default::default()
            }),
            cancel: AtomicBool::new(false),
        });
        let stdout = child.stdout.take().ok_or_else(lock_error)?;
        let stderr = child.stderr.take().ok_or_else(lock_error)?;
        *slot = Some(job.clone());
        tauri::async_runtime::spawn(async move {
            let mut group = ProcessGroup(child.id());
            let out = tokio::spawn(drain(stdout, job.clone(), false));
            let err = tokio::spawn(drain(stderr, job.clone(), true));
            let result = loop {
                if job.cancel.load(Ordering::Acquire) {
                    group.terminate();
                    let _ = child.start_kill();
                    break child.wait().await;
                }
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Err(error) => break Err(error),
                    Ok(None) => tokio::time::sleep(Duration::from_millis(40)).await,
                }
            };
            // Background processes are not persistent command-line sessions.
            group.terminate();
            for mut reader in [out, err] {
                if tokio::time::timeout(Duration::from_millis(500), &mut reader)
                    .await
                    .is_err()
                {
                    reader.abort();
                    let _ = reader.await;
                    if let Ok(mut output) = job.output.lock() {
                        output.truncated = true;
                    }
                }
            }
            if let Ok(mut output) = job.output.lock() {
                output.cancelled = job.cancel.load(Ordering::Acquire);
                match result {
                    Ok(status) => output.exit_code = status.code(),
                    Err(error) => output.error = Some(AppError::from(error).op("command")),
                }
                output.done = true;
            }
        });
        Ok(id)
    }

    fn job(&self, id: u32) -> AppResult<Arc<Job>> {
        let slot = self.0.lock().map_err(|_| lock_error())?;
        let job = slot
            .as_ref()
            .ok_or_else(|| AppError::new(code::FS_NOT_FOUND))?;
        if job.output.lock().map_err(|_| lock_error())?.id != id {
            return Err(AppError::new(code::FS_NOT_FOUND));
        }
        Ok(job.clone())
    }
}

struct ProcessGroup(Option<u32>);
impl ProcessGroup {
    fn terminate(&mut self) {
        if let Some(id) = self.0.take() {
            #[cfg(unix)]
            // SAFETY: a fresh process group was created with this child's PID.
            unsafe {
                libc::kill(-(id as i32), libc::SIGKILL);
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill.exe")
                    .args(["/PID", &id.to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
    }
}
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.terminate();
    }
}

async fn drain(mut stream: impl AsyncRead + Unpin, job: Arc<Job>, stderr: bool) {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => {
                let keep = n.min(OUTPUT_LIMIT - bytes.len());
                bytes.extend_from_slice(&buffer[..keep]);
                if let Ok(mut output) = job.output.lock() {
                    output.truncated |= keep < n;
                    if keep == 0 {
                        continue;
                    }
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    if stderr {
                        output.stderr = text;
                    } else {
                        output.stdout = text;
                    }
                }
            }
            Err(error) => {
                if let Ok(mut output) = job.output.lock() {
                    output.error = Some(AppError::from(error).op("command"));
                }
                break;
            }
        }
    }
}

#[tauri::command]
pub async fn command_start(
    state: tauri::State<'_, CommandLine>,
    path: String,
    command: String,
) -> AppResult<u32> {
    state.start(&path, &command)
}
#[tauri::command]
pub fn command_status(state: tauri::State<'_, CommandLine>, id: u32) -> AppResult<CommandOutput> {
    Ok(state
        .job(id)?
        .output
        .lock()
        .map_err(|_| lock_error())?
        .clone())
}
#[tauri::command]
pub fn command_cancel(state: tauri::State<'_, CommandLine>, id: u32) -> AppResult<()> {
    state.job(id)?.cancel.store(true, Ordering::Release);
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    async fn finish(service: &CommandLine, id: u32) -> CommandOutput {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let output = service.job(id).unwrap().output.lock().unwrap().clone();
                if output.done {
                    return output;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("command completed")
    }

    #[tokio::test]
    async fn cwd_quoting_stderr_and_nonzero_exit() {
        let dir = std::env::temp_dir().join(format!("hortcmd-command-{}", crate::net::unique_id()));
        std::fs::create_dir(&dir).unwrap();
        let nested = dir.join("folder with ' quotes");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("file name"), "content").unwrap();
        let service = CommandLine::default();
        let id = service
            .start(
                nested.to_str().unwrap(),
                "cat 'file name'; printf problem >&2; exit 7",
            )
            .unwrap();
        let output = finish(&service, id).await;
        assert_eq!(output.stdout, "content");
        assert_eq!(output.stderr, "problem");
        assert_eq!(output.exit_code, Some(7));
        assert!(!output.cancelled);
        assert!(output.error.is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn rejects_invalid_input_and_remote_locations_without_spawning() {
        let service = CommandLine::default();
        for (path, command) in [
            ("/", "  "),
            ("relative", "echo hi"),
            ("/", "a\0b"),
            ("sftp://host/path", "echo hi"),
        ] {
            assert!(service.start(path, command).is_err());
        }
        assert!(service.0.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn cancels_process_group_and_rejects_overlap_and_stale_ids() {
        let service = CommandLine::default();
        let id = service
            .start("/", "sleep 30 & printf started; wait")
            .unwrap();
        assert!(service.start("/", "echo should-not-run").is_err());
        tokio::time::timeout(Duration::from_secs(2), async {
            while service.job(id).unwrap().output.lock().unwrap().stdout != "started" {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("descendant started before cancellation");
        service
            .job(id)
            .unwrap()
            .cancel
            .store(true, Ordering::Release);
        let output = finish(&service, id).await;
        assert!(output.cancelled);
        assert!(
            !output.truncated,
            "descendants must release their output pipes"
        );
        let next = service.start("/", "printf next").unwrap();
        assert!(service.job(id).is_err());
        assert_eq!(finish(&service, next).await.stdout, "next");
    }

    #[tokio::test]
    async fn output_is_bounded_and_stdin_is_closed() {
        let service = CommandLine::default();
        let id = service.start("/", "cat; head -c 300000 /dev/zero").unwrap();
        let output = finish(&service, id).await;
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout.len(), OUTPUT_LIMIT);
        assert!(output.truncated);
    }
}
