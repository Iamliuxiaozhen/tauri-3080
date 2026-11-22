//! Typed view over `config.toml`.
//!
//! Two layers exist on purpose:
//!
//! * [`ConfigFile`] owns the `toml_edit` document and is responsible for
//!   *preserving* comments and unknown keys.
//! * [`AppConfig`] is the typed, validated snapshot the UI works with.
//!
//! The typed layer is strict about values that would break the launcher (an
//! unknown locale, a profile pointing outside the config tree) and repairs them
//! instead of failing to start; every repair is reported back to the user.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, TableLike};

use crate::app_id;
use crate::error::Error;
use crate::platform;

/// Supported UI languages.
pub const SUPPORTED_LANGUAGES: [&str; 4] = ["en", "zh-CN", "zh-TW", "pt-BR"];

/// What happens before the window navigates to the profile URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PrelaunchMode {
    /// Run the hook, then open the window.
    #[default]
    Run,
    /// Run the hook, then wait until the port accepts connections.
    WaitForPort,
    /// Never run the hook.
    Skip,
}

impl PrelaunchMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::WaitForPort => "wait-for-port",
            Self::Skip => "skip",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "wait-for-port" | "wait_for_port" | "wait" => Self::WaitForPort,
            "skip" | "off" | "disabled" => Self::Skip,
            _ => Self::Run,
        }
    }
}

pub const MIN_TIMEOUT_SECS: u64 = 1;
pub const MAX_TIMEOUT_SECS: u64 = 600;
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_WAIT_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_WINDOW_WIDTH: u32 = 1280;
pub const DEFAULT_WINDOW_HEIGHT: u32 = 860;
pub const MIN_WINDOW_WIDTH: u32 = 640;
pub const MAX_WINDOW_WIDTH: u32 = 7680;
pub const MIN_WINDOW_HEIGHT: u32 = 480;
pub const MAX_WINDOW_HEIGHT: u32 = 4320;

/// Application-wide settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// UI language tag.
    pub locale: String,
    /// User-facing application name.
    pub display_name: String,
    /// Stored icon file name inside [`platform::icons_dir`].
    pub icon_path: Option<String>,
    /// Profile opened on start-up.
    pub default_profile: String,
    /// Hide to the tray instead of quitting when the last window closes.
    pub close_to_tray: bool,
    /// Remembered window geometry.
    pub window: WindowSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            locale: "en".to_owned(),
            display_name: app_id::DEFAULT_DISPLAY_NAME.to_owned(),
            icon_path: None,
            default_profile: platform::DEFAULT_PROFILE_ID.to_owned(),
            close_to_tray: false,
            window: WindowSettings::default(),
        }
    }
}

/// Remembered window geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    pub width: u32,
    pub height: u32,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
        }
    }
}

/// One launcher profile: a URL, a hook and an icon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Stable identifier; also the script directory name.
    pub id: String,
    /// Shown in the window title and menus.
    pub name: String,
    /// URL opened in the window.
    pub url: String,
    /// Optional per-profile icon file name inside [`platform::icons_dir`].
    pub icon_path: Option<String>,
    /// Hook behaviour.
    pub prelaunch: PrelaunchMode,
    /// Explicit script file name; `None` means "auto-detect".
    pub script: Option<String>,
    /// Arguments passed to the script.
    pub args: Vec<String>,
    /// Extra environment variables.
    pub env: BTreeMap<String, String>,
    /// Hard timeout for one hook invocation.
    pub timeout_secs: u64,
    /// How long `wait-for-port` waits.
    pub wait_timeout_secs: u64,
    /// Kill a still-running hook for the same profile before starting a new one.
    pub kill_previous: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: platform::DEFAULT_PROFILE_ID.to_owned(),
            name: app_id::DEFAULT_DISPLAY_NAME.to_owned(),
            url: "http://127.0.0.1:3080".to_owned(),
            icon_path: None,
            prelaunch: PrelaunchMode::Run,
            script: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            wait_timeout_secs: DEFAULT_WAIT_TIMEOUT_SECS,
            kill_previous: true,
        }
    }
}

impl Profile {
    /// Builds the profile written on first launch.
    #[must_use]
    pub fn initial() -> Self {
        Self::default()
    }
}

/// Typed configuration snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub settings: AppSettings,
    /// Profiles keyed by identifier.
    #[serde(rename = "profile")]
    pub profiles: BTreeMap<String, Profile>,
}

impl AppConfig {
    /// Starter configuration.
    #[must_use]
    pub fn initial() -> Self {
        let profile = Profile::initial();
        let mut profiles = BTreeMap::new();
        profiles.insert(profile.id.clone(), profile);
        Self {
            settings: AppSettings::default(),
            profiles,
        }
    }

    /// Looks up a profile.
    pub fn profile(&self, id: &str) -> Result<&Profile, Error> {
        self.profiles
            .get(id)
            .ok_or_else(|| Error::UnknownProfile(id.to_owned()))
    }

    /// Profile used at start-up, with a safe fallback chain.
    #[must_use]
    pub fn boot_profile(&self) -> Profile {
        if let Some(profile) = self.profiles.get(&self.settings.default_profile) {
            return profile.clone();
        }
        self.profiles
            .values()
            .next()
            .cloned()
            .unwrap_or_else(Profile::initial)
    }

    /// Ordered profile list for menus and the home page.
    #[must_use]
    pub fn profile_list(&self) -> Vec<&Profile> {
        self.profiles.values().collect()
    }

    /// Validates and repairs the snapshot, returning human-readable repairs.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut repairs = Vec::new();

        if !SUPPORTED_LANGUAGES.contains(&self.settings.locale.as_str()) {
            repairs.push(format!(
                "unsupported locale `{}`, falling back to `en`",
                self.settings.locale
            ));
            self.settings.locale = "en".to_owned();
        }

        let display_name = self.settings.display_name.trim().to_owned();
        if display_name.is_empty() {
            repairs.push("empty display name, restoring the default".to_owned());
            self.settings.display_name = app_id::DEFAULT_DISPLAY_NAME.to_owned();
        } else {
            self.settings.display_name = display_name.chars().take(64).collect();
        }

        if let Some(icon) = self.settings.icon_path.clone() {
            if !is_safe_file_name(&icon) {
                repairs.push(format!("ignoring unsafe icon path `{icon}`"));
                self.settings.icon_path = None;
            }
        }

        if self.profiles.is_empty() {
            repairs.push("no profiles found, recreating the default profile".to_owned());
            let profile = Profile::initial();
            self.profiles.insert(profile.id.clone(), profile);
        }

        let ids: Vec<String> = self.profiles.keys().cloned().collect();
        let mut normalized: BTreeMap<String, Profile> = BTreeMap::new();
        for id in ids {
            let Some(mut profile) = self.profiles.remove(&id) else {
                continue;
            };
            if profile.id.trim().is_empty() {
                profile.id = id.clone();
            }
            profile.normalize(&mut repairs);
            if profile.id != id {
                repairs.push(format!("profile key `{id}` is now `{}`", profile.id));
            }
            if normalized.contains_key(&profile.id) {
                repairs.push(format!("duplicate profile id `{}` dropped", profile.id));
                continue;
            }
            normalized.insert(profile.id.clone(), profile);
        }
        self.profiles = normalized;

        if !self.profiles.contains_key(&self.settings.default_profile) {
            let fallback = self
                .profiles
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| platform::DEFAULT_PROFILE_ID.to_owned());
            repairs.push(format!(
                "default profile `{}` is missing, falling back to `{fallback}`",
                self.settings.default_profile
            ));
            self.settings.default_profile = fallback;
        }

        let window = &mut self.settings.window;
        let width = window.width.clamp(MIN_WINDOW_WIDTH, MAX_WINDOW_WIDTH);
        let height = window.height.clamp(MIN_WINDOW_HEIGHT, MAX_WINDOW_HEIGHT);
        if (width, height) != (window.width, window.height) {
            repairs.push(format!(
                "window size {}x{} clamped to {width}x{height}",
                window.width, window.height
            ));
            *window = WindowSettings { width, height };
        }

        repairs
    }
}

impl Profile {
    /// Validates and repairs a single profile, appending repairs to `repairs`.
    pub fn normalize(&mut self, repairs: &mut Vec<String>) {
        let sanitized = platform::sanitize_component(&self.id);
        if sanitized != self.id {
            repairs.push(format!("renamed profile id `{}` to `{sanitized}`", self.id));
            self.id = sanitized;
        }

        let display_name = self.name.trim().to_owned();
        if display_name.is_empty() {
            self.name = self.id.clone();
        } else {
            self.name = display_name.chars().take(64).collect();
        }

        if let Some(icon) = self.icon_path.clone() {
            if !is_safe_file_name(&icon) {
                repairs.push(format!("ignoring unsafe icon path `{icon}`"));
                self.icon_path = None;
            }
        }

        if let Some(script) = self.script.clone() {
            if !is_safe_file_name(&script) {
                repairs.push(format!("ignoring unsafe script name `{script}`"));
                self.script = None;
            }
        }

        if let Err(error) = validate_url(&self.url) {
            repairs.push(format!("invalid URL `{}`: {error}", self.url));
            self.url = Profile::initial().url;
        }

        if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&self.timeout_secs) {
            repairs.push(format!(
                "timeout {}s out of range, reset to {DEFAULT_TIMEOUT_SECS}s",
                self.timeout_secs
            ));
            self.timeout_secs = DEFAULT_TIMEOUT_SECS;
        }
        if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&self.wait_timeout_secs) {
            repairs.push(format!(
                "wait timeout {}s out of range, reset to {DEFAULT_WAIT_TIMEOUT_SECS}s",
                self.wait_timeout_secs
            ));
            self.wait_timeout_secs = DEFAULT_WAIT_TIMEOUT_SECS;
        }

        if self.args.len() > 32 {
            self.args.truncate(32);
            repairs.push("too many script arguments, truncated to 32".to_owned());
        }
        if self.env.len() > 128 {
            self.env = self
                .env
                .iter()
                .take(128)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            repairs.push("too many environment variables, truncated to 128".to_owned());
        }
    }
}

/// Rejects file names that could escape the config directory.
#[must_use]
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.contains(['/', '\\', '\0'])
        && name != "."
        && name != ".."
        && !name.starts_with('.')
}

/// Accepts absolute `http(s)` URLs and `file://` URLs.
pub fn validate_url(raw: &str) -> Result<(), Error> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidUrl("URL must not be empty".to_owned()));
    }
    let lowered = trimmed.to_ascii_lowercase();

    if lowered.starts_with("file://") {
        return Ok(());
    }

    let rest = if let Some(rest) = lowered.strip_prefix("http://") {
        rest
    } else if let Some(rest) = lowered.strip_prefix("https://") {
        rest
    } else {
        return Err(Error::InvalidUrl(
            "URL must start with http://, https:// or file://".to_owned(),
        ));
    };

    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.starts_with(':') {
        return Err(Error::InvalidUrl("URL is missing a host".to_owned()));
    }
    Ok(())
}

/// `toml_edit` document wrapper that preserves comments and unknown keys.
#[derive(Debug, Clone)]
pub struct ConfigFile {
    path: PathBuf,
    document: DocumentMut,
    text: String,
}

impl ConfigFile {
    /// Loads `path`, writing a documented starter file when it is missing.
    pub fn load_or_create(path: &Path) -> Result<Self, Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = if path.is_file() {
            fs::read_to_string(path)?
        } else {
            let starter = starter_text();
            atomic_write(path, starter.as_bytes())?;
            starter
        };

        let document = if text.trim().is_empty() {
            DocumentMut::new()
        } else {
            text.parse::<DocumentMut>()?
        };

        Ok(Self {
            path: path.to_path_buf(),
            document,
            text,
        })
    }

    /// Loads the main configuration file.
    pub fn load() -> Result<Self, Error> {
        Self::load_or_create(&platform::main_config_path())
    }

    /// Parses an in-memory document.
    pub fn from_text(text: &str) -> Result<Self, Error> {
        Ok(Self {
            path: PathBuf::new(),
            document: text.parse::<DocumentMut>()?,
            text: text.to_owned(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn document(&self) -> &DocumentMut {
        &self.document
    }

    /// Typed snapshot, including comments-free parsing of unknown keys.
    pub fn typed(&self) -> Result<AppConfig, Error> {
        let mut config: AppConfig = toml::from_str(&self.document.to_string())?;
        config.normalize();
        Ok(config)
    }

    /// Sets `settings.<key>`.
    pub fn set_setting(&mut self, key: &str, item: Item) {
        let root = self.document.as_table_mut();
        if !root.get("settings").is_some_and(toml_edit::Item::is_table) {
            root.insert("settings", Item::Table(Table::new()));
        }
        if let Some(settings) = root["settings"].as_table_mut() {
            settings.insert(key, item);
        }
    }

    /// Sets `profile.<id>.<key>`.
    pub fn set_profile(&mut self, id: &str, key: &str, item: Item) {
        let root = self.document.as_table_mut();
        if !root.get("profile").is_some_and(toml_edit::Item::is_table) {
            root.insert("profile", Item::Table(Table::new()));
        }
        if let Some(profiles) = root["profile"].as_table_mut() {
            if !profiles.contains_key(id) {
                profiles.insert(id, Item::Table(Table::new()));
            }
            if let Some(profile) = profiles[id].as_table_mut() {
                profile.set_implicit(false);
                profile.insert(key, item);
            }
        }
    }

    /// Merges a freshly serialised document into this one.
    ///
    /// Tables that exist in both documents are merged key by key, so unrelated
    /// keys, comments and the user's ordering survive. Tables that only exist
    /// in `fresh` are inserted as-is; keys that disappeared from `fresh` are
    /// removed (that is how profile deletion works).
    pub fn merge_from(&mut self, fresh: &DocumentMut) {
        let root = self.document.as_table_mut();
        let fresh_keys: Vec<String> = fresh.iter().map(|(key, _)| key.to_owned()).collect();

        for key in root
            .iter()
            .map(|(key, _)| key.to_owned())
            .collect::<Vec<_>>()
        {
            if !fresh_keys.contains(&key) {
                root.remove(&key);
            }
        }

        for key in fresh_keys {
            let Some(fresh_item) = fresh.get(&key) else {
                continue;
            };
            match fresh_item {
                Item::Table(fresh_table) => {
                    let existing_is_table = root.get(&key).is_some_and(Item::is_table);
                    if !existing_is_table {
                        root.insert(&key, fresh_item.clone());
                        continue;
                    }
                    let Some(existing) = root[&key].as_table_mut() else {
                        continue;
                    };
                    if fresh_table.is_implicit() {
                        existing.set_implicit(true);
                    }
                    merge_table(existing, fresh_table);
                    if key == "profile" {
                        // Profiles are fully app-managed: an entry missing from
                        // the typed snapshot was deleted by the user.
                        let known: Vec<String> = fresh_table
                            .iter()
                            .map(|(name, _)| name.to_owned())
                            .collect();
                        let stale: Vec<String> = existing
                            .iter()
                            .map(|(name, _)| name.to_owned())
                            .filter(|name| !known.contains(name))
                            .collect();
                        for name in stale {
                            existing.remove(&name);
                        }
                    }
                }
                other => {
                    root.insert(&key, other.clone());
                }
            }
        }
    }

    /// Writes the document back atomically.
    pub fn save(&mut self) -> Result<(), Error> {
        let text = self.document.to_string();
        atomic_write(&self.path, text.as_bytes())?;
        self.text = text;
        Ok(())
    }
}

/// Reads a trimmed string.
#[must_use]
pub fn table_str(table: &dyn TableLike, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(Item::as_str)
        .map(|value| value.trim().to_owned())
}

/// Reads a boolean.
#[must_use]
pub fn table_bool(table: &dyn TableLike, key: &str) -> Option<bool> {
    table.get(key).and_then(Item::as_bool)
}

/// Reads a non-negative integer.
#[must_use]
pub fn table_u64(table: &dyn TableLike, key: &str) -> Option<u64> {
    table
        .get(key)
        .and_then(Item::as_integer)
        .and_then(|value| u64::try_from(value).ok())
}

/// `[profile.*]` identifiers in file order.
#[must_use]
pub fn profile_ids(document: &DocumentMut) -> Vec<String> {
    document
        .get("profile")
        .and_then(Item::as_table)
        .map(|table| table.iter().map(|(key, _)| key.to_owned()).collect())
        .unwrap_or_default()
}

/// The documented starter document.
#[must_use]
pub fn starter_text() -> String {
    const TEMPLATE: &str = r#"# tauri-3080 configuration
#
# Everything the launcher owns lives in this directory:
#   config.toml        this file
#   scripts/<id>/      pre-launch hooks, one directory per profile
#   icons/             custom app and profile icons
#   logs/              pre-launch output
#
# The file can be edited by hand. Unknown keys are preserved.

[settings]
# UI language: "en", "zh-CN", "zh-TW", "pt-BR".
locale = "en"
# Name shown in the title bar, the taskbar and the tray.
display_name = "127.0.0.1:3080"
# File name inside ./icons, set from the settings page.
# icon_path = "app-icon.png"
# Profile opened on start-up.
default_profile = "default"
# Hide to the tray instead of quitting when the last window closes.
close_to_tray = false

[settings.window]
width = 1280
height = 860

# One table per launcher profile. Copy this table and change `id`, `name` and
# `url` to keep another port or another pre-launch hook side by side.
[profile.default]
id = "default"
name = "127.0.0.1:3080"
url = "http://127.0.0.1:3080"
# "run"            run the hook, then open the window
# "wait-for-port"  run the hook, then wait for the port before opening
# "skip"           never run the hook
prelaunch = "run"
# Script file inside ./scripts/default. When unset the launcher looks for
# pre-launch.sh (Unix) or pre-launch.ps1 / .cmd / .bat (Windows).
# script = "pre-launch.sh"
# args = ["--flag"]
# timeout_secs = 30
# wait_timeout_secs = 60
# kill_previous = true
#
# [profile.default.env]
# MY_TOKEN = "…"
"#;
    TEMPLATE.to_owned()
}

/// Writes `contents` to `path` through a temporary file in the same directory so
/// a crash can never leave a half-written document behind.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path.file_name().map_or_else(
        || "config".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temporary = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temporary, contents)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

/// Creates every directory the launcher expects to exist.
pub fn ensure_layout() -> Result<(), Error> {
    for directory in [
        platform::config_dir(),
        platform::profiles_dir(),
        platform::icons_dir(),
        platform::scripts_dir(),
        platform::logs_dir(),
    ] {
        fs::create_dir_all(directory)?;
    }
    Ok(())
}

/// Writes the commented starter script for `profile` when its script directory
/// has none yet. Returns `true` when a file was created.
pub fn ensure_example_script(profile: &Profile) -> Result<bool, Error> {
    let directory = platform::script_dir_for(&profile.id);
    fs::create_dir_all(&directory)?;

    let mut candidates: Vec<String> = SCRIPT_FILE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    if let Some(custom) = profile.script.as_deref() {
        if is_safe_file_name(custom) {
            candidates.push(custom.to_owned());
        }
    }
    if candidates.iter().any(|name| directory.join(name).is_file()) {
        return Ok(false);
    }

    let (file_name, body) = example_script();
    let path = directory.join(file_name);
    atomic_write(&path, body.as_bytes())?;
    make_executable(&path)?;
    Ok(true)
}

/// Script names auto-detected inside a profile script directory.
pub const SCRIPT_FILE_NAMES: [&str; 4] = [
    "pre-launch.sh",
    "pre-launch.ps1",
    "pre-launch.cmd",
    "pre-launch.bat",
];

/// Starter hook: a no-op that documents what a useful one looks like.
#[must_use]
pub fn example_script() -> (&'static str, String) {
    if cfg!(windows) {
        (
            "pre-launch.ps1",
            concat!(
                "# Pre-launch hook for 127.0.0.1:3080.\r\n",
                "# Runs before the window opens, every time a window starts.\r\n",
                "# Delete this file to stop running anything.\r\n",
                "#\r\n",
                "# Typical use: make sure the local service is running.\r\n",
                "# if (-not (Test-NetConnection -ComputerName 127.0.0.1 -Port 3080 -InformationLevel Quiet)) {\r\n",
                "#     Start-Process -NoNewWindow my-service -ArgumentList '--port','3080'\r\n",
                "# }\r\n",
                "Write-Output 'pre-launch: nothing to do'\r\n",
            )
            .to_owned(),
        )
    } else {
        (
            "pre-launch.sh",
            concat!(
                "#!/usr/bin/env sh\n",
                "# Pre-launch hook for 127.0.0.1:3080.\n",
                "# Runs before the window opens, every time a window starts.\n",
                "# Delete this file to stop running anything.\n",
                "#\n",
                "# Typical use: make sure the local service is running.\n",
                "#\n",
                "#   if ! curl -fsS http://127.0.0.1:3080/health >/dev/null 2>&1; then\n",
                "#     nohup my-service --port 3080 >\"$HOME/.config/tauri-3080/logs/service.log\" 2>&1 &\n",
                "#   fi\n",
                "#\n",
                "echo 'pre-launch: nothing to do'\n",
            )
            .to_owned(),
        )
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<(), Error> {
    // Executability is expressed by the file extension on Windows.
    Ok(())
}

/// Merges `fresh` into `existing`, preserving comments attached to values that
/// are replaced.
fn merge_table(existing: &mut Table, fresh: &Table) {
    let fresh_keys: Vec<String> = fresh.iter().map(|(key, _)| key.to_owned()).collect();

    for key in existing
        .iter()
        .map(|(key, _)| key.to_owned())
        .collect::<Vec<_>>()
    {
        if !fresh_keys.contains(&key) && !existing[&key].is_table() {
            existing.remove(&key);
        }
    }

    for key in fresh_keys {
        let Some(fresh_item) = fresh.get(&key) else {
            continue;
        };
        match fresh_item {
            Item::Table(fresh_table) => {
                if !existing.get(&key).is_some_and(Item::is_table) {
                    existing.insert(&key, fresh_item.clone());
                    continue;
                }
                if let Some(inner) = existing[&key].as_table_mut() {
                    inner.set_implicit(false);
                    merge_table(inner, fresh_table);
                }
            }
            Item::Value(fresh_value) => {
                if let Some(Item::Value(current)) = existing.get_mut(&key) {
                    // Keep comments attached to the old value.
                    let prefix = current.decor().prefix().cloned();
                    let suffix = current.decor().suffix().cloned();
                    let mut replacement = fresh_value.clone();
                    if let Some(prefix) = prefix {
                        replacement.decor_mut().set_prefix(prefix);
                    }
                    if let Some(suffix) = suffix {
                        replacement.decor_mut().set_suffix(suffix);
                    }
                    *current = replacement;
                } else {
                    existing.insert(&key, fresh_item.clone());
                }
            }
            other => {
                existing.insert(&key, other.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_item(file: &ConfigFile) -> Item {
        file.document()["settings"].clone()
    }

    #[test]
    fn starter_document_is_valid() {
        let file = ConfigFile::from_text(&starter_text()).unwrap();
        assert_eq!(profile_ids(file.document()), vec!["default"]);

        let settings = settings_item(&file);
        let table = settings.as_table().unwrap();
        assert_eq!(table_str(table, "locale").as_deref(), Some("en"));
        assert_eq!(table_bool(table, "close_to_tray"), Some(false));
        let window = table["window"].as_table().unwrap();
        assert_eq!(table_u64(window, "width"), Some(1280));
        assert_eq!(table_u64(window, "height"), Some(860));

        let config = file.typed().unwrap();
        assert_eq!(config.profiles["default"].url, "http://127.0.0.1:3080");
        assert_eq!(config.settings.default_profile, "default");
    }

    #[test]
    fn editing_keeps_comments_and_unknown_keys() {
        let mut file = ConfigFile::from_text(&starter_text()).unwrap();
        file.set_setting("locale", toml_edit::value("zh-CN"));
        file.set_setting("future_option", toml_edit::value(true));
        file.set_profile("default", "name", toml_edit::value("Chat"));

        let text = file.document().to_string();
        assert!(
            text.contains("# One table per launcher profile"),
            "comments are preserved"
        );
        assert!(text.contains("# Script file inside ./scripts/default"));
        assert!(text.contains("locale = \"zh-CN\""));
        assert!(text.contains("future_option = true"));
        assert!(text.contains("name = \"Chat\""));
        assert!(text.contains("url = \"http://127.0.0.1:3080\""));
    }

    #[test]
    fn parses_a_hand_written_document() {
        let raw = r#"
[settings]
locale = "zh-TW"
display_name = "我的工作区"

[profile.default]
id = "default"
name = "工作区"
url = "http://127.0.0.1:3080"
prelaunch = "wait-for-port"
timeout_secs = 90

[profile.second]
id = "second"
name = "Second"
url = "https://example.com"
"#;
        let file = ConfigFile::from_text(raw).unwrap();
        assert_eq!(profile_ids(file.document()), vec!["default", "second"]);

        let config = file.typed().unwrap();
        assert_eq!(config.settings.locale, "zh-TW");
        assert_eq!(config.settings.display_name, "我的工作区");
        assert_eq!(
            config.profiles["default"].prelaunch,
            PrelaunchMode::WaitForPort
        );
        assert_eq!(config.profiles["default"].timeout_secs, 90);
        assert_eq!(config.profiles["second"].url, "https://example.com");
    }

    #[test]
    fn creates_a_documented_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let file = ConfigFile::load_or_create(&path).unwrap();
        assert!(path.is_file());
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("# tauri-3080 configuration"));
        assert!(raw.contains("[profile.default]"));
        assert_eq!(file.path(), path);
    }

    #[test]
    fn broken_toml_is_an_error_and_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[settings\nlocale = ").unwrap();
        assert!(ConfigFile::load_or_create(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "[settings\nlocale = ");
    }

    #[test]
    fn save_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut file = ConfigFile::load_or_create(&path).unwrap();
        file.set_setting("display_name", toml_edit::value("Renamed"));
        file.save().unwrap();

        let reloaded = ConfigFile::load_or_create(&path).unwrap();
        let settings = settings_item(&reloaded);
        assert_eq!(
            table_str(settings.as_table().unwrap(), "display_name").as_deref(),
            Some("Renamed")
        );
    }

    #[test]
    fn repairs_hostile_values() {
        let mut config = AppConfig {
            settings: AppSettings {
                locale: "klingon".to_owned(),
                display_name: "   ".to_owned(),
                icon_path: Some("../../etc/passwd".to_owned()),
                default_profile: "ghost".to_owned(),
                close_to_tray: false,
                window: WindowSettings {
                    width: 10,
                    height: 100_000,
                },
            },
            profiles: BTreeMap::new(),
        };
        let repairs = config.normalize();
        assert!(!repairs.is_empty());
        assert_eq!(config.settings.locale, "en");
        assert_eq!(config.settings.display_name, app_id::DEFAULT_DISPLAY_NAME);
        assert!(config.settings.icon_path.is_none());
        assert_eq!(config.settings.default_profile, "default");
        assert_eq!(config.settings.window.width, MIN_WINDOW_WIDTH);
        assert_eq!(config.settings.window.height, MAX_WINDOW_HEIGHT);
    }

    #[test]
    fn repairs_a_hostile_profile() {
        let mut profile = Profile {
            id: "my profile".to_owned(),
            name: "  ".to_owned(),
            url: "ftp://example.com".to_owned(),
            icon_path: Some("../evil.png".to_owned()),
            script: Some("../evil.sh".to_owned()),
            timeout_secs: 0,
            wait_timeout_secs: 10_000,
            ..Profile::default()
        };
        let mut repairs = Vec::new();
        profile.normalize(&mut repairs);
        assert_eq!(profile.id, "my-profile");
        assert_eq!(profile.name, "my-profile");
        assert_eq!(profile.url, "http://127.0.0.1:3080");
        assert!(profile.icon_path.is_none());
        assert!(profile.script.is_none());
        assert_eq!(profile.timeout_secs, DEFAULT_TIMEOUT_SECS);
        assert_eq!(profile.wait_timeout_secs, DEFAULT_WAIT_TIMEOUT_SECS);
        assert_eq!(repairs.len(), 6);
    }

    #[test]
    fn validates_urls() {
        assert!(validate_url("http://127.0.0.1:3080").is_ok());
        assert!(validate_url("https://example.com/path?q=1#frag").is_ok());
        assert!(validate_url("HTTPS://EXAMPLE.COM").is_ok());
        assert!(validate_url("file:///tmp/index.html").is_ok());
        assert!(validate_url("http://").is_err());
        assert!(validate_url("http://:3080").is_err());
        assert!(validate_url("javascript:alert(1)").is_err());
        assert!(validate_url("").is_err());
    }

    #[test]
    fn parses_prelaunch_modes_leniently() {
        assert_eq!(
            PrelaunchMode::parse("wait-for-port"),
            PrelaunchMode::WaitForPort
        );
        assert_eq!(PrelaunchMode::parse("WAIT"), PrelaunchMode::WaitForPort);
        assert_eq!(PrelaunchMode::parse("skip"), PrelaunchMode::Skip);
        assert_eq!(PrelaunchMode::parse("nonsense"), PrelaunchMode::Run);
        assert_eq!(PrelaunchMode::Run.as_str(), "run");
    }

    #[test]
    fn boot_profile_falls_back_when_default_is_missing() {
        let mut config = AppConfig::initial();
        config.settings.default_profile = "removed".to_owned();
        assert_eq!(config.boot_profile().id, "default");
    }

    #[test]
    fn rejects_unsafe_file_names() {
        assert!(is_safe_file_name("logo.png"));
        assert!(!is_safe_file_name("../logo.png"));
        assert!(!is_safe_file_name("a/b.png"));
        assert!(!is_safe_file_name(".hidden"));
        assert!(!is_safe_file_name(""));
    }
}
