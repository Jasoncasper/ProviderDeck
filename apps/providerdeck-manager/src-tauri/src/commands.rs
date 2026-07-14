use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use providerdeck_core::install::SILENT_BINARY;
use providerdeck_core::settings::SettingsStore;
use providerdeck_core::status::{LaunchStatus, StatusStore};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use crate::install;

#[derive(Debug, Clone)]
pub struct CommandResult<T>
where
    T: Serialize,
{
    pub status: String,
    pub message: String,
    pub payload: T,
}

impl<T: Serialize> Serialize for CommandResult<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut object =
            match serde_json::to_value(&self.payload).map_err(serde::ser::Error::custom)? {
                Value::Object(object) => object,
                value => {
                    let mut object = serde_json::Map::new();
                    object.insert("payload".to_string(), value);
                    object
                }
            };
        object.insert("status".to_string(), Value::String(self.status.clone()));
        object.insert("message".to_string(), Value::String(self.message.clone()));
        if self.status == "failed" {
            object
                .entry("errorCode".to_string())
                .or_insert_with(|| Value::String("command_failed".to_string()));
            object
                .entry("rolledBack".to_string())
                .or_insert(Value::Bool(false));
            object
                .entry("recoveryRequired".to_string())
                .or_insert(Value::Bool(false));
        }
        Value::Object(object).serialize(serializer)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionPayload {
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathState {
    pub status: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewPayload {
    pub codex_app: PathState,
    pub codex_version: Option<String>,
    pub silent_shortcut: PathState,
    pub management_shortcut: PathState,
    pub latest_launch: Option<LaunchStatus>,
    pub current_version: String,
    pub update_status: String,
    pub settings_path: String,
    pub logs_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayProfileTestPayload {
    pub http_status: u16,
    pub endpoint: String,
    pub response_preview: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    #[serde(default)]
    pub app_path: String,
    #[serde(default = "default_debug_port")]
    pub debug_port: u16,
    #[serde(default = "default_helper_port")]
    pub helper_port: u16,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRequest {
    #[serde(default = "default_log_lines")]
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogsPayload {
    pub path: String,
    pub text: String,
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticsPayload {
    pub report: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupPayload {
    pub show_update: bool,
}

#[tauri::command]
pub fn backend_version() -> CommandResult<VersionPayload> {
    ok(
        "后端版本已读取。",
        VersionPayload {
            version: providerdeck_core::version::VERSION.to_string(),
        },
    )
}

#[tauri::command]
pub fn startup_options() -> CommandResult<StartupPayload> {
    ok(
        "启动参数已读取。",
        StartupPayload {
            show_update: startup_should_show_update(),
        },
    )
}

pub fn startup_should_show_update() -> bool {
    should_show_update(
        std::env::args(),
        std::env::var("PROVIDERDECK_SHOW_UPDATE").ok().as_deref(),
    )
}

fn should_show_update<I, S>(args: I, env_value: Option<&str>) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--show-update") || env_value == Some("1")
}

#[tauri::command]
pub async fn load_overview() -> CommandResult<OverviewPayload> {
    let payload = tauri::async_runtime::spawn_blocking(load_overview_payload).await;
    let Ok((codex_app_path, entrypoints, latest_launch)) = payload else {
        return failed(
            "概览后台任务失败。",
            OverviewPayload {
                codex_app: path_state(None),
                codex_version: None,
                silent_shortcut: path_state(None),
                management_shortcut: path_state(None),
                latest_launch: None,
                current_version: providerdeck_core::version::VERSION.to_string(),
                update_status: "not_checked".to_string(),
                settings_path: providerdeck_core::paths::default_settings_path()
                    .to_string_lossy()
                    .to_string(),
                logs_path: providerdeck_core::paths::default_diagnostic_log_path()
                    .to_string_lossy()
                    .to_string(),
            },
        );
    };
    ok(
        "概览已加载。",
        OverviewPayload {
            codex_version: codex_app_path
                .as_deref()
                .and_then(providerdeck_core::app_paths::codex_app_version),
            codex_app: path_state(codex_app_path),
            silent_shortcut: shortcut_state(entrypoints.silent_shortcut),
            management_shortcut: shortcut_state(entrypoints.management_shortcut),
            latest_launch,
            current_version: providerdeck_core::version::VERSION.to_string(),
            update_status: "not_checked".to_string(),
            settings_path: providerdeck_core::paths::default_settings_path()
                .to_string_lossy()
                .to_string(),
            logs_path: providerdeck_core::paths::default_diagnostic_log_path()
                .to_string_lossy()
                .to_string(),
        },
    )
}

#[tauri::command]
pub fn launch_providerdeck(request: LaunchRequest) -> CommandResult<Value> {
    spawn_providerdeck_launch(request, "已在后台开始，可稍后查看概览状态。")
}

#[tauri::command]
pub fn restart_providerdeck(request: LaunchRequest) -> CommandResult<Value> {
    providerdeck_core::watcher::stop_launcher_processes();
    if !providerdeck_core::watcher::stop_codex_processes() {
        return failed(
            "ChatGPT 尚未完全退出，请稍后重试。",
            json!({
                "debugPort": request.debug_port,
                "helperPort": request.helper_port
            }),
        );
    }
    spawn_providerdeck_launch(request, "ChatGPT 已请求重启，启动任务正在后台运行。")
}

fn spawn_providerdeck_launch(
    request: LaunchRequest,
    accepted_message: &str,
) -> CommandResult<Value> {
    let debug_port = request.debug_port;
    let helper_port = request.helper_port;
    // 检查端口是否已被占用（防止多个同类应用同时运行）
    let addr = format!("127.0.0.1:{debug_port}");
    if let Ok(addr) = addr.parse() {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok() {
            return failed(
                "端口被占用，请先关闭开启的其他同类效果应用",
                json!({ "debugPort": debug_port, "helperPort": helper_port }),
            );
        }
    }
    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
        "manager.launch_requested",
        json!({
            "debug_port": debug_port,
            "helper_port": helper_port,
            "app_path": request.app_path.trim()
        }),
    );
    match spawn_silent_launcher(&request) {
        Ok(()) => CommandResult {
            status: "ok".to_string(),
            message: accepted_message.to_string(),
            payload: json!({
                "debugPort": debug_port,
                "helperPort": helper_port
            }),
        },
        Err(error) => failed(
            &format!("启动静默入口失败：{error}"),
            json!({
                "debugPort": debug_port,
                "helperPort": helper_port
            }),
        ),
    }
}

fn spawn_silent_launcher(request: &LaunchRequest) -> anyhow::Result<()> {
    let launcher = providerdeck_core::install::companion_binary_path(SILENT_BINARY);
    let mut command = std::process::Command::new(&launcher);
    if !request.app_path.trim().is_empty() {
        command.arg("--app-path").arg(request.app_path.trim());
    }
    command
        .arg("--debug-port")
        .arg(request.debug_port.to_string())
        .arg("--helper-port")
        .arg(request.helper_port.to_string());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("无法启动 {}：{error}", launcher.to_string_lossy()))
}

#[tauri::command]
pub fn open_external_url(url: String) -> CommandResult<Value> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return failed("只允许打开 http 或 https 链接。", json!({}));
    }
    match open_url(trimmed) {
        Ok(()) => ok("已在系统浏览器打开链接。", json!({ "url": trimmed })),
        Err(error) => failed(&format!("打开链接失败：{error}"), json!({ "url": trimmed })),
    }
}

#[tauri::command]
pub async fn check_update() -> CommandResult<Value> {
    match providerdeck_core::update::check_for_update(providerdeck_core::version::VERSION).await {
        Ok(update) => {
            let status = if update.update_available {
                "ok"
            } else {
                "not_checked"
            };
            CommandResult {
                status: status.to_string(),
                message: if update.update_available {
                    "发现可用更新。".to_string()
                } else {
                    "当前已是最新版本。".to_string()
                },
                payload: json!({
                    "currentVersion": update.current_version,
                    "latestVersion": update.latest_version,
                    "releaseSummary": update.release_summary,
                    "assetName": update.asset_name,
                    "assetUrl": update.asset_url,
                    "updateAvailable": update.update_available,
                    "progress": 0
                }),
            }
        }
        Err(error) => failed(
            &format!("检查更新失败：{error}"),
            json!({
                "currentVersion": providerdeck_core::version::VERSION,
                "latestVersion": Value::Null,
                "releaseSummary": "",
                "assetName": Value::Null,
                "assetUrl": Value::Null,
                "updateAvailable": false,
                "progress": 0
            }),
        ),
    }
}

#[tauri::command]
pub async fn perform_update(
    release: Option<providerdeck_core::update::Release>,
) -> CommandResult<Value> {
    let Some(release) = release else {
        return failed(
            "请先检查更新并选择可下载的 Release asset。",
            json!({
                "currentVersion": providerdeck_core::version::VERSION,
                "progress": 0
            }),
        );
    };
    let download_dir = providerdeck_core::paths::default_app_state_dir().join("updates");
    match providerdeck_core::update::perform_update(&release, &download_dir).await {
        Ok(result) => ok(
            "安装包已下载并启动，请按安装向导完成更新。",
            json!({
                "currentVersion": providerdeck_core::version::VERSION,
                "latestVersion": result.release.version,
                "releaseSummary": result.release.body,
                "installedPath": result.installer_path.to_string_lossy(),
                "launched": result.launched,
                "progress": 100
            }),
        ),
        Err(error) => failed(
            &format!("安装更新失败：{error}"),
            json!({
                "currentVersion": providerdeck_core::version::VERSION,
                "latestVersion": release.version,
                "releaseSummary": release.body,
                "progress": 0
            }),
        ),
    }
}

#[tauri::command]
pub fn read_latest_logs(request: LogRequest) -> CommandResult<LogsPayload> {
    let path = providerdeck_core::paths::default_diagnostic_log_path();
    match read_tail(&path, request.lines) {
        Ok(text) => ok(
            "日志已读取。",
            LogsPayload {
                path: path.to_string_lossy().to_string(),
                text,
                lines: request.lines,
            },
        ),
        Err(error) => failed(
            &format!("读取日志失败：{error}"),
            LogsPayload {
                path: path.to_string_lossy().to_string(),
                text: String::new(),
                lines: request.lines,
            },
        ),
    }
}

#[tauri::command]
pub fn copy_diagnostics() -> CommandResult<DiagnosticsPayload> {
    ok(
        "诊断报告已生成。",
        DiagnosticsPayload {
            report: diagnostics_report(),
        },
    )
}

#[tauri::command]
pub fn write_diagnostic_event(event: String, detail: Value) -> CommandResult<Value> {
    let event = sanitize_manager_event(&event);
    match providerdeck_core::diagnostic_log::append_diagnostic_log(&event, detail) {
        Ok(()) => ok("诊断日志已写入。", json!({})),
        Err(error) => failed(&format!("写入诊断日志失败：{error}"), json!({})),
    }
}

fn sanitize_manager_event(event: &str) -> String {
    let suffix = event
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let suffix = suffix.trim_matches(['.', '_', '-']).trim();
    if suffix.is_empty() {
        "manager.ui.event".to_string()
    } else if suffix.starts_with("manager.") {
        suffix.to_string()
    } else {
        format!("manager.ui.{suffix}")
    }
}

fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        providerdeck_core::windows_open_url(url)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("启动系统浏览器失败：{error}"))
    }
}

fn diagnostics_report() -> String {
    let (codex_app_path, entrypoints, latest_launch) = load_overview_payload();
    let overview = ok(
        "概览已加载。",
        OverviewPayload {
            codex_version: codex_app_path
                .as_deref()
                .and_then(providerdeck_core::app_paths::codex_app_version),
            codex_app: path_state(codex_app_path),
            silent_shortcut: shortcut_state(entrypoints.silent_shortcut),
            management_shortcut: shortcut_state(entrypoints.management_shortcut),
            latest_launch,
            current_version: providerdeck_core::version::VERSION.to_string(),
            update_status: "not_checked".to_string(),
            settings_path: providerdeck_core::paths::default_settings_path()
                .to_string_lossy()
                .to_string(),
            logs_path: providerdeck_core::paths::default_diagnostic_log_path()
                .to_string_lossy()
                .to_string(),
        },
    );
    let settings = SettingsStore::default().load().unwrap_or_default();
    let generated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    serde_json::to_string_pretty(&json!({
        "generatedAtMs": generated_at_ms,
        "version": providerdeck_core::version::VERSION,
        "overview": overview.payload,
        "settings": settings,
        "logs": {
            "diagnosticLogPath": providerdeck_core::paths::default_diagnostic_log_path(),
            "latestStatusPath": providerdeck_core::paths::default_latest_status_path()
        },
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH
        }
    }))
    .unwrap_or_else(|error| format!("诊断报告序列化失败：{error}"))
}

fn load_overview_payload() -> (
    Option<PathBuf>,
    install::EntryPointState,
    Option<LaunchStatus>,
) {
    let settings = SettingsStore::default().load().unwrap_or_default();
    (
        providerdeck_core::app_paths::resolve_codex_app_dir_with_saved(
            None,
            Some(settings.codex_app_path.as_str()),
        ),
        install::inspect_entrypoints(),
        StatusStore::default().load_latest().unwrap_or(None),
    )
}

fn read_tail(path: &Path, max_lines: usize) -> std::io::Result<String> {
    let contents = fs::read_to_string(path)?;
    let mut lines = contents.lines().rev().take(max_lines).collect::<Vec<_>>();
    lines.reverse();
    Ok(lines.join("\n"))
}

fn path_state(path: Option<PathBuf>) -> PathState {
    match path {
        Some(path) => PathState {
            status: "found".to_string(),
            path: Some(path.to_string_lossy().to_string()),
        },
        None => PathState {
            status: "missing".to_string(),
            path: None,
        },
    }
}

fn shortcut_state(shortcut: install::ShortcutState) -> PathState {
    PathState {
        status: if shortcut.installed {
            "installed".to_string()
        } else {
            "missing".to_string()
        },
        path: shortcut.path,
    }
}

fn ok<T: Serialize>(message: &str, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "ok".to_string(),
        message: message.to_string(),
        payload,
    }
}

fn failed<T: Serialize>(message: &str, payload: T) -> CommandResult<T> {
    CommandResult {
        status: "failed".to_string(),
        message: message.to_string(),
        payload,
    }
}

fn default_debug_port() -> u16 {
    9229
}

fn default_helper_port() -> u16 {
    providerdeck_core::ports::DEFAULT_HELPER_PORT
}

fn default_log_lines() -> usize {
    200
}

// ==================== 智能路由管理命令 ====================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingConfigPayload {
    pub config: providerdeck_core::router::SmartRouterConfig,
    pub config_path: String,
}

fn normalize_routing_config(
    config: providerdeck_core::router::SmartRouterConfig,
) -> providerdeck_core::router::SmartRouterConfig {
    providerdeck_core::router::normalize_router_config(config)
}

#[tauri::command]
pub fn load_routing_config() -> CommandResult<RoutingConfigPayload> {
    let config_path = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    let config = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|c| toml::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        providerdeck_core::router::SmartRouterConfig::default()
    };
    let config = normalize_routing_config(config);
    CommandResult {
        status: "ok".to_string(),
        message: "路由配置加载成功".to_string(),
        payload: RoutingConfigPayload {
            config,
            config_path: config_path.to_string_lossy().to_string(),
        },
    }
}

#[tauri::command]
pub fn save_routing_config(
    mut config: providerdeck_core::router::SmartRouterConfig,
) -> CommandResult<RoutingConfigPayload> {
    let config_path = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    // 保持原有 API key（前端可能发回的是脱敏值）
    if let Ok(old_raw) = std::fs::read_to_string(&config_path) {
        if let Ok(old_config) =
            toml::from_str::<providerdeck_core::router::SmartRouterConfig>(&old_raw)
        {
            for provider in &mut config.providers {
                if let Some(old) = old_config.providers.iter().find(|p| p.id == provider.id) {
                    if provider.api_key
                        == providerdeck_core::router::api_key_masked_str(&old.api_key)
                    {
                        provider.api_key = old.api_key.clone();
                    }
                }
            }
        }
    }
    config = normalize_routing_config(config);
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let toml_content = match toml::to_string_pretty(&config) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                status: "failed".to_string(),
                message: format!("序列化配置失败: {}", e),
                payload: RoutingConfigPayload {
                    config,
                    config_path: config_path.to_string_lossy().to_string(),
                },
            };
        }
    };
    match std::fs::write(&config_path, &toml_content) {
        Ok(_) => CommandResult {
            status: "ok".to_string(),
            message: "路由配置保存成功".to_string(),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        },
        Err(e) => CommandResult {
            status: "failed".to_string(),
            message: format!("保存配置失败: {}", e),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        },
    }
}

#[tauri::command]
pub fn upsert_provider(
    mut provider: providerdeck_core::router::SmartProvider,
) -> CommandResult<RoutingConfigPayload> {
    let config_path = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    let mut config: providerdeck_core::router::SmartRouterConfig = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|c| toml::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        providerdeck_core::router::SmartRouterConfig::default()
    };
    config = normalize_routing_config(config);
    if provider.builtin || provider.id == "openai" {
        return CommandResult {
            status: "failed".to_string(),
            message: "内置 provider 不可编辑".to_string(),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        };
    }
    if let Some(existing) = config.providers.iter_mut().find(|p| p.id == provider.id) {
        // 保持原有 API key（前端可能发回的是脱敏值）
        let incoming_key = std::mem::replace(&mut provider.api_key, String::new());
        if incoming_key == providerdeck_core::router::api_key_masked_str(&existing.api_key) {
            provider.api_key = existing.api_key.clone();
        } else {
            provider.api_key = incoming_key;
        }
        *existing = provider;
    } else {
        config.providers.push(provider);
    }
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let toml_content = match toml::to_string_pretty(&config) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                status: "failed".to_string(),
                message: format!("序列化配置失败: {}", e),
                payload: RoutingConfigPayload {
                    config,
                    config_path: config_path.to_string_lossy().to_string(),
                },
            };
        }
    };
    match std::fs::write(&config_path, &toml_content) {
        Ok(_) => CommandResult {
            status: "ok".to_string(),
            message: "模型保存成功".to_string(),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        },
        Err(e) => CommandResult {
            status: "failed".to_string(),
            message: format!("保存配置失败: {}", e),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        },
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProviderRequest {
    provider_id: String,
}

#[tauri::command]
pub fn delete_provider(request: DeleteProviderRequest) -> CommandResult<RoutingConfigPayload> {
    let provider_id = request.provider_id;
    let config_path = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    let mut config: providerdeck_core::router::SmartRouterConfig = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|c| toml::from_str(&c).ok())
            .unwrap_or_default()
    } else {
        providerdeck_core::router::SmartRouterConfig::default()
    };
    config = normalize_routing_config(config);
    if provider_id == "openai" {
        return CommandResult {
            status: "failed".to_string(),
            message: "内置 provider 不可删除".to_string(),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        };
    }
    config.providers.retain(|p| p.id != provider_id);
    if config.vision_fallback_model == provider_id {
        config.vision_fallback_model = String::new();
    }
    if config.fallback_provider == provider_id {
        config.fallback_provider = "openai".to_string();
    }
    config = normalize_routing_config(config);
    if let Some(parent) = config_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let toml_content = match toml::to_string_pretty(&config) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult {
                status: "failed".to_string(),
                message: format!("序列化配置失败: {}", e),
                payload: RoutingConfigPayload {
                    config,
                    config_path: config_path.to_string_lossy().to_string(),
                },
            };
        }
    };
    match std::fs::write(&config_path, &toml_content) {
        Ok(_) => CommandResult {
            status: "ok".to_string(),
            message: "模型已删除".to_string(),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        },
        Err(e) => CommandResult {
            status: "failed".to_string(),
            message: format!("删除配置失败: {}", e),
            payload: RoutingConfigPayload {
                config,
                config_path: config_path.to_string_lossy().to_string(),
            },
        },
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusPayload {
    pub app_server_connected: bool,
    pub bridge_injected: bool,
    pub helper_healthy: bool,
    pub helper_port: u16,
    pub connected_threads: usize,
    pub switch_phase: String,
}

fn switch_journal() -> providerdeck_core::switch_journal::SwitchJournal {
    providerdeck_core::switch_journal::SwitchJournal::new(
        providerdeck_core::paths::default_app_state_dir().join("switch-journal.json"),
    )
}

#[tauri::command]
pub fn load_provider_catalog() -> CommandResult<Value> {
    let path = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    match providerdeck_core::provider_catalog::catalog_from_path(
        &path,
        providerdeck_core::ports::active_helper_port(),
        providerdeck_core::local_auth::runtime_bearer_token(),
    ) {
        Ok(mut catalog) => {
            for provider in catalog.providers.values_mut() {
                provider.bearer_token.clear();
            }
            ok("Provider catalog 已加载。", json!({ "catalog": catalog }))
        }
        Err(error) => failed(
            &format!("Provider catalog 加载失败：{error}"),
            json!({
                "errorCode": "catalog_load_failed",
                "rolledBack": false,
                "recoveryRequired": false
            }),
        ),
    }
}

#[tauri::command]
pub fn load_provider_config() -> CommandResult<RoutingConfigPayload> {
    load_routing_config()
}

#[tauri::command]
pub fn save_provider_config(
    config: providerdeck_core::router::SmartRouterConfig,
) -> CommandResult<RoutingConfigPayload> {
    save_routing_config(config)
}

#[tauri::command]
pub fn load_runtime_status() -> CommandResult<RuntimeStatusPayload> {
    let latest = StatusStore::default().load_latest().ok().flatten();
    let debug_port = latest
        .as_ref()
        .and_then(|status| status.debug_port)
        .unwrap_or(9229);
    let helper_port = latest
        .as_ref()
        .and_then(|status| status.helper_port)
        .unwrap_or_else(providerdeck_core::ports::active_helper_port);
    let app_server_connected = TcpStream::connect_timeout(
        &format!("127.0.0.1:{debug_port}")
            .parse()
            .expect("valid loopback address"),
        Duration::from_millis(250),
    )
    .is_ok();
    let helper_healthy = TcpStream::connect_timeout(
        &format!("127.0.0.1:{helper_port}")
            .parse()
            .expect("valid loopback address"),
        Duration::from_millis(250),
    )
    .is_ok();
    let switch_phase = switch_journal()
        .load_value()
        .ok()
        .flatten()
        .and_then(|value| {
            value
                .get("phase")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "stable".to_string());
    ok(
        "Runtime 状态已加载。",
        RuntimeStatusPayload {
            app_server_connected,
            bridge_injected: app_server_connected,
            helper_healthy,
            helper_port,
            connected_threads: usize::from(switch_phase != "stable"),
            switch_phase,
        },
    )
}

#[tauri::command]
pub fn load_switch_journal() -> CommandResult<Value> {
    match switch_journal().load_value() {
        Ok(record) => ok("切换 journal 已加载。", json!({ "record": record })),
        Err(error) => failed(
            &format!("切换 journal 读取失败：{error}"),
            json!({
                "record": null,
                "errorCode": "journal_load_failed",
                "rolledBack": false,
                "recoveryRequired": true
            }),
        ),
    }
}

#[tauri::command]
pub async fn recover_thread_runtime() -> CommandResult<Value> {
    let latest = StatusStore::default().load_latest().ok().flatten();
    let debug_port = latest
        .as_ref()
        .and_then(|status| status.debug_port)
        .unwrap_or(9229);
    let helper_port = latest
        .as_ref()
        .and_then(|status| status.helper_port)
        .unwrap_or_else(providerdeck_core::ports::active_helper_port);
    providerdeck_core::watcher::stop_launcher_processes();
    providerdeck_core::watcher::stop_codex_processes();
    std::thread::sleep(Duration::from_millis(500));
    let result = spawn_providerdeck_launch(
        LaunchRequest {
            app_path: String::new(),
            debug_port,
            helper_port,
        },
        "ProviderDeck runtime 已重新启动；原任务重新打开后可再次选择模型。",
    );
    if result.status == "ok" {
        let _ = switch_journal().clear();
    }
    result
}

#[tauri::command]
pub fn import_codexmate_config() -> CommandResult<Value> {
    let source = directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".codex-session-delete"))
        .unwrap_or_else(|| PathBuf::from(".codex-session-delete"));
    let destination = providerdeck_core::paths::default_app_state_dir();
    match providerdeck_core::legacy_import::import_codexmate_config(&source, &destination) {
        Ok(result) => ok(
            "CodexMate provider 配置导入完成。",
            json!({ "imported": result.imported, "skipped": result.skipped }),
        ),
        Err(error) => failed(
            &format!("导入失败，ProviderDeck 默认配置保持不变：{error}"),
            json!({
                "errorCode": "legacy_import_failed",
                "rolledBack": true,
                "recoveryRequired": false
            }),
        ),
    }
}

#[tauri::command]
pub fn safe_exit_providerdeck(app: tauri::AppHandle) -> CommandResult<Value> {
    providerdeck_core::watcher::stop_launcher_processes();
    providerdeck_core::watcher::stop_codex_processes();
    let result = ok("ProviderDeck 已安全退出。", json!({}));
    app.exit(0);
    result
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchModelsPayload {
    pub http_status: u16,
    pub models: Vec<String>,
}

#[tauri::command]
pub async fn fetch_provider_models(
    mut provider: providerdeck_core::router::SmartProvider,
) -> CommandResult<FetchModelsPayload> {
    let base_url = provider.base_url.trim().to_string();
    // 如果前端传来的是脱敏 key，从磁盘配置恢复真实 key
    let config_path_ref = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    if let Ok(raw) = std::fs::read_to_string(&config_path_ref) {
        if let Ok(stored) = toml::from_str::<providerdeck_core::router::SmartRouterConfig>(&raw) {
            if let Some(existing) = stored.providers.iter().find(|p| p.id == provider.id) {
                if provider.api_key
                    == providerdeck_core::router::api_key_masked_str(&existing.api_key)
                {
                    provider.api_key = existing.api_key.clone();
                }
            }
        }
    }
    let api_key = provider.api_key.trim().to_string();
    if base_url.is_empty() {
        return CommandResult {
            status: "failed".to_string(),
            message: "Base URL 不能为空".to_string(),
            payload: FetchModelsPayload {
                http_status: 0,
                models: vec![],
            },
        };
    }
    let models_url =
        providerdeck_core::protocol_proxy::models_url_with(&base_url, provider.use_full_url);
    let user_agent = if provider.user_agent.trim().is_empty() {
        format!("ProviderDeck/{}", providerdeck_core::version::VERSION)
    } else {
        provider.user_agent.trim().to_string()
    };
    let client = reqwest::Client::new();
    match client
        .get(&models_url)
        .bearer_auth(&api_key)
        .header(reqwest::header::USER_AGENT, user_agent)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            if (200..300).contains(&status) {
                let models: Vec<String> = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v.get("data").cloned())
                    .and_then(|data| serde_json::from_value::<Vec<serde_json::Value>>(data).ok())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                item.get("id").and_then(|id| id.as_str()).map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                CommandResult {
                    status: "ok".to_string(),
                    message: format!("拉取到 {} 个模型", models.len()),
                    payload: FetchModelsPayload {
                        http_status: status,
                        models,
                    },
                }
            } else {
                CommandResult {
                    status: "failed".to_string(),
                    message: format!("上游返回 HTTP {}，请检查 Base URL 和 API Key", status),
                    payload: FetchModelsPayload {
                        http_status: status,
                        models: vec![],
                    },
                }
            }
        }
        Err(e) => CommandResult {
            status: "failed".to_string(),
            message: format!("请求失败: {}", e),
            payload: FetchModelsPayload {
                http_status: 0,
                models: vec![],
            },
        },
    }
}
#[tauri::command]
pub async fn test_smart_provider(
    mut provider: providerdeck_core::router::SmartProvider,
) -> CommandResult<RelayProfileTestPayload> {
    let base_url = provider.base_url.trim().to_string();
    // 如果前端传来的是脱敏 key，从磁盘配置恢复真实 key
    let config_path_ref = providerdeck_core::paths::default_app_state_dir().join("routing.toml");
    if let Ok(raw) = std::fs::read_to_string(&config_path_ref) {
        if let Ok(stored) = toml::from_str::<providerdeck_core::router::SmartRouterConfig>(&raw) {
            if let Some(existing) = stored.providers.iter().find(|p| p.id == provider.id) {
                if provider.api_key
                    == providerdeck_core::router::api_key_masked_str(&existing.api_key)
                {
                    provider.api_key = existing.api_key.clone();
                }
            }
        }
    }
    let api_key = provider.api_key.trim().to_string();
    if base_url.is_empty() {
        return CommandResult {
            status: "failed".to_string(),
            message: "Base URL 不能为空".to_string(),
            payload: RelayProfileTestPayload {
                http_status: 0,
                endpoint: String::new(),
                response_preview: String::new(),
            },
        };
    }
    let user_agent = if provider.user_agent.trim().is_empty() {
        format!("ProviderDeck/{}", providerdeck_core::version::VERSION)
    } else {
        provider.user_agent.trim().to_string()
    };
    let client = reqwest::Client::new();
    use providerdeck_core::router::ProviderProtocol;
    // ChatCompletions 协议用轻量 chat completions 请求探活，避免依赖自建网关不一定提供的 /models 端点；
    // Responses / Anthropic / Custom 等标准 API 通常提供 /models 端点，保持 GET 探活。
    let (test_url, request_builder) = match provider.protocol {
        ProviderProtocol::ChatCompletions => {
            let url = providerdeck_core::protocol_proxy::chat_completions_url_with(
                &base_url,
                provider.use_full_url,
            );
            let model = if provider.target_model.trim().is_empty() {
                provider.id.clone()
            } else {
                provider.target_model.trim().to_string()
            };
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role":"user","content":"ping"}],
                "max_tokens": 1,
                "stream": false
            });
            let builder = client
                .post(&url)
                .bearer_auth(&api_key)
                .header(reqwest::header::USER_AGENT, &user_agent)
                .json(&body)
                .timeout(std::time::Duration::from_secs(15));
            (url, builder)
        }
        _ => {
            let url = providerdeck_core::protocol_proxy::models_url_with(
                &base_url,
                provider.use_full_url,
            );
            let builder = client
                .get(&url)
                .bearer_auth(&api_key)
                .header(reqwest::header::USER_AGENT, &user_agent)
                .timeout(std::time::Duration::from_secs(10));
            (url, builder)
        }
    };
    match request_builder.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            let preview = if body.len() > 500 {
                format!("{}...", &body[..500])
            } else {
                body
            };
            CommandResult {
                status: if (200..400).contains(&status) {
                    "ok"
                } else {
                    "failed"
                }
                .to_string(),
                message: format!(
                    "HTTP {} - {}",
                    status,
                    if (200..400).contains(&status) {
                        "连接成功"
                    } else {
                        "连接失败"
                    }
                ),
                payload: RelayProfileTestPayload {
                    http_status: status,
                    endpoint: test_url,
                    response_preview: preview,
                },
            }
        }
        Err(e) => CommandResult {
            status: "failed".to_string(),
            message: format!("连接失败: {}", e),
            payload: RelayProfileTestPayload {
                http_status: 0,
                endpoint: test_url,
                response_preview: String::new(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_version_returns_structured_payload() {
        let result = backend_version();

        assert_eq!(result.status, "ok");
        assert!(!result.payload.version.is_empty());
    }

    #[test]
    fn startup_options_returns_structured_payload() {
        let result = startup_options();

        assert_eq!(result.status, "ok");
    }

    #[test]
    fn startup_options_honors_show_update_environment() {
        unsafe {
            std::env::set_var("PROVIDERDECK_SHOW_UPDATE", "1");
        }

        let result = startup_options();

        unsafe {
            std::env::remove_var("PROVIDERDECK_SHOW_UPDATE");
        }

        assert_eq!(result.status, "ok");
        assert!(result.payload.show_update);
    }

    #[test]
    fn startup_options_honors_show_update_argument() {
        assert!(should_show_update(
            ["providerdeck-manager.exe", "--show-update"],
            None
        ));
    }

    #[test]
    fn overview_contains_expected_operational_fields() {
        let result = tauri::async_runtime::block_on(load_overview());

        assert_eq!(result.status, "ok");
        assert!(!result.payload.current_version.is_empty());
        assert!(
            result.payload.codex_version.is_none()
                || result
                    .payload
                    .codex_version
                    .as_deref()
                    .is_some_and(|version| !version.is_empty())
        );
        assert!(matches!(
            result.payload.codex_app.status.as_str(),
            "found" | "missing"
        ));
        assert!(matches!(
            result.payload.silent_shortcut.status.as_str(),
            "installed" | "missing"
        ));
    }

    #[test]
    fn update_install_requires_release_payload() {
        let result = tauri::async_runtime::block_on(perform_update(None));

        assert_eq!(result.status, "failed");
        assert!(result.message.contains("请先检查更新"));
    }

    #[test]
    fn missing_logs_return_failed_status() {
        let result = read_latest_logs(LogRequest { lines: 25 });

        if result.payload.text.is_empty() {
            assert_eq!(result.status, "failed");
        }
    }

    #[test]
    fn open_external_url_rejects_non_http_urls() {
        let result = open_external_url("file:///C:/Windows/win.ini".to_string());

        assert_eq!(result.status, "failed");
        assert!(result.message.contains("只允许打开 http 或 https 链接"));
    }

    #[test]
    fn failed_commands_always_include_recovery_metadata() {
        let value = serde_json::to_value(failed("failed", json!({}))).unwrap();
        assert_eq!(value["errorCode"], "command_failed");
        assert_eq!(value["rolledBack"], false);
        assert_eq!(value["recoveryRequired"], false);
    }

    #[test]
    fn restart_stops_when_chatgpt_does_not_exit() {
        let source = include_str!("commands.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();

        assert!(production.contains("if !providerdeck_core::watcher::stop_codex_processes()"));
        assert!(production.contains("ChatGPT 尚未完全退出"));
    }
}
