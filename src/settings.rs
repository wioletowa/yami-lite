// Настройки: обычное окно niri (app-id yami-lite-settings) — группы вкладок слева, содержимое
// справа. Всё применяется сразу и пишется в ~/.config/yami-lite/settings.json (его можно править
// и руками — шелл подхватит).
//
// Вкладка — строка в GROUPS: ключ, имя, значок, слова для поиска и функция рисования (каждая в
// своём модуле). Содержимое листается колесом: сколько его, узнаём по зонам, которые вкладка
// нарисовала (Canvas::reach), так что новая строка настроек сама удлиняет прокрутку.

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Tone, Ui};
pub use crate::bartab::BarEd;
use crate::settabs::{tab_about, tab_color, tab_dock, tab_look, tab_notif, tab_system, tab_widgets};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

/// наименьший размер окна: под него свёрстаны вкладки
pub const SIZE: (i32, i32) = (800, 600);
const SIDE_W: i32 = 168;
const ROW_H: i32 = 26;
const HEAD_H: i32 = 22;
/// шаг колеса, пикселей
const WHEEL: i32 = 48;

pub struct Tab {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub draw: fn(&mut Ui, &State, R),
    /// у вкладки свой список с прокруткой: общую не включать
    pub own_scroll: bool,
}

const fn tab(id: &'static str, name: &'static str, icon: &'static str, draw: fn(&mut Ui, &State, R)) -> Tab {
    Tab { id, name, icon, draw, own_scroll: false }
}

/// группы по смыслу: как выглядит, рабочий стол, что делают руки, система
pub const GROUPS: &[(&str, &[Tab])] = &[
    (
        "внешний вид",
        &[
            tab("color", "цвет", "drop", tab_color),
            tab("wall", "обои", "image", crate::wall::tab),
            tab("look", "вид", "eye", tab_look),
        ],
    ),
    (
        "рабочий стол",
        &[
            tab("bar", "бар", "menu", crate::bartab::tab_bar),
            tab("widgets", "виджеты", "grid", tab_widgets),
            tab("dock", "док", "tile", tab_dock),
        ],
    ),
    (
        "система",
        &[
            tab("system", "система", "gear", tab_system),
            tab("notif", "уведомления", "bell", tab_notif),
            tab("games", "игры", "play", crate::games::tab),
            tab("about", "о шелле", "about", tab_about),
        ],
    ),
];

/// все вкладки подряд
pub fn tabs() -> impl Iterator<Item = &'static Tab> {
    GROUPS.iter().flat_map(|g| g.1.iter())
}

/// вкладка по ключу (из IPC)
pub fn find(id: &str) -> Option<&'static Tab> {
    tabs().find(|t| t.id == id)
}

/// где рисовать содержимое (видимая часть) — общее для рисования и колеса
fn body_rect(w: i32, h: i32) -> R {
    let fr = R::new(0, 0, w, h);
    R::new(1 + SIDE_W + 20, 48, fr.right() - SIDE_W - 1 - 48, fr.h - 64)
}

fn side_rect(h: i32) -> R {
    R::new(1, 31, SIDE_W, h - 32)
}

/// прокрутка открытой вкладки (сдвиг запомнен для неё; другая вкладка начинается сверху)
pub fn scroll_of(st: &State) -> i32 {
    if st.sscroll.1 == st.stab {
        st.sscroll.0
    } else {
        0
    }
}

/// полоса прокрутки: клетки дорожки и бегунок
fn scrollbar(ui: &mut Ui, r: R, view: i32, full: i32, at: i32) {
    let t = ui.t;
    if full <= view {
        return;
    }
    ui.cv.fill(R::new(r.x + 1, r.y, 2, r.h), t.border.alpha(0.5));
    let th = (r.h * view / full).max(18);
    let ty = r.y + (r.h - th) * at / (full - view).max(1);
    ui.cv.fill(R::new(r.x, ty, 4, th), t.red);
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let fr = R::new(0, 0, w, h);
    // окно, а не панель: без тени, на всю поверхность; размытие под всем окном (у панелей его
    // просит тень pxbox, у окна тени нет)
    if t.glass_bg().a() < 255 {
        ui.cv.blur_rect(fr, 0);
    }
    ui.cv.pxbox(fr, &BoxStyle::new(t.glass_bg(), t.border));
    ui.zone(fr, Act::Block, Cur::Default);
    let sub = crate::tf!("yami-lite {} · {:.0} МБ", "yami-lite {} · {:.0} MB", env!("CARGO_PKG_VERSION"), st.rss);
    ui.header(R::new(1, 1, fr.w - 2, 30), "gear", "настройки", &sub);
    // за заголовок окно двигает niri (крестик зарегистрирован позже — он сверху)
    ui.zone(R::new(1, 1, fr.w - 2, 30), Act::MDrag, Cur::Default);
    ui.ghost(R::new(fr.right() - 28, 5, 24, 22), "close", Act::Settings);
    // вкладки группами; список длиннее окна — листается колесом
    let side = side_rect(h);
    ui.cv.fill(side, t.panel.alpha(0.5));
    ui.cv.fill(R::new(side.right(), side.y, 1, side.h), t.border);
    ui.zone(side, Act::SSideScroll, Cur::Default);
    ui.cv.push_clip(side);
    let mut y = side.y + 8 - st.stab_scroll;
    for (gname, tabs) in GROUPS {
        ui.cv.fill(R::new(side.x + 12, y + 8, 4, 4), t.red.alpha(0.7));
        ui.text_in(PX, R::new(side.x + 22, y, side.w - 30, HEAD_H - 4), gname, t.text_faint);
        y += HEAD_H;
        for tb in tabs.iter() {
            let r = R::new(side.x + 8, y, side.w - 16, ROW_H - 2);
            ui.button(r, tb.name, tb.icon, Tone::Ghost, st.stab == tb.id, true, Act::STab(tb.id));
            if st.stab == tb.id {
                ui.cv.fill(R::new(r.x - 8, r.y, 3, r.h), t.red);
            }
            y += ROW_H;
        }
        y += 6;
    }
    let side_full = y + st.stab_scroll - side.y;
    st.stab_h.set(side_full);
    ui.cv.pop_clip();
    scrollbar(ui, R::new(side.right() - 5, side.y + 2, 4, side.h - 4), side.h, side_full, st.stab_scroll);
    // содержимое: фон-зона для колеса, вкладка со сдвигом, полоса прокрутки справа
    let body = body_rect(w, h);
    let tb = find(st.stab).unwrap_or(&GROUPS[0].1[0]);
    let off = if tb.own_scroll { 0 } else { scroll_of(st) };
    ui.zone(R::new(side.right() + 1, 31, fr.right() - side.right() - 2, fr.h - 32), Act::SScroll, Cur::Default);
    ui.cv.push_clip(R::new(body.x - 4, body.y - 4, body.w + 8, body.h + 8));
    ui.cv.reach = i32::MIN;
    (tb.draw)(ui, st, R::new(body.x, body.y - off, body.w, body.h));
    let full = if tb.own_scroll { body.h } else { (ui.cv.reach + off + 12 - body.y).max(body.h) };
    st.sscroll_h.set(full);
    ui.cv.pop_clip();
    scrollbar(ui, R::new(body.right() + 18, body.y, 4, body.h), body.h, full, off);
    crate::dropdown::draw(ui, st, &SKind::Settings, w, h);
}

impl App {
    /// окно настроек сейчас: размер от niri (не открыто — наименьший)
    fn settings_size(&self) -> (i32, i32) {
        self.surfs.values().find(|s| s.kind == SKind::Settings).map_or(SIZE, |s| (s.w, s.h))
    }

    /// открыть окно настроек (открыто — закрыть)
    pub fn toggle_settings(&mut self) {
        if self.st.settings_shown {
            self.close_settings();
        } else {
            self.open_settings();
        }
    }

    pub fn open_settings(&mut self) {
        if self.st.locked || self.st.settings_shown {
            return;
        }
        self.close_popup();
        self.close_menu();
        self.tab_probe();
        self.st.hex.set(&self.st.set.accent.clone());
        if self.st.sel_widget == 0 {
            self.st.sel_widget = self.st.set.widgets.first().map(|w| w.id).unwrap_or(0);
        }
        self.st.settings_shown = self.mk_window(SKind::Settings, "yami-lite-settings", "настройки", SIZE).is_some();
        self.dirty(&SKind::Bar);
    }

    pub fn close_settings(&mut self) {
        for id in self.find(&SKind::Settings) {
            self.destroy(id);
        }
        self.st.settings_shown = false;
        self.st.field_focus = None;
        self.st.drop = None;
        // код настроек велик, а нужен изредка: через полминуты — из памяти
        self.shed_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        self.dirty(&SKind::Bar);
    }

    /// колесо над вкладкой: листать, если есть куда (true — пролистали)
    pub fn settings_scroll(&mut self, d: i32) -> bool {
        self.drop_close();
        let (w, h) = self.settings_size();
        let view = body_rect(w, h).h;
        let max = (self.st.sscroll_h.get() - view).max(0);
        if max == 0 || find(self.st.stab).is_some_and(|t| t.own_scroll) {
            return false;
        }
        let at = (scroll_of(&self.st) + d * WHEEL).clamp(0, max);
        self.st.sscroll = (at, self.st.stab);
        self.dirty_panels();
        true
    }

    /// колесо над списком вкладок
    pub fn settings_side_scroll(&mut self, d: i32) {
        let h = self.settings_size().1;
        let max = (self.st.stab_h.get() - side_rect(h).h).max(0);
        self.st.stab_scroll = (self.st.stab_scroll + d * WHEEL).clamp(0, max);
        self.dirty_panels();
    }
}

pub fn title(ui: &mut Ui, x: i32, y: i32, w: i32, s: &str) -> i32 {
    let t = ui.t;
    ui.cv.fill(R::new(x, y + 7, 6, 6), t.red);
    ui.text_in(PX, R::new(x + 14, y, w - 14, 20), s, t.text);
    ui.cv.fill(R::new(x, y + 24, w, 1), t.border);
    y + 34
}

/// число в пикселях: ‹ значение › — щелчок по стрелке или колесо меняют на шаг
pub fn num_btn(ui: &mut Ui, r: R, label: &str, v: i32, key: &'static str) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if hov { t.surface_hi } else { t.surface }, if hov { t.red } else { t.border }).bevel(true));
    ui.text_in(PX, R::new(r.x + 8, r.y, r.w - 80, r.h), label, t.text_dim);
    ui.zone(r, Act::SNum(key, 1), Cur::Pointer);
    let (l, rr) = (R::new(r.right() - 72, r.y + 2, 16, r.h - 4), R::new(r.right() - 18, r.y + 2, 16, r.h - 4));
    for (b, icon) in [(l, "left"), (rr, "right")] {
        ui.cv.icon(icon, b.x + 1, b.cy() - 7, 2, if ui.hover(b) { t.ember } else { t.red });
    }
    ui.zone(l, Act::SNum(key, -1), Cur::Pointer);
    ui.zone(rr, Act::SNum(key, 1), Cur::Pointer);
    ui.cv.text_center(ui.f, PX, R::new(l.right(), r.y, rr.x - l.right(), r.h), &format!("{v}"), t.text);
}

/// кнопка выбора: подпись слева, значение справа; подписи — всё место, что значение не заняло
pub fn opt_btn(ui: &mut Ui, r: R, label: &str, val: &str, act: Act) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if hov { t.surface_hi } else { t.surface }, if hov { t.red } else { t.border }).bevel(true));
    // значение не наезжает на подпись: длинное — с многоточием, подписи — не меньше трети
    let lw = ui.f.width(PX, (ui.tr)(label)).min(r.w / 3);
    let room = r.w - 32 - lw - 12;
    let tv = (ui.tr)(val).to_string();
    let v = if ui.f.width(PX, &tv) > room { ui.f.elide(PX, &tv, room) } else { tv };
    let vw = ui.f.width(PX, &v);
    ui.text_in(PX, R::new(r.x + 8, r.y, r.w - 32 - vw - 12, r.h), label, t.text_dim);
    ui.cv.text_right(ui.f, PX, R::new(r.x, r.y, r.w - 24, r.h), &v, t.text);
    // список — стрелка вниз, переход (шрифты) — вправо
    ui.cv.icon(if matches!(act, Act::Drop(k) if !crate::dropdown::pair(k)) { "down" } else { "right" }, r.right() - 18, r.cy() - 7, 2, t.red);
    ui.zone(r, act, Cur::Pointer);
}


pub(crate) fn cycle<'a>(list: &[(&'a str, &'a str)], cur: &str) -> &'a str {
    let i = list.iter().position(|x| x.0 == cur).map(|i| (i + 1) % list.len()).unwrap_or(0);
    list[i].0
}

impl App {
    /// при открытии вкладки: свои темы читаются с диска
    pub fn tab_probe(&mut self) {
        if self.st.stab == "color" && self.st.color_page == 1 {
            self.theme_do("list", 0);
        }
    }

    pub fn settings_key(&mut self, e: &KeyEvent) {
        if self.st.drop.is_some() && e.keysym == Keysym::Escape {
            return self.drop_close();
        }
        if let Some(f) = self.st.field_focus {
            match e.keysym {
                Keysym::Escape => self.st.field_focus = None,
                Keysym::Return | Keysym::KP_Enter => {
                    if f == "hex" {
                        self.apply_hex();
                    } else if f == "game_apps" {
                        self.settings_click(&Act::SCycle("game_apply"));
                    } else if f == "night_geo" {
                        self.settings_click(&Act::SCycle("geo_apply"));
                    } else if f == "wh_key" {
                        self.settings_click(&Act::SCycle("wh_key_apply"));
                    } else if f == "wh_dir" {
                        self.settings_click(&Act::SCycle("wh_dir_apply"));
                    } else if f == "font_search" {
                        self.font_first();
                    } else if f == "launcher_cmd" {
                        self.settings_click(&Act::SCycle("launcher_apply"));
                    } else {
                        self.settings_click(&Act::SCycle("wall_apply"));
                    }
                }
                _ => {
                    let mods = self.mods;
                    let field = match f {
                        "hex" => &mut self.st.hex,
                        "game_apps" => &mut self.st.game_field,
                        "night_geo" => &mut self.st.geo_field,
                        "launcher_cmd" => &mut self.st.launcher_field,
                        "font_search" => &mut self.st.font_search,
                        "wh_key" => &mut self.st.wh_key_field,
                        "wh_dir" => &mut self.st.wh_dir_field,
                        _ => &mut self.st.wall_field,
                    };
                    App::edit_field(field, e, &mods);
                    // поиск сменился — список с начала
                    if f == "font_search" {
                        self.st.font_scroll = 0;
                    }
                }
            }
            self.dirty_panels();
            return;
        }
        match e.keysym {
            Keysym::Escape => self.close_settings(),
            Keysym::Tab | Keysym::ISO_Left_Tab | Keysym::Down | Keysym::Up => {
                let back = e.keysym == Keysym::ISO_Left_Tab || e.keysym == Keysym::Up || self.mods.shift;
                let ids: Vec<&'static str> = tabs().map(|t| t.id).collect();
                let n = ids.len();
                let i = ids.iter().position(|x| *x == self.st.stab).unwrap_or(0);
                self.st.stab = ids[if back { (i + n - 1) % n } else { (i + 1) % n }];
                self.tab_probe();
            }
            // листать вкладку: страница, начало, конец
            Keysym::Page_Down | Keysym::Page_Up => {
                let (w, h) = self.settings_size();
                let page = (body_rect(w, h).h - 40) / WHEEL;
                self.settings_scroll(if e.keysym == Keysym::Page_Down { page } else { -page });
            }
            Keysym::Home => {
                self.settings_scroll(-1000);
            }
            Keysym::End => {
                self.settings_scroll(1000);
            }
            _ => {}
        }
        if self.st.settings_shown {
            self.dirty_panels();
        }
    }
}
