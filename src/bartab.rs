// Настройки → бар: модули фишками в три ряда (слева, по центру, справа), перетаскивание между
// рядами и из списка «добавить» (встроенные модули и скрипты бара), отступы и вид разделителей.

use crate::app::{App, SKind, State};
use crate::config::ALL_MODULES;
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::settings::*;
use crate::ui::{Act, Cur, Tone, Ui};

pub(crate) fn side_name(i: usize) -> &'static str {
    ["слева", "по центру", "справа"][i]
}

/// перетаскивание модулей бара во вкладке «бар»
#[derive(Default)]
pub struct BarEd {
    pub drag: Option<BDrag>,
    /// места фишек без перетаскиваемой: (сторона, индекс, прямоугольник) — для попадания
    pub rects: std::cell::RefCell<Vec<(usize, usize, R)>>,
    /// ряды сторон (куда можно бросить)
    pub areas: std::cell::Cell<[R; 3]>,
    /// раздел вкладки: false — модули, true — вид (форма, отступы, разделители)
    pub look: bool,
}

pub struct BDrag {
    /// откуда: сторона и индекс, или новый модуль из «добавить»
    pub from: Option<(usize, usize)>,
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// где схватили фишку
    pub off: (i32, i32),
    pub target: Option<(usize, usize)>,
    /// когда сменилось место — раскрываем щель
    pub at: std::time::Instant,
}

pub(crate) fn mod_name(m: &str) -> &str {
    // модуль-скрипт — по имени файла
    if let Some(n) = m.strip_prefix("x:") {
        return n;
    }
    ALL_MODULES.iter().find(|x| x.0 == m).map(|x| x.1).unwrap_or(m)
}

pub(crate) fn chip_w(ui: &mut Ui, m: &str) -> i32 {
    // ручка 16 + содержимое + крестик 20
    if m == "sep" {
        16 + 12 + 20
    } else {
        16 + ui.w(PX, mod_name(m)) + 4 + 20
    }
}

/// фишки строками в area (перенос по ширине); высота ряда 24, шаг 28
pub(crate) fn flow(ws: &[i32], area: R) -> Vec<R> {
    let (mut x, mut y) = (area.x + 4, area.y + 4);
    ws.iter()
        .map(|&w| {
            if x + w > area.right() - 4 && x > area.x + 4 {
                x = area.x + 4;
                y += 28;
            }
            let r = R::new(x, y, w, 24);
            x += w + 6;
            r
        })
        .collect()
}

pub(crate) fn chip(ui: &mut Ui, r: R, m: &str, lifted: bool) {
    let t = ui.t;
    let hov = ui.hover(r) && !lifted;
    let st = BoxStyle::new(if lifted || hov { t.surface_hi } else { t.surface }, if lifted { t.ember } else if hov { t.red } else { t.border }).bevel(true);
    ui.cv.pxbox(r, &if lifted { st.shadow(4) } else { st });
    // ручка: 2×3 точки
    let gc = if hov || lifted { t.red } else { t.text_faint };
    for i in 0..3 {
        for j in 0..2 {
            ui.cv.fill(R::new(r.x + 5 + j * 4, r.cy() - 5 + i * 4, 2, 2), gc);
        }
    }
    if m == "sep" {
        ui.cv.sep(r.x + 20, r.cy(), t.text_dim);
    } else {
        ui.text_in(PX, R::new(r.x + 16, r.y, r.w - 36, r.h), mod_name(m), t.text);
    }
}

pub(crate) fn tab_bar(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let ed = &st.bar_ed;
    let drag = ed.drag.as_ref();
    // два раздела: модули (перетаскивание) и вид (форма бара, отступы, разделители)
    let bw = 86;
    ui.button(R::new(b.right() - 2 * bw - 6, b.y - 3, bw, 22), "модули", "menu", Tone::Ghost, !ed.look, true, Act::SCycle("bar_page_mods"));
    ui.button(R::new(b.right() - bw, b.y - 3, bw, 22), "вид", "eye", Tone::Ghost, ed.look, true, Act::SCycle("bar_page_look"));
    if ed.look {
        tab_bar_look(ui, st, b);
        return;
    }
    let mut y = title(ui, b.x, b.y, b.w - 2 * bw - 12, "модули бара — тащи мышкой, × убрать");
    let sides = [&s.bar_left, &s.bar_center, &s.bar_right];
    let mut areas = [R::new(0, 0, 0, 0); 3];
    let mut hit = vec![];
    for (si, list) in sides.iter().enumerate() {
        ui.text_in(PX, R::new(b.x, y + 4, 76, 20), side_name(si), t.red);
        // без перетаскиваемой фишки
        let items: Vec<(usize, &String)> = list.iter().enumerate().filter(|(i, _)| drag.and_then(|d| d.from) != Some((si, *i))).collect();
        let ws: Vec<i32> = items.iter().map(|(_, m)| chip_w(ui, m)).collect();
        let mut area = R::new(b.x + 80, y, b.w - 80, 32);
        let base = flow(&ws, area);
        // щель под фишку там, куда её несут: раскрывается ступенями по 2 px
        let mut shown_ws = ws.clone();
        let mut gap_at = None;
        if let Some(d) = drag {
            if let Some((ts, ti)) = d.target {
                if ts == si {
                    let full = chip_w(ui, &d.name);
                    let dur = if s.anim() { 110.0 * s.anim_speed.max(0.1) } else { 0.0 };
                    let k = if dur <= 0.0 { 1.0 } else { (d.at.elapsed().as_millis() as f32 / dur).min(1.0) };
                    if k < 1.0 {
                        ui.anim = true;
                    }
                    let w = (((full as f32 * (1.0 - (1.0 - k) * (1.0 - k))) / 2.0).round() as i32 * 2).max(2);
                    let ti = ti.min(shown_ws.len());
                    shown_ws.insert(ti, w);
                    gap_at = Some(ti);
                }
            }
        }
        let shown = flow(&shown_ws, area);
        let rows_h = shown.iter().chain(base.iter()).map(|r| r.bottom()).max().unwrap_or(area.y + 28) - area.y + 4;
        area.h = rows_h.max(32);
        let target_here = drag.and_then(|d| d.target).map(|t| t.0 == si).unwrap_or(false);
        ui.cv.pxbox(area, &BoxStyle::new(t.code_bg, if target_here { t.red } else { t.border }));
        let mut k = 0;
        for (j, r) in shown.iter().enumerate() {
            if Some(j) == gap_at {
                ui.cv.ants(*r, t.ember, 2, 0);
                continue;
            }
            let (i, m) = items[k];
            k += 1;
            chip(ui, *r, m, false);
            ui.zone(*r, Act::SModDrag(si, i), Cur::Grab);
            if drag.is_none() {
                ui.ghost(R::new(r.right() - 20, r.y + 2, 18, 20), "close", Act::SModDel(si, i));
            }
        }
        for (j, r) in base.iter().enumerate() {
            hit.push((si, j, *r));
        }
        areas[si] = area;
        y += area.h + 6;
    }
    *ed.rects.borrow_mut() = hit;
    ed.areas.set(areas);
    // добавить: всё, чего нет в баре, и разделитель (их можно сколько угодно)
    y += 4;
    ui.text_in(PX, R::new(b.x, y, b.w, 20), "добавить — тащи в бар или кликни:", t.text_dim);
    y += 22;
    let all: Vec<&String> = s.bar_left.iter().chain(&s.bar_center).chain(&s.bar_right).collect();
    // и модули-скрипты из ~/.config/yami/modules
    let plugs = crate::plugins::available();
    let free: Vec<&str> = ALL_MODULES.iter().map(|m| m.0).chain(plugs.iter().map(|p| p.as_str())).filter(|id| *id == "sep" || !all.iter().any(|m| m == id)).collect();
    let ws: Vec<i32> = free.iter().map(|m| chip_w(ui, m)).collect();
    let rs = flow(&ws, R::new(b.x - 4, y - 4, b.w + 8, 0));
    for (r, id) in rs.iter().zip(&free) {
        chip(ui, *r, id, false);
        ui.cv.icon("plus", r.right() - 17, r.cy() - 7, 2, t.text_faint);
        ui.zone(*r, Act::SModNew(id.to_string()), Cur::Grab);
    }
    // фишка в руке — поверх всего
    if let Some(d) = drag {
        let w = chip_w(ui, &d.name);
        chip(ui, R::new(d.x - d.off.0, d.y - d.off.1 - 2, w, 24), &d.name, true);
    }
}

/// вкладка «бар» → вид: форма бара, отступы, разделители, вид часов и процессора
fn tab_bar_look(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    // форма: полоса во всю ширину, плашка уже экрана, плавающая с отступом от краёв
    let mut y = title(ui, b.x, b.y, b.w - 2 * 86 - 12, "форма бара");
    let cw = (b.w - 24) / 3;
    let col = |i: i32| b.x + i * (cw + 12);
    ui.toggle(R::new(col(0), y, cw, 40), "плавает", "отступ от краёв, тень", s.bar_float, Act::SBool("bar_float"));
    num_btn(ui, R::new(col(1), y + 6, cw, 26), "ширина, %", s.num("bar_width_pc"), "bar_width_pc");
    num_btn(ui, R::new(col(2), y + 6, cw, 26), "отступ", s.num("bar_margin"), "bar_margin");
    y += 46;
    let align = if s.bar_center_align == "after-left" { "за левыми" } else { "по центру" };
    opt_btn(ui, R::new(col(0), y, cw, 26), "модули середины", align, Act::SCycle("bar_center_align"));
    let pos = if crate::bar::low(s) { "снизу" } else { "сверху" };
    opt_btn(ui, R::new(col(1), y, cw, 26), "край экрана", pos, Act::SCycle("bar_pos"));
    let shape = if crate::bar::strip(s) {
        "полоса во всю ширину"
    } else if s.bar_float {
        "плашка с отступом"
    } else {
        "плашка у края"
    };
    ui.text_in(PX, R::new(col(2) + 4, y, cw - 4, 26), shape, t.text_faint);
    y += 40;
    y = title(ui, b.x, y, b.w, "отступы и разделители");
    let style = match s.bar_sep.as_str() {
        "line" => "черта",
        "blocks" => "клетки",
        "gap" => "пусто",
        _ => "точки",
    };
    let color = match s.bar_sep_color.as_str() {
        "accent" => "акцент",
        "dim" => "тусклый",
        _ => "рамка",
    };
    let cpu = match s.cpu_style.as_str() {
        "graph" => "график",
        "number" => "число",
        _ => "шкала",
    };
    // три столбца: разделитель, края и модули, окна; числа — стрелками или колесом
    opt_btn(ui, R::new(col(0), y, cw, 26), "разделитель", style, Act::SCycle("bar_sep"));
    opt_btn(ui, R::new(col(1), y, cw, 26), "цвет", color, Act::SCycle("bar_sep_color"));
    num_btn(ui, R::new(col(2), y, cw, 26), "ширина", s.num("bar_sep_w"), "bar_sep_w");
    y += 32;
    for (i, (k, label)) in [("bar_pad_l", "край слева"), ("bar_pad_r", "край справа"), ("bar_gap", "промежуток")].into_iter().enumerate() {
        num_btn(ui, R::new(col(i as i32), y, cw, 26), label, s.num(k), k);
    }
    y += 32;
    for (i, (k, label)) in [("bar_mid", "до окон"), ("bar_task_gap", "зазор окон"), ("task_width", "кнопка окна")].into_iter().enumerate() {
        num_btn(ui, R::new(col(i as i32), y, cw, 26), label, s.num(k), k);
    }
    y += 34;
    opt_btn(ui, R::new(col(0), y, cw, 26), "цп/память", cpu, Act::SCycle("cpu_style"));
    ui.toggle(R::new(col(1), y, cw, 26), "часы с секундами", "", s.clock_seconds, Act::SBool("clock_seconds"));
    ui.toggle(R::new(col(2), y, cw, 26), "дата в часах", "", s.clock_date, Act::SBool("clock_date"));
    y += 32;
    // #11: трей одной стрелкой, температура столбиком
    opt_btn(ui, R::new(col(0), y, cw, 26), "трей", if s.tray_mode == "arrow" { "стрелкой" } else { "значки" }, Act::SCycle("tray_mode"));
    opt_btn(ui, R::new(col(1), y, cw, 26), "температура", if s.temp_style == "compact" { "коротко" } else { "как цп" }, Act::SCycle("temp_style"));
    // #13: эквалайзер модуля и всплывашки музыки, что всплывашка показывает
    let eq = if s.media_viz == "off" { "выкл" } else { crate::widgets::name_of(crate::widgets::STYLES, &s.media_viz) };
    opt_btn(ui, R::new(col(2), y, cw, 26), "эквалайзер", eq, Act::SCycle("media_viz"));
    y += 32;
    opt_btn(ui, R::new(col(0), y, cw * 2 + (col(1) - col(0) - cw), 26), "всплывашка музыки", crate::widgets::name_of(crate::config::MEDIA_VIEWS, &s.media_view), Act::SCycle("media_view"));
    ui.toggle(R::new(col(2), y, cw, 26), "трей клетками", "", s.tray_pixel, Act::SBool("tray_pixel"));
    y += 32;
    // #21: ноутбук от сети — заряженная батарея в баре не нужна
    ui.toggle(R::new(col(0), y, cw * 2 + (col(1) - col(0) - cw), 26), "прятать полную батарею", "", s.bat_hide_full, Act::SBool("bat_hide_full"));
    y += 40;
    y = title(ui, b.x, y, b.w, "как выглядит");
    shape_preview(ui, st, R::new(b.x, y, b.w, (b.bottom() - y).clamp(40, 96)));
}

/// схема края экрана 1920 px с баром (верх или низ): бар (полоса, плашка или плавающий), его
/// модули и окна рядом — всё в масштабе, тем же расчётом, что у настоящего бара (bar::geo)
fn shape_preview(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let s = &st.set;
    ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
    let inner = r.inset(3);
    ui.cv.push_clip(inner);
    ui.cv.fill(inner, t.bg);
    let k = inner.w as f32 / 1920.0;
    let sc = |v: i32| (v as f32 * k).round() as i32;
    let (top, side, _) = crate::bar::geo(s, 1920);
    let strip = crate::bar::strip(s);
    let bh = sc(crate::app::BAR_H).max(8);
    let low = crate::bar::low(s);
    let by = if low { inner.bottom() - sc(top) - bh } else { inner.y + sc(top) };
    let bar = R::new(inner.x + sc(side), by, sc(1920 - 2 * side - if strip { 0 } else { crate::bar::SHADE }), bh);
    // окна под баром (над нижним) — с отступами из «окон»
    let gap = sc(s.num("win_top")).max(2);
    let (wy, wb) = if low { (inner.y - 4, bar.y - gap) } else { (bar.bottom() + gap, inner.bottom() + 4) };
    let (wl, wg) = (sc(s.num("win_left")).max(1), sc(s.num("win_gap")).max(1));
    let ww = (inner.w - 2 * wl - wg) / 2;
    for i in 0..2 {
        let wr = R::new(inner.x + wl + i * (ww + wg), wy, ww, wb - wy);
        ui.cv.fill(wr, t.surface);
        ui.cv.fill(R::new(wr.x, wr.y, wr.w, 1), if i == 0 { t.red } else { t.border });
    }
    if strip {
        ui.cv.fill(bar, t.panel);
        ui.cv.fill(R::new(bar.x, if low { bar.y } else { bar.bottom() - 1 }, bar.w, 1), t.border);
    } else {
        ui.cv.pxbox(bar, &BoxStyle::new(t.panel, t.border).shadow(1));
    }
    // модули: слева — акцентные клетки, справа — тусклые, в середине — часы или окна
    let cy = bar.y + bar.h / 2 - 1;
    for i in 0..3 {
        ui.cv.fill(R::new(bar.x + 4 + i * 6, cy - 1, 4, 4), if i == 0 { t.red } else { t.dim });
    }
    for i in 0..4 {
        ui.cv.fill(R::new(bar.right() - 8 - i * 6, cy - 1, 4, 4), t.text_faint);
    }
    // середина — часы или окна (#28: окна — обычный модуль, стоят там же, где остальные)
    if !s.bar_center.is_empty() {
        let win = s.bar_center.iter().any(|m| m == "tasks");
        let cw = if win { (bar.w / 6).max(18) } else { 18 };
        let cx = if s.bar_center_align == "after-left" { bar.x + 26 } else { bar.x + bar.w / 2 - cw / 2 };
        ui.cv.fill(R::new(cx, cy - 1, cw, 4), if win { t.text_dim } else { t.text });
    }
    ui.cv.pop_clip();
}

impl App {
    /// тащим фишку модуля: место считаем по рядам последнего кадра
    pub fn bar_drag(&mut self, a: &Act, zone: R, press: (f64, f64), x: f64, y: f64) {
        let (x, y) = (x as i32, y as i32);
        if self.st.bar_ed.drag.is_none() {
            let (from, name) = match a {
                Act::SModDrag(si, i) => {
                    let l = [&self.st.set.bar_left, &self.st.set.bar_center, &self.st.set.bar_right][*si];
                    let Some(m) = l.get(*i) else { return };
                    (Some((*si, *i)), m.clone())
                }
                Act::SModNew(id) => (None, id.to_string()),
                _ => return,
            };
            let off = (press.0 as i32 - zone.x, press.1 as i32 - zone.y);
            self.st.bar_ed.drag = Some(BDrag { from, name, x, y, off, target: None, at: std::time::Instant::now() });
        }
        let areas = self.st.bar_ed.areas.get();
        let side = areas.iter().position(|r| R::new(r.x - 8, r.y - 8, r.w + 16, r.h + 16).contains(x as f64, y as f64));
        let target = side.map(|si| {
            let n = self.st.bar_ed.rects.borrow().iter().filter(|(s, _, r)| *s == si && (r.bottom() <= y || (r.y <= y + 2 && r.x + r.w / 2 < x))).count();
            (si, n)
        });
        let Some(d) = self.st.bar_ed.drag.as_mut() else { return };
        (d.x, d.y) = (x, y);
        if d.target != target {
            d.target = target;
            d.at = std::time::Instant::now();
        }
        self.dirty(&SKind::Modal);
    }

    /// отпустили: переставить (мимо рядов — фишка возвращается на место)
    pub fn bar_drop(&mut self) {
        let Some(d) = self.st.bar_ed.drag.take() else { return };
        if let Some((ts, ti)) = d.target {
            let s = &mut self.st.set;
            if let Some((fs, fi)) = d.from {
                let l = s.side_mut(fs);
                if fi < l.len() {
                    l.remove(fi);
                }
            }
            let l = s.side_mut(ts);
            let ti = ti.min(l.len());
            l.insert(ti, d.name);
            self.settings_changed();
        }
        self.dirty(&SKind::Modal);
    }
}
