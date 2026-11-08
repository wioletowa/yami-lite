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
                    None if s.control_tiles.len() < 6 => s.control_tiles.push(k.to_string()),
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
                    "wall_kanji" => s.wall_kanji = !s.wall_kanji,
                    "bar_float" => s.bar_float = !s.bar_float,
                    "widget_contrast" => s.widget_contrast = !s.widget_contrast,
                    "blur_shell" => s.blur_shell = !s.blur_shell,
                    "clock_seconds" => s.clock_seconds = !s.clock_seconds,
                    "tray_pixel" => s.tray_pixel = !s.tray_pixel,
                    "bat_hide_full" => s.bat_hide_full = !s.bat_hide_full,
                    "clock_date" => s.clock_date = !s.clock_date,
                    "dnd" => s.dnd = !s.dnd,
                    "cast_dnd" => s.cast_dnd = !s.cast_dnd,
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
                    "lyrics_net" => {
                        s.lyrics_net = !s.lyrics_net;
                        // тексты из сети включили: «не нашлось» для текущего трека — искать заново
                        self.st.lyrics = Default::default();
                        self.lyrics_want();
                    }
                    _ => {}
                }
                self.settings_changed();
            }
            Act::SCycle("font_open") => self.font_toggle(),
            Act::SFont(n) => self.font_pick(n.clone()),
            Act::SCycle(k) => match *k {
                "cpu_style" => {
                    s.cpu_style = cycle(&[("gauge", ""), ("graph", ""), ("number", "")], &s.cpu_style).into();
                    self.settings_changed();
                }
                "osd_test" => {
                    // первое включённое: выключенные OSD не показываются и для пробы
                    if let Some(k) = crate::notif::OSD_KINDS.iter().map(|k| k.0).find(|k| self.st.set.osd_kinds.iter().any(|x| x == k)) {
                        self.show_osd(k);
                    }
                }
                "ws_style" => {
                    s.ws_style = if s.ws_style == "dots" { "boxes" } else { "dots" }.into();
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
                    self.dirty_panels();
                }
                "theme_save" => self.theme_do("save", 0),
                "bar_page_mods" | "bar_page_look" => {
                    self.st.bar_ed.look = *k == "bar_page_look";
                    self.dirty_panels();
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
                // клик (не перетаскивание): в привычное место
                let m = m.to_string();
                let m = &m;
                if m == "tasks" {
                    s.bar_center.push(m.clone());
                } else if matches!(m.as_str(), "launcher" | "workspaces") {
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
                if *k == "launcher_cmd" && self.st.launcher_field.text.is_empty() {
                    let p = self.st.set.launcher_cmd.clone();
                    self.st.launcher_field.set(&p);
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
            self.st.field_focus = None;
            self.settings_changed();
        }
    }
}
