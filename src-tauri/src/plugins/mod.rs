//! Versioned extension contract and plugin discovery.
pub mod broker;
pub mod js_host;
pub mod manifest;
pub mod protocol;
pub mod registry;

pub fn navigation_allowed(url: &tauri::Url) -> bool {
    crate::is_trusted_origin(url)
        || url.scheme() == "hortplugin"
        || (url.scheme() == "http" && url.host_str() == Some("hortplugin.localhost"))
}

pub mod native_abi;
pub mod native_host;
pub mod native_worker;

pub mod launch;

pub mod context;
pub mod views;

pub mod actions;
pub mod events;
pub mod transfers;

pub mod filesystem;

pub mod management;
pub mod package;
