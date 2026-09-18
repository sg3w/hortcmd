//! Bounded adapters to the core filesystem services; authorization is performed
//! by the broker before this module is called.
use super::{
    actions::validate_path,
    protocol::{PluginEncoding, PluginRequest},
};
use crate::{
    commands::fs,
    vfs::{
        self,
        caps::{self, Capability},
    },
};
use base64::Engine;
use serde_json::{json, Value};
use std::io::Read;
const MAX_READ: u32 = 256 * 1024;
pub fn execute(request: &PluginRequest) -> Result<Value, String> {
    match request {
        PluginRequest::Read {
            path,
            max_bytes,
            encoding,
        } => {
            if *max_bytes == 0 || *max_bytes > MAX_READ {
                return Err("Read limit must be 1 to 262144 bytes".into());
            }
            validate_path(path)?;
            caps::require(path, Capability::Preview).map_err(|e| e.to_string())?;
            let (provider, resolved) = vfs::resolve(path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            provider
                .open_read(&resolved)
                .map_err(|e| e.to_string())?
                .take(u64::from(*max_bytes) + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let truncated = bytes.len() > *max_bytes as usize;
            bytes.truncate(*max_bytes as usize);
            let content = match encoding {
                PluginEncoding::Utf8 => String::from_utf8_lossy(&bytes).into_owned(),
                PluginEncoding::Base64 => base64::engine::general_purpose::STANDARD.encode(&bytes),
            };
            Ok(json!({"content":content,"truncated":truncated,"bytes":bytes.len()}))
        }
        PluginRequest::List { path } => {
            validate_path(path)?;
            caps::require(path, Capability::Browse).map_err(|e| e.to_string())?;
            let listing = tauri::async_runtime::block_on(fs::dir::list_dir(path.clone(), None))
                .map_err(|e| e.to_string())?;
            if listing.entries.len() > 10000 {
                return Err("Directory exceeds the 10000-entry plugin limit".into());
            }
            serde_json::to_value(listing).map_err(|e| e.to_string())
        }
        PluginRequest::Preview { path } => {
            validate_path(path)?;
            serde_json::to_value(
                fs::preview::read_preview(path.clone(), MAX_READ).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        }
        PluginRequest::Write { path, content } => {
            validate_path(path)?;
            if content.len() > MAX_READ as usize {
                return Err("Write exceeds 256 KiB".into());
            }
            fs::file::write_text_file(path.clone(), content.clone(), false)
                .map(|written| json!({"written":written}))
                .map_err(|e| e.to_string())
        }
        _ => Err("Not a filesystem request".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_reads_and_core_create_only_writes() {
        let root =
            std::env::temp_dir().join(format!("hortcmd-plugin-io-{}", crate::net::unique_id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("source.php").to_string_lossy().into_owned();
        let write = PluginRequest::Write {
            path: path.clone(),
            content: "<?php echo 'source only'; ?>".into(),
        };
        assert_eq!(execute(&write).unwrap()["written"], true);
        assert_eq!(execute(&write).unwrap()["written"], false);
        let read = PluginRequest::Read {
            path: path.clone(),
            max_bytes: 5,
            encoding: PluginEncoding::Utf8,
        };
        let value = execute(&read).unwrap();
        assert_eq!(value["content"], "<?php");
        assert_eq!(value["truncated"], true);
        assert!(execute(&PluginRequest::Read {
            path: path.clone(),
            max_bytes: MAX_READ + 1,
            encoding: PluginEncoding::Utf8
        })
        .is_err());
        assert!(execute(&PluginRequest::Read {
            path: "relative".into(),
            max_bytes: 5,
            encoding: PluginEncoding::Utf8
        })
        .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
