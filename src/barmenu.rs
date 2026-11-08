// ПКМ по модулю бара: меню с его быстрыми действиями и его настройками вида (то же, что во вкладке
// «бар» → «вид», но у самого модуля), внизу — убрать из бара и все настройки бара. ПКМ по пустому
// месту бара — панель управления (control.rs). Окна и трей оставляют свои меню.

use crate::app::App;
use crate::menu::{MAct, MItem, MenuState};
use crate::ui::Act;

/// выбор из ряда: пункт с галочкой у текущего, значение — в Set
fn pick(key: &'static str, list: &[(&'static str, &'static str)], cur: &str) -> Vec<MItem> {
    list.iter().map(|(v, n)| MItem::new(n, "", MAct::Set(key, v.to_string())).check(*v == cur)).collect()
}

fn name_of<'a>(list: &[(&'a str, &'a str)], id: &str) -> &'a str {
    list.iter().find(|x| x.0 == id).map_or(list[0].1, |x| x.1)
}

fn toggle(label: &str, key: &'static str, on: bool) -> MItem {
    MItem::new(label, "", MAct::Act(Act::SBool(key))).check(on)
}

fn sub(label: &str, icon: &str, key: &'static str, list: &[(&'static str, &'static str)], cur: &str) -> MItem {
    MItem::sub(&format!("{}: {}", crate::i18n::tr(label), crate::i18n::tr(name_of(list, cur))), icon, pick(key, list, cur))
}

const CPU: &[(&str, &str)] = &[("gauge", "шкала"), ("graph", "график"), ("number", "число")];
const TEMP: &[(&str, &str)] = &[("full", "как цп"), ("compact", "коротко")];
const TRAY: &[(&str, &str)] = &[("icons", "значки"), ("arrow", "стрелкой")];
const WS: &[(&str, &str)] = &[("boxes", "квадраты"), ("dots", "точки")];
const SEP: &[(&str, &str)] = &[("dots", "точки"), ("line", "черта"), ("blocks", "клетки"), ("gap", "пусто")];
const SEP_C: &[(&str, &str)] = &[("border", "рамка"), ("accent", "акцент"), ("dim", "тусклый")];

impl App {
    /// меню модуля m бара на мониторе out у точки (x, y)
    pub fn open_module_menu(&mut self, m: &str, out: &str, x: i32, y: i32) {
        let s = &self.st.set;
        let mut items: Vec<MItem> = vec![];
        match m {
            "workspaces" => items.push(sub("вид", "grid", "ws_style", WS, &s.ws_style)),
            "clock" => {
                items.push(MItem::new("календарь", "cal", MAct::Act(Act::Popup("cal"))));
                items.push(toggle("секунды", "clock_seconds", s.clock_seconds));
                items.push(toggle("дата", "clock_date", s.clock_date));
            }
            "cpu" | "ram" | "gpu" | "disk" => {
                items.push(MItem::new("процессы", "cpu", MAct::Act(Act::SysMon)));
                items.push(sub("вид", "chart", "cpu_style", CPU, &s.cpu_style));
            }
            "temp" => items.push(sub("вид", "temp", "temp_style", TEMP, &s.temp_style)),
            "tray" => {
                items.push(sub("вид", "menu", "tray_mode", TRAY, &s.tray_mode));
                items.push(toggle("значки клетками", "tray_pixel", s.tray_pixel));
            }
            "media" => {
                let mut eq: Vec<(&'static str, &'static str)> = crate::widgets::STYLES.to_vec();
                eq.push(("off", "выкл"));
                items.push(MItem::new("пауза / играть", "play", MAct::Act(Act::MediaPlay)));
                items.push(MItem::new("другой плеер", "music", MAct::Act(Act::MediaPlayer)));
                items.push(sub("эквалайзер", "music", "media_viz", &eq, &s.media_viz));
                items.push(sub("всплывашка", "eye", "media_view", crate::config::MEDIA_VIEWS, &s.media_view));
            }
            "vol" => {
                items.push(MItem::new(if self.st.audio.muted { "включить звук" } else { "выключить звук" }, "mute", MAct::Act(Act::VolMute)));
                items.push(MItem::new(if self.st.audio.mic_muted { "включить микрофон" } else { "выключить микрофон" }, "mic", MAct::Act(Act::MicMute)));
                items.push(MItem::new("микшер", "speaker", MAct::Act(Act::Pavu)));
            }
            "net" | "netspeed" => items.push(MItem::new("настройки сети", "wifi", MAct::Act(Act::NetEditor))),
            "bt" => items.push(MItem::new("устройства bluetooth", "bt", MAct::Act(Act::BtManager))),
            "bat" => items.push(toggle("прятать полную батарею", "bat_hide_full", s.bat_hide_full)),
            "notif" => {
                items.push(toggle("не беспокоить", "dnd", s.dnd));
                items.push(toggle("тихо при показе экрана", "cast_dnd", s.cast_dnd));
                items.push(MItem::new("очистить историю", "trash", MAct::Act(Act::NotifClear)));
            }
            "night" => items.push(toggle("ночной свет", "night", s.night)),
            "launcher" => items.push(MItem::new("своя команда…", "term", MAct::OpenSettings("bar"))),
            "tasks" => items.push(MItem::new("ширина кнопок…", "max", MAct::OpenSettings("bar"))),
            _ => {}
        }
        // разделитель стоит в любом ряду: его вид — у всех
        if s.on_bar("sep") {
            items.push(sub("разделители", "dots", "bar_sep", SEP, &s.bar_sep));
            items.push(sub("цвет разделителей", "drop", "bar_sep_color", SEP_C, &s.bar_sep_color));
        }
        if !items.is_empty() {
            items.push(MItem::sep());
        }
        items.push(MItem::new("убрать из бара", "close", MAct::BarRemove(m.to_string())).danger());
        items.push(MItem::new("настройки бара…", "gear", MAct::OpenSettings("bar")));
        let title = crate::i18n::tr(crate::bartab::mod_name(m)).to_string();
        self.open_menu(MenuState::new(out, x, y, &title, items));
    }

    /// значение настройки из меню модуля (строковые ключи вида бара)
    pub fn menu_set(&mut self, key: &str, val: String) {
        let s = &mut self.st.set;
        match key {
            "ws_style" => s.ws_style = val,
            "cpu_style" => s.cpu_style = val,
            "temp_style" => s.temp_style = val,
            "tray_mode" => s.tray_mode = val,
            "media_viz" => s.media_viz = val,
            "media_view" => s.media_view = val,
            "bar_sep" => s.bar_sep = val,
            "bar_sep_color" => s.bar_sep_color = val,
            _ => return,
        }
        self.settings_changed();
    }

    pub fn bar_remove(&mut self, m: &str) {
        let s = &mut self.st.set;
        for l in [&mut s.bar_left, &mut s.bar_center, &mut s.bar_right] {
            if let Some(i) = l.iter().position(|x| x == m) {
                l.remove(i);
                break;
            }
        }
        self.settings_changed();
    }
}
