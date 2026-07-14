use std::path::{Path, PathBuf};
use std::time::Instant;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::settings::{BackendSettings, SettingsStore};
use crate::status::StatusStore;

use std::sync::Arc;
pub type DevtoolsOpener = Arc<dyn Fn(&str) -> anyhow::Result<()> + Send + Sync>;

#[derive(Clone)]
pub struct BridgeContext {
    settings: Arc<dyn BridgeSettingsService>,
    runtime: Arc<dyn BridgeRuntimeService>,
}

impl BridgeContext {
    pub fn new(
        settings: Arc<dyn BridgeSettingsService>,
        runtime: Arc<dyn BridgeRuntimeService>,
    ) -> Self {
        Self { settings, runtime }
    }

    pub fn core(runtime: Arc<dyn BridgeRuntimeService>) -> Self {
        Self::new(Arc::new(CoreSettingsService::default()), runtime)
    }
}

#[async_trait]
pub trait BridgeSettingsService: Send + Sync {
    async fn get_settings(&self) -> anyhow::Result<BackendSettings>;
    async fn set_settings(&self, payload: Value) -> anyhow::Result<BackendSettings>;
}

#[async_trait]
pub trait BridgeRuntimeService: Send + Sync {
    async fn open_devtools(&self) -> anyhow::Result<Value>;
    async fn open_manager(&self) -> anyhow::Result<Value>;
    async fn backend_status(&self) -> anyhow::Result<Value>;
    async fn repair_backend(&self) -> anyhow::Result<Value>;
}

pub async fn handle_bridge_request(
    ctx: BridgeContext,
    path: &str,
    payload: Value,
) -> serde_json::Value {
    let started = Instant::now();
    let _ = crate::diagnostic_log::append_diagnostic_log(
        "bridge.request",
        json!({
            "path": path,
            "payload_keys": payload
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        }),
    );
    let result = match path {
        "/providerdeck/catalog" => providerdeck_catalog_value(),
        "/providerdeck/switch-journal/save" => save_switch_journal(payload.clone()),
        "/providerdeck/switch-journal/load" => load_switch_journal(),
        "/providerdeck/switch-journal/clear" => clear_switch_journal(),
        "/providerdeck/thread-history/safety" => thread_history_safety_value(&payload),
        "/settings/get" => settings_value(ctx.settings.get_settings().await),
        "/settings/set" => settings_value(ctx.settings.set_settings(payload.clone()).await),
        "/devtools/open" => ctx.runtime.open_devtools().await,
        "/manager/open" => ctx.runtime.open_manager().await,
        "/backend/status" => ctx.runtime.backend_status().await,
        "/backend/repair" => ctx.runtime.repair_backend().await,
        "/diagnostics/log" => diagnostic_log_value(payload.clone()),
        _ => {
            let _ = crate::diagnostic_log::append_diagnostic_log(
                "bridge.unknown_path",
                json!({
                    "path": path
                }),
            );
            return json!({
                "status": "failed",
                "session_id": "",
                "message": "Unknown bridge path"
            });
        }
    };

    let response = result.unwrap_or_else(|error| failed_from_error(&payload, error));
    let _ = crate::diagnostic_log::append_diagnostic_log(
        "bridge.response",
        json!({
            "path": path,
            "elapsed_ms": started.elapsed().as_millis() as u64,
            "status": response.get("status").and_then(Value::as_str).unwrap_or("")
        }),
    );
    response
}

fn switch_journal() -> crate::switch_journal::SwitchJournal {
    crate::switch_journal::SwitchJournal::new(
        crate::paths::default_app_state_dir().join("switch-journal.json"),
    )
}

fn providerdeck_catalog_value() -> anyhow::Result<Value> {
    Ok(serde_json::to_value(
        crate::provider_catalog::catalog_from_path(
            &crate::paths::default_app_state_dir().join("routing.toml"),
            crate::ports::active_helper_port(),
            crate::local_auth::runtime_bearer_token(),
        )?,
    )?)
}

fn save_switch_journal(payload: Value) -> anyhow::Result<Value> {
    switch_journal().save_value(&payload)?;
    Ok(json!({ "status": "ok" }))
}

fn load_switch_journal() -> anyhow::Result<Value> {
    Ok(json!({
        "status": "ok",
        "record": switch_journal().load_value()?
    }))
}

fn clear_switch_journal() -> anyhow::Result<Value> {
    switch_journal().clear()?;
    Ok(json!({ "status": "ok" }))
}

fn thread_history_safety_value(payload: &Value) -> anyhow::Result<Value> {
    let thread_id = payload
        .get("threadId")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("threadId is required"))?;
    let (rollout_found, safety) = crate::thread_history::thread_history_safety(thread_id)?;
    Ok(json!({
        "status": "ok",
        "rolloutFound": rollout_found,
        "requiresCompaction": safety.requires_compaction,
        "model": safety.model,
    }))
}

#[derive(Default)]
pub struct CoreSettingsService {
    store: SettingsStore,
}

#[async_trait]
impl BridgeSettingsService for CoreSettingsService {
    async fn get_settings(&self) -> anyhow::Result<BackendSettings> {
        self.store.load()
    }

    async fn set_settings(&self, payload: Value) -> anyhow::Result<BackendSettings> {
        self.store.update(payload)
    }
}

#[derive(Clone)]
pub struct CoreRuntimeService {
    debug_port: u16,
    status_store: StatusStore,
    devtools_opener: Option<DevtoolsOpener>,
    devtools_target_id: Option<String>,
}

impl CoreRuntimeService {
    pub fn new(debug_port: u16, status_store: StatusStore) -> Self {
        Self {
            debug_port,
            status_store,
            devtools_opener: None,
            devtools_target_id: None,
        }
    }

    pub fn with_devtools_opener(mut self, opener: DevtoolsOpener) -> Self {
        self.devtools_opener = Some(opener);
        self
    }

    pub fn with_devtools_target_id(mut self, target_id: impl Into<String>) -> Self {
        self.devtools_target_id = Some(target_id.into());
        self
    }
}

#[async_trait]
impl BridgeRuntimeService for CoreRuntimeService {
    async fn open_devtools(&self) -> anyhow::Result<Value> {
        let target_id = self
            .devtools_target_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No DevTools target configured"))?;
        let url = devtools_url(self.debug_port, target_id);
        if let Some(opener) = &self.devtools_opener {
            opener(&url)?;
        }
        Ok(json!({
            "status": "ok",
            "target_id": target_id,
            "url": url
        }))
    }

    async fn open_manager(&self) -> anyhow::Result<Value> {
        let manager_path = manager_exe_path();
        if !manager_path.exists() {
            anyhow::bail!("未找到管理工具：{}", manager_path.display());
        }
        spawn_manager(&manager_path)?;
        Ok(json!({
            "status": "ok",
            "path": manager_path.to_string_lossy()
        }))
    }

    async fn backend_status(&self) -> anyhow::Result<Value> {
        let _ = self.status_store.load_latest();
        let _ = crate::diagnostic_log::append_diagnostic_log(
            "bridge.backend_status_ok",
            json!({
                "debug_port": self.debug_port,
                "version": crate::version::VERSION
            }),
        );
        Ok(json!({"status": "ok", "message": "后端已连接", "version": crate::version::VERSION}))
    }

    async fn repair_backend(&self) -> anyhow::Result<Value> {
        self.backend_status().await
    }
}

fn manager_exe_path() -> PathBuf {
    crate::install::option_or_current_exe(&None, crate::install::MANAGER_BINARY)
}

fn spawn_manager(manager_path: &Path) -> anyhow::Result<()> {
    // 检查是否已有实例在运行（通过检测单实例保护端口）
    if crate::ports::is_port_in_use(crate::ports::MANAGER_GUARD_PORT) {
        // 已有实例在运行，使用 macOS open 命令激活窗口
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("open")
                .arg("-a")
                .arg("ProviderDeck")
                .spawn();
            return Ok(());
        }
        // Windows: 尝试启动新实例（会自动激活已有窗口）
        #[cfg(windows)]
        {
            let mut command = std::process::Command::new(manager_path);
            command.creation_flags(crate::windows_create_no_window());
            let _ = command.spawn();
            return Ok(());
        }
    }
    let mut command = std::process::Command::new(manager_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::windows_create_no_window());
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("启动管理工具失败：{error}"))
}

fn settings_value(result: anyhow::Result<BackendSettings>) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(result?)?)
}

fn diagnostic_log_value(payload: Value) -> anyhow::Result<Value> {
    let event = payload
        .get("event")
        .and_then(Value::as_str)
        .map(sanitize_diagnostic_event)
        .unwrap_or_else(|| "event".to_string());
    crate::diagnostic_log::append_diagnostic_log(&format!("renderer.{event}"), payload)?;
    Ok(json!({
        "status": "ok",
        "message": "日志已记录"
    }))
}

fn sanitize_diagnostic_event(event: &str) -> String {
    let sanitized = event
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "event".to_string()
    } else {
        sanitized
    }
}

fn failed_from_error(payload: &Value, error: anyhow::Error) -> Value {
    json!({
        "status": "failed",
        "session_id": payload
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        "message": error.to_string()
    })
}

pub fn devtools_url(debug_port: u16, target_id: &str) -> String {
    format!(
        "http://127.0.0.1:{debug_port}/devtools/inspector.html?ws=127.0.0.1:{debug_port}/devtools/page/{target_id}"
    )
}
