// Вид приложений берётся из настроек шелла (~/.config/yami-shell.json): палитра и тона,
// язык, анимации. Файл перечитывается, когда меняется, — приложения перекрашиваются вместе
// с шеллом, без перезапуска.

use crate::theme::{parse_hex, Theme, PRESETS};
use std::time::SystemTime;

#[derive(Clone)]
pub struct Look {
    pub theme: Theme,
    /// "" (по языку системы), "ru", "en", "pt-BR"
    pub lang: String,
    pub animations: bool,
    pub anim_speed: f32,
    pub icon_theme: String,
    pub blur: bool,
    /// значки программ пиксельными в палитре
    pub icon_pixel: bool,
    /// пиксельные значки в тонах палитры, а не в своих цветах
    pub icon_tint: bool,
    /// шрифты: шелла и отдельно файлов и просмотрщика ("" — как у шелла; у шелла "" — Tiny5)
    pub font_shell: String,
    pub font_files: String,
    pub font_view: String,
    stamp: Option<SystemTime>,
}

fn stamp() -> Option<SystemTime> {
    std::fs::metadata(crate::paths::shell_settings()).and_then(|m| m.modified()).ok()
}

impl Look {
    pub fn load() -> Look {
        Look::load_mode(None)
    }

    /// light: Some — тема поверх настроек (снимки `shot --theme light`)
    pub fn load_mode(light: Option<bool>) -> Look {
        let v: serde_json::Value = std::fs::read_to_string(crate::paths::shell_settings()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::Value::Null);
        let mut l = Look::from_json(&v, light);
        l.stamp = stamp();
        l
    }

    /// вид из JSON в формате настроек шелла (экран входа читает копию, которую выкладывает шелл)
    pub fn from_json(v: &serde_json::Value, light: Option<bool>) -> Look {
        let f = |k: &str, d: f32| v.get(k).and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(d);
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d);
        let palette = s("palette");
        let accent = parse_hex(&s("accent")).unwrap_or_else(|| PRESETS.iter().find(|p| p.id == palette).unwrap_or(&PRESETS[0]).accent);
        crate::gfx::set_reveal(crate::gfx::Reveal::from_id(&s("reveal")), v.get("revealGrain").and_then(|x| x.as_i64()).unwrap_or(1) as i32);
        Look {
            theme: Theme::new(accent, f("tint", 1.0), f("glass", 0.55), f("lift", 0.0), f("inner", 1.0), light.unwrap_or_else(|| b("light", false))).apply(),
            lang: s("lang"),
            // лёгкий режим шелла (слабая или виртуальная машина) выключает анимации и здесь
            animations: b("animations", true) && !b("lite", false),
            anim_speed: f("animSpeed", 1.0),
            icon_theme: s("iconTheme"),
            blur: b("blurShell", true) && !b("lite", false),
            icon_pixel: b("iconPixel", true),
            icon_tint: b("iconTint", false),
            font_shell: s("fontShell"),
            font_files: s("fontFiles"),
            font_view: s("fontView"),
            stamp: None,
        }
    }

    /// шрифт приложения: свой или как у шелла
    pub fn font(&self, app: &str) -> &str {
        let own = match app {
            "files" => &self.font_files,
            "view" => &self.font_view,
            _ => &self.font_shell,
        };
        if own.is_empty() {
            &self.font_shell
        } else {
            own
        }
    }

    /// настройки шелла изменились с прошлой загрузки
    pub fn stale(&self) -> bool {
        stamp() != self.stamp
    }

    /// длительность анимации с учётом скорости из настроек; 0 — анимации выключены
    pub fn dur(&self, ms: f32) -> f32 {
        if self.animations {
            ms * self.anim_speed.max(0.1)
        } else {
            0.0
        }
    }
}

/// «сделать обоями» из файлов и просмотрщика: путь — в настройки шелла, остальное не трогаем.
/// Шелл замечает новый файл настроек сам и готовит картинку (wall.rs).
pub fn set_wallpaper(p: &std::path::Path) -> bool {
    let path = crate::paths::shell_settings();
    let Some(mut v) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return false };
    let Some(o) = v.as_object_mut() else { return false };
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    o.insert("wallpaper".into(), serde_json::Value::String(abs.to_string_lossy().to_string()));
    crate::paths::write_atomic(&path, &(serde_json::to_string_pretty(&v).unwrap_or_default() + "\n")).is_ok()
}
