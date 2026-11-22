//! Native menus and tray labels.
//!
//! The menu bar lives in Rust because it must exist before the webview does.
//! Strings are resolved through the same translation table the frontend uses
//! ([`crate::i18n`]), so a language switch updates both sides.

use tauri::menu::{Menu, MenuBuilder, MenuItem, SubmenuBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::i18n;
use crate::state::{AppState, WindowKind};
use crate::windows;

/// Tray icon id.
pub const TRAY_ID: &str = "main-tray";

/// Accelerators advertised in the README and shown next to the menu items.
pub const ACCEL_NEW_WINDOW: &str = "CmdOrCtrl+N";
pub const ACCEL_SETTINGS: &str = "CmdOrCtrl+Comma";
pub const ACCEL_RELOAD: &str = "CmdOrCtrl+R";
pub const ACCEL_QUIT: &str = "CmdOrCtrl+Q";

/// Menu ids handled by [`handle_event`].
pub mod ids {
    pub const NEW_WINDOW: &str = "new-window";
    pub const OPEN_PROFILE: &str = "open-profile";
    pub const RELOAD: &str = "reload";
    pub const SETTINGS: &str = "settings";
    pub const QUIT: &str = "quit";
    pub const ABOUT: &str = "about";
    pub const SHOW: &str = "show";
}

/// Builds the application menu for the given language.
pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    locale: &str,
    profiles: &[crate::config::Profile],
) -> tauri::Result<Menu<R>> {
    let translate = |key: &str| i18n::lookup(locale, key);

    let settings_item = MenuItem::with_id(
        app,
        ids::SETTINGS,
        translate("menu.settings"),
        true,
        Some(ACCEL_SETTINGS),
    )?;
    let new_window_item = MenuItem::with_id(
        app,
        ids::NEW_WINDOW,
        translate("menu.newWindow"),
        true,
        Some(ACCEL_NEW_WINDOW),
    )?;
    let about_item =
        MenuItem::with_id(app, ids::ABOUT, translate("menu.about"), true, None::<&str>)?;
    let quit_item = MenuItem::with_id(
        app,
        ids::QUIT,
        translate("menu.quit"),
        true,
        Some(ACCEL_QUIT),
    )?;
    let reload_item = MenuItem::with_id(
        app,
        ids::RELOAD,
        translate("menu.reload"),
        true,
        Some(ACCEL_RELOAD),
    )?;

    #[cfg(not(target_os = "macos"))]
    let menu = SubmenuBuilder::new(app, translate("menu.app"))
        .item(&settings_item)
        .item(&new_window_item)
        .separator()
        .item(&about_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let mut profile_items: Vec<MenuItem<R>> = Vec::new();
    for profile in profiles {
        let id = format!("{}:{}", ids::OPEN_PROFILE, profile.id);
        profile_items.push(MenuItem::with_id(
            app,
            id,
            &profile.name,
            true,
            None::<&str>,
        )?);
    }

    let mut profiles_builder = SubmenuBuilder::new(app, translate("menu.profiles"));
    for item in &profile_items {
        profiles_builder = profiles_builder.item(item);
    }
    let profiles_menu = profiles_builder
        .separator()
        .item(&new_window_item)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, translate("menu.view"))
        .item(&reload_item)
        .item(&settings_item)
        .build()?;

    #[cfg(target_os = "macos")]
    {
        // macOS expects an application menu first and an edit menu for the
        // standard text shortcuts to work. The application menu carries the
        // standard "About/Settings/Quit" trio so it behaves like every other
        // Mac app.
        let app_menu = SubmenuBuilder::new(app, translate("menu.app"))
            .item(&about_item)
            .separator()
            .item(&settings_item)
            .separator()
            .item(&quit_item)
            .build()?;
        let edit_menu = SubmenuBuilder::new(app, translate("menu.edit"))
            .undo()
            .redo()
            .separator()
            .cut()
            .copy()
            .paste()
            .select_all()
            .build()?;
        MenuBuilder::new(app)
            .item(&app_menu)
            .item(&edit_menu)
            .item(&profiles_menu)
            .item(&view_menu)
            .build()
    }

    #[cfg(not(target_os = "macos"))]
    {
        MenuBuilder::new(app)
            .item(&menu)
            .item(&profiles_menu)
            .item(&view_menu)
            .build()
    }
}

/// Applies the menu to the whole application (and therefore every new window).
pub fn install<R: Runtime>(app: &AppHandle<R>, locale: &str) {
    let profiles = app
        .try_state::<AppState>()
        .map(|state| state.snapshot().profiles.into_values().collect::<Vec<_>>())
        .unwrap_or_default();
    let locale = locale.to_owned();

    let handle = app.clone();
    let _ = handle.clone().run_on_main_thread(move || {
        let Ok(menu) = build_menu(&handle, &locale, &profiles) else {
            return;
        };
        let _ = handle.set_menu(menu);
    });
}

/// Builds the tray menu and icon.
pub fn install_tray<R: Runtime>(app: &AppHandle<R>, locale: &str) -> tauri::Result<()> {
    let translate = |key: &str| i18n::lookup(locale, key);

    let menu = MenuBuilder::new(app)
        .text(ids::SHOW, translate("tray.show"))
        .text(ids::NEW_WINDOW, translate("tray.newWindow"))
        .text(ids::SETTINGS, translate("tray.settings"))
        .separator()
        .text(ids::QUIT, translate("tray.quit"))
        .build()?;

    let icon = tauri::image::Image::from_bytes(crate::icons::TRAY_ICON_PNG)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(translate("app.name"))
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = windows::show_or_create_main(app, None);
            }
        })
        .build(app)?;

    Ok(())
}

/// Handles menu and tray events.
pub fn handle_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    if let Some(profile_id) = id.strip_prefix(&format!("{}:", ids::OPEN_PROFILE)) {
        let _ = windows::show_or_create_main(app, Some(profile_id));
        return;
    }

    match id {
        ids::NEW_WINDOW | ids::SHOW => {
            let _ = windows::open_main_window(app, None);
        }
        ids::SETTINGS | ids::ABOUT => {
            let _ = windows::open_settings_window(app, WindowKind::Main);
        }
        ids::RELOAD => {
            if let Some(window) = app.get_webview_window(windows::MAIN_LABEL) {
                let _ = window.eval("window.location.reload()");
            }
        }
        ids::QUIT => {
            app.exit(0);
        }
        _ => {}
    }
}

/// Rebuilds the menu after a language or profile change.
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let config = state.snapshot();
    install(app, &config.settings.locale);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_menu_ids_are_stable() {
        assert_eq!(
            format!("{}:default", ids::OPEN_PROFILE),
            "open-profile:default"
        );
        assert!(ids::QUIT != ids::SETTINGS);
    }
}
