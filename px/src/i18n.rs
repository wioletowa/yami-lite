//! Язык интерфейса: русский (исходный) или английский. Пусто — по LC_ALL, LC_MESSAGES, LANG
//! (первое непустое).

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

/// значение для настроек: «ru», «en»; пусто — язык системы
pub fn normalize(tag: &str) -> &'static str {
    let tag = tag.trim().split(['.', '@']).next().unwrap_or("").replace('_', "-").to_ascii_lowercase();
    match tag.split('-').next().unwrap_or("") {
        "ru" => "ru",
        "en" => "en",
        _ => "",
    }
}

fn resolve(explicit: &str, locales: &[&str]) -> bool {
    let source = if explicit.is_empty() {
        locales.iter().copied().find(|s| !s.is_empty()).unwrap_or("")
    } else {
        explicit
    };
    normalize(source) != "ru"
}

pub fn set(lang: &str) {
    let locales: Vec<String> = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().map(|k| std::env::var(k).unwrap_or_default()).collect();
    let refs: Vec<&str> = locales.iter().map(String::as_str).collect();
    ENGLISH.store(resolve(lang, &refs), Ordering::Relaxed);
}

pub fn en() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn translated() -> bool {
    en()
}

/// кнопка языка в настройках по кругу: авто, English, Русский
pub fn next(lang: &str) -> &'static str {
    match normalize(lang) {
        "" => "en",
        "en" => "ru",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_aliases_and_precedence() {
        assert_eq!(normalize("ru_RU.UTF-8"), "ru");
        assert_eq!(normalize("en-US"), "en");
        assert_eq!(normalize("pt_BR.UTF-8"), "");
        assert!(!resolve("", &["", "ru_RU.UTF-8", "en_US.UTF-8"]));
        assert!(resolve("", &["de_DE.UTF-8"]));
        assert!(resolve("en", &["ru_RU.UTF-8"]));
    }

    #[test]
    fn settings_cycle() {
        let mut lang = "";
        for expected in ["en", "ru", ""] {
            lang = next(lang);
            assert_eq!(lang, expected);
        }
    }
}
