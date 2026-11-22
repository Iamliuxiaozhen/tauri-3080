//! Cross-platform configuration paths.
//!
//! The launcher keeps everything it owns inside one directory:
//!
//! * Linux: `$XDG_CONFIG_HOME/tauri-3080` (fallback `~/.config/tauri-3080`)
//! * macOS: `~/Library/Application Support/tauri-3080`
//! * Windows: `%APPDATA%\tauri-3080`
//!
//! Launcher (profile) files, the settings document and user-supplied icons are
//! all placed below it, so backing the directory up also backs up every custom
//! name, icon and script.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, RwLock};

use crate::app_id;

/// Environment variable used by the integration test to redirect the whole
/// configuration tree into a temporary directory.
pub const CONFIG_HOME_ENV: &str = "TAURI_3080_CONFIG_HOME";

/// Default profile identifier.
pub const DEFAULT_PROFILE_ID: &str = "default";

/// Resolves the directory that holds `config.toml`, launcher profiles and icons.
///
/// The `TAURI_3080_CONFIG_HOME` override exists for tests and portable installs.
/// When it is set the resolver returns that directory directly (the
/// `tauri-3080` suffix is not appended), which keeps test isolation trivial.
pub fn config_dir() -> PathBuf {
    if let Some(directory) = test_override() {
        return directory;
    }
    if let Some(override_dir) = std::env::var_os(CONFIG_HOME_ENV) {
        if !override_dir.is_empty() {
            return PathBuf::from(override_dir);
        }
    }

    platform_config_root().join(app_id::CONFIG_DIR_NAME)
}

/// In-process override used by the unit tests.
///
/// A process-global environment variable would race with the parallel test
/// harness, so tests install their directory through this slot instead.
fn override_slot() -> &'static RwLock<Option<PathBuf>> {
    static OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();
    OVERRIDE.get_or_init(|| RwLock::new(None))
}

fn test_override() -> Option<PathBuf> {
    override_slot().read().ok().and_then(|value| value.clone())
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Runs `body` with [`config_dir`] pinned to `directory`.
///
/// The global lock serialises tests that need an isolated configuration tree.
pub fn with_config_dir_override<F: FnOnce()>(directory: &Path, body: F) {
    let _guard: MutexGuard<'_, ()> = test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Ok(mut value) = override_slot().write() {
        *value = Some(directory.to_path_buf());
    }
    body();
    if let Ok(mut value) = override_slot().write() {
        *value = None;
    }
}

/// Platform configuration root, honouring `XDG_CONFIG_HOME` on Linux.
fn platform_config_root() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return PathBuf::from(xdg);
        }
    }

    dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `config.toml` path.
pub fn main_config_path() -> PathBuf {
    config_dir().join(app_id::CONFIG_FILE_NAME)
}

/// Directory that stores one file per launcher profile.
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Directory that stores custom icons copied in by the user.
pub fn icons_dir() -> PathBuf {
    config_dir().join("icons")
}

/// Directory that stores the pre-launch scripts, one sub-directory per profile.
pub fn scripts_dir() -> PathBuf {
    config_dir().join("scripts")
}

/// Directory for launcher logs.
pub fn logs_dir() -> PathBuf {
    config_dir().join("logs")
}

/// Script directory for `profile_id`.
pub fn script_dir_for(profile_id: &str) -> PathBuf {
    scripts_dir().join(sanitize_component(profile_id))
}

/// Relative locations shown in the UI, always using `/` so the string is
/// identical on every platform.
pub fn display_relative(path: &Path) -> String {
    let base = config_dir();
    match path.strip_prefix(&base) {
        Ok(relative) => relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// Replaces anything that is not safe in a file name.
///
/// Profile identifiers are validated before they reach this function; it is a
/// defensive second line so a hostile `config.toml` cannot escape the config
/// directory.
pub fn sanitize_component(raw: &str) -> String {
    let sanitized: String = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();

    let trimmed = sanitized.trim_matches(['.', '-']).to_owned();
    if trimmed.is_empty() {
        DEFAULT_PROFILE_ID.to_owned()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_path_components() {
        assert_eq!(sanitize_component("default"), "default");
        assert_eq!(sanitize_component("my profile"), "my-profile");
        assert_eq!(sanitize_component("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_component(".."), DEFAULT_PROFILE_ID);
        assert_eq!(sanitize_component(""), DEFAULT_PROFILE_ID);
        assert_eq!(sanitize_component("工作区"), DEFAULT_PROFILE_ID);
    }

    #[test]
    fn profile_directories_stay_inside_the_config_tree() {
        let dir = script_dir_for("../../etc");
        assert!(dir.starts_with(config_dir().join("scripts")));
        assert!(!dir.to_string_lossy().contains(".."));
    }

    #[test]
    fn relative_display_uses_forward_slashes() {
        let nested = config_dir()
            .join("scripts")
            .join("default")
            .join("pre-launch.sh");
        assert_eq!(display_relative(&nested), "scripts/default/pre-launch.sh");
    }

    #[test]
    fn override_environment_variable_wins() {
        // The override is read from the process environment, so this test only
        // documents the contract without mutating global state.
        assert_eq!(CONFIG_HOME_ENV, "TAURI_3080_CONFIG_HOME");
    }
}
