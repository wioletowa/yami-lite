// Применение настроек: любое изменение (панель, правка settings.json руками, CLI) пересобирает
// тему и шрифты, перекрашивает выбранные программы (recolor.rs), перезапускает процессы шелла.

use crate::app::*;
use crate::config::{self, Settings};
use crate::font::Fonts;
use crate::theme::Theme;

impl App {
    // ── настройки ───────────────────────────────────────────────────────────
    pub fn settings_changed(&mut self) {
        let s = &self.st.set;
        crate::i18n::set(&s.lang);
        if s.font_shell != self.res.font {
            self.res.font = s.font_shell.clone();
            self.res.fonts = Fonts::with(&self.res.font);
        }
        self.st.t = Theme::new(s.accent_rgb(), s.tint, s.glass, s.lift, s.inner, s.light).apply();
        yami_px::gfx::set_reveal(yami_px::gfx::Reveal::from_id(&s.reveal), s.reveal_grain);
        // набор значков системы сменился — собрать заново
        let iset = crate::icons::system_theme();
        if iset != self.res.iset {
            self.res.icons = crate::icons::Icons::new(&iset);
            self.res.iset = iset;
        }
        self.res.icons.set_style(self.st.t.icon_ramp(), s.icon_pixel, s.icon_tint);
        self.write_palette();
        self.side_sync();
        self.sync_wall();
        self.setup_idle();
        self.sync_bar_outputs();
        self.sync_widgets();
        self.sync_dock();
        self.sync_bar();
        self.dirty_all();
    }

    /// записать настройки сейчас (перед перезапуском), даже посреди перетаскивания
    pub fn save_now(&mut self) {
        let j = self.st.set.to_json();
        if j != self.set_saved {
            let _ = config::write_atomic(&config::settings_path(), &j);
            self.set_saved = j;
        }
    }

    pub(crate) fn save_settings_if_changed(&mut self) {
        if self.press.is_some() || self.st.set == self.set_seen {
            return;
        }
        self.set_seen = self.st.set.clone();
        let j = self.st.set.to_json();
        if j != self.set_saved {
            let _ = config::write_atomic(&config::settings_path(), &j);
            self.set_saved = j;
            self.set_mtime = std::fs::metadata(config::settings_path()).and_then(|m| m.modified()).ok();
        }
    }

    pub(crate) fn check_settings_file(&mut self) {
        let m = std::fs::metadata(config::settings_path()).and_then(|m| m.modified()).ok();
        if m != self.set_mtime {
            self.set_mtime = m;
            let s = Settings::load();
            if s != self.st.set {
                self.st.set = s;
                self.set_saved = self.st.set.to_json();
                self.settings_changed();
            }
        }
    }

    /// палитра или список программ сменились — перекрасить их в нити (запись файлов, gsettings)
    pub fn write_palette(&mut self) {
        let key = format!("{}|{:?}", self.st.t.palette_json(), self.st.set.theme_apps);
        if key == self.palette_written {
            return;
        }
        self.palette_written = key;
        let (p, apps) = (crate::recolor::Pal::of(&self.st.t), self.st.set.theme_apps.clone());
        std::thread::spawn(move || crate::recolor::apply(&p, &apps));
    }

    /// ползунок настроек: значение в настройки, всё зависящее — следом
    pub fn slider_set(&mut self, key: &str, v: f32) {
        crate::prefs::settings_set_f32(&mut self.st.set, key, v);
        self.settings_changed();
    }

    /// процессы при шелле: ночной свет — по настройкам
    pub fn side_sync(&mut self) {
        self.night_sync();
    }
}

/// ползунки настроек (SSlider): число в поле Settings по ключу и обратно
pub fn settings_set_f32(s: &mut Settings, key: &str, v: f32) {
    match key {
        "tint" => s.tint = (v * 10.0).round() / 10.0,
        "glass" => s.glass = (v * 100.0).round() / 100.0,
        "lift" => s.lift = (v * 100.0).round() / 100.0,
        "inner" => s.inner = (v * 100.0).round() / 100.0,
        "dim" => s.dim = (v * 100.0).round() / 100.0,
        "anim_speed" => s.anim_speed = (v * 10.0).round() / 10.0,
        "dock_size" => s.dock_size = ((v / 4.0).round() * 4.0) as i32,
        "lock_after" => s.lock_after = v.round() as u32,
        "screen_off_after" => s.screen_off_after = v.round() as u32,
        "notif_timeout" => s.notif_timeout = v.round().max(2.0) as u32,
        "task_width" => s.task_width = (v / 10.0).round() as i32 * 10,
        "wall_pixel" => s.wall_pixel = v.round().max(1.0) as i32,
        "wall_dim" => s.wall_dim = (v * 20.0).round() / 20.0,
        _ => {}
    }
}

pub fn settings_get_f32(s: &Settings, key: &str) -> f32 {
    match key {
        "tint" => s.tint,
        "glass" => s.glass,
        "lift" => s.lift,
        "inner" => s.inner,
        "dim" => s.dim,
        "anim_speed" => s.anim_speed,
        "dock_size" => s.dock_size as f32,
        "lock_after" => s.lock_after as f32,
        "screen_off_after" => s.screen_off_after as f32,
        "notif_timeout" => s.notif_timeout as f32,
        "task_width" => s.task_width as f32,
        "wall_pixel" => s.wall_pixel as f32,
        "wall_dim" => s.wall_dim,
        _ => 0.0,
    }
}
