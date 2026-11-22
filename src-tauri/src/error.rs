//! Error type shared by the Tauri commands.
//!
//! Commands return `Result<T, Error>`; the error serialises to its message so
//! the frontend can display it without extra plumbing.

use serde::{Serialize, Serializer};

/// Every recoverable failure the launcher can report to the UI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid configuration file: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("could not serialise the configuration: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),

    #[error("invalid TOML: {0}")]
    TomlEdit(#[from] toml_edit::TomlError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("the image could not be decoded: {0}")]
    Image(#[from] image::ImageError),

    #[error("the requested profile does not exist: {0}")]
    UnknownProfile(String),

    #[error("a profile with this identifier already exists: {0}")]
    DuplicateProfile(String),

    #[error("invalid value for `{field}`: {reason}")]
    Invalid { field: &'static str, reason: String },

    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("invalid URL: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("the icon file must be a PNG, JPEG, GIF, WebP or ICO image")]
    UnsupportedIcon,

    #[error("{0}")]
    Message(String),
}

/// Serialise errors as their human-readable message.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tauri::Error> for Error {
    fn from(error: tauri::Error) -> Self {
        Self::Message(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_to_the_display_message() {
        let error = Error::UnknownProfile("nightly".into());
        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            "\"the requested profile does not exist: nightly\""
        );
    }

    #[test]
    fn every_variant_has_a_message() {
        let errors = [
            Error::UnknownProfile("a".into()),
            Error::DuplicateProfile("b".into()),
            Error::Invalid {
                field: "port",
                reason: "must be between 1 and 65535".into(),
            },
            Error::InvalidUrl("nope".into()),
            Error::UnsupportedIcon,
            Error::Message("boom".into()),
        ];
        for error in errors {
            assert!(!error.to_string().is_empty());
        }
    }
}
