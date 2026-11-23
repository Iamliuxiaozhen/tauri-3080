//! Window management.
//!
//! Two kinds of window exist:
//!
//! * the **launcher window** (`main`, `main-1`, `main-2`, …) that hosts the
//!   home page — one per profile, and the user can open as many as they like;
//! * the **settings window** (a single instance at most).
//!
//! Opening several launcher windows is a first-class feature: every window is
//! bound to one profile and gets its own pre-launch run.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow};

use crate::i18n;
use crate::state::{AppState, WindowKind};
use crate::ui_script;

/// Label of the first launcher window.
pub const MAIN_LABEL: &str = "main";
/// Label of the settings window.
pub const SETTINGS_LABEL: &str = "settings";

static WINDOW_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Next free launcher label index.
fn next_window_index() -> u32 {
    WINDOW_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Maps a window label to the profile it is bound to.
fn window_profiles() -> &'static Mutex<std::collections::BTreeMap<String, String>> {
    static MAP: OnceLock<Mutex<std::collections::BTreeMap<String, String>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
}

/// Returns the profile a window is bound to.
#[must_use]
pub fn profile_for_window(label: &str) -> Option<String> {
    window_profiles()
        .lock()
        .ok()
        .and_then(|map| map.get(label).cloned())
}

fn bind_window(label: &str, profile_id: &str) {
    if let Ok(mut map) = window_profiles().lock() {
        map.insert(label.to_owned(), profile_id.to_owned());
    }
}

fn unbind_window(label: &str) {
    if let Ok(mut map) = window_profiles().lock() {
        map.remove(label);
    }
}

/// Returns the kind of a window label.
#[must_use]
pub fn kind_of(label: &str) -> WindowKind {
    if label == SETTINGS_LABEL {
        WindowKind::Settings
    } else {
        WindowKind::Main
    }
}

/// Opens a new launcher window.
///
/// When `profile_id` is `None` the default profile from the configuration is
/// used.
pub fn open_main_window<R: Runtime>(
    app: &AppHandle<R>,
    profile_id: Option<&str>,
) -> Result<WebviewWindow<R>, String> {
    let Some(state) = app.try_state::<AppState>() else {
        return Err("application state is not ready".to_owned());
    };
    let config = state.snapshot();
    let profile = match profile_id {
        Some(id) => config
            .profile(id)
            .map_err(|error| error.to_string())?
            .clone(),
        None => config.boot_profile(),
    };

    let index = next_window_index();
    let label = if index == 0 {
        MAIN_LABEL.to_owned()
    } else {
        format!("main-{index}")
    };

    let window = build_main_window(app, &label, &profile, &config)?;
    bind_window(&label, &profile.id);
    state.counters.window_opened();

    // The launcher page runs the pre-launch hook itself once it is ready, so
    // the window can show progress.
    let _ = window.show();
    let _ = window.set_focus();
    Ok(window)
}

fn build_main_window<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    profile: &crate::config::Profile,
    config: &crate::config::AppConfig,
) -> Result<WebviewWindow<R>, String> {
    let (width, height) = (config.settings.window.width, config.settings.window.height);
    let title = window_title(&config.settings.display_name, &profile.name);

    let icon = app
        .try_state::<AppState>()
        .map(|state| state.app_icon(config.settings.icon_path.as_deref()));

    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(f64::from(width), f64::from(height))
        .min_inner_size(640.0, 480.0)
        .center()
        .visible(false)
        .initialization_script(ui_script::bootstrap_script(
            label,
            WindowKind::Main,
            Some(&profile.id),
        ));

    if let Some(icon) = icon {
        builder = builder
            .icon(icon.to_tauri())
            .map_err(|error| error.to_string())?;
    }

    let window = builder.build().map_err(|error| error.to_string())?;
    Ok(window)
}

/// Opens the (single) settings window.
pub fn open_settings_window<R: Runtime>(
    app: &AppHandle<R>,
    _from: WindowKind,
) -> Result<WebviewWindow<R>, String> {
    if let Some(existing) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(existing);
    }

    let Some(state) = app.try_state::<AppState>() else {
        return Err("application state is not ready".to_owned());
    };
    let config = state.snapshot();
    let title = i18n::lookup(&config.settings.locale, "settings.title");

    let mut builder =
        WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
            .title(title)
            .inner_size(980.0, 760.0)
            .min_inner_size(760.0, 560.0)
            .center()
            .visible(false)
            .initialization_script(ui_script::bootstrap_script(
                SETTINGS_LABEL,
                WindowKind::Settings,
                None,
            ));

    if let Some(icon) = app
        .try_state::<AppState>()
        .map(|state| state.app_icon(config.settings.icon_path.as_deref()))
    {
        builder = builder
            .icon(icon.to_tauri())
            .map_err(|error| error.to_string())?;
    }

    let window = builder.build().map_err(|error| error.to_string())?;
    let _ = window.show();
    let _ = window.set_focus();
    Ok(window)
}

/// Focuses an existing launcher window or opens a new one.
pub fn show_or_create_main<R: Runtime>(
    app: &AppHandle<R>,
    profile_id: Option<&str>,
) -> Result<WebviewWindow<R>, String> {
    if let Some(window) = app.get_webview_window(MAIN_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        if let Some(id) = profile_id {
            let _ = window.eval(format!(
                "window.__TAURI3080_SWITCH_PROFILE__ && window.__TAURI3080_SWITCH_PROFILE__({})",
                serde_json::to_string(id).unwrap_or_else(|_| "\"\"".to_owned())
            ));
        }
        return Ok(window);
    }
    open_main_window(app, profile_id)
}

/// Rewrites every window title after a rename.
pub fn refresh_titles<R: Runtime>(app: &AppHandle<R>, display_name: &str) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let config = state.snapshot();
    for (label, window) in app.webview_windows() {
        let title = if label == SETTINGS_LABEL {
            i18n::lookup(&config.settings.locale, "settings.title")
        } else {
            let profile_name = profile_for_window(&label)
                .and_then(|id| config.profiles.get(&id).map(|profile| profile.name.clone()))
                .unwrap_or_else(|| config.boot_profile().name);
            window_title(display_name, &profile_name)
        };
        let _ = window.set_title(&title);
    }
}

/// Applies the current icon to every open window.
pub fn refresh_icons<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let config = state.snapshot();
    let icon = state.app_icon(config.settings.icon_path.as_deref());
    let image = icon.to_tauri();
    for window in app.webview_windows().values() {
        let _ = window.set_icon(image.clone());
        crate::platform_info::refresh_window_icon(window, &icon);
    }

    let _ = crate::platform_info::set_application_icon(&icon, &config.settings.display_name);
}

/// URL of a bundled page, honouring the dev server when running `tauri dev`.
#[must_use]
pub fn launcher_url<R: Runtime>(app: &AppHandle<R>, page: &str) -> String {
    let page = page.trim_start_matches('/');
    // `is_dev()` is true for `cargo build` / `tauri dev`, where the UI is served
    // by Vite; release builds always use the assets embedded in the binary.
    if tauri::is_dev() {
        if let Some(dev_url) = app.config().build.dev_url.as_ref() {
            return format!("{}/{page}", dev_url.as_str().trim_end_matches('/'));
        }
    }
    if cfg!(windows) {
        format!("http://tauri.localhost/{page}")
    } else {
        format!("tauri://localhost/{page}")
    }
}

/// Title bar text.
#[must_use]
pub fn window_title(display_name: &str, profile_name: &str) -> String {
    if profile_name.is_empty() || profile_name == display_name {
        display_name.to_owned()
    } else {
        format!("{display_name} — {profile_name}")
    }
}

/// Returns every open window to its bundled page, re-running the hook.
///
/// Reloading with `window.location.reload()` would refresh the remote page
/// instead, which defeats the point: the pre-launch script would not run again
/// and the user would stay on a service that may have been restarted.
///
/// Returns the number of windows that were asked to navigate.
pub fn reload_all<R: Runtime>(app: &AppHandle<R>) -> usize {
    let mut reloaded = 0;
    for (label, window) in app.webview_windows() {
        let page = if label == SETTINGS_LABEL {
            "settings.html"
        } else {
            "index.html"
        };
        if let Ok(url) = tauri::Url::parse(&launcher_url(app, page)) {
            if window.navigate(url).is_ok() {
                reloaded += 1;
            }
        }
    }
    reloaded
}

/// Forgets a closed window.
pub fn on_window_closed(label: &str) {
    unbind_window(label);
}

/// True when no launcher window is open any more.
#[must_use]
pub fn no_main_windows<R: Runtime>(app: &AppHandle<R>) -> bool {
    !app.webview_windows()
        .keys()
        .any(|label| label == MAIN_LABEL || label.starts_with("main-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_titles_are_derived_from_the_profile() {
        assert_eq!(window_title("My App", "My App"), "My App");
        assert_eq!(window_title("My App", "Work"), "My App — Work");
        assert_eq!(window_title("My App", ""), "My App");
    }

    #[test]
    fn labels_map_to_window_kinds() {
        assert_eq!(kind_of("main"), WindowKind::Main);
        assert_eq!(kind_of("main-3"), WindowKind::Main);
        assert_eq!(kind_of("settings"), WindowKind::Settings);
    }

    #[test]
    fn binding_is_scoped_to_the_window_label() {
        bind_window("main-9", "default");
        assert_eq!(profile_for_window("main-9").as_deref(), Some("default"));
        unbind_window("main-9");
        assert!(profile_for_window("main-9").is_none());
    }
}
