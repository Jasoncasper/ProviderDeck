#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|arg| arg == "--show-update") {
        unsafe {
            std::env::set_var("PROVIDERDECK_SHOW_UPDATE", "1");
        }
    }

    // Launcher mode: spawned by the GUI to launch Codex and inject bridge.
    // Detected by the presence of --debug-port and --helper-port args.
    let args: Vec<String> = std::env::args().collect();
    let mut app_path: Option<String> = None;
    let mut debug_port: Option<u16> = None;
    let mut helper_port: Option<u16> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--app-path" if i + 1 < args.len() => {
                app_path = Some(args[i + 1].clone());
                i += 1;
            }
            "--debug-port" if i + 1 < args.len() => {
                debug_port = args[i + 1].parse().ok();
                i += 1;
            }
            "--helper-port" if i + 1 < args.len() => {
                helper_port = args[i + 1].parse().ok();
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    if let (Some(debug_port), Some(helper_port)) = (debug_port, helper_port) {
        providerdeck_manager_lib::install_panic_logger();
        let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
            "launcher.mode_enter",
            serde_json::json!({
                "app_path": app_path,
                "debug_port": debug_port,
                "helper_port": helper_port
            }),
        );
        let options = providerdeck_core::launcher::LaunchOptions {
            app_dir: app_path.map(std::path::PathBuf::from),
            debug_port,
            helper_port,
            status_store: providerdeck_core::status::StatusStore::default(),
        };
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        match rt.block_on(providerdeck_core::launcher::launch_and_inject(options)) {
            Ok(handle) => {
                let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                    "launcher.success",
                    serde_json::json!({}),
                );
                // Keep the helper alive until Codex exits
                let _ = rt.block_on(handle.wait_for_codex_exit());
            }
            Err(error) => {
                let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                    "launcher.failed",
                    serde_json::json!({"error": error.to_string()}),
                );
                std::process::exit(1);
            }
        }
        return;
    }

    providerdeck_manager_lib::run();
}
