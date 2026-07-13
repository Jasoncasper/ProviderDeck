pub mod app_paths;
pub mod assets;
pub mod bridge;
pub mod cdp;
pub mod codex_config;
pub mod diagnostic_log;
pub mod http_client;
pub mod image_compressor;
pub mod install;
pub mod launcher;
pub mod legacy_import;
pub mod local_auth;
pub mod paths;
pub mod ports;
pub mod protocol_proxy;
pub mod provider_catalog;
pub mod proxy;
pub mod router;
pub mod routes;
pub mod settings;
pub mod status;
pub mod switch_journal;
pub mod update;
pub mod version;
pub mod watcher;
#[cfg(windows)]
mod windows_integration;

#[cfg(windows)]
pub fn windows_create_no_window() -> u32 {
    windows_integration::CREATE_NO_WINDOW
}

#[cfg(windows)]
pub fn windows_open_url(url: &str) -> anyhow::Result<()> {
    windows_integration::open_url(url)
}
