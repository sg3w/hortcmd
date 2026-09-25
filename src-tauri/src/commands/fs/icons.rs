//! Bounded icon transport. No VFS provider is opened for an icon request.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct FileIconRequest {
    pub id: String,
    pub name: String,
    /// Only supplied for a local, non-archive entry; links always use type icons.
    pub path: Option<String>,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub pixels: u16,
}

#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct FileIconResult {
    pub id: String,
    /// Bounded PNG data URL; None explicitly means use the simple icon.
    pub image: Option<String>,
}

static WORKERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[tauri::command]
pub async fn file_icons(requests: Vec<FileIconRequest>) -> Vec<FileIconResult> {
    if requests.len() > 32 {
        return Vec::new();
    }
    let Ok(permit) = WORKERS.try_acquire() else {
        return requests.into_iter().map(|r| fallback(r.id)).collect();
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let mut batch = IconBatch::default();
        requests
            .into_iter()
            .map(|request| render(request, &mut batch))
            .collect()
    })
    .await
    .unwrap_or_default()
}

fn fallback(id: String) -> FileIconResult {
    FileIconResult { id, image: None }
}

#[derive(Default)]
struct IconBatch {
    #[cfg(target_os = "macos")]
    native: macos::Batch,
}

fn render(request: FileIconRequest, _batch: &mut IconBatch) -> FileIconResult {
    if !(12..=128).contains(&request.pixels)
        || request.id.len() > 8192
        || request.name.len() > 1024
        || request.path.as_ref().is_some_and(|p| p.len() > 4096)
    {
        return fallback(request.id);
    }
    #[cfg(target_os = "macos")]
    let image = macos::render(&request, &mut _batch.native);
    #[cfg(not(target_os = "macos"))]
    let image = None;
    FileIconResult {
        id: request.id,
        image,
    }
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> FileIconRequest {
        FileIconRequest {
            id: "test".into(),
            name: "test.txt".into(),
            path: None,
            is_dir: false,
            is_symlink: false,
            pixels: 0,
        }
    }
    #[test]
    fn invalid_sizes_fall_back() {
        assert!(render(request(), &mut IconBatch::default()).image.is_none());
    }
    #[tokio::test]
    async fn rejects_unbounded_batches() {
        assert!(file_icons(vec![request(); 33]).await.is_empty());
    }
    #[tokio::test]
    async fn busy_worker_does_not_grow_a_queue() {
        let _permit = WORKERS.acquire().await.unwrap();
        let result = file_icons(vec![request()]).await;
        assert_eq!(result[0].id, "test");
        assert!(result[0].image.is_none());
    }
}
