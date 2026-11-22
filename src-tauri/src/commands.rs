//! IPC commands exposed to the webview.
//!
//! `#[tauri::command]` deserialises every argument by value, so the usual
//! "pass by reference" advice does not apply here.
#![allow(clippy::needless_pass_by_value)]
//!
//! The frontend never touches the filesystem directly: every mutation goes
//! through one of these commands, which validate their input, update the shared
//! [`AppState`] and emit `config-changed` / `prelaunch-progress` events so all
//! open windows stay in sync.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

use crate::config::{self, AppConfig, AppSettings, PrelaunchMode, Profile};
use crate::error::Error;
use crate::platform;
use crate::platform_info;
use crate::prelaunch::{self, Cancellation, PrelaunchOutcome, PrelaunchProgress};
use crate::state::{AppState, Bootstrap, Origin, Paths, PlatformInfo, WindowKind};
use crate::windows;

/// Event emitted whenever the configuration changes.
pub const EVENT_CONFIG_CHANGED: &str = "config-changed";
/// Event emitted while a pre-launch hook runs.
pub const EVENT_PRELAUNCH_PROGRESS: &str = "prelaunch-progress";
/// Event emitted when a window asks the others to open settings.
pub const EVENT_OPEN_SETTINGS: &str = "open-settings";

/// Payload of [`EVENT_CONFIG_CHANGED`].
#[derive(Debug, Clone, Serialize)]
pub struct ConfigChanged {
    pub settings: AppSettings,
    pub profiles: Vec<Profile>,
    pub repairs: Vec<String>,
}

/// Validated input for creating or updating a profile.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInput {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub icon_path: Option<String>,
    #[serde(default)]
    pub prelaunch: PrelaunchMode,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub wait_timeout_secs: Option<u64>,
    #[serde(default)]
    pub kill_previous: Option<bool>,
}

/// Validated input for the settings page.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub locale: String,
    pub display_name: String,
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub close_to_tray: Option<bool>,
}

/// Result of saving an icon.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconUpdate {
    pub icon_path: Option<String>,
    pub custom: bool,
}

/// Result of an explicit pre-launch run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrelaunchRunResult {
    pub outcome: PrelaunchOutcome,
    pub cancelled: bool,
}

fn state_of(app: &AppHandle) -> Result<&AppState, Error> {
    app.try_state::<AppState>()
        .map(|state| state.inner())
        .ok_or_else(|| Error::Message("application state is not ready".to_owned()))
}

fn broadcast(app: &AppHandle, repairs: &[String]) {
    let Ok(state) = state_of(app) else {
        return;
    };
    let config = state.snapshot();
    let payload = ConfigChanged {
        settings: config.settings.clone(),
        profiles: config.profiles.values().cloned().collect(),
        repairs: repairs.to_vec(),
    };
    let _ = app.emit(EVENT_CONFIG_CHANGED, payload);
}

/// Complete snapshot for a window that just loaded.
#[tauri::command]
pub fn bootstrap(window: WebviewWindow, state: State<'_, AppState>) -> Bootstrap {
    let config = state.snapshot();
    let label = window.label().to_owned();
    let kind = windows::kind_of(&label);
    let profile_id = if kind == WindowKind::Main {
        windows::profile_for_window(&label).or_else(|| Some(config.boot_profile().id))
    } else {
        None
    };

    Bootstrap {
        settings: config.settings.clone(),
        profiles: config.profiles.values().cloned().collect(),
        platform: AppState::platform_info(),
        paths: AppState::paths(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        origin: Origin {
            label,
            kind,
            profile_id,
        },
        repairs: state.repairs(),
    }
}

/// Current configuration, without the platform metadata.
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.snapshot()
}

/// Re-reads `config.toml` from disk.
#[tauri::command]
pub fn reload_config(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<String>, Error> {
    let app = window.app_handle().clone();
    let repairs = state.reload()?;
    broadcast(&app, &repairs);
    let settings = state.settings();
    windows::refresh_titles(&app, &settings.display_name);
    windows::refresh_icons(&app);
    crate::menus::refresh(&app);
    Ok(repairs)
}

/// Renders the current configuration as TOML.
#[tauri::command]
pub fn config_as_toml(state: State<'_, AppState>) -> Result<String, Error> {
    toml::to_string(&state.snapshot()).map_err(Error::from)
}

/// Paths, for the "open folder" buttons.
#[tauri::command]
pub fn get_paths() -> Paths {
    AppState::paths()
}

/// Static platform information.
#[tauri::command]
pub fn get_platform() -> PlatformInfo {
    AppState::platform_info()
}

/// Updates the settings page values.
#[tauri::command]
pub fn save_settings(
    window: WebviewWindow,
    state: State<'_, AppState>,
    input: SettingsInput,
) -> Result<Vec<String>, Error> {
    let mut config = state.snapshot();
    config.settings.locale = crate::i18n::normalize_locale(&input.locale);
    config.settings.display_name = input.display_name.trim().to_owned();
    if let Some(default_profile) = input.default_profile {
        config.settings.default_profile = default_profile;
    }
    if let Some(close_to_tray) = input.close_to_tray {
        config.settings.close_to_tray = close_to_tray;
    }

    let repairs = state.update(config)?;
    let app = window.app_handle().clone();
    let settings = state.settings();
    windows::refresh_titles(&app, &settings.display_name);
    crate::menus::refresh(&app);
    broadcast(&app, &repairs);
    Ok(repairs)
}

/// Creates a new profile.
#[tauri::command]
pub fn create_profile(
    window: WebviewWindow,
    state: State<'_, AppState>,
    input: ProfileInput,
) -> Result<Profile, Error> {
    let mut config = state.snapshot();
    let id = unique_profile_id(&config, &input.name);
    let profile = build_profile(id.clone(), &input)?;

    if config.profiles.contains_key(&id) {
        return Err(Error::DuplicateProfile(id));
    }
    config.profiles.insert(id.clone(), profile.clone());
    state.update(config)?;

    config::ensure_example_script(&profile).ok();
    let app = window.app_handle().clone();
    broadcast(&app, &[]);
    crate::menus::refresh(&app);
    Ok(profile)
}

/// Updates an existing profile.
#[tauri::command]
pub fn update_profile(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    input: ProfileInput,
) -> Result<Profile, Error> {
    let mut config = state.snapshot();
    if !config.profiles.contains_key(&id) {
        return Err(Error::UnknownProfile(id));
    }
    let profile = build_profile(id.clone(), &input)?;
    config.profiles.insert(id, profile.clone());
    state.update(config)?;

    let app = window.app_handle().clone();
    broadcast(&app, &[]);
    crate::menus::refresh(&app);
    windows::refresh_titles(&app, &profile.name);
    Ok(profile)
}

/// Deletes a profile and its script directory.
#[tauri::command]
pub fn delete_profile(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>, Error> {
    let mut config = state.snapshot();
    if config.profiles.len() <= 1 {
        return Err(Error::Invalid {
            field: "profile",
            reason: "at least one profile must remain".to_owned(),
        });
    }
    config.profiles.remove(&id);
    if config.settings.default_profile == id {
        if let Some(first) = config.profiles.keys().next().cloned() {
            config.settings.default_profile = first;
        }
    }
    let repairs = state.update(config)?;

    let script_dir = platform::script_dir_for(&id);
    if script_dir.starts_with(platform::config_dir()) && script_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&script_dir);
    }

    let app = window.app_handle().clone();
    broadcast(&app, &repairs);
    crate::menus::refresh(&app);
    Ok(repairs)
}

fn unique_profile_id(config: &AppConfig, name: &str) -> String {
    let base = platform::sanitize_component(&slugify(name));
    let base = if base == platform::DEFAULT_PROFILE_ID && !config.profiles.is_empty() {
        "profile".to_owned()
    } else {
        base
    };
    if !config.profiles.contains_key(&base) {
        return base;
    }
    for suffix in 2..1000 {
        let candidate = format!("{base}-{suffix}");
        if !config.profiles.contains_key(&candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", std::process::id())
}

/// Turns a display name into an identifier-safe slug.
#[must_use]
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "profile".to_owned()
    } else {
        trimmed
    }
}

fn build_profile(id: String, input: &ProfileInput) -> Result<Profile, Error> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(Error::Invalid {
            field: "name",
            reason: "the profile needs a name".to_owned(),
        });
    }
    if name.chars().count() > 64 {
        return Err(Error::Invalid {
            field: "name",
            reason: "the name must be at most 64 characters".to_owned(),
        });
    }
    config::validate_url(&input.url)?;

    let mut profile = Profile {
        id,
        name: name.to_owned(),
        url: input.url.trim().to_owned(),
        icon_path: input
            .icon_path
            .clone()
            .filter(|name| config::is_safe_file_name(name)),
        prelaunch: input.prelaunch,
        script: input
            .script
            .clone()
            .filter(|name| config::is_safe_file_name(name)),
        args: input.args.iter().take(32).cloned().collect(),
        env: input
            .env
            .iter()
            .take(128)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        timeout_secs: input.timeout_secs.unwrap_or(config::DEFAULT_TIMEOUT_SECS),
        wait_timeout_secs: input
            .wait_timeout_secs
            .unwrap_or(config::DEFAULT_WAIT_TIMEOUT_SECS),
        kill_previous: input.kill_previous.unwrap_or(true),
    };
    let mut repairs = Vec::new();
    profile.normalize(&mut repairs);
    Ok(profile)
}

/// Copies the chosen image into the config directory and applies it.
#[tauri::command]
pub fn set_app_icon(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<IconUpdate, Error> {
    let source = PathBuf::from(&path);
    let stored = crate::icons::import_icon(&source)?;

    let mut config = state.snapshot();
    config.settings.icon_path = Some(stored.clone());
    state.update(config)?;

    let app = window.app_handle().clone();
    windows::refresh_icons(&app);
    broadcast(&app, &[]);
    Ok(IconUpdate {
        icon_path: Some(stored),
        custom: true,
    })
}

/// Restores the icon embedded in the binary.
#[tauri::command]
pub fn reset_app_icon(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<IconUpdate, Error> {
    let mut config = state.snapshot();
    config.settings.icon_path = None;
    state.update(config)?;

    let stored = platform::icons_dir().join(crate::icons::CUSTOM_ICON_FILE);
    if stored.is_file() {
        let _ = std::fs::remove_file(stored);
    }

    let app = window.app_handle().clone();
    windows::refresh_icons(&app);
    broadcast(&app, &[]);
    Ok(IconUpdate {
        icon_path: None,
        custom: false,
    })
}

/// Copies the chosen image into the config directory and binds it to a profile.
#[tauri::command]
pub fn set_profile_icon(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<IconUpdate, Error> {
    let bytes = std::fs::read(&path)?;
    crate::icons::decode_png(&bytes)?;

    let file_name = format!("profile-{}.png", platform::sanitize_component(&id));
    config::atomic_write(&platform::icons_dir().join(&file_name), &bytes)?;

    let mut config = state.snapshot();
    let profile = config
        .profiles
        .get_mut(&id)
        .ok_or_else(|| Error::UnknownProfile(id.clone()))?;
    profile.icon_path = Some(file_name.clone());
    state.update(config)?;

    let app = window.app_handle().clone();
    broadcast(&app, &[]);
    Ok(IconUpdate {
        icon_path: Some(file_name),
        custom: true,
    })
}

/// Returns the current app icon as a `data:` URL.
///
/// The settings window uses it for the live preview; `asset:` URLs are
/// deliberately not enabled, so base64 keeps the CSP surface small.
#[tauri::command]
pub fn app_icon_data_url(state: State<'_, AppState>) -> String {
    let config = state.snapshot();
    let (bitmap, _custom) = crate::icons::load_icon(config.settings.icon_path.as_deref());
    let png = crate::icons::resized_png(&bitmap, 256);
    if png.is_empty() {
        return String::new();
    }
    format!("data:image/png;base64,{}", base64_encode(&png))
}

/// Minimal base64 encoder (standard alphabet, padded).
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Opens a native file picker and imports the selected image as the app icon.
#[tauri::command]
pub async fn pick_app_icon(window: WebviewWindow) -> Result<Option<IconUpdate>, Error> {
    use tauri_plugin_dialog::DialogExt;

    let picked = window
        .app_handle()
        .dialog()
        .file()
        .add_filter("Image", &["png", "jpg", "jpeg", "webp", "ico", "gif"])
        .blocking_pick_file();

    let Some(file_path) = picked else {
        return Ok(None);
    };
    let path = file_path
        .simplified()
        .into_path()
        .map_err(|error| Error::Message(error.to_string()))?;

    let app = window.app_handle().clone();
    let state = state_of(&app)?.snapshot();
    let stored = crate::icons::import_icon(&path)?;

    let mut config = state;
    config.settings.icon_path = Some(stored.clone());
    state_of(&app)?.update(config)?;
    windows::refresh_icons(&app);
    broadcast(&app, &[]);

    Ok(Some(IconUpdate {
        icon_path: Some(stored),
        custom: true,
    }))
}

/// Runs the pre-launch hook for `id` and streams progress to the window.
#[tauri::command]
pub async fn run_prelaunch(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<PrelaunchRunResult, Error> {
    let profile = state.profile(&id)?;
    if profile.kill_previous {
        prelaunch::kill_running(&id);
    }

    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    let cancellation = Cancellation::new();
    let cancel_for_task = cancellation.clone();
    let target = app.clone();

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        prelaunch::run_with_progress(&profile, &cancel_for_task, |progress| {
            let payload: PrelaunchProgress = progress;
            let _ = target.emit_to(&label, EVENT_PRELAUNCH_PROGRESS, payload);
        })
    })
    .await
    .map_err(|error| Error::Message(error.to_string()))?;

    Ok(PrelaunchRunResult {
        cancelled: outcome.status == prelaunch::OutcomeStatus::Cancelled,
        outcome,
    })
}

/// Navigates the calling window to `url`.
///
/// This is the last step of the launcher page: once the hook has finished it
/// replaces the bundled UI with the profile URL.
#[tauri::command]
pub fn open_profile_url(window: WebviewWindow, url: String) -> Result<(), Error> {
    config::validate_url(&url)?;
    let parsed =
        tauri::Url::parse(url.trim()).map_err(|error| Error::InvalidUrl(error.to_string()))?;
    window.navigate(parsed)?;
    Ok(())
}

/// Brings the launcher page back after a remote page replaced it.
#[tauri::command]
pub fn go_home(window: WebviewWindow) -> Result<(), Error> {
    let page = if window.label() == windows::SETTINGS_LABEL {
        "settings.html"
    } else {
        "index.html"
    };
    let target = windows::launcher_url(&window.app_handle().clone(), page);
    window.navigate(tauri::Url::parse(&target)?)?;
    Ok(())
}

/// Cancels the hook running for `id`, if any.
#[tauri::command]
pub fn cancel_prelaunch(id: String) -> bool {
    prelaunch::kill_running(&id)
}

/// Opens a new launcher window.
#[tauri::command]
pub fn open_window(
    window: WebviewWindow,
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> Result<(), Error> {
    let _ = state;
    windows::open_main_window(&window.app_handle().clone(), profile_id.as_deref())
        .map(|_| ())
        .map_err(Error::Message)
}

/// Opens (or focuses) the settings window.
#[tauri::command]
pub fn open_settings(window: WebviewWindow) -> Result<(), Error> {
    windows::open_settings_window(&window.app_handle().clone(), WindowKind::Main)
        .map(|_| ())
        .map_err(Error::Message)
}

/// Closes the calling window.
#[tauri::command]
pub fn close_window(window: WebviewWindow) {
    let _ = window.close();
}

/// Reveals a directory in the OS file manager.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), Error> {
    let resolved = PathBuf::from(&path);
    let config_dir = platform::config_dir();
    if !resolved.starts_with(&config_dir) {
        return Err(Error::Invalid {
            field: "path",
            reason: "only paths inside the configuration directory can be revealed".to_owned(),
        });
    }
    platform_info::reveal_in_file_manager(&resolved).map_err(Error::Message)
}

/// Ensures the script directory for `id` exists and returns its path.
#[tauri::command]
pub fn ensure_script_dir(state: State<'_, AppState>, id: String) -> Result<String, Error> {
    let profile = state.profile(&id)?;
    config::ensure_example_script(&profile)?;
    Ok(platform::script_dir_for(&id).to_string_lossy().into_owned())
}

/// Window kind of the calling webview, for the frontend router.
#[tauri::command]
pub fn window_kind(window: WebviewWindow) -> WindowKind {
    windows::kind_of(window.label())
}

/// Records a frontend error so `--smoke` runs can explain a blank window.
#[tauri::command]
pub fn report_frontend_error(message: String) {
    eprintln!("tauri-3080: frontend error: {message}");
    crate::state::record_frontend_error(message);
}

/// Reports that the frontend finished booting.
///
/// The smoke mode waits for this flag: it is the only signal that proves the
/// bundled HTML, the ES module graph and the IPC bridge all work together.
#[tauri::command]
pub fn report_ready(window: WebviewWindow) {
    if let Some(state) = window.app_handle().try_state::<AppState>() {
        state.counters.window_opened();
        crate::state::mark_frontend_ready();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_reference_encoding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn slugifies_names() {
        assert_eq!(slugify("My Workspace"), "my-workspace");
        assert_eq!(slugify("  Chat  "), "chat");
        assert_eq!(slugify("工作区"), "profile");
        assert_eq!(slugify("a///b"), "a-b");
        assert_eq!(slugify(""), "profile");
        assert_eq!(slugify("A_B"), "a-b");
    }

    #[test]
    fn unique_ids_never_collide() {
        let mut config = AppConfig::initial();
        assert_eq!(unique_profile_id(&config, "Chat"), "chat");
        config.profiles.insert(
            "chat".to_owned(),
            Profile {
                id: "chat".to_owned(),
                ..Profile::default()
            },
        );
        assert_eq!(unique_profile_id(&config, "Chat"), "chat-2");
    }

    #[test]
    fn builds_and_validates_profiles() {
        let input = ProfileInput {
            name: " Work ".to_owned(),
            url: "http://127.0.0.1:3080".to_owned(),
            icon_path: Some("../evil.png".to_owned()),
            prelaunch: PrelaunchMode::WaitForPort,
            script: Some("start.sh".to_owned()),
            args: vec!["--flag".to_owned()],
            env: BTreeMap::from([("A".to_owned(), "1".to_owned())]),
            timeout_secs: Some(90),
            wait_timeout_secs: None,
            kill_previous: None,
        };
        let profile = build_profile("work".to_owned(), &input).unwrap();
        assert_eq!(profile.name, "Work");
        assert!(profile.icon_path.is_none(), "unsafe icon dropped");
        assert_eq!(profile.script.as_deref(), Some("start.sh"));
        assert_eq!(profile.timeout_secs, 90);
        assert_eq!(profile.wait_timeout_secs, config::DEFAULT_WAIT_TIMEOUT_SECS);
        assert!(profile.kill_previous);
    }

    #[test]
    fn rejects_invalid_profiles() {
        let input = ProfileInput {
            name: "  ".to_owned(),
            url: "http://127.0.0.1:3080".to_owned(),
            icon_path: None,
            prelaunch: PrelaunchMode::Run,
            script: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            timeout_secs: None,
            wait_timeout_secs: None,
            kill_previous: None,
        };
        assert!(build_profile("x".to_owned(), &input).is_err());

        let input = ProfileInput {
            name: "ok".to_owned(),
            url: "javascript:alert(1)".to_owned(),
            ..input
        };
        assert!(build_profile("x".to_owned(), &input).is_err());
    }
}
