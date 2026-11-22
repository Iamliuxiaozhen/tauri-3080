//! i18n on the Rust side.
//!
//! Only the *native* chrome needs translating in Rust: menu bar items, the tray
//! menu, window titles and a handful of error messages. The web UI owns its own
//! catalog (`src/i18n/`), but both use the same language tags and the same key
//! names so they can never drift apart.

/// Language tag used when the configuration does not name a supported one.
pub const FALLBACK_LOCALE: &str = "en";

/// Supported language tags, in menu order.
pub const SUPPORTED_LOCALES: [&str; 4] = ["en", "zh-CN", "zh-TW", "pt-BR"];

/// English strings (the reference table).
const EN: &[(&str, &str)] = &[
    ("app.name", "127.0.0.1:3080"),
    ("menu.app", "127.0.0.1:3080"),
    ("menu.settings", "Settings…"),
    ("menu.newWindow", "New Window"),
    ("menu.profiles", "Profiles"),
    ("menu.view", "View"),
    ("menu.edit", "Edit"),
    ("menu.reload", "Reload"),
    ("menu.about", "About"),
    ("menu.quit", "Quit"),
    ("tray.show", "Show Window"),
    ("tray.newWindow", "New Window"),
    ("tray.settings", "Settings…"),
    ("tray.quit", "Quit"),
    ("settings.title", "Settings"),
    ("window.loading", "Starting…"),
];

/// Simplified Chinese.
const ZH_CN: &[(&str, &str)] = &[
    ("menu.settings", "设置…"),
    ("menu.newWindow", "新建窗口"),
    ("menu.profiles", "配置"),
    ("menu.view", "视图"),
    ("menu.edit", "编辑"),
    ("menu.reload", "重新加载"),
    ("menu.about", "关于"),
    ("menu.quit", "退出"),
    ("tray.show", "显示窗口"),
    ("tray.newWindow", "新建窗口"),
    ("tray.settings", "设置…"),
    ("tray.quit", "退出"),
    ("settings.title", "设置"),
    ("window.loading", "正在启动…"),
];

/// Traditional Chinese.
const ZH_TW: &[(&str, &str)] = &[
    ("menu.settings", "設定…"),
    ("menu.newWindow", "新增視窗"),
    ("menu.profiles", "設定檔"),
    ("menu.view", "檢視"),
    ("menu.edit", "編輯"),
    ("menu.reload", "重新載入"),
    ("menu.about", "關於"),
    ("menu.quit", "結束"),
    ("tray.show", "顯示視窗"),
    ("tray.newWindow", "新增視窗"),
    ("tray.settings", "設定…"),
    ("tray.quit", "結束"),
    ("settings.title", "設定"),
    ("window.loading", "正在啟動…"),
];

/// Brazilian Portuguese.
const PT_BR: &[(&str, &str)] = &[
    ("menu.settings", "Configurações…"),
    ("menu.newWindow", "Nova janela"),
    ("menu.profiles", "Perfis"),
    ("menu.view", "Exibir"),
    ("menu.edit", "Editar"),
    ("menu.reload", "Recarregar"),
    ("menu.about", "Sobre"),
    ("menu.quit", "Sair"),
    ("tray.show", "Mostrar janela"),
    ("tray.newWindow", "Nova janela"),
    ("tray.settings", "Configurações…"),
    ("tray.quit", "Sair"),
    ("settings.title", "Configurações"),
    ("window.loading", "Iniciando…"),
];

/// Returns the table for `locale`, falling back to English.
fn table(locale: &str) -> &'static [(&'static str, &'static str)] {
    match normalize_locale(locale).as_str() {
        "zh-CN" => ZH_CN,
        "zh-TW" => ZH_TW,
        "pt-BR" => PT_BR,
        _ => EN,
    }
}

/// Normalises `zh`, `zh-Hans`, `pt`, `pt_BR`, … to a supported tag.
#[must_use]
pub fn normalize_locale(locale: &str) -> String {
    let tag = locale.trim().replace('_', "-");
    let lower = tag.to_ascii_lowercase();

    if lower == "zh-cn" || lower == "zh-hans" || lower == "zh-sg" || lower == "zh" {
        return "zh-CN".to_owned();
    }
    if lower == "zh-tw" || lower == "zh-hant" || lower == "zh-hk" || lower == "zh-mo" {
        return "zh-TW".to_owned();
    }
    if lower == "pt-br" || lower == "pt" || lower.starts_with("pt-") {
        return "pt-BR".to_owned();
    }
    if lower.starts_with("en") {
        return "en".to_owned();
    }
    FALLBACK_LOCALE.to_owned()
}

/// Looks a key up, falling back to English and then to the key itself.
#[must_use]
pub fn lookup(locale: &str, key: &str) -> String {
    let localized = table(locale)
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| (*value).to_owned());

    localized
        .or_else(|| {
            EN.iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, value)| (*value).to_owned())
        })
        .unwrap_or_else(|| key.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_translates_every_english_key() {
        for locale in SUPPORTED_LOCALES {
            if locale == FALLBACK_LOCALE {
                continue;
            }
            let table = table(locale);
            for (key, _) in EN {
                // The product name and the app menu keep the product name.
                if *key == "app.name" || *key == "menu.app" {
                    continue;
                }
                assert!(
                    table.iter().any(|(candidate, _)| candidate == key),
                    "{locale} is missing {key}"
                );
            }
        }
    }

    #[test]
    fn normalises_tags() {
        assert_eq!(normalize_locale("zh"), "zh-CN");
        assert_eq!(normalize_locale("zh_Hans"), "zh-CN");
        assert_eq!(normalize_locale("zh-HK"), "zh-TW");
        assert_eq!(normalize_locale("pt_BR"), "pt-BR");
        assert_eq!(normalize_locale("pt-PT"), "pt-BR");
        assert_eq!(normalize_locale("en-US"), "en");
        assert_eq!(normalize_locale("kl"), "en");
    }

    #[test]
    fn unknown_keys_fall_back_to_the_key() {
        assert_eq!(lookup("zh-CN", "does.not.exist"), "does.not.exist");
        assert_eq!(lookup("en", "menu.quit"), "Quit");
        assert_eq!(lookup("zh-TW", "menu.quit"), "結束");
        assert_eq!(lookup("pt-BR", "settings.title"), "Configurações");
        assert_eq!(lookup("zh-CN", "menu.quit"), "退出");
    }
}
