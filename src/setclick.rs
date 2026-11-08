// Настройки: что делает щелчок (Act::S…) — меняет Settings и применяет (settings_changed);
// вкладкам со своими файлами — их обработчики (обои, клавиши, обновления).

use crate::app::{App, SKind};
use crate::config::Widget;
use crate::settings::cycle;
use crate::theme::{parse_hex, PRESETS};
use crate::ui::Act;
use crate::widgets::{self, ANIMS, COLORS, SIZES, SOURCES, STYLES};

impl App {
    /// клики внутри настроек; true — обработано
    pub fn settings_click(&mut self, a: &Act) -> bool {
        let s = &mut self.st.set;
        match a {
            Act::STab(id) => {
                self.st.stab = id;
                self.st.field_focus = None;
                self.keys_capture(None);
                if *id == "screens" {
                    self.disp_open();
                }
                self.tab_probe();
            }
            Act::SPreset(i) => {
                s.palette = PRESETS[*i].id.into();
                s.accent.clear();
                // цвет выбран руками: обои его больше не перекрашивают
                s.wall_accent = false;
                self.st.hex.set(&format!("#{:06x}", PRESETS[*i].accent));
                self.settings_changed();
            }
            Act::SNum(k, d) => {
                s.step_num(k, *d);
                self.settings_changed();
                // отступы уведомлений и OSD: поверхность создаётся заново на новом месте
                match *k {
                    "notif_offset" => self.renew_notif(SKind::Notifs),
                    "osd_offset" => self.renew_notif(SKind::Osd),
                    _ => {}
                }
            }
            Act::STplForce(i) => {
                if let Some(f) = self.st.restyle.tpls.get(*i).map(|t| t.file.clone()) {
                    self.templates_run(Some(f));
                }
            }
            Act::SThemeApp(id) => {
                let on = match s.theme_apps.iter().position(|a| a == id) {
                    Some(i) => {
                        s.theme_apps.remove(i);
                        false
                    }
                    None => {
                        s.theme_apps.push(id.to_string());
                        true
                    }
                };
                // kitty включили руками: наш файл и строка include (выключение её уберёт theme.py)
                if on && *id == "kitty" {
                    crate::prefs::kitty_attach();
                }
                self.settings_changed();
                self.theme_rerun();
                return true;
            }
            Act::SBool(k) => {
                match *k {
                    "animations" => s.animations = !s.animations,
                    "lite" => s.lite = !s.lite,
                    "anim_close" => s.anim_close = !s.anim_close,
                    "scanlines" => s.scanlines = !s.scanlines,
                    "icon_pixel" => s.icon_pixel = !s.icon_pixel,
                    "icon_tint" => s.icon_tint = !s.icon_tint,
                    "stacking" => {
                        s.stacking = !s.stacking;
                        let on = s.stacking;
                        self.st.niri.set_stacking(on);
                        self.sync_titles();
                    }
                    "titlebars" => {
                        s.titlebars = !s.titlebars;
                        self.sync_titles();
                    }
                    "wall_kanji" => s.wall_kanji = !s.wall_kanji,
                    "wall_skwd" | "wall_skwd_colors" => {
                        if *k == "wall_skwd" {
                            s.wall_skwd = !s.wall_skwd;
                        } else {
                            s.wall_skwd_colors = !s.wall_skwd_colors;
                        }
                        self.settings_changed();
                        // сразу, не ждать тика: обои и палитра skwd-wall
                        self.skwd_tick();
                        self.skwd_colors();
                        self.dirty(&SKind::Modal);
                        return true;
                    }
                    "bar_float" => s.bar_float = !s.bar_float,
                    "widget_contrast" => s.widget_contrast = !s.widget_contrast,
                    "blur_shell" => s.blur_shell = !s.blur_shell,
                    "blur_term" => s.blur_term = !s.blur_term,
                    "clock_seconds" => s.clock_seconds = !s.clock_seconds,
                    "tray_pixel" => s.tray_pixel = !s.tray_pixel,
                    "bat_hide_full" => s.bat_hide_full = !s.bat_hide_full,
                    "clock_date" => s.clock_date = !s.clock_date,
                    "dnd" => s.dnd = !s.dnd,
                    "shot_copy" => s.shot_copy = !s.shot_copy,
                    "float_media" => s.float_media = !s.float_media,
                    "dock" => s.dock = !s.dock,
                    "dock_bounce" => s.dock_bounce = !s.dock_bounce,
                    "dock_running" => s.dock_running = !s.dock_running,
                    "dock_labels" => s.dock_labels = !s.dock_labels,
                    "dock_launcher" => s.dock_launcher = !s.dock_launcher,
                    "dock_trash" => s.dock_trash = !s.dock_trash,
                    "float_pip" => s.float_pip = !s.float_pip,
                    "shot_save" => s.shot_save = !s.shot_save,
                    "shot_loupe" => s.shot_loupe = !s.shot_loupe,
                    "shot_notify" => s.shot_notify = !s.shot_notify,
                    "game_fullscreen" => s.game_fullscreen = !s.game_fullscreen,
                    "game_vrr" => s.game_vrr = !s.game_vrr,
                    "game_tearing" => s.game_tearing = !s.game_tearing,
                    "game_cache" => s.game_cache = !s.game_cache,
                    "game_dnd" => s.game_dnd = !s.game_dnd,
                    "game_profile" => s.game_profile = !s.game_profile,
                    "game_ntsync" => s.game_ntsync = !s.game_ntsync,
                    "game_gamemode" => s.game_gamemode = !s.game_gamemode,
                    "night" => s.night = !s.night,
                    "session_restore" => s.session_restore = !s.session_restore,
                    "update_check" => s.update_check = !s.update_check,
                    "fetch_full" => {
                        // theme.py читает настройки с диска: сохранить, потом перерисовать fastfetch
                        s.fetch_full = !s.fetch_full;
                        self.settings_changed();
                        self.save_now();
                        std::thread::spawn(|| {
                            let _ = crate::proc::theme_py().arg("--fetch").status();
                        });
                        self.dirty(&SKind::Modal);
                        return true;
                    }
                    k if crate::login::toggle(s, k) => {}
                    k if crate::agents::toggle(s, k) => {
                        // тексты из сети включили: «не нашлось» для текущего трека — искать заново
                        if k == "lyrics_net" {
                            self.st.lyrics = Default::default();
                            self.lyrics_want();
                        }
                    }
                    "browser_menus" | "browser_font" => {
                        if *k == "browser_menus" {
                            s.browser_menus = !s.browser_menus;
                        } else {
                            s.browser_font = !s.browser_font;
                        }
                        self.settings_changed();
                        self.apply_fonts();
                        self.dirty(&SKind::Modal);
                        return true;
                    }
                    _ => {}
                }
                self.settings_changed();
                // превью наборов — в новом виде значков
                if matches!(*k, "icon_pixel" | "icon_tint") && self.st.sets.is_some() {
                    self.st.sets = None;
                    self.sets_load();
                }
            }
            Act::SIconSet(i) => {
                let Some(name) = self.st.sets.as_ref().and_then(|x| x.icons.get(*i)).map(|c| c.name.clone()) else { return true };
                s.icon_theme = name;
                self.icons_apply();
            }
            Act::SCursorSet(i) => {
                let Some(name) = self.st.sets.as_ref().and_then(|x| x.cursors.get(*i)).map(|c| c.name.clone()) else { return true };
                s.cursor = name;
                self.settings_changed();
                self.apply_cursor();
            }
            Act::SCycle(k) if k.starts_with("font_") && *k != "font_size" => {
                let k: &str = k;
                return self.font_cycle(k);
            }
            Act::SCycle(k) => match *k {
                "cpu_style" => {
                    s.cpu_style = cycle(&[("gauge", ""), ("graph", ""), ("number", "")], &s.cpu_style).into();
                    self.settings_changed();
                }
                "notif_pos" => {
                    s.notif_pos = cycle(crate::notif::POS, &s.notif_pos).into();
                    self.settings_changed();
                    self.renew_notif(SKind::Notifs);
                }
                "osd_pos" => {
                    s.osd_pos = cycle(crate::notif::OSD_POS, &s.osd_pos).into();
                    self.settings_changed();
                    self.renew_notif(SKind::Osd);
                }
                "shot_confirm" => {
                    s.shot_confirm = if s.shot_confirm == "instant" { "space" } else { "instant" }.into();
                    self.settings_changed();
                }
                "media_view" => self.media_view_next(),
                "media_viz" => {
                    let mut l = crate::widgets::STYLES.to_vec();
                    l.push(("off", "выкл"));
                    s.media_viz = cycle(&l, &s.media_viz).into();
                    self.media_sync();
                }
                "tray_mode" => {
                    s.tray_mode = if s.tray_mode == "arrow" { "icons" } else { "arrow" }.into();
                    self.settings_changed();
                }
                "temp_style" => {
                    s.temp_style = if s.temp_style == "compact" { "full" } else { "compact" }.into();
                    self.settings_changed();
                }
                "color_page_pal" | "color_page_themes" | "color_page_apps" => {
                    self.st.color_page = match *k {
                        "color_page_themes" => 1,
                        "color_page_apps" => 2,
                        _ => 0,
                    };
                    if self.st.color_page == 1 {
                        self.theme_do("list", 0);
                    }
                    if self.st.color_page == 2 {
                        self.st.restyle = crate::restyle::load();
                    }
                    self.dirty(&SKind::Modal);
                }
                "theme_rerun" => self.theme_rerun(),
                "templates_open" => {
                    let d = crate::config::config_dir().join("yami/templates");
                    crate::prefs::templates_example(&d);
                    App::open_dir(d.display().to_string());
                }
                "theme_save" => self.theme_do("save", 0),
                "bar_page_mods" | "bar_page_look" => {
                    self.st.bar_ed.look = *k == "bar_page_look";
                    self.dirty(&SKind::Modal);
                }
                "bar_center_align" => {
                    s.bar_center_align = if s.bar_center_align == "after-left" { "center" } else { "after-left" }.into();
                    self.settings_changed();
                }
                "bar_pos" => {
                    s.bar_pos = if s.bar_pos == "bottom" { "top" } else { "bottom" }.into();
                    self.settings_changed();
                }
                "bar_sep" => {
                    s.bar_sep = cycle(&[("dots", ""), ("line", ""), ("blocks", ""), ("gap", "")], &s.bar_sep).into();
                    self.settings_changed();
                }
                "bar_sep_color" => {
                    s.bar_sep_color = cycle(&[("border", ""), ("accent", ""), ("dim", "")], &s.bar_sep_color).into();
                    self.settings_changed();
                }
                "reveal" => {
                    s.reveal = cycle(&[("dither", ""), ("scan", ""), ("noise", ""), ("wipe", ""), ("blocks", "")], &s.reveal).into();
                    self.settings_changed();
                }
                "dock_anim" => {
                    s.dock_anim = cycle(&[("grow", ""), ("wave", ""), ("lift", ""), ("none", "")], &s.dock_anim).into();
                    self.settings_changed();
                }
                "font_size" => {
                    s.font_size = match s.font_size {
                        0 => 16,
                        16 => 24,
                        24 => 32,
                        _ => 0,
                    };
                    self.settings_changed();
                    self.apply_fonts();
                }
                "hex_apply" => self.apply_hex(),
                "theme_dark" | "theme_light" => {
                    s.light = *k == "theme_light";
                    self.settings_changed();
                }
                "lang" => {
                    s.lang = yami_px::i18n::next(&s.lang).into();
                    self.settings_changed();
                }
                "wall_apply" => {
                    let p = self.st.wall_field.text.clone();
                    self.set_wall(&p);
                }
                "curtain_style" => {
                    s.curtain_style = crate::curtain::next_style(&s.curtain_style).into();
                    self.settings_changed();
                }
                "shot_delay" => {
                    s.shot_delay = match s.shot_delay {
                        3 => 5,
                        5 => 10,
                        _ => 3,
                    };
                    self.settings_changed();
                }
                "set_add" => self.sets_add(),
                "cursor_size" => {
                    s.cursor_size = match s.cursor_size {
                        24 => 32,
                        32 => 48,
                        48 => 64,
                        _ => 24,
                    };
                    self.settings_changed();
                    self.apply_cursor();
                }
                "dock_side" => {
                    s.dock_side = match s.dock_side.as_str() {
                        "bottom" => "left",
                        "left" => "right",
                        "right" => "top",
                        _ => "bottom",
                    }
                    .into();
                    // край сменился — пересоздать
                    for k in [SKind::Dock, SKind::DockEdge] {
                        for id in self.find(&k) {
                            self.destroy(id);
                        }
                    }
                    self.settings_changed();
                }
                "dock_hide" => {
                    let s = &mut self.st.set;
                    s.dock_hide = match s.dock_hide.as_str() {
                        "smart" => "always",
                        "always" => "never",
                        _ => "smart",
                    }
                    .into();
                    for k in [SKind::Dock, SKind::DockEdge] {
                        for id in self.find(&k) {
                            self.destroy(id);
                        }
                    }
                    self.settings_changed();
                }
                "kb_apply" => {
                    self.st.set.kb_layout = self.st.keys.lay.text.trim().to_string();
                    self.st.keys.lay.clear();
                    self.st.field_focus = None;
                    self.settings_changed();
                }
                "kb_switch" => {
                    let sw = crate::keys::SWITCHES;
                    let i = sw.iter().position(|x| x.0 == self.st.set.kb_switch).map(|i| (i + 1) % sw.len()).unwrap_or(0);
                    self.st.set.kb_switch = sw[i].0.to_string();
                    self.settings_changed();
                }
                "weather_apply" => {
                    // город сменился: старый ответ не показывать, спросить сразу
                    self.st.set.weather_city = self.st.weather_field.text.trim().chars().take(64).collect();
                    self.st.field_focus = None;
                    self.weather_reset();
                }
                "weather_units" => {
                    self.st.set.weather_units = if self.st.set.weather_units == "f" { "c" } else { "f" }.into();
                    self.weather_reset();
                }
                "geo_apply" => {
                    let g = self.st.geo_field.text.trim().to_string();
                    // пусто — по часам; иначе только верные координаты
                    if g.is_empty() || crate::night::geo(&g).is_some() {
                        self.st.set.night_geo = g;
                        self.st.field_focus = None;
                        self.settings_changed();
                    }
                }
                "game_apply" => {
                    let p = self.st.game_field.text.trim().to_string();
                    self.st.set.game_apps = p;
                    self.st.field_focus = None;
                    self.settings_changed();
                }
                "float_apply" => {
                    let p = self.st.float_field.text.trim().to_string();
                    self.st.set.float_apps = p;
                    self.st.field_focus = None;
                    self.settings_changed();
                }
                "shot_dir_apply" => {
                    let p = self.st.shot_field.text.trim().to_string();
                    self.st.set.shot_dir = p;
                    self.st.field_focus = None;
                    self.settings_changed();
                }
                "do_lock" => {
                    self.close_modal();
                    self.lock_now();
                }
                "reload" => self.ipc("reload"),
                k if self.update_cycle(k) || self.agents_cycle(k) || self.login_cycle(k) => {}
                "open_config" => {
                    // на диске должно быть то, что сейчас в настройках
                    let j = self.st.set.to_json();
                    let _ = crate::config::write_atomic(&crate::config::settings_path(), &j);
                    self.set_saved = j;
                    // настройки — слой над окнами: редактор открылся бы под ними (#8)
                    self.close_modal();
                    App::spawn(&["xdg-open".into(), crate::config::settings_path().display().to_string()]);
                }
                _ => {}
            },
            Act::STheme(i) => self.theme_do("apply", *i),
            Act::SThemeDel(i) => self.theme_do("remove", *i),
            Act::SModDel(si, i) => {
                let l = s.side_mut(*si);
                if *i < l.len() {
                    l.remove(*i);
                }
                self.settings_changed();
            }
            Act::SModNew(m) => {
                // клик (не перетаскивание): в привычное место
                let m = m.to_string();
                let m = &m;
                if m == "tasks" {
                    s.bar_center.push(m.clone());
                } else if matches!(m.as_str(), "launcher" | "workspaces" | "mode") {
                    s.bar_left.push(m.clone());
                } else {
                    // справа — перед настройками и питанием
                    let at = s.bar_right.iter().position(|x| x == "settings" || x == "power").unwrap_or(s.bar_right.len());
                    s.bar_right.insert(at, m.clone());
                }
                self.settings_changed();
            }
            Act::SWidgetSel(id) => self.st.sel_widget = *id,
            Act::SWidgetAdd(k) => {
                let id = s.next_widget_id();
                let mut w = Widget::new(id, k);
                if *k == "net" {
                    w.style = "line".into();
                }
                if *k == "media" {
                    w.style = "bars".into();
                }
                // скрипт — первый из папки модулей
                if *k == "script" {
                    w.source = crate::plugins::scripts().into_iter().next().unwrap_or_default();
                }
                s.widgets.push(w);
                self.st.sel_widget = id;
                self.settings_changed();
            }
            Act::SWidgetDel(id) => {
                s.widgets.retain(|w| w.id != *id);
                self.st.sel_widget = self.st.set.widgets.first().map(|w| w.id).unwrap_or(0);
                self.settings_changed();
            }
            Act::SWidgetOpt(id, key) => {
                if let Some(w) = s.widget_mut(*id) {
                    let v = match *key {
                        "source" if w.kind == "script" => {
                            let v = crate::plugins::scripts();
                            v.iter().position(|n| *n == w.source).map(|i| v[(i + 1) % v.len()].clone()).or_else(|| v.first().cloned()).unwrap_or_default()
                        }
                        "source" => cycle(SOURCES, &w.source).to_string(),
                        "style" => cycle(STYLES, &w.style).to_string(),
                        "anim" => cycle(ANIMS, &w.anim).to_string(),
                        "size" => cycle(SIZES, &w.size).to_string(),
                        "color" => cycle(COLORS, &w.color).to_string(),
                        _ => String::new(),
                    };
                    widgets::set_opt(w, key, &v);
                }
                self.settings_changed();
            }
            Act::SEdit => {
                self.close_all();
                self.set_editing(true);
            }
            Act::SResetPos => {
                let id = self.st.sel_widget;
                if let Some(w) = self.st.set.widget_mut(id) {
                    w.x = -1;
                    w.y = -1;
                }
                self.settings_changed();
            }
            Act::DockPin(id, _) => {
                let id = id.clone();
                self.st.set.dock_pins.retain(|p| *p != id);
                self.settings_changed();
            }
            Act::SField(k) => {
                self.st.field_focus = Some(k);
                if *k == "float_apps" && self.st.float_field.text.is_empty() {
                    let p = self.st.set.float_apps.clone();
                    self.st.float_field.set(&p);
                }
                if *k == "kb_layout" && self.st.keys.lay.text.is_empty() {
                    let p = self.st.set.kb_layout.clone();
                    self.st.keys.lay.set(&p);
                }
                if *k == "weather_city" && self.st.weather_field.text.is_empty() {
                    let p = self.st.set.weather_city.clone();
                    self.st.weather_field.set(&p);
                }
                if *k == "night_geo" && self.st.geo_field.text.is_empty() {
                    let p = self.st.set.night_geo.clone();
                    self.st.geo_field.set(&p);
                }
                if *k == "game_apps" && self.st.game_field.text.is_empty() {
                    let p = self.st.set.game_apps.clone();
                    self.st.game_field.set(&p);
                }
                if *k == "shot_dir" && self.st.shot_field.text.is_empty() {
                    let p = self.st.set.shot_dir.clone();
                    self.st.shot_field.set(&p);
                }
                if *k == "wall" && self.st.wall_field.text.is_empty() {
                    let p = self.st.set.wallpaper.clone();
                    self.st.wall_field.set(&p);
                }
            }
            _ => return self.wall_click(a) || self.keys_click(a) || self.update_click(a),
        }
        self.dirty(&SKind::Modal);
        true
    }

    pub(crate) fn apply_hex(&mut self) {
        let mut h = self.st.hex.text.trim().to_string();
        if !h.starts_with('#') {
            h.insert(0, '#');
        }
        if parse_hex(&h).is_some() {
            self.st.set.accent = h.to_lowercase();
            self.st.set.wall_accent = false;
            self.st.field_focus = None;
            self.settings_changed();
        }
    }
}
