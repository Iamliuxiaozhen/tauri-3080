//! Shared application state.
//!
//! Exactly one [`AppState`] lives for the whole process and is shared by every
//! window: the configuration is the single source of truth, so a rename made in
//! the settings window is visible in the launcher window immediately.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;

use crate::config::{AppConfig, AppSettings, ConfigFile, Profile};
use crate::error::Error;
use crate::icons::IconBitmap;

/// Window kinds the launcher can open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowKind {
    /// The home / launcher window.
    Main,
    /// The settings window.
    Settings,
}

/// A snapshot handed to the frontend on boot.
#[derive(Debug, Clone, Serialize)]
pub struct Bootstrap {
    pub settings: AppSettings,
    pub profiles: Vec<Profile>,
    pub platform: PlatformInfo,
    pub paths: Paths,
    pub version: String,
    pub origin: Origin,
    pub repairs: Vec<String>,
}

/// Where the calling window lives.
#[derive(Debug, Clone, Serialize)]
pub struct Origin {
    pub label: String,
    pub kind: WindowKind,
    /// Profile this window is bound to (`None` for the settings window).
    pub profile_id: Option<String>,
}

/// Static facts about the running build.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub arch: &'static str,
    /// File name of the pre-launch script auto-detected on this platform.
    pub script_name: &'static str,
    /// Shell used to run the script, for the UI hint.
    pub shell: &'static str,
}

/// Paths shown in the settings window.
#[derive(Debug, Clone, Serialize)]
pub struct Paths {
    pub config_dir: String,
    pub config_file: String,
    pub scripts_dir: String,
    pub icons_dir: String,
    pub logs_dir: String,
}

/// Set once the frontend has called `report_ready`.
static FRONTEND_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Last error the frontend reported, if any.
static FRONTEND_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Stores a frontend error for the smoke report.
pub fn record_frontend_error(message: String) {
    if let Ok(mut slot) = FRONTEND_ERROR.lock() {
        *slot = Some(message);
    }
}

/// The last frontend error, if any.
pub fn frontend_error() -> Option<String> {
    FRONTEND_ERROR.lock().ok().and_then(|slot| slot.clone())
}

/// Records that a window finished booting its UI.
pub fn mark_frontend_ready() {
    FRONTEND_READY.store(true, Ordering::SeqCst);
}

/// Whether any window reported that its UI is ready.
#[must_use]
pub fn frontend_ready() -> bool {
    FRONTEND_READY.load(Ordering::SeqCst)
}

/// Boot counters used by the smoke test and the "about" panel.
#[derive(Debug, Default)]
pub struct RuntimeCounters {
    opened_windows: AtomicU32,
}

impl RuntimeCounters {
    pub fn window_opened(&self) -> u32 {
        self.opened_windows.fetch_add(1, Ordering::SeqCst) + 1
    }

    #[must_use]
    pub fn opened(&self) -> u32 {
        self.opened_windows.load(Ordering::SeqCst)
    }
}

/// Process-wide state.
pub struct AppState {
    file: Mutex<ConfigFile>,
    config: Mutex<AppConfig>,
    repairs: Mutex<Vec<String>>,
    icon: Mutex<IconCache>,
    pub counters: RuntimeCounters,
}

#[derive(Debug, Default)]
struct IconCache {
    /// Signature of the bitmap currently in memory.
    signature: String,
    bitmap: Option<IconBitmap>,
}

impl AppState {
    /// Loads the configuration and prepares the directory layout.
    pub fn bootstrap() -> Result<Self, Error> {
        crate::config::ensure_layout()?;
        let file = ConfigFile::load()?;
        let mut config = file.typed()?;
        let repairs = config.normalize();

        // Make sure at least one profile has a hook to edit.
        if let Some(profile) = config.profiles.get(&config.settings.default_profile) {
            let _ = crate::config::ensure_example_script(profile);
        }

        Ok(Self {
            file: Mutex::new(file),
            config: Mutex::new(config),
            repairs: Mutex::new(repairs),
            icon: Mutex::new(IconCache::default()),
            counters: RuntimeCounters::default(),
        })
    }

    fn lock_config(&self) -> MutexGuard<'_, AppConfig> {
        self.config
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock_file(&self) -> MutexGuard<'_, ConfigFile> {
        self.file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Current typed configuration.
    pub fn snapshot(&self) -> AppConfig {
        self.lock_config().clone()
    }

    /// Current settings.
    pub fn settings(&self) -> AppSettings {
        self.lock_config().settings.clone()
    }

    /// A single profile.
    pub fn profile(&self, id: &str) -> Result<Profile, Error> {
        self.lock_config().profile(id).cloned()
    }

    /// Repairs applied on the last load.
    pub fn repairs(&self) -> Vec<String> {
        self.repairs
            .lock()
            .map(|repairs| repairs.clone())
            .unwrap_or_default()
    }

    /// Replaces the configuration, persists it and returns the repairs applied.
    ///
    /// Both the typed snapshot and the `toml_edit` document are updated, so the
    /// comments in a hand-written file survive a save from the UI.
    pub fn update(&self, mut config: AppConfig) -> Result<Vec<String>, Error> {
        let repairs = config.normalize();
        {
            let mut file = self.lock_file();
            apply_to_document(&mut file, &config)?;
            file.save()?;
        }
        *self.lock_config() = config;
        if let Ok(mut stored) = self.repairs.lock() {
            *stored = repairs.clone();
        }
        Ok(repairs)
    }

    /// Re-reads the configuration from disk (used after the user edits the
    /// file by hand).
    pub fn reload(&self) -> Result<Vec<String>, Error> {
        let file = ConfigFile::load()?;
        let mut config = file.typed()?;
        let repairs = config.normalize();
        {
            let mut stored = self.lock_file();
            *stored = file;
        }
        *self.lock_config() = config;
        if let Ok(mut stored) = self.repairs.lock() {
            *stored = repairs.clone();
        }
        Ok(repairs)
    }

    /// Loads the configured app icon, caching the decoded bitmap.
    pub fn app_icon(&self, icon_path: Option<&str>) -> IconBitmap {
        let (bitmap, _custom) = crate::icons::load_icon(icon_path);
        let signature = bitmap.signature();
        if let Ok(mut cache) = self.icon.lock() {
            if cache.signature == signature {
                if let Some(cached) = cache.bitmap.clone() {
                    return cached;
                }
            }
            cache.signature = signature;
            cache.bitmap = Some(bitmap.clone());
        }
        bitmap
    }

    /// Paths shown in the settings page.
    #[must_use]
    pub fn paths() -> Paths {
        let dir = crate::platform::config_dir();
        Paths {
            config_dir: dir.to_string_lossy().into_owned(),
            config_file: crate::platform::main_config_path()
                .to_string_lossy()
                .into_owned(),
            scripts_dir: crate::platform::scripts_dir()
                .to_string_lossy()
                .into_owned(),
            icons_dir: crate::platform::icons_dir().to_string_lossy().into_owned(),
            logs_dir: crate::platform::logs_dir().to_string_lossy().into_owned(),
        }
    }

    /// Static platform information.
    #[must_use]
    pub fn platform_info() -> PlatformInfo {
        PlatformInfo {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            script_name: if cfg!(windows) {
                "pre-launch.ps1"
            } else {
                "pre-launch.sh"
            },
            shell: if cfg!(windows) {
                "PowerShell / cmd"
            } else {
                "sh"
            },
        }
    }
}

/// Mirrors the typed configuration into the editable document.
///
/// The typed snapshot is serialised first and then merged into the existing
/// document, so comments and unknown keys survive while deleted profiles and
/// settings disappear.
fn apply_to_document(file: &mut ConfigFile, config: &AppConfig) -> Result<(), Error> {
    let serialized = toml::to_string(config)?;
    let fresh: toml_edit::DocumentMut = serialized.parse()?;
    file.merge_from(&fresh);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PrelaunchMode;

    fn with_temp_config<F: FnOnce()>(body: F) {
        let dir = tempfile::tempdir().unwrap();
        crate::platform::with_config_dir_override(dir.path(), body);
    }

    #[test]
    fn bootstrap_creates_files_and_an_example_hook() {
        with_temp_config(|| {
            let state = AppState::bootstrap().unwrap();
            let config = state.snapshot();
            assert_eq!(
                config.settings.display_name,
                crate::app_id::DEFAULT_DISPLAY_NAME
            );
            assert!(crate::platform::main_config_path().is_file());
            let script = crate::platform::script_dir_for("default").join("pre-launch.sh");
            assert!(script.is_file() || cfg!(windows));
        });
    }

    #[test]
    fn updates_persist_and_keep_comments() {
        with_temp_config(|| {
            let state = AppState::bootstrap().unwrap();
            let mut config = state.snapshot();
            config.settings.display_name = "My Workspace".to_owned();
            config.settings.locale = "pt-BR".to_owned();
            state.update(config).unwrap();

            let raw = std::fs::read_to_string(crate::platform::main_config_path()).unwrap();
            assert!(raw.contains("# tauri-3080 configuration"));
            assert!(raw.contains("display_name = \"My Workspace\""));
            assert!(raw.contains("locale = \"pt-BR\""));

            let reloaded = AppState::bootstrap().unwrap().snapshot();
            assert_eq!(reloaded.settings.display_name, "My Workspace");
            assert_eq!(reloaded.settings.locale, "pt-BR");
        });
    }

    #[test]
    fn adding_a_profile_round_trips() {
        with_temp_config(|| {
            let state = AppState::bootstrap().unwrap();
            let mut config = state.snapshot();
            let profile = Profile {
                id: "second".to_owned(),
                name: "Second".to_owned(),
                url: "http://127.0.0.1:4000".to_owned(),
                prelaunch: PrelaunchMode::Skip,
                ..Profile::default()
            };
            config.profiles.insert(profile.id.clone(), profile);
            state.update(config).unwrap();

            let reloaded = AppState::bootstrap().unwrap().snapshot();
            assert_eq!(reloaded.profiles.len(), 2);
            assert_eq!(reloaded.profiles["second"].url, "http://127.0.0.1:4000");
            assert_eq!(reloaded.profiles["second"].prelaunch, PrelaunchMode::Skip);
        });
    }

    #[test]
    fn deleting_a_profile_removes_it_from_disk() {
        with_temp_config(|| {
            let state = AppState::bootstrap().unwrap();
            let mut config = state.snapshot();
            config.profiles.insert(
                "temp".to_owned(),
                Profile {
                    id: "temp".to_owned(),
                    name: "Temp".to_owned(),
                    ..Profile::default()
                },
            );
            state.update(config).unwrap();
            let mut config = state.snapshot();
            config.profiles.remove("temp");
            state.update(config).unwrap();

            let raw = std::fs::read_to_string(crate::platform::main_config_path()).unwrap();
            assert!(!raw.contains("[profile.temp]"));
        });
    }

    #[test]
    fn counters_increment() {
        let counters = RuntimeCounters::default();
        assert_eq!(counters.window_opened(), 1);
        assert_eq!(counters.window_opened(), 2);
        assert_eq!(counters.opened(), 2);
    }
}
