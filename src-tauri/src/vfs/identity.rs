//! Per-process connection generations; weak references prevent pointer reuse.
use super::Provider;
use crate::errors::{code, AppError, AppResult};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
type Entry = (Weak<dyn Provider>, String);
pub fn generation(provider: &Arc<dyn Provider>) -> AppResult<String> {
    if provider.store_id() == "local" {
        return Ok("local".into());
    }
    static ENTRIES: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    let mut entries = ENTRIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| AppError::new(code::FS_IO))?;
    let key = hex::encode(Sha256::digest(provider.store_id().as_bytes()));
    if let Some((old, generation)) = entries.get(&key) {
        if old.ptr_eq(&Arc::downgrade(provider)) {
            return Ok(generation.clone());
        }
    }
    entries.retain(|_, (provider, _)| provider.strong_count() > 0);
    if entries.len() >= 256 {
        return Err(AppError::new(code::VALIDATION_BAD_INPUT).op("connection-generation-limit"));
    }
    let generation = crate::net::unique_id();
    entries.insert(key, (Arc::downgrade(provider), generation.clone()));
    Ok(generation)
}
