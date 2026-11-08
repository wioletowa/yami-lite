// Настройки → бар: модули в три колонки (начало, центр, конец) — перетаскивание за ручку, у
// каждого настройки (шестерёнка — окошко поверх вкладки), скрыть и убрать, «добавить» внизу
// колонки; второй раздел — вид бара: форма, отступы, плитки панели управления.

use crate::app::{App, State};
use crate::config::ALL_MODULES;
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::settings::*;
use crate::ui::{Act, Cur, Tone, Ui};

pub(crate) fn side_name(i: usize) -> &'static str {
    ["начало", "центр", "конец"][i]
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
    /// открытое окошко: настройки модуля (имя) или список «добавить» для колонки
    pub gear: Option<String>,
    pub add: Option<usize>,
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
    ALL_MODULES.iter().find(|x| x.0 == m).map(|x| x.1).unwrap_or(m)
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

/// есть ли у модуля свои настройки (шестерёнка)
fn has_opts(m: &str) -> bool {
    matches!(m, "workspaces" | "clock" | "cpu" | "ram" | "gpu" | "disk" | "temp" | "tray" | "media" | "bat" | "tasks" | "sep" | "launcher" | "notif" | "night")
}

const ROW: i32 = 30;

pub(crate) fn tab_bar(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let ed = &st.bar_ed;
    let drag = ed.drag.as_ref();
    let bw = 86;
    ui.button(R::new(b.right() - 2 * bw - 6, b.y - 3, bw, 22), "модули", "menu", Tone::Ghost, !ed.look, true, Act::SCycle("bar_page_mods"));
    ui.button(R::new(b.right() - bw, b.y - 3, bw, 22), "вид", "eye", Tone::Ghost, ed.look, true, Act::SCycle("bar_page_look"));
    if ed.look {
        tab_bar_look(ui, st, b);
        return;
    }
    let y0 = title(ui, b.x, b.y, b.w - 2 * bw - 12, "модули бара");
    let sides = [&s.bar_left, &s.bar_center, &s.bar_right];
    let cw = (b.w - 24) / 3;
    let mut areas = [R::new(0, 0, 0, 0); 3];
    let mut hit = vec![];
    let mut add_btn = [R::new(0, 0, 0, 0); 3];
    for (si, list) in sides.iter().enumerate() {
        let x = b.x + si as i32 * (cw + 12);
        // строки без той, что в руке; щель там, куда её несут
        let items: Vec<(usize, &String)> = list.iter().enumerate().filter(|(i, _)| drag.and_then(|d| d.from) != Some((si, *i))).collect();
        let gap = drag.and_then(|d| d.target).filter(|t| t.0 == si).map(|t| t.1.min(items.len()));
        let n = items.len() + gap.is_some() as usize;
        let col = R::new(x, y0, cw, 40 + n.max(1) as i32 * (ROW + 4) + 40);
        let target_here = gap.is_some();
        ui.cv.pxbox(col, &BoxStyle::new(t.code_bg, if target_here { t.red } else { t.border }));
        ui.text_in(PX, R::new(col.x + 10, col.y + 8, cw - 90, 20), side_name(si), t.text);
        ui.button(R::new(col.right() - 82, col.y + 6, 74, 22), "сбросить", "", Tone::Ghost, false, true, Act::SBarReset(si));
        let mut y = col.y + 36;
        let mut k = 0;
        for j in 0..n {
            let r = R::new(col.x + 6, y, cw - 12, ROW);
            if Some(j) == gap {
                ui.cv.ants(r, t.ember, 2, 0);
            } else {
                let (i, m) = items[k];
                k += 1;
                module_row(ui, st, r, si, i, m, drag.is_none());
                hit.push((si, k - 1, r));
            }
            y += ROW + 4;
        }
        if n == 0 {
            ui.text_in(PX, R::new(col.x + 12, y + 4, cw - 24, 20), "пусто", t.text_faint);
            y += ROW + 4;
        }
        let ab = R::new(col.x + 6, y + 2, cw - 12, 26);
        ui.button(ab, "добавить", "plus", Tone::Ghost, ed.add == Some(si), true, Act::SBarAdd(si));
        add_btn[si] = ab;
        areas[si] = col;
    }
    *ed.rects.borrow_mut() = hit;
    ed.areas.set(areas);
    // строка в руке — поверх всего
    if let Some(d) = drag {
        let r = R::new(d.x - d.off.0, d.y - d.off.1 - 2, cw - 12, ROW);
        chip(ui, r, &d.name, true);
    }
    // окошки поверх вкладки: список «добавить» под кнопкой или настройки модуля по центру
    if let Some(si) = ed.add {
        add_list(ui, st, b, add_btn[si]);
    } else if let Some(m) = &ed.gear {
        gear_dialog(ui, st, b, m);
    }
}

/// строка модуля: ручка (тащить), имя, шестерёнка, глаз (скрыть в баре), ×
fn module_row(ui: &mut Ui, st: &State, r: R, si: usize, i: usize, m: &str, live: bool) {
    let t = ui.t;
    let hidden = st.set.bar_hidden.iter().any(|x| x == m);
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if hov { t.surface_hi } else { t.surface }, if hov { t.red } else { t.border }).bevel(true));
    let gc = if hov { t.red } else { t.text_faint };
    for a in 0..3 {
        for c in 0..2 {
            ui.cv.fill(R::new(r.x + 6 + c * 4, r.cy() - 5 + a * 4, 2, 2), gc);
        }
    }
    let buttons = 3 * 22;
    if m == "sep" {
        ui.cv.sep(r.x + 22, r.cy(), t.text_dim);
    } else {
        ui.text_in(PX, R::new(r.x + 18, r.y, r.w - 22 - buttons, r.h), mod_name(m), if hidden { t.text_faint } else { t.text });
    }
    ui.zone(R::new(r.x, r.y, r.w - buttons, r.h), Act::SModDrag(si, i), Cur::Grab);
    if !live {
        return;
    }
    let mut bx = r.right() - 22;
    ui.ghost(R::new(bx, r.y + 4, 20, 22), "close", Act::SModDel(si, i));
    bx -= 22;
    ui.ghost(R::new(bx, r.y + 4, 20, 22), if hidden { "shade" } else { "eye" }, Act::SModHide(m.to_string()));
    bx -= 22;
    if has_opts(m) {
        ui.ghost(R::new(bx, r.y + 4, 20, 22), "gear", Act::SModGear(m.to_string()));
    }
}

/// «добавить»: всё, чего нет в баре (разделителей — сколько угодно), списком под кнопкой
fn add_list(ui: &mut Ui, st: &State, b: R, at: R) {
    let t = ui.t;
    let s = &st.set;
    let all: Vec<&String> = s.bar_left.iter().chain(&s.bar_center).chain(&s.bar_right).collect();
    let free: Vec<&str> = ALL_MODULES.iter().map(|m| m.0).filter(|id| *id == "sep" || !all.iter().any(|m| m == id)).collect();
    // клик мимо — закрыть
    ui.zone(R::new(b.x - 40, b.y - 40, b.w + 80, b.h + 400), Act::SBarClose, Cur::Default);
    let h = (free.len() as i32 * 26 + 8).min(b.bottom() - at.bottom() - 4).max(60);
    let up = at.bottom() + h > b.bottom() + 200;
    let box_r = R::new(at.x, if up { at.y - h - 4 } else { at.bottom() + 4 }, at.w, h);
    ui.cv.pxbox(box_r, &BoxStyle::new(t.glass_panel(), t.red).shadow(4));
    let mut y = box_r.y + 4;
    for id in free {
        if y + 26 > box_r.bottom() {
            break;
        }
        let r = R::new(box_r.x + 4, y, box_r.w - 8, 24);
        if ui.hover(r) {
            ui.cv.fill(r, t.surface_hi);
        }
        ui.cv.icon("plus", r.x + 4, r.cy() - 7, 2, t.text_faint);
        if id == "sep" {
            ui.cv.sep(r.x + 26, r.cy(), t.text_dim);
        } else {
            ui.text_in(PX, R::new(r.x + 24, r.y, r.w - 28, r.h), mod_name(id), t.text);
        }
        ui.zone(r, Act::SModNew(id.to_string()), Cur::Pointer);
        y += 26;
    }
}

/// настройки модуля: окошко по центру вкладки
fn gear_dialog(ui: &mut Ui, st: &State, b: R, m: &str) {
    let t = ui.t;
    let s = &st.set;
    ui.zone(R::new(b.x - 40, b.y - 40, b.w + 80, b.h + 400), Act::SBarClose, Cur::Default);
    let w = 380.min(b.w);
    let d = R::new(b.x + (b.w - w) / 2, b.y + 40, w, 280);
    ui.cv.pxbox(d, &BoxStyle::new(t.glass_panel(), t.red).shadow(6));
    ui.zone(d, Act::Block, Cur::Default);
    ui.cv.icon("gear", d.x + 12, d.y + 12, 2, t.red);
    ui.text_in(PX, R::new(d.x + 34, d.y + 9, d.w - 70, 20), mod_name(m), t.text);
    ui.ghost(R::new(d.right() - 30, d.y + 6, 24, 22), "close", Act::SBarClose);
    ui.cv.fill(R::new(d.x + 1, d.y + 34, d.w - 2, 1), t.border);
    let (x, rw) = (d.x + 14, d.w - 28);
    let mut y = d.y + 46;
    let opt = |ui: &mut Ui, y: &mut i32, label: &str, val: &str, act: Act| {
        opt_btn(ui, R::new(x, *y, rw, 26), label, val, act);
        *y += 32;
    };
    let tog = |ui: &mut Ui, y: &mut i32, label: &str, on: bool, key: &'static str| {
        ui.toggle(R::new(x, *y, rw, 26), label, "", on, Act::SBool(key));
        *y += 32;
    };
    let num = |ui: &mut Ui, y: &mut i32, label: &str, key: &'static str| {
        num_btn(ui, R::new(x, *y, rw, 26), label, s.num(key), key);
        *y += 32;
    };
    let pick = |list: &[(&'static str, &'static str)], cur: &str| -> &'static str { list.iter().find(|v| v.0 == cur).map_or(list[0].1, |v| v.1) };
    match m {
        "workspaces" => opt(ui, &mut y, "вид", pick(&[("boxes", "квадраты с номером"), ("dots", "точки")], &s.ws_style), Act::SCycle("ws_style")),
        "clock" => {
            tog(ui, &mut y, "секунды", s.clock_seconds, "clock_seconds");
            tog(ui, &mut y, "дата", s.clock_date, "clock_date");
        }
        "cpu" | "ram" | "gpu" | "disk" => opt(ui, &mut y, "вид", pick(&[("gauge", "шкала"), ("graph", "график"), ("number", "число")], &s.cpu_style), Act::SCycle("cpu_style")),
        "temp" => opt(ui, &mut y, "вид", if s.temp_style == "compact" { "коротко" } else { "как у процессора" }, Act::SCycle("temp_style")),
        "tray" => {
            opt(ui, &mut y, "вид", if s.tray_mode == "arrow" { "стрелкой" } else { "значки в ряд" }, Act::SCycle("tray_mode"));
            tog(ui, &mut y, "значки клетками", s.tray_pixel, "tray_pixel");
        }
        "media" => {
            let eq = if s.media_viz == "off" { "выкл" } else { crate::widgets::name_of(crate::widgets::STYLES, &s.media_viz) };
            opt(ui, &mut y, "эквалайзер", eq, Act::SCycle("media_viz"));
            opt(ui, &mut y, "всплывашка", crate::widgets::name_of(crate::config::MEDIA_VIEWS, &s.media_view), Act::SCycle("media_view"));
        }
        "bat" => tog(ui, &mut y, "прятать полную батарею", s.bat_hide_full, "bat_hide_full"),
        "tasks" => {
            num(ui, &mut y, "ширина кнопки", "task_width");
            num(ui, &mut y, "зазор между окнами", "bar_task_gap");
        }
        "sep" => {
            let style = pick(&[("dots", "точки"), ("line", "черта"), ("blocks", "клетки"), ("gap", "пусто")], &s.bar_sep);
            opt(ui, &mut y, "вид", style, Act::SCycle("bar_sep"));
            opt(ui, &mut y, "цвет", pick(&[("border", "рамка"), ("accent", "акцент"), ("dim", "тусклый")], &s.bar_sep_color), Act::SCycle("bar_sep_color"));
            num(ui, &mut y, "ширина", "bar_sep_w");
        }
        "launcher" => {
            ui.text_in(PX, R::new(x, y, rw, 20), "команда (её же запускает кнопка в доке)", t.text_dim);
            y += 24;
            let focused = st.field_focus == Some("launcher_cmd");
            let hint = if s.launcher_cmd.is_empty() { "fuzzel, rofi -show drun, …" } else { s.launcher_cmd.as_str() };
            ui.field(R::new(x, y, rw, 28), &st.launcher_field, hint, focused, false, Act::SField("launcher_cmd"));
            y += 34;
            ui.button(R::new(x, y, rw, 26), "применить", "check", Tone::Primary, false, true, Act::SCycle("launcher_apply"));
        }
        "notif" => {
            tog(ui, &mut y, "не беспокоить", s.dnd, "dnd");
            tog(ui, &mut y, "тихо при демонстрации", s.cast_dnd, "cast_dnd");
        }
        "night" => tog(ui, &mut y, "ночной свет", s.night, "night"),
        _ => {}
    }
    let _ = y;
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
    let ws = if s.ws_style == "dots" { "точки" } else { "квадраты" };
    opt_btn(ui, R::new(col(0), y, cw * 2 + (col(1) - col(0) - cw), 26), "рабочие столы", ws, Act::SCycle("ws_style"));
    y += 32;
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
    // панель управления открывается правым щелчком по бару
    y = title(ui, b.x, y, b.w, "панель управления");
    ui.text_in(PX, R::new(b.x, y - 6, b.w, 18), &crate::tf!("открывается правым щелчком по бару · плиток: {} из 6", "opens with a right click on the bar · tiles: {} of 6", s.control_tiles.len()), t.text_faint);
    y += 18;
    for (i, (k, name, _)) in crate::control::TILES.iter().enumerate() {
        let on = s.control_tiles.iter().any(|x| x == k);
        ui.toggle(R::new(col(i as i32 % 3), y + (i as i32 / 3) * 32, cw, 26), name, "", on, Act::SCtlTile(k));
    }
    y += (crate::control::TILES.len() as i32 + 2) / 3 * 32 + 8;
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
            let n = self.st.bar_ed.rects.borrow().iter().filter(|(s, _, r)| *s == si && r.y + r.h / 2 < y).count();
            (si, n)
        });
        let Some(d) = self.st.bar_ed.drag.as_mut() else { return };
        (d.x, d.y) = (x, y);
        if d.target != target {
            d.target = target;
            d.at = std::time::Instant::now();
        }
        self.dirty_panels();
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
        self.dirty_panels();
    }
}
