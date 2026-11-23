//! Tauri application entry point.
//!
//! The whole point of the app is small: open `http://127.0.0.1:3080` in a native
//! window. Everything else — profiles, pre-launch hooks, renaming, i18n — exists
//! to make that one job pleasant and reproducible.
//!
//! # Boot sequence
//!
//! 1. `AppState::bootstrap` creates `~/.config/tauri-3080` and reads
//!    `config.toml`.
//! 2. The menu bar, tray icon and the first launcher window are created.
//! 3. The webview loads the bundled `index.html`, which runs the pre-launch
//!    hook and then navigates the window to the profile URL.
//!
//! # Smoke mode
//!
//! With `TAURI_3080_SMOKE=<output.json>` the launcher boots, verifies the
//! window and the IPC bridge, writes a JSON report and exits on its own. The
//! `smoke` GitHub workflow uses it under `xvfb` so every push proves the binary
//! actually starts.

pub mod app_id;
pub mod commands;
pub mod config;
pub mod error;
pub mod i18n;
pub mod icons;
pub mod menus;
pub mod platform;
pub mod platform_info;
pub mod prelaunch;
pub mod state;
pub mod ui_script;
pub mod windows;

use std::time::{Duration, Instant};

use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::AppState;

/// Environment variable that turns on smoke mode (value: report path).
pub const SMOKE_ENV: &str = "TAURI_3080_SMOKE";

/// Environment variable read by the integration test to isolate the config.
pub const CONFIG_HOME_ENV: &str = platform::CONFIG_HOME_ENV;

/// Builds and runs the application.
pub fn run() {
    let smoke_report = std::env::var_os(SMOKE_ENV).map(std::path::PathBuf::from);

    let builder = tauri::Builder::default()
        // Must be the first plugin: a second launch hands its arguments to the
        // running instance and exits before any window is created.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // The user launched the app again, so surface the existing window
            // instead of starting a second process.
            let _ = windows::show_or_create_main(app, None);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::get_config,
            commands::config_as_toml,
            commands::reload_config,
            commands::get_paths,
            commands::get_platform,
            commands::save_settings,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::set_app_icon,
            commands::reset_app_icon,
            commands::set_profile_icon,
            commands::pick_app_icon,
            commands::app_icon_data_url,
            commands::run_prelaunch,
            commands::open_profile_url,
            commands::go_home,
            commands::cancel_prelaunch,
            commands::open_window,
            commands::open_settings,
            commands::close_window,
            commands::reveal_path,
            commands::ensure_script_dir,
            commands::window_kind,
            commands::report_ready,
            commands::report_frontend_error,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            let state = match AppState::bootstrap() {
                Ok(state) => state,
                Err(error) => {
                    // Without a usable configuration there is nothing sensible
                    // to show; report the problem and stop instead of opening a
                    // blank window.
                    eprintln!("tauri-3080: could not load the configuration: {error}");
                    write_smoke_report(
                        smoke_report.as_deref(),
                        &SmokeReport::failure(&error.to_string()),
                    );
                    handle.exit(1);
                    return Ok(());
                }
            };

            let settings = state.settings();
            app.manage(state);

            #[cfg(target_os = "macos")]
            {
                let _ = handle.set_activation_policy(tauri::ActivationPolicy::Regular);
            }

            if let Err(error) = windows::open_main_window(&handle, None) {
                eprintln!("tauri-3080: could not open the main window: {error}");
            }

            crate::menus::install(&handle, &settings.locale);
            if let Err(error) = crate::menus::install_tray(&handle, &settings.locale) {
                // A missing tray implementation must not stop the launcher.
                eprintln!("tauri-3080: tray icon unavailable: {error}");
            }
            windows::refresh_icons(&handle);

            if let Some(report_path) = smoke_report.clone() {
                // Smoke mode runs on its own thread so it can sleep without
                // blocking the UI thread, and exits the process when done.
                let smoke_handle = handle.clone();
                std::thread::spawn(move || run_smoke_checks(&smoke_handle, &report_path));
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let label = window.label().to_owned();
                let app = window.app_handle().clone();
                let close_to_tray = app
                    .try_state::<AppState>()
                    .is_some_and(|state| state.settings().close_to_tray);

                if close_to_tray && label != windows::SETTINGS_LABEL {
                    api.prevent_close();
                    let _ = window.hide();
                }
                if label == windows::SETTINGS_LABEL {
                    // Settings is recreated on demand, never hidden.
                }
            }
            WindowEvent::Destroyed => {
                let label = window.label().to_owned();
                windows::on_window_closed(&label);
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri-3080");

    builder.run(move |app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            let close_to_tray = app_handle
                .try_state::<AppState>()
                .is_some_and(|state| state.settings().close_to_tray);
            if close_to_tray && std::env::var_os(SMOKE_ENV).is_none() {
                api.prevent_exit();
            }
        }
    });
}

#[derive(Debug, serde::Serialize)]
struct SmokeReport {
    ok: bool,
    version: String,
    platform: String,
    config_dir: String,
    window_created: bool,
    frontend_ready: bool,
    ipc_ok: bool,
    config_loaded: bool,
    durations_ms: Durations,
    window_title: String,
    url: String,
    navigated: bool,
    errors: Vec<String>,
}

#[derive(Debug, Default, serde::Serialize)]
struct Durations {
    setup: u64,
    window: u64,
    frontend: u64,
    total: u64,
}

impl SmokeReport {
    fn failure(message: &str) -> Self {
        let mut report = Self {
            ok: false,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            config_dir: platform::config_dir().to_string_lossy().into_owned(),
            window_created: false,
            frontend_ready: false,
            ipc_ok: false,
            config_loaded: false,
            durations_ms: Durations::default(),
            window_title: String::new(),
            url: String::new(),
            navigated: false,
            errors: Vec::new(),
        };
        report.errors.push(message.to_owned());
        report
    }
}

fn write_smoke_report(path: Option<&std::path::Path>, report: &SmokeReport) {
    let Some(path) = path else {
        return;
    };
    let Ok(json) = serde_json::to_string_pretty(report) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, json);
}

/// URL prefix the window is expected to navigate to.
fn config_url_prefix(app: &tauri::AppHandle) -> String {
    let profile = app
        .try_state::<AppState>()
        .map(|state| state.snapshot().boot_profile());
    match profile {
        Some(profile) => {
            let trimmed = profile.url.trim_end_matches('/').to_owned();
            format!("{trimmed}/")
        }
        None => "http://127.0.0.1:3080/".to_owned(),
    }
}

fn run_smoke_checks(app: &tauri::AppHandle, report_path: &std::path::Path) {
    let started = Instant::now();
    let mut report = SmokeReport::failure("smoke run did not finish");
    report.errors.clear();
    report.platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    report.config_dir = platform::config_dir().to_string_lossy().into_owned();

    // The window is created synchronously during `setup`, so it must exist.
    report.window_created = app.get_webview_window(windows::MAIN_LABEL).is_some();
    if !report.window_created {
        report
            .errors
            .push("the main window was not created".to_owned());
    }
    report.config_loaded = platform::main_config_path().is_file();
    if !report.config_loaded {
        report.errors.push("config.toml was not written".to_owned());
    }

    // The frontend calls `report_ready` once its UI is live. Waiting for that
    // flag proves the bundled page, the module graph and the IPC bridge work.
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline && !state::frontend_ready() {
        std::thread::sleep(Duration::from_millis(250));
    }
    if let Some(window) = app.get_webview_window(windows::MAIN_LABEL) {
        report.window_title = window.title().unwrap_or_default();
        report.url = window.url().map(|url| url.to_string()).unwrap_or_default();
    }
    report.frontend_ready = state::frontend_ready();
    report.ipc_ok = report.frontend_ready;
    if !report.frontend_ready {
        report
            .errors
            .push("the launcher frontend never reported readiness over IPC".to_owned());
        if let Some(error) = state::frontend_error() {
            report.errors.push(error);
        }
    }

    // The window must have left the bundled page for the profile URL: that is
    // what "the launcher actually opens the local service" means. Wait for it,
    // because the page navigates only after the pre-launch hook has finished.
    let expected = config_url_prefix(app);
    let navigation_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < navigation_deadline {
        if let Some(window) = app.get_webview_window(windows::MAIN_LABEL) {
            report.url = window.url().map(|url| url.to_string()).unwrap_or_default();
            report.window_title = window.title().unwrap_or_default();
            if report.url.starts_with(&expected) || report.url.starts_with("https://") {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    report.navigated = report.url.starts_with(&expected) || report.url.starts_with("https://");
    if !report.navigated {
        report.errors.push(format!(
            "the window did not navigate to the profile URL (stayed on `{}`)",
            report.url
        ));
    }

    report.durations_ms.total = started.elapsed().as_millis() as u64;
    report.ok =
        report.window_created && report.config_loaded && report.frontend_ready && report.navigated;
    write_smoke_report(Some(report_path), &report);

    app.exit(i32::from(!report.ok));
}
