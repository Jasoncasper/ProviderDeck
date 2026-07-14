pub mod commands;
pub mod install;

use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

pub fn run() {
    install_panic_logger();
    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
        "manager.start",
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION")
        }),
    );
    let Some(_guard) = acquire_single_instance_guard() else {
        return;
    };
    let show_update = commands::startup_should_show_update();
    let run_result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            #[cfg(target_os = "macos")]
            let tray_icon =
                tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
            #[cfg(not(target_os = "macos"))]
            let tray_icon = app.default_window_icon().cloned().unwrap_or_else(|| {
                tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))
                    .expect("Failed to load embedded icon")
            });

            let tray_builder = TrayIconBuilder::new().icon(tray_icon);
            #[cfg(target_os = "macos")]
            let tray_builder = tray_builder.icon_as_template(true);

            let _tray = tray_builder
                .tooltip("ProviderDeck — 点击显示/隐藏")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        let _ = providerdeck_core::watcher::uninstall_watcher();
                        providerdeck_core::watcher::stop_launcher_processes();
                        providerdeck_core::watcher::stop_codex_processes();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            let url = if show_update {
                "index.html?showUpdate=1"
            } else {
                "index.html"
            };
            let window =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App(url.into()))
                    .title("ProviderDeck")
                    .inner_size(1180.0, 820.0)
                    .min_inner_size(960.0, 720.0)
                    .build()?;

            let w = window.clone();
            #[cfg(target_os = "macos")]
            let close_app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = w.hide();
                    #[cfg(target_os = "macos")]
                    let _ =
                        close_app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::backend_version,
            commands::startup_options,
            commands::load_overview,
            commands::launch_providerdeck,
            commands::restart_providerdeck,
            commands::open_external_url,
            commands::check_update,
            commands::perform_update,
            commands::read_latest_logs,
            commands::copy_diagnostics,
            commands::write_diagnostic_event,
            commands::load_routing_config,
            commands::save_routing_config,
            commands::upsert_provider,
            commands::delete_provider,
            commands::fetch_provider_models,
            commands::test_smart_provider,
            commands::load_provider_catalog,
            commands::load_provider_config,
            commands::save_provider_config,
            commands::load_runtime_status,
            commands::load_switch_journal,
            commands::recover_thread_runtime,
            commands::safe_exit_providerdeck
        ])
        .run(tauri::generate_context!());
    if let Err(error) = run_result {
        let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
            "manager.run_failed",
            serde_json::json!({
                "error": error.to_string()
            }),
        );
    }
}

pub fn install_panic_logger() {
    std::panic::set_hook(Box::new(|panic_info| {
        let payload = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|message| (*message).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "非字符串 panic payload".to_string());
        let location = panic_info.location().map(|location| {
            serde_json::json!({
                "file": location.file(),
                "line": location.line(),
                "column": location.column()
            })
        });
        let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
            "manager.panic",
            serde_json::json!({
                "payload": payload,
                "location": location
            }),
        );
    }));
}

fn acquire_single_instance_guard() -> Option<std::net::TcpListener> {
    match providerdeck_core::ports::acquire_loopback_port_guard(
        providerdeck_core::ports::MANAGER_GUARD_PORT,
    ) {
        Ok(listener) => Some(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                "manager.already_running",
                serde_json::json!({
                    "guard_port": providerdeck_core::ports::MANAGER_GUARD_PORT
                }),
            );
            None
        }
        Err(error) => {
            let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                "manager.guard_failed",
                serde_json::json!({
                    "guard_port": providerdeck_core::ports::MANAGER_GUARD_PORT,
                    "error": error.to_string()
                }),
            );
            match std::net::TcpListener::bind(("127.0.0.1", 0)) {
                Ok(listener) => Some(listener),
                Err(fallback_error) => {
                    let _ = providerdeck_core::diagnostic_log::append_diagnostic_log(
                        "manager.guard_fallback_failed",
                        serde_json::json!({
                            "error": fallback_error.to_string()
                        }),
                    );
                    None
                }
            }
        }
    }
}
