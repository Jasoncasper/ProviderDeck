use providerdeck_core::watcher::{
    build_spawn_launcher_command, build_watcher_install_plan, cdp_listening, codex_process_ids,
    disable_watcher_at, enable_watcher_at, filter_killable_launcher_processes,
    macos_app_process_ids, watcher_disabled_flag,
};

#[test]
fn cdp_listening_returns_true_for_bound_loopback_port() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();

    assert!(cdp_listening(port));
}

#[test]
fn cdp_listening_returns_false_for_closed_port() {
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    };

    assert!(!cdp_listening(port));
}

#[test]
fn watcher_enable_and_disable_toggle_flag() {
    let dir = tempfile::tempdir().unwrap();
    let flag = watcher_disabled_flag(dir.path());

    disable_watcher_at(dir.path()).unwrap();
    assert!(flag.exists());

    enable_watcher_at(dir.path()).unwrap();
    assert!(!flag.exists());
}

#[test]
fn watcher_install_plan_registers_rust_launcher_at_logon() {
    let plan = build_watcher_install_plan("C:/Tools/providerdeck.exe".into(), 9333);

    assert_eq!(plan.run_value_name, "ProviderDeckWatcher");
    assert_eq!(
        plan.run_value,
        "\"C:/Tools/providerdeck.exe\" --debug-port 9333"
    );
    assert_eq!(plan.shortcut_name, "ProviderDeckWatcher.lnk");
    assert_eq!(plan.shortcut_target, "C:/Tools/providerdeck.exe");
    assert_eq!(plan.shortcut_arguments, "--debug-port 9333");
}

#[test]
fn spawn_launcher_command_points_to_silent_binary_only() {
    let command = build_spawn_launcher_command("C:/Tools/providerdeck.exe", 9444);

    assert_eq!(command[0], "C:/Tools/providerdeck.exe");
    assert!(command.contains(&"--debug-port".to_string()));
    assert!(command.contains(&"9444".to_string()));
    assert!(!command.iter().any(|part| part.contains("manager")));
}

#[test]
fn codex_process_filter_keeps_only_windowsapps_codex_processes() {
    let processes = [
        (
            11,
            r"C:\Program Files\WindowsApps\OpenAI.Codex_1.0.0.0_x64__abc\app\Codex.exe",
        ),
        (12, r"C:\Tools\Codex.exe"),
        (
            13,
            r"C:\Program Files\WindowsApps\Other.App_1.0.0.0_x64__abc\app\Codex.exe",
        ),
    ];

    assert_eq!(codex_process_ids(processes), vec![11]);
}

#[test]
fn macos_app_process_filter_matches_chatgpt_bundle_processes() {
    let app_paths = ["/Applications/Codex.app", "/Applications/ChatGPT.app"];
    let processes = [
        (11, "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT"),
        (
            12,
            "/Applications/ChatGPT.app/Contents/Frameworks/Codex Framework.framework/Helpers/Codex (Renderer).app/Contents/MacOS/Codex (Renderer)",
        ),
        (
            13,
            "/Applications/ChatGPT Classic.app/Contents/MacOS/ChatGPT",
        ),
        (14, "/Applications/Other.app/Contents/MacOS/ChatGPT"),
    ];

    assert_eq!(macos_app_process_ids(processes, app_paths), vec![11, 12]);
}

#[test]
fn launcher_process_filter_protects_current_process_ancestry() {
    let processes = [
        (10, 0, "providerdeck.exe"),
        (20, 10, "providerdeck.exe"),
        (30, 20, "providerdeck.exe"),
        (40, 10, "providerdeck.exe"),
        (50, 10, "providerdeck-manager.exe"),
    ];

    assert_eq!(filter_killable_launcher_processes(processes, 30), vec![40]);
}
