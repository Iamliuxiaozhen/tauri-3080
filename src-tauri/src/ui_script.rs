//! Small scripts injected into every window before the page runs.
//!
//! They answer one question the frozen `tauri.conf.json` cannot: *which window
//! is this, and which profile does it belong to?* The frontend reads
//! `window.__TAURI3080__` synchronously during boot instead of waiting for an
//! IPC round trip (which would flash the wrong UI).

use crate::state::WindowKind;

/// Name of the global the bootstrap script installs.
pub const BOOTSTRAP_GLOBAL: &str = "__TAURI3080__";

/// Builds the initialisation script for `label`.
#[must_use]
pub fn bootstrap_script(label: &str, kind: WindowKind, profile_id: Option<&str>) -> String {
    let kind = match kind {
        WindowKind::Main => "main",
        WindowKind::Settings => "settings",
    };
    let label = serde_json::to_string(label).unwrap_or_else(|_| "\"main\"".to_owned());
    let profile = profile_id
        .map(|id| serde_json::to_string(id).unwrap_or_else(|_| "null".to_owned()))
        .unwrap_or_else(|| "null".to_owned());

    format!(
        "window.{BOOTSTRAP_GLOBAL} = Object.freeze({{ label: {label}, kind: \"{kind}\", profileId: {profile} }});"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_label_and_profile() {
        let script = bootstrap_script("main-2", WindowKind::Main, Some("work"));
        assert!(script.contains("label: \"main-2\""));
        assert!(script.contains("kind: \"main\""));
        assert!(script.contains("profileId: \"work\""));
        assert!(script.starts_with("window.__TAURI3080__ = Object.freeze("));
    }

    #[test]
    fn settings_windows_have_no_profile() {
        let script = bootstrap_script("settings", WindowKind::Settings, None);
        assert!(script.contains("kind: \"settings\""));
        assert!(script.contains("profileId: null"));
    }

    #[test]
    fn escapes_hostile_labels() {
        let script = bootstrap_script("a\";alert(1);//", WindowKind::Main, Some("x\"y"));
        // The payload stays inside the JSON string literal.
        assert!(script.contains("\"a\\\";alert(1);//\""));
        assert!(script.contains("profileId: \"x\\\"y\""));
    }
}
