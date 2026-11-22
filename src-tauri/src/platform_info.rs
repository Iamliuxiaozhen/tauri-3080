//! Platform-specific window and icon integration.
//!
//! Only two things genuinely need OS APIs:
//!
//! * the macOS dock icon, which `WebviewWindow::set_icon` cannot change;
//! * the Windows taskbar icon, which is refreshed from the window handle.
//!
//! Everything else is delegated to Tauri.
//!
//! The crate lints against `unsafe_code`; the Objective-C calls below are the
//! one exception and each one carries a SAFETY comment.
#![allow(unsafe_code)]

use std::path::Path;

use crate::icons::IconBitmap;

/// Updates the icon the operating system shows for the whole application
/// (the macOS dock, or the Windows console-free AppUserModelID icon).
///
/// Returns `Ok(false)` when the platform has no separate app-level icon and the
/// window icon is all there is.
pub fn set_application_icon(icon: &IconBitmap, app_name: &str) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app_name;
        macos::set_dock_icon(icon)?;
        Ok(true)
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On Linux and Windows the window icon is the application icon.
        let _ = (icon, app_name);
        Ok(false)
    }
}

/// Refreshes the taskbar icon of a window on Windows.
pub fn refresh_window_icon<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>, icon: &IconBitmap) {
    #[cfg(windows)]
    {
        windows::refresh_taskbar_icon(window, icon);
    }

    #[cfg(not(windows))]
    {
        // Windows caches the taskbar icon per window class; elsewhere
        // `WebviewWindow::set_icon` is enough.
        let _ = (window, icon);
    }
}

/// Opens a directory in the OS file manager.
pub fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    let target = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(path).to_path_buf()
    };

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(&target)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use objc2::rc::Retained;
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    use crate::icons::IconBitmap;

    /// Replaces the dock icon with `icon`.
    pub fn set_dock_icon(icon: &IconBitmap) -> Result<(), String> {
        let png = crate::icons::resized_png(icon, 512);
        if png.is_empty() {
            return Err("could not encode the dock icon".to_owned());
        }

        let marker = MainThreadMarker::new()
            .ok_or_else(|| "dock icons must be set from the main thread".to_owned())?;
        let application = NSApplication::sharedApplication(marker);
        let data = NSData::with_bytes(&png);
        // `NSImage` is not main-thread-only, but we hold the marker anyway
        // because the dock icon itself must be changed on the main thread.
        let image: Retained<NSImage> = NSImage::initWithData(NSImage::alloc(), &data)
            .ok_or_else(|| "could not decode the dock icon".to_owned())?;
        // SAFETY: called on the main thread (the marker proves it) with a valid
        // image instance.
        unsafe {
            application.setApplicationIconImage(Some(&image));
        }
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use tauri::WebviewWindow;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageW, ICON_BIG, ICON_SMALL, WM_SETICON,
    };

    use crate::icons::IconBitmap;

    /// Re-sends the window icon so Explorer picks up the new one.
    pub fn refresh_taskbar_icon<R: tauri::Runtime>(window: &WebviewWindow<R>, icon: &IconBitmap) {
        let Ok(handle) = window.hwnd() else {
            return;
        };
        let raw: HWND = handle.0 as HWND;

        if let Some(hicon) = create_hicon(icon) {
            // SAFETY: sending messages to a window handle owned by this
            // process. The HICON is deliberately never destroyed: Windows uses
            // it for the lifetime of the window and destroys it with the HWND.
            let _ = unsafe { SendMessageW(raw, WM_SETICON, ICON_BIG as usize, hicon as isize) };
            let _ = unsafe { SendMessageW(raw, WM_SETICON, ICON_SMALL as usize, hicon as isize) };
        }
    }

    fn create_hicon(icon: &IconBitmap) -> Option<*mut std::ffi::c_void> {
        if icon.rgba.is_empty() || icon.width == 0 || icon.height == 0 {
            return None;
        }
        let width = i32::try_from(icon.width).ok()?;
        let height = i32::try_from(icon.height).ok()?;
        // An all-zero AND mask means "use the alpha channel", which is what a
        // PNG decodes to.
        let mask = vec![0_u8; ((icon.width.div_ceil(8)) * icon.height) as usize];
        // SAFETY: the RGBA and mask buffers outlive the call and their sizes
        // match the documented contract of `CreateIcon`.
        let created = unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::CreateIcon(
                std::ptr::null_mut(),
                width,
                height,
                1,
                32,
                icon.rgba.as_ptr(),
                mask.as_ptr(),
            )
        };
        (!created.is_null()).then_some(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_is_accepted() {
        let (icon, _) = crate::icons::load_icon(None);
        // Only asserts that the call does not panic off the main thread paths
        // that are unavailable in the unit test environment.
        assert!(icon.width > 0);
        let _ = set_application_icon(&icon, "test");
    }
}
