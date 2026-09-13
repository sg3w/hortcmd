//! Thin IPC adapters for the directory refresh service.
use super::refresh::subscriptions::{self, RefreshBatch, RefreshSubscription};
use crate::errors::{code, AppError, AppResult};

fn stopped(error: tokio::task::JoinError) -> AppError {
    AppError::new(code::FS_IO)
        .op("directory-refresh")
        .detail(error)
}

#[tauri::command]
pub async fn refresh_subscribe(subscriptions: Vec<RefreshSubscription>) -> AppResult<()> {
    tokio::task::spawn_blocking(move || subscriptions::subscribe(subscriptions))
        .await
        .map_err(stopped)?
}

#[tauri::command]
pub async fn refresh_poll(visible: bool) -> AppResult<RefreshBatch> {
    tokio::task::spawn_blocking(move || subscriptions::poll(visible))
        .await
        .map_err(stopped)
}

#[tauri::command]
pub async fn refresh_read(key: String, serial: u32) -> AppResult<super::dir::DirListing> {
    subscriptions::read(key, serial).await
}

#[tauri::command]
pub fn refresh_finish(key: String, serial: u32, error: Option<String>) {
    subscriptions::finish(key, serial, error);
}
