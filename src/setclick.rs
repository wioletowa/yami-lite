// Настройки: что делает щелчок (Act::S…) — меняет Settings и применяет (settings_changed);
// вкладкам со своими файлами — их обработчики (обои).

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
                self.st.bar_ed.gear = None;
                self.st.bar_ed.add = None;
                self.tab_probe();
            }
            Act::SWallPick(i) => self.wall_pick(*i),
            Act::SPreset(i) => {
                s.palette = PRESETS[*i].id.into();
                s.accent.clear();
                // цвет выбран руками: обои его больше не перекрашивают
                s.wall_accent = false;
                s.accent2.clear();
                self.st.hex.set(&format!("#{:06x}", PRESETS[*i].accent));
                self.settings_changed();
            }
            Act::SNum(k, d) => {
                s.step_num(k, *d);
                self.settings_changed();
                // отступы уведомлений и OSD: поверхность создаётся заново на новом месте
                match *k {
                    "notif_offset" | "notif_offset_x" => self.renew_notif(SKind::Notifs),
                    "osd_offset" => self.renew_notif(SKind::Osd),
                    _ => {}
                }
            }
            Act::SThemeApp(id) => {
                match s.theme_apps.iter().position(|a| a == id) {
                    Some(i) => {
                        s.theme_apps.remove(i);
                    }
                    None => s.theme_apps.push(id.to_string()),
                }
                self.settings_changed();
                return true;
            }
            Act::SCtlTile(k) => {
                match s.control_tiles.iter().position(|x| x == k) {
                    // хоть одна плитка остаётся
                    Some(i) if s.control_tiles.len() > 1 => {
                        s.control_tiles.remove(i);
                    }
                    Some(_) => {}
                    None if s.control_tiles.len() < crate::control::MAX_TILES => s.control_tiles.push(k.to_string()),
                    None => {}
                }
                self.settings_changed();
            }
            Act::SOsd(k) => {
                match s.osd_kinds.iter().position(|x| x == k) {
                    Some(i) => {
                        s.osd_kinds.remove(i);
                    }
                    None => s.osd_kinds.push(k.to_string()),
                }
                self.settings_changed();
            }
            Act::SBool(k) => {
                match *k {
                    "animations" => s.animations = !s.animations,
                    "anim_close" => s.anim_close = !s.anim_close,
                    "scanlines" => s.scanlines = !s.scanlines,
                    "icon_pixel" => s.icon_pixel = !s.icon_pixel,
                    "icon_tint" => s.icon_tint = !s.icon_tint,
                    "bar_float" => s.bar_float = !s.bar_float,
                    "widget_contrast" => s.widget_contrast = !s.widget_contrast,
                    "blur_shell" => s.blur_shell = !s.blur_shell,
                    "clock_seconds" => s.clock_seconds = !s.clock_seconds,
                    "tray_pixel" => s.tray_pixel = !s.tray_pixel,
                    "bat_hide_full" => s.bat_hide_full = !s.bat_hide_full,
                    "clock_date" => s.clock_date = !s.clock_date,
                    "dnd" => s.dnd = !s.dnd,
                    "cast_dnd" => s.cast_dnd = !s.cast_dnd,
                    "notif_hover_pause" => s.notif_hover_pause = !s.notif_hover_pause,
                    "notif_critical_stay" => s.notif_critical_stay = !s.notif_critical_stay,
                    "notif_app_timeout" => s.notif_app_timeout = !s.notif_app_timeout,
                    "dock" => s.dock = !s.dock,
                    "dock_bounce" => s.dock_bounce = !s.dock_bounce,
                    "dock_running" => s.dock_running = !s.dock_running,
                    "dock_labels" => s.dock_labels = !s.dock_labels,
                    "dock_launcher" => s.dock_launcher = !s.dock_launcher,
                    "dock_trash" => s.dock_trash = !s.dock_trash,
                    "game_dnd" => s.game_dnd = !s.game_dnd,
                    "night" => s.night = !s.night,
                    "agent_notif" => s.agent_notif = !s.agent_notif,
                    "agent_tray" => s.agent_tray = !s.agent_tray,
                    "media_art_net" => s.media_art_net = !s.media_art_net,
                    _ => {}
                }
                self.settings_changed();
            }
            Act::SFontOpen(k) => self.font_toggle(k),
            Act::SFont(n) => self.font_pick(n.clone()),
            Act::SCycle(k) => match *k {
                "osd_test" => {
                    // первое включённое: выключенные OSD не показываются и для пробы
                    if let Some(k) = crate::notif::OSD_KINDS.iter().map(|k| k.0).find(|k| self.st.set.osd_kinds.iter().any(|x| x == k)) {
                        self.show_osd(k);
                    }
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
                    self.dirty_panels();
                }
                "theme_save" => self.theme_do("save", 0),
                "hex_apply" => self.apply_hex(),
                "theme_dark" | "theme_light" => {
                    s.light = *k == "theme_light";
                    self.settings_changed();
                }
                "wh_key_apply" => {
                    let k: String = self.st.wh_key_field.text.trim().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                    self.st.set.wallhaven_key = k;
                    yami_px::ui::wipe(&mut self.st.wh_key_field.text);
                    self.st.wh_key_field.cur = 0;
                    self.st.field_focus = None;
                    self.settings_changed();
                }
                "wh_dir_apply" => {
                    self.st.set.wallhaven_dir = self.st.wh_dir_field.text.trim().to_string();
                    self.st.field_focus = None;
                    self.settings_changed();
                }
                "wall_apply" => {
                    let p = self.st.wall_field.text.clone();
                    self.set_wall(&p);
                }
                "weather_apply" => {
                    self.st.set.weather_city = self.st.weather_field.text.trim().chars().take(80).collect();
                    self.st.field_focus = None;
                    self.settings_changed();
                    self.weather_reset();
                }
                "weather_units" => {
                    self.st.set.weather_units = if self.st.set.weather_units == "f" { "c" } else { "f" }.into();
                    self.settings_changed();
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
                "do_lock" => {
                    self.close_modal();
                    self.lock_now();
                }
                "reload" => self.ipc("reload"),
                "launcher_apply" => {
                    self.st.set.launcher_cmd = self.st.launcher_field.text.trim().to_string();
                    self.st.field_focus = None;
                    self.settings_changed();
                }
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
                // клик в списке «добавить»: в конец своей колонки (без списка — в привычное место)
                let m = m.to_string();
                match self.st.bar_ed.add.take() {
                    Some(si) => s.side_mut(si).push(m),
                    None if m == "tasks" => s.bar_center.push(m),
                    None if matches!(m.as_str(), "launcher" | "workspaces") => s.bar_left.push(m),
                    None => {
                        let at = s.bar_right.iter().position(|x| x == "settings" || x == "power").unwrap_or(s.bar_right.len());
                        s.bar_right.insert(at, m);
                    }
                }
                self.settings_changed();
            }
            Act::SModHide(m) => {
                match s.bar_hidden.iter().position(|x| x == m) {
                    Some(i) => {
                        s.bar_hidden.remove(i);
                    }
                    None => s.bar_hidden.push(m.clone()),
                }
                self.settings_changed();
            }
            Act::SModGear(m) => {
                self.st.bar_ed.add = None;
                self.st.bar_ed.gear = Some(m.clone());
            }
            Act::SBarAdd(si) => {
                self.st.bar_ed.gear = None;
                self.st.bar_ed.add = if self.st.bar_ed.add == Some(*si) { None } else { Some(*si) };
            }
            Act::SBarClose => {
                self.st.bar_ed.add = None;
                self.st.bar_ed.gear = None;
                self.st.field_focus = None;
            }
            Act::SBarReset(si) => {
                let d = crate::config::Settings::default();
                *s.side_mut(*si) = [d.bar_left, d.bar_center, d.bar_right][*si].clone();
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
                if *k == "wh_dir" && self.st.wh_dir_field.text.is_empty() {
                    let p = self.st.set.wallhaven_dir.clone();
                    self.st.wh_dir_field.set(&p);
                }
                if *k == "launcher_cmd" && self.st.launcher_field.text.is_empty() {
                    let p = self.st.set.launcher_cmd.clone();
                    self.st.launcher_field.set(&p);
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
                if *k == "wall" && self.st.wall_field.text.is_empty() {
                    let p = self.st.set.wallpaper.clone();
                    self.st.wall_field.set(&p);
                }
            }
            _ => return self.wall_click(a),
        }
        self.dirty_panels();
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
            self.st.set.accent2.clear();
            self.st.field_focus = None;
            self.settings_changed();
        }
    }
}
