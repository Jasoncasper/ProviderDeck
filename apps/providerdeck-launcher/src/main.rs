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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let options = parse_launch_options(args.iter());
    if is_watcher_mode(&args) {
        return run_watcher(options).await;
    }
    let Some(_guard) = acquire_single_instance_guard()? else {
        return Ok(());
    };
    tokio::spawn(async {
        let _ = notify_manager_when_update_available().await;
    });
    let hooks = LauncherHooks::default();
    let handle = launch_and_inject_with_hooks(options, &hooks).await?;
    handle.wait_for_codex_exit().await?;
    Ok(())
}

fn is_watcher_mode<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--watch")
}

#[cfg(target_os = "macos")]
async fn run_watcher(options: LaunchOptions) -> Result<()> {
    let launcher_path = std::env::current_exe().context("failed to resolve watcher executable")?;
    let hooks = LauncherHooks::default();
    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
        "watcher.start",
        json!({
            "debug_port": options.debug_port,
            "helper_port": options.helper_port
        }),
    );
    loop {
        if providerdeck_core::watcher::default_watcher_disabled_flag().exists() {
            return Ok(());
        }
        let chatgpt_processes = providerdeck_core::watcher::find_codex_processes();
        let cdp_ready = providerdeck_core::watcher::cdp_listening(options.debug_port);
        let launcher_running = providerdeck_core::watcher::cdp_listening(
            providerdeck_core::ports::LAUNCHER_GUARD_PORT,
        );
        if providerdeck_core::watcher::should_take_over(
            !chatgpt_processes.is_empty(),
            cdp_ready,
            launcher_running,
        ) {
            let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                "watcher.takeover_requested",
                json!({ "chatgpt_process_count": chatgpt_processes.len() }),
            );
            let outcome = attempt_watcher_takeover(
                || hooks.wait_for_network_ready(),
                || {
                    providerdeck_core::watcher::cdp_listening(
                        providerdeck_core::ports::LAUNCHER_GUARD_PORT,
                    )
                },
                || providerdeck_core::watcher::stop_codex_processes(),
                || spawn_managed_launcher(&launcher_path, &options),
            )
            .await;
            match outcome {
                WatcherTakeoverOutcome::Started => {
                    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                        "watcher.takeover_started",
                        json!({ "debug_port": options.debug_port }),
                    );
                }
                WatcherTakeoverOutcome::NetworkUnavailable(error) => {
                    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                        "watcher.takeover_deferred",
                        json!({ "reason": "network_unavailable", "error": error }),
                    );
                }
                WatcherTakeoverOutcome::LauncherAlreadyRunning => {
                    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                        "watcher.takeover_deferred",
                        json!({ "reason": "launcher_already_running" }),
                    );
                }
                WatcherTakeoverOutcome::StopFailed => {
                    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                        "watcher.takeover_failed",
                        json!({ "error": "ChatGPT processes did not stop in time" }),
                    );
                }
                WatcherTakeoverOutcome::LaunchFailed(error) => {
                    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                        "watcher.takeover_failed",
                        json!({ "error": error }),
                    );
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs_f64(
                providerdeck_core::watcher::TAKEOVER_FAILURE_BACKOFF_SECONDS,
            ))
            .await;
            continue;
        }
        tokio::time::sleep(std::time::Duration::from_secs_f64(
            providerdeck_core::watcher::WATCHER_INTERVAL_SECONDS,
        ))
        .await;
    }
}

#[derive(Debug, PartialEq, Eq)]
enum WatcherTakeoverOutcome {
    Started,
    NetworkUnavailable(String),
    LauncherAlreadyRunning,
    StopFailed,
    LaunchFailed(String),
}

async fn attempt_watcher_takeover<WaitNetwork, WaitFuture, LauncherRunning, StopCodex, Spawn>(
    wait_for_network: WaitNetwork,
    launcher_running: LauncherRunning,
    stop_codex: StopCodex,
    spawn_launcher: Spawn,
) -> WatcherTakeoverOutcome
where
    WaitNetwork: FnOnce() -> WaitFuture,
    WaitFuture: std::future::Future<Output = anyhow::Result<()>>,
    LauncherRunning: FnOnce() -> bool,
    StopCodex: FnOnce() -> bool,
    Spawn: FnOnce() -> anyhow::Result<()>,
{
    if let Err(error) = wait_for_network().await {
        return WatcherTakeoverOutcome::NetworkUnavailable(error.to_string());
    }
    if launcher_running() {
        return WatcherTakeoverOutcome::LauncherAlreadyRunning;
    }
    if !stop_codex() {
        return WatcherTakeoverOutcome::StopFailed;
    }
    match spawn_launcher() {
        Ok(()) => WatcherTakeoverOutcome::Started,
        Err(error) => WatcherTakeoverOutcome::LaunchFailed(error.to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
async fn run_watcher(_options: LaunchOptions) -> Result<()> {
    anyhow::bail!("watch mode is only supported on macOS")
}

#[cfg(target_os = "macos")]
fn spawn_managed_launcher(launcher_path: &Path, options: &LaunchOptions) -> anyhow::Result<()> {
    let mut command = std::process::Command::new(launcher_path);
    if let Some(app_dir) = options.app_dir.as_deref() {
        command.arg("--app-path").arg(app_dir);
    }
    command
        .arg("--debug-port")
        .arg(options.debug_port.to_string())
        .arg("--helper-port")
        .arg(options.helper_port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .with_context(|| format!("failed to spawn {}", launcher_path.display()))
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

    async fn wait_for_network_ready(&self) -> anyhow::Result<()> {
        self.core.wait_for_network_ready().await
    }

    async fn start_helper(&self, helper_port: u16) -> anyhow::Result<()> {
        self.core.start_helper(helper_port).await
    }

    async fn repair_codex_config(&self, helper_port: u16) -> anyhow::Result<()> {
        self.core.repair_codex_config(helper_port).await
    }

    async fn start_transport_prearm(
        &self,
        debug_port: u16,
        helper_port: u16,
    ) -> anyhow::Result<()> {
        self.core
            .start_transport_prearm(debug_port, helper_port)
            .await
    }

    async fn finish_transport_prearm(&self) -> anyhow::Result<()> {
        self.core.finish_transport_prearm().await
    }

    async fn stop_transport_prearm(&self) {
        self.core.stop_transport_prearm().await;
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
        _ctx: BridgeContext,
    ) -> anyhow::Result<()> {
        self.core.inject(debug_port, helper_port).await
    }

    async fn inject(&self, debug_port: u16, helper_port: u16) -> anyhow::Result<()> {
        self.core.inject(debug_port, helper_port).await
    }

    async fn start_bridge_watchdog(&self, debug_port: u16, helper_port: u16) -> anyhow::Result<()> {
        self.core
            .start_bridge_watchdog(debug_port, helper_port)
            .await
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
    use std::sync::atomic::{AtomicBool, Ordering};

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
    fn watcher_mode_is_selected_explicitly() {
        assert!(is_watcher_mode(["--watch", "--debug-port", "9229"]));
        assert!(!is_watcher_mode(["--debug-port", "9229"]));
    }

    #[test]
    fn macos_watcher_checks_runtime_before_relaunching() {
        let production = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let watcher_branch = production.find("if is_watcher_mode(&args)").unwrap();
        let launcher_guard = production.find("acquire_single_instance_guard()?").unwrap();

        assert!(watcher_branch < launcher_guard);
        assert!(production.contains("should_take_over("));
        assert!(production.contains("LAUNCHER_GUARD_PORT"));
        assert!(production.contains("stop_codex_processes()"));
        assert!(production.contains("TAKEOVER_FAILURE_BACKOFF_SECONDS"));
    }

    #[tokio::test]
    async fn watcher_takeover_keeps_chatgpt_running_when_network_is_unavailable() {
        let stop_called = AtomicBool::new(false);
        let spawn_called = AtomicBool::new(false);

        let outcome = attempt_watcher_takeover(
            || async { anyhow::bail!("proxy unavailable") },
            || false,
            || {
                stop_called.store(true, Ordering::SeqCst);
                true
            },
            || {
                spawn_called.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert_eq!(
            outcome,
            WatcherTakeoverOutcome::NetworkUnavailable("proxy unavailable".to_string())
        );
        assert!(!stop_called.load(Ordering::SeqCst));
        assert!(!spawn_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn watcher_takeover_rechecks_launcher_guard_after_network_wait() {
        let stop_called = AtomicBool::new(false);
        let spawn_called = AtomicBool::new(false);

        let outcome = attempt_watcher_takeover(
            || async { Ok(()) },
            || true,
            || {
                stop_called.store(true, Ordering::SeqCst);
                true
            },
            || {
                spawn_called.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert_eq!(outcome, WatcherTakeoverOutcome::LauncherAlreadyRunning);
        assert!(!stop_called.load(Ordering::SeqCst));
        assert!(!spawn_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn watcher_takeover_stops_chatgpt_only_after_safety_checks() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let wait_events = Arc::clone(&events);
        let guard_events = Arc::clone(&events);
        let stop_events = Arc::clone(&events);
        let spawn_events = Arc::clone(&events);

        let outcome = attempt_watcher_takeover(
            || async move {
                wait_events.lock().unwrap().push("wait-network");
                Ok(())
            },
            || {
                guard_events.lock().unwrap().push("check-launcher");
                false
            },
            || {
                stop_events.lock().unwrap().push("stop-chatgpt");
                true
            },
            || {
                spawn_events.lock().unwrap().push("spawn-launcher");
                Ok(())
            },
        )
        .await;

        assert_eq!(outcome, WatcherTakeoverOutcome::Started);
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                "wait-network",
                "check-launcher",
                "stop-chatgpt",
                "spawn-launcher"
            ]
        );
    }

    #[test]
    fn launcher_uses_single_instance_guard_before_launching() {
        let source = include_str!("main.rs");

        assert!(source.contains("acquire_single_instance_guard()?"));
        assert!(source.contains("LAUNCHER_GUARD_PORT"));
        assert!(source.contains("launcher.already_running"));
    }

    #[test]
    fn launcher_delegates_codex_config_repair_to_core_hooks() {
        let source = include_str!("main.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();

        assert!(production.contains("async fn repair_codex_config"));
        assert!(production.contains("self.core.repair_codex_config(helper_port).await"));
    }

    #[test]
    fn launcher_delegates_network_readiness_to_core_hooks() {
        let source = include_str!("main.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let hook_impl = production
            .split("impl LaunchHooks for LauncherHooks")
            .nth(1)
            .unwrap();

        assert!(hook_impl.contains("async fn wait_for_network_ready"));
        assert!(hook_impl.contains("self.core.wait_for_network_ready().await"));
    }

    #[test]
    fn launcher_delegates_bridge_watchdog_to_core_hooks() {
        let source = include_str!("main.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let watchdog_impl = production
            .split("async fn start_bridge_watchdog")
            .nth(1)
            .unwrap()
            .split("async fn write_status")
            .next()
            .unwrap();

        assert!(watchdog_impl.contains("self.core"));
        assert!(watchdog_impl.contains(".start_bridge_watchdog(debug_port, helper_port)"));
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
