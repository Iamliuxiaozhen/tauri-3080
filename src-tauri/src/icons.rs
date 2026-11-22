//! User-supplied application icon.
//!
//! The settings page lets the user pick a PNG. The file is copied into
//! `$CONFIG/icons/app-icon.png` (so the original may be moved or deleted) and
//! decoded once into an RGBA buffer that is reused for every window, the tray
//! and the macOS dock.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::atomic_write;
use crate::error::Error;
use crate::platform;

/// Canonical in-app file name for the custom app icon.
pub const CUSTOM_ICON_FILE: &str = "app-icon.png";

/// Icon embedded in the binary, used until the user picks another one.
pub const DEFAULT_ICON_PNG: &[u8] = include_bytes!("../icons/app-icon.png");

/// Small icon used for tray entries and the taskbar badge.
pub const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray-icon.png");

/// Maximum accepted upload size (8 MiB) and edge length (4096 px).
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EDGE: u32 = 4096;

/// A decoded window icon.
#[derive(Debug, Clone)]
pub struct IconBitmap {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl IconBitmap {
    /// Wraps the bitmap into a Tauri image.
    #[must_use]
    pub fn to_tauri(&self) -> tauri::image::Image<'static> {
        tauri::image::Image::new_owned(self.rgba.clone(), self.width, self.height)
    }

    /// Cheap signature used for cache invalidation.
    #[must_use]
    pub fn signature(&self) -> String {
        format!("{}x{}-{}", self.width, self.height, self.rgba.len())
    }
}

/// Decodes a PNG (or ICO/JPEG) file into an RGBA bitmap.
pub fn decode_png(bytes: &[u8]) -> Result<IconBitmap, Error> {
    let decoded = image::load_from_memory(bytes)?;
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return Err(Error::UnsupportedIcon);
    }
    if width > MAX_EDGE || height > MAX_EDGE {
        return Err(Error::Invalid {
            field: "icon",
            reason: format!("image is {width}x{height}; the maximum is {MAX_EDGE}px per edge"),
        });
    }
    Ok(IconBitmap {
        rgba: decoded.into_rgba8().into_raw(),
        width,
        height,
    })
}

/// Absolute path of the stored custom icon, if the setting points at a file
/// that still exists.
#[must_use]
pub fn stored_icon_path(file_name: Option<&str>) -> Option<PathBuf> {
    let name = file_name?;
    if !crate::config::is_safe_file_name(name) {
        return None;
    }
    let path = platform::icons_dir().join(name);
    path.is_file().then_some(path)
}

/// Copies `source` into the icon directory and returns the stored file name.
///
/// The file is validated *before* it replaces the current icon, so an invalid
/// selection never destroys a working one.
pub fn import_icon(source: &Path) -> Result<String, Error> {
    let metadata = fs::metadata(source)?;
    if metadata.len() > MAX_BYTES {
        return Err(Error::Invalid {
            field: "icon",
            reason: "the image is larger than 8 MiB".to_owned(),
        });
    }
    let bytes = fs::read(source)?;
    decode_png(&bytes)?;

    fs::create_dir_all(platform::icons_dir())?;
    atomic_write(&platform::icons_dir().join(CUSTOM_ICON_FILE), &bytes)?;
    Ok(CUSTOM_ICON_FILE.to_owned())
}

/// Loads the configured icon, falling back to the embedded one.
///
/// The returned flag is `true` when the custom icon was used.
#[must_use]
pub fn load_icon(file_name: Option<&str>) -> (IconBitmap, bool) {
    if let Some(path) = stored_icon_path(file_name) {
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(bitmap) = decode_png(&bytes) {
                return (bitmap, true);
            }
        }
    }
    match decode_png(DEFAULT_ICON_PNG) {
        Ok(bitmap) => (bitmap, false),
        Err(_) => (
            IconBitmap {
                rgba: Vec::new(),
                width: 0,
                height: 0,
            },
            false,
        ),
    }
}

/// Renders the icon at `size` for the macOS dock / Windows taskbar.
#[must_use]
pub fn resized_png(bitmap: &IconBitmap, size: u32) -> Vec<u8> {
    let Some(buffer) = image::RgbaImage::from_raw(bitmap.width, bitmap.height, bitmap.rgba.clone())
    else {
        return Vec::new();
    };
    let resized =
        image::imageops::resize(&buffer, size, size, image::imageops::FilterType::Lanczos3);
    let mut bytes = Vec::new();
    if image::DynamicImage::ImageRgba8(resized)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .is_err()
    {
        return Vec::new();
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes() {
        let (bitmap, custom) = load_icon(Some("../nope.png"));
        assert!(!custom);
        assert!(bitmap.width >= 128);
        assert_eq!(bitmap.rgba.len() as u32, bitmap.width * bitmap.height * 4);
    }

    #[test]
    fn rejects_oversized_images() {
        let image = image::RgbaImage::from_pixel(5000, 8, image::Rgba([1, 2, 3, 255]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        assert!(decode_png(&bytes).is_err());
    }

    #[test]
    fn imports_and_reads_back_a_custom_icon() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var(platform::CONFIG_HOME_ENV, dir.path());
        let source = dir.path().join("picked.png");
        let image = image::RgbaImage::from_pixel(32, 32, image::Rgba([9, 9, 250, 255]));
        image.save(&source).expect("write the sample icon");

        let name = import_icon(&source).unwrap();
        assert_eq!(name, CUSTOM_ICON_FILE);
        let (bitmap, custom) = load_icon(Some(&name));
        std::env::remove_var(platform::CONFIG_HOME_ENV);

        assert!(custom);
        assert_eq!((bitmap.width, bitmap.height), (32, 32));
    }

    #[test]
    fn signature_changes_with_size() {
        let bitmap = decode_png(DEFAULT_ICON_PNG).unwrap();
        assert!(bitmap
            .signature()
            .starts_with(&format!("{}x{}", bitmap.width, bitmap.height)));
    }

    #[test]
    fn resizing_produces_a_png() {
        let bitmap = decode_png(DEFAULT_ICON_PNG).unwrap();
        let bytes = resized_png(&bitmap, 64);
        assert_eq!(&bytes[..4], b"\x89PNG");
        assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 64);
    }
}
