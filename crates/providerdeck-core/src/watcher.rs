use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const WATCHER_INTERVAL_SECONDS: f64 = 3.0;
pub const CDP_PROBE_TIMEOUT_SECONDS: f64 = 0.5;
pub const TAKEOVER_FAILURE_BACKOFF_SECONDS: f64 = 30.0;
pub const WATCHER_RUN_NAME: &str = "ProviderDeckWatcher";
pub const WATCHER_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const WATCHER_STARTUP_SHORTCUT_NAME: &str = "ProviderDeckWatcher.lnk";
pub const MACOS_WATCHER_LABEL: &str = "com.jasoncasper.providerdeck.watcher";

#[cfg(target_os = "macos")]
static MACOS_WATCHER_INSTALL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatcherInstallPlan {
    pub run_value_name: String,
    pub run_value: String,
    pub shortcut_name: String,
    pub shortcut_target: String,
    pub shortcut_arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacOsWatcherInstallPlan {
    pub label: String,
    pub plist_path: PathBuf,
    pub plist: String,
}

pub fn watcher_disabled_flag(root: &Path) -> PathBuf {
    root.join("watcher.disabled")
}

pub fn default_watcher_disabled_flag() -> PathBuf {
    watcher_disabled_flag(&crate::paths::default_app_state_dir())
}

pub fn enable_watcher_at(root: &Path) -> std::io::Result<()> {
    let flag = watcher_disabled_flag(root);
    if flag.exists() {
        std::fs::remove_file(flag)?;
    }
    Ok(())
}

pub fn disable_watcher_at(root: &Path) -> std::io::Result<()> {
    let flag = watcher_disabled_flag(root);
    if let Some(parent) = flag.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(flag, b"disabled")
}

pub fn enable_watcher() -> std::io::Result<()> {
    enable_watcher_at(&crate::paths::default_app_state_dir())
}

pub fn disable_watcher() -> std::io::Result<()> {
    disable_watcher_at(&crate::paths::default_app_state_dir())
}

pub fn cdp_listening(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
}

pub fn should_take_over(chatgpt_running: bool, cdp_ready: bool, launcher_running: bool) -> bool {
    chatgpt_running && !cdp_ready && !launcher_running
}

pub fn build_spawn_launcher_command(launcher_path: &str, debug_port: u16) -> Vec<String> {
    vec![
        launcher_path.to_string(),
        "--debug-port".to_string(),
        debug_port.to_string(),
    ]
}

pub fn build_watcher_install_plan(launcher_path: PathBuf, debug_port: u16) -> WatcherInstallPlan {
    let launcher = launcher_path.to_string_lossy().to_string();
    let arguments = format!("--debug-port {debug_port}");
    WatcherInstallPlan {
        run_value_name: WATCHER_RUN_NAME.to_string(),
        run_value: format!("\"{launcher}\" {arguments}"),
        shortcut_name: WATCHER_STARTUP_SHORTCUT_NAME.to_string(),
        shortcut_target: launcher,
        shortcut_arguments: arguments,
    }
}

pub fn build_macos_watcher_command(launcher_path: &str, debug_port: u16) -> Vec<String> {
    vec![
        launcher_path.to_string(),
        "--watch".to_string(),
        "--debug-port".to_string(),
        debug_port.to_string(),
    ]
}

pub fn build_macos_watcher_install_plan(
    launcher_path: PathBuf,
    launch_agents_dir: PathBuf,
    debug_port: u16,
) -> MacOsWatcherInstallPlan {
    let label = MACOS_WATCHER_LABEL.to_string();
    let plist_path = launch_agents_dir.join(format!("{label}.plist"));
    let arguments = build_macos_watcher_command(&launcher_path.to_string_lossy(), debug_port)
        .into_iter()
        .map(|argument| format!("    <string>{}</string>", escape_xml(&argument)))
        .collect::<Vec<_>>()
        .join("\n");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{arguments}
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
      <key>SuccessfulExit</key>
      <false/>
  </dict>
  <key>ProcessType</key>
  <string>Background</string>
</dict>
</plist>
"#
    );
    MacOsWatcherInstallPlan {
        label,
        plist_path,
        plist,
    }
}

pub fn macos_watcher_needs_reload(
    existing_plist: Option<&str>,
    desired_plist: &str,
    service_loaded: bool,
) -> bool {
    !service_loaded || existing_plist != Some(desired_plist)
}

pub fn wait_for_macos_service_removal_with(
    mut service_loaded: impl FnMut() -> bool,
    mut sleep: impl FnMut(Duration),
) -> bool {
    const MAX_ATTEMPTS: usize = 120;
    for attempt in 0..MAX_ATTEMPTS {
        if !service_loaded() {
            return true;
        }
        if attempt + 1 < MAX_ATTEMPTS {
            sleep(Duration::from_millis(50));
        }
    }
    false
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn codex_process_ids<'a>(processes: impl IntoIterator<Item = (u32, &'a str)>) -> Vec<u32> {
    processes
        .into_iter()
        .filter_map(|(process_id, executable)| {
            let executable = executable.to_ascii_lowercase();
            executable
                .contains("\\windowsapps\\openai.codex_")
                .then_some(process_id)
        })
        .collect()
}

pub fn macos_app_process_ids<'a, 'b>(
    processes: impl IntoIterator<Item = (u32, &'a str)>,
    app_paths: impl IntoIterator<Item = &'b str>,
) -> Vec<u32> {
    let app_paths = app_paths
        .into_iter()
        .map(|path| path.to_ascii_lowercase())
        .collect::<Vec<_>>();
    processes
        .into_iter()
        .filter_map(|(process_id, command)| {
            let command = command.to_ascii_lowercase();
            app_paths
                .iter()
                .any(|app_path| command.contains(app_path))
                .then_some(process_id)
        })
        .collect()
}

pub fn wait_for_process_shutdown_with(
    mut process_ids: impl FnMut() -> Vec<u32>,
    mut sleep: impl FnMut(Duration),
) -> bool {
    const MAX_ATTEMPTS: usize = 20;
    for attempt in 0..MAX_ATTEMPTS {
        if process_ids().is_empty() {
            return true;
        }
        if attempt + 1 < MAX_ATTEMPTS {
            sleep(Duration::from_millis(250));
        }
    }
    false
}

pub fn parse_process_ids(output: &str, current_process_id: u32) -> Vec<u32> {
    let mut process_ids = output
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|process_id| *process_id != current_process_id)
        .collect::<Vec<_>>();
    process_ids.sort_unstable();
    process_ids.dedup();
    process_ids
}

pub fn filter_killable_launcher_processes<'a>(
    processes: impl IntoIterator<Item = (u32, u32, &'a str)>,
    current_process_id: u32,
) -> Vec<u32> {
    let processes = processes.into_iter().collect::<Vec<_>>();
    let parents = processes
        .iter()
        .map(|(process_id, parent_process_id, _)| (*process_id, *parent_process_id))
        .collect::<HashMap<_, _>>();
    let mut protected = HashSet::new();
    let mut cursor = current_process_id;
    while cursor != 0 && protected.insert(cursor) {
        cursor = parents.get(&cursor).copied().unwrap_or(0);
    }
    processes
        .into_iter()
        .filter(|(process_id, _, exe_file)| {
            !protected.contains(process_id) && exe_file.eq_ignore_ascii_case("providerdeck.exe")
        })
        .map(|(process_id, _, _)| process_id)
        .collect()
}

#[cfg(windows)]
pub fn install_watcher(launcher_path: &Path, debug_port: u16) -> anyhow::Result<()> {
    let plan = build_watcher_install_plan(launcher_path.to_path_buf(), debug_port);
    crate::windows_integration::set_current_user_string_value(
        WATCHER_RUN_KEY,
        &plan.run_value_name,
        &plan.run_value,
    )?;
    create_startup_shortcut(launcher_path, &plan.shortcut_arguments)?;
    spawn_launcher(launcher_path, debug_port);
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn install_watcher(launcher_path: &Path, debug_port: u16) -> anyhow::Result<()> {
    let _install_guard = MACOS_WATCHER_INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let launch_agents_dir = directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join("Library/LaunchAgents"))
        .ok_or_else(|| anyhow::anyhow!("unable to resolve macOS LaunchAgents directory"))?;
    let plan = build_macos_watcher_install_plan(
        launcher_path.to_path_buf(),
        launch_agents_dir,
        debug_port,
    );
    let domain = current_user_launchd_domain()?;
    let service = format!("{domain}/{}", plan.label);
    let existing_plist = std::fs::read_to_string(&plan.plist_path).ok();
    if !macos_watcher_needs_reload(
        existing_plist.as_deref(),
        &plan.plist,
        macos_service_loaded(&service),
    ) {
        enable_watcher()?;
        return Ok(());
    }
    if let Some(parent) = plan.plist_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary_path = plan.plist_path.with_extension("plist.tmp");
    std::fs::write(&temporary_path, &plan.plist)?;
    std::fs::rename(&temporary_path, &plan.plist_path)?;
    if let Err(error) = enable_watcher() {
        let _ =
            rollback_macos_watcher_files(&plan.plist_path, &crate::paths::default_app_state_dir());
        return Err(error.into());
    }

    let bootout = std::process::Command::new("launchctl")
        .args(["bootout", &service])
        .output();
    if !wait_for_macos_service_removal_with(|| macos_service_loaded(&service), std::thread::sleep) {
        let detail = bootout
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stderr).trim().to_string())
            .filter(|detail| !detail.is_empty())
            .unwrap_or_else(|| "service remained loaded after bootout".to_string());
        let _ =
            rollback_macos_watcher_files(&plan.plist_path, &crate::paths::default_app_state_dir());
        anyhow::bail!("failed to stop existing macOS watcher: {detail}");
    }
    let output = match std::process::Command::new("launchctl")
        .arg("bootstrap")
        .arg(&domain)
        .arg(&plan.plist_path)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            let _ = rollback_macos_watcher_files(
                &plan.plist_path,
                &crate::paths::default_app_state_dir(),
            );
            return Err(error.into());
        }
    };
    if !output.status.success() {
        let _ =
            rollback_macos_watcher_files(&plan.plist_path, &crate::paths::default_app_state_dir());
        anyhow::bail!(
            "failed to install macOS watcher: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn install_watcher(_launcher_path: &Path, _debug_port: u16) -> anyhow::Result<()> {
    anyhow::bail!("watcher install is only supported on Windows and macOS")
}

#[cfg(windows)]
pub fn uninstall_watcher() -> anyhow::Result<()> {
    let _ =
        crate::windows_integration::delete_current_user_value(WATCHER_RUN_KEY, WATCHER_RUN_NAME);
    if let Some(shortcut) = startup_shortcut_path() {
        let _ = std::fs::remove_file(shortcut);
    }
    stop_launcher_processes();
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall_watcher() -> anyhow::Result<()> {
    let launch_agents_dir = directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join("Library/LaunchAgents"))
        .ok_or_else(|| anyhow::anyhow!("unable to resolve macOS LaunchAgents directory"))?;
    let plist_path = launch_agents_dir.join(format!("{MACOS_WATCHER_LABEL}.plist"));
    if let Ok(domain) = current_user_launchd_domain() {
        let service = format!("{domain}/{MACOS_WATCHER_LABEL}");
        let _ = std::process::Command::new("launchctl")
            .args(["bootout", &service])
            .output();
    }
    rollback_macos_watcher_files(&plist_path, &crate::paths::default_app_state_dir())?;
    Ok(())
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn uninstall_watcher() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn current_user_launchd_domain() -> anyhow::Result<String> {
    let output = std::process::Command::new("id").arg("-u").output()?;
    if !output.status.success() {
        anyhow::bail!("unable to resolve current macOS user id");
    }
    let user_id = String::from_utf8(output.stdout)?.trim().to_string();
    if user_id.is_empty() || !user_id.bytes().all(|byte| byte.is_ascii_digit()) {
        anyhow::bail!("invalid current macOS user id");
    }
    Ok(format!("gui/{user_id}"))
}

#[cfg(target_os = "macos")]
fn macos_service_loaded(service: &str) -> bool {
    std::process::Command::new("launchctl")
        .args(["print", service])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn rollback_macos_watcher_files(plist_path: &Path, state_root: &Path) -> std::io::Result<()> {
    let disable_result = disable_watcher_at(state_root);
    let remove_result = match std::fs::remove_file(plist_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    disable_result.and(remove_result)
}

#[cfg(windows)]
pub fn find_codex_processes() -> Vec<u32> {
    codex_process_ids(
        crate::windows_integration::enumerate_processes()
            .into_iter()
            .filter(|process| process.exe_file.eq_ignore_ascii_case("codex.exe"))
            .filter_map(|process| {
                process
                    .executable_path
                    .as_deref()
                    .map(|path| (process.process_id, path.to_string_lossy().to_string()))
            })
            .collect::<Vec<_>>()
            .iter()
            .map(|(pid, path)| (*pid, path.as_str())),
    )
}

#[cfg(not(windows))]
pub fn find_codex_processes() -> Vec<u32> {
    let mut pids = Vec::new();
    let app_paths = [
        "/Applications/Codex.app",
        "/Applications/OpenAI Codex.app",
        "/Applications/OpenAI.Codex.app",
        "/Applications/ChatGPT.app",
    ];
    for app_path in app_paths {
        if let Ok(output) = std::process::Command::new("pgrep")
            .args(["-f", app_path])
            .output()
        {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                pids.extend(stdout.lines().filter_map(|l| l.trim().parse::<u32>().ok()));
            }
        }
    }
    // Also kill codex CLI processes
    if let Ok(output) = std::process::Command::new("pgrep")
        .args(["-f", "codex watch"])
        .output()
    {
        if let Ok(stdout) = String::from_utf8(output.stdout) {
            pids.extend(stdout.lines().filter_map(|l| l.trim().parse::<u32>().ok()));
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(windows)]
pub fn stop_launcher_processes() {
    let processes = crate::windows_integration::enumerate_processes();
    let killable = filter_killable_launcher_processes(
        processes.iter().map(|process| {
            (
                process.process_id,
                process.parent_process_id,
                process.exe_file.as_str(),
            )
        }),
        std::process::id(),
    );
    for process_id in killable {
        let _ = crate::windows_integration::terminate_process(process_id);
    }
}

#[cfg(not(windows))]
fn find_launcher_processes() -> Vec<u32> {
    let port = format!("-iTCP:{}", crate::ports::LAUNCHER_GUARD_PORT);
    let lsof = if cfg!(target_os = "macos") {
        "/usr/sbin/lsof"
    } else {
        "lsof"
    };
    std::process::Command::new(lsof)
        .args(["-nP", "-t", &port, "-sTCP:LISTEN"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| parse_process_ids(&output, std::process::id()))
        .unwrap_or_default()
}

#[cfg(not(windows))]
pub fn stop_launcher_processes() {
    let kill = if cfg!(target_os = "macos") {
        "/bin/kill"
    } else {
        "kill"
    };
    for process_id in find_launcher_processes() {
        let _ = std::process::Command::new(kill)
            .arg(process_id.to_string())
            .output();
    }
    let _ = wait_for_process_shutdown_with(find_launcher_processes, std::thread::sleep);
}

#[cfg(windows)]
pub fn stop_codex_processes() -> bool {
    for process_id in find_codex_processes() {
        let _ = crate::windows_integration::terminate_process(process_id);
    }
    wait_for_process_shutdown_with(find_codex_processes, std::thread::sleep)
}

#[cfg(not(windows))]
pub fn stop_codex_processes() -> bool {
    for process_id in find_codex_processes() {
        let _ = std::process::Command::new("kill")
            .arg(format!("{process_id}"))
            .output();
    }
    let stopped = wait_for_process_shutdown_with(find_codex_processes, std::thread::sleep);
    if !stopped {
        let _ = crate::diagnostic_log::append_diagnostic_log(
            "codex.stop_timeout",
            serde_json::json!({
                "remaining_process_ids": find_codex_processes()
            }),
        );
    }
    stopped
}

#[cfg(windows)]
fn create_startup_shortcut(launcher_path: &Path, arguments: &str) -> anyhow::Result<()> {
    let Some(shortcut_path) = startup_shortcut_path() else {
        anyhow::bail!("无法定位 Windows 启动目录")
    };
    crate::windows_integration::create_shortcut(&crate::windows_integration::ShortcutSpec {
        path: shortcut_path,
        target: launcher_path.to_path_buf(),
        arguments: arguments.to_string(),
        working_directory: launcher_path.parent().map(Path::to_path_buf),
        description: "ProviderDeck watcher".to_string(),
        icon: None,
        show_minimized: true,
    })
}

#[cfg(windows)]
fn spawn_launcher(launcher_path: &Path, debug_port: u16) {
    let command = build_spawn_launcher_command(&launcher_path.to_string_lossy(), debug_port);
    if let Some((exe, args)) = command.split_first() {
        let mut command = Command::new(exe);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::windows_integration::CREATE_NO_WINDOW);
        let _ = command.spawn();
    }
}

#[cfg(windows)]
fn startup_shortcut_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|appdata| {
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
            .join(WATCHER_STARTUP_SHORTCUT_NAME)
    })
}

#[cfg(test)]
mod tests {
    use super::{rollback_macos_watcher_files, watcher_disabled_flag};

    #[test]
    fn macos_watcher_rollback_disables_runtime_and_removes_plist() {
        let root = tempfile::tempdir().unwrap();
        let plist_path = root.path().join("watcher.plist");
        std::fs::write(&plist_path, "plist").unwrap();

        rollback_macos_watcher_files(&plist_path, root.path()).unwrap();

        assert!(watcher_disabled_flag(root.path()).exists());
        assert!(!plist_path.exists());
    }
}
