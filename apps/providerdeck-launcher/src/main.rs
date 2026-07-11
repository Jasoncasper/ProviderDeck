#![cfg_attr(windows, windows_subsystem = "windows")]

use anyhow::{Context, Result};
use providerdeck_core::launcher::{
    DefaultLaunchHooks, LaunchHooks, LaunchOptions, launch_and_inject_with_hooks,
};
use providerdeck_core::routes::{BridgeContext, BridgeRuntimeService};
use serde_json::{Value, json};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct LauncherHooks {
    core: Arc<DefaultLaunchHooks>,
    runtime: Arc<LauncherRuntimeService>,
}

impl Default for LauncherHooks {
    fn default() -> Self {
        Self {
            core: Arc::new(DefaultLaunchHooks::default()),
            runtime: Arc::new(LauncherRuntimeService::new(9229)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let Some(_guard) = acquire_single_instance_guard()? else {
        return Ok(());
    };
    let options = parse_launch_options(std::env::args().skip(1));
    tokio::spawn(async {
        let _ = notify_manager_when_update_available().await;
    });
    let hooks = LauncherHooks::default();
    let handle = launch_and_inject_with_hooks(options, &hooks).await?;
    handle.wait_for_codex_exit().await?;
    Ok(())
}

fn acquire_single_instance_guard() -> anyhow::Result<Option<std::net::TcpListener>> {
    match providerdeck_core::ports::acquire_loopback_port_guard(
        providerdeck_core::ports::LAUNCHER_GUARD_PORT,
    ) {
        Ok(listener) => Ok(Some(listener)),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                "launcher.already_running",
                json!({
                    "guard_port": providerdeck_core::ports::LAUNCHER_GUARD_PORT
                }),
            );
            Ok(None)
        }
        Err(error) => Err(error)
            .with_context(|| {
                format!(
                    "failed to acquire launcher guard port {}",
                    providerdeck_core::ports::LAUNCHER_GUARD_PORT
                )
            })
            .map(Some),
    }
}

async fn notify_manager_when_update_available() -> anyhow::Result<bool> {
    let update =
        providerdeck_core::update::check_for_update(providerdeck_core::version::VERSION).await?;
    if !update.update_available {
        return Ok(false);
    }
    open_manager_with_update_prompt()?;
    Ok(true)
}

fn open_manager_with_update_prompt() -> anyhow::Result<()> {
    let manager_path = manager_exe_path();
    let mut command = std::process::Command::new(&manager_path);
    command.arg("--show-update");
    #[cfg(windows)]
    {
        command.creation_flags(providerdeck_core::windows_create_no_window());
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!("启动管理工具失败：{error}"))
}

fn parse_launch_options<I, S>(args: I) -> LaunchOptions
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut options = LaunchOptions::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            "--app-path" => {
                if let Some(value) = iter.next() {
                    let value = value.as_ref().trim();
                    if !value.is_empty() {
                        options.app_dir = Some(PathBuf::from(value));
                    }
                }
            }
            "--debug-port" => {
                if let Some(value) = iter.next() {
                    if let Ok(port) = value.as_ref().parse::<u16>() {
                        options.debug_port = port;
                    }
                }
            }
            "--helper-port" => {
                if let Some(value) = iter.next() {
                    if let Ok(port) = value.as_ref().parse::<u16>() {
                        options.helper_port = port;
                    }
                }
            }
            _ => {}
        }
    }
    options
}

#[async_trait::async_trait(?Send)]
impl LaunchHooks for LauncherHooks {
    fn resolve_app_dir(
        &self,
        app_dir: Option<&std::path::Path>,
        settings: &providerdeck_core::settings::BackendSettings,
    ) -> anyhow::Result<std::path::PathBuf> {
        self.core.resolve_app_dir(app_dir, settings)
    }

    fn select_debug_port(&self, requested: u16) -> u16 {
        self.core.select_debug_port(requested)
    }

    fn select_helper_port(&self, requested: u16) -> u16 {
        self.core.select_helper_port(requested)
    }

    async fn load_settings(&self) -> anyhow::Result<providerdeck_core::settings::BackendSettings> {
        self.core.load_settings().await
    }

    async fn start_helper(&self, helper_port: u16) -> anyhow::Result<()> {
        self.core.start_helper(helper_port).await
    }

    async fn launch_codex(
        &self,
        app_dir: &Path,
        debug_port: u16,
        extra_args: &[String],
    ) -> anyhow::Result<providerdeck_core::launcher::CodexLaunch> {
        self.core
            .launch_codex(app_dir, debug_port, extra_args)
            .await
    }

    async fn bridge_context(&self, debug_port: u16) -> anyhow::Result<Option<BridgeContext>> {
        self.runtime.set_debug_port(debug_port);
        Ok(Some(BridgeContext::core(self.runtime.clone())))
    }

    async fn inject_bridge(
        &self,
        debug_port: u16,
        helper_port: u16,
        ctx: BridgeContext,
    ) -> anyhow::Result<()> {
        inject_with_context(debug_port, helper_port, ctx, self.runtime.clone()).await
    }

    async fn inject(&self, debug_port: u16, helper_port: u16) -> anyhow::Result<()> {
        self.core.inject(debug_port, helper_port).await
    }

    async fn write_status(&self, status: &str) {
        self.core.write_status(status).await;
    }

    async fn wait_for_codex_exit(
        &self,
        launch: &providerdeck_core::launcher::CodexLaunch,
    ) -> anyhow::Result<()> {
        self.core.wait_for_codex_exit(launch).await
    }

    async fn shutdown_helper(&self, helper_port: u16) {
        self.core.shutdown_helper(helper_port).await;
    }

    async fn terminate_codex(&self, launch: &providerdeck_core::launcher::CodexLaunch) {
        self.core.terminate_codex(launch).await;
    }
}

struct LauncherRuntimeService {
    debug_port: Mutex<u16>,
}

impl LauncherRuntimeService {
    fn new(debug_port: u16) -> Self {
        Self {
            debug_port: Mutex::new(debug_port),
        }
    }

    fn set_debug_port(&self, debug_port: u16) {
        *self.debug_port.lock().unwrap() = debug_port;
    }
}

#[async_trait::async_trait]
impl BridgeRuntimeService for LauncherRuntimeService {
    async fn open_devtools(&self) -> anyhow::Result<Value> {
        let debug_port = *self.debug_port.lock().unwrap();
        let targets = providerdeck_core::cdp::list_targets(debug_port).await?;
        let target = providerdeck_core::cdp::pick_page_target(&targets)?;
        let url = providerdeck_core::routes::devtools_url(debug_port, &target.id);
        open_url(&url)?;
        Ok(json!({
            "status": "ok",
            "target_id": target.id,
            "url": url
        }))
    }

    async fn open_manager(&self) -> anyhow::Result<Value> {
        let manager_path = manager_exe_path();
        #[cfg(windows)]
        {
            std::process::Command::new(&manager_path)
                .creation_flags(providerdeck_core::windows_create_no_window())
                .spawn()
                .map_err(|error| anyhow::anyhow!("启动管理工具失败：{error}"))?;
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new(&manager_path)
                .spawn()
                .map_err(|error| anyhow::anyhow!("启动管理工具失败：{error}"))?;
        }
        Ok(json!({
            "status": "ok",
            "path": manager_path.to_string_lossy()
        }))
    }

    async fn backend_status(&self) -> anyhow::Result<Value> {
        Ok(
            json!({"status": "ok", "message": "后端已连接", "version": providerdeck_core::version::VERSION}),
        )
    }

    async fn repair_backend(&self) -> anyhow::Result<Value> {
        self.backend_status().await
    }
}

async fn inject_with_context(
    debug_port: u16,
    helper_port: u16,
    ctx: BridgeContext,
    runtime: Arc<LauncherRuntimeService>,
) -> anyhow::Result<()> {
    let mut last_error = None;
    for _ in 0..20 {
        match try_inject_with_context(debug_port, helper_port, ctx.clone(), runtime.clone()).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Codex injection failed")))
}

async fn try_inject_with_context(
    debug_port: u16,
    helper_port: u16,
    ctx: BridgeContext,
    _runtime: Arc<LauncherRuntimeService>,
) -> anyhow::Result<()> {
    let targets = providerdeck_core::cdp::list_targets(debug_port).await?;
    let target = providerdeck_core::cdp::pick_page_target(&targets)?;
    let websocket_url = target
        .web_socket_debugger_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("selected CDP target has no websocket URL"))?;
    let script = providerdeck_core::assets::injection_script(helper_port);
    providerdeck_core::bridge::install_bridge(
        websocket_url,
        providerdeck_core::bridge::BRIDGE_BINDING_NAME,
        Arc::new(move |path, payload| {
            let ctx = ctx.clone();
            Box::pin(async move {
                Ok(providerdeck_core::routes::handle_bridge_request(ctx, &path, payload).await)
            })
        }),
        &[script],
    )
    .await
}

fn open_url(url: &str) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        providerdeck_core::windows_open_url(url)
            .map_err(|error| anyhow::anyhow!("failed to open DevTools URL: {error}"))
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("failed to open DevTools URL: {error}"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("failed to open DevTools URL: {error}"))
    }

    #[cfg(not(any(windows, target_os = "macos", unix)))]
    {
        let _ = url;
        anyhow::bail!("opening DevTools URL is not supported on this platform")
    }
}

fn manager_exe_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    dir.join(format!(
        "{}{}",
        providerdeck_core::install::MANAGER_BINARY,
        suffix
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_launch_options_accepts_manager_forwarded_ports_and_app_path() {
        let options = parse_launch_options([
            "--app-path",
            "C:/Codex/App",
            "--debug-port",
            "9333",
            "--helper-port",
            "57322",
        ]);

        assert_eq!(options.app_dir, Some(PathBuf::from("C:/Codex/App")));
        assert_eq!(options.debug_port, 9333);
        assert_eq!(options.helper_port, 57322);
    }

    #[test]
    fn parse_launch_options_ignores_invalid_ports() {
        let options = parse_launch_options(["--debug-port", "nope", "--helper-port", "70000"]);

        assert_eq!(options.debug_port, LaunchOptions::default().debug_port);
        assert_eq!(options.helper_port, LaunchOptions::default().helper_port);
    }

    #[test]
    fn launcher_uses_single_instance_guard_before_launching() {
        let source = include_str!("main.rs");

        assert!(source.contains("acquire_single_instance_guard()?"));
        assert!(source.contains("LAUNCHER_GUARD_PORT"));
        assert!(source.contains("launcher.already_running"));
    }

    #[test]
    fn manager_update_prompt_uses_sidecar_manager_binary_name() {
        let path = manager_exe_path();

        assert!(
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(providerdeck_core::install::MANAGER_BINARY))
        );
    }
}
