//! Stable identifiers used on disk.
//!
//! They intentionally do not depend on the user-facing name: renaming the app
//! must never move the configuration directory.

/// Directory name inside the platform configuration root.
pub const CONFIG_DIR_NAME: &str = "tauri-3080";

/// Bundle identifier from `tauri.conf.json`.
pub const BUNDLE_IDENTIFIER: &str = "dev.tauri3080.desktop";

/// File name of the main configuration document.
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// Name used until the user renames the app in the settings page.
pub const DEFAULT_DISPLAY_NAME: &str = "127.0.0.1:3080";

/// Default URL opened by the launcher.
pub const DEFAULT_URL: &str = "http://127.0.0.1:3080";
