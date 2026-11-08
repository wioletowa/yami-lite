// Виджеты рабочего стола: свой слой bottom (над обоями, под окнами). Каждый — со своими
// настройками (вид графика, анимация, размер, цвет, подложка…): ПКМ по виджету или
// настройки → виджеты. В режиме расстановки таскаются мышью по сетке 8 px.

use crate::app::{App, SKind, State};
use crate::clock::{local_now, RU_DAY_FULL, RU_MON, RU_MON_NOM};
use crate::config::Widget;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, C, R};
use crate::menu::{MAct, MItem, MenuState};
use crate::theme::{Theme, WHITE};
use crate::ui::{Act, Cur, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};

pub const KINDS: &[(&str, &str)] = &[("clock", "часы"), ("graph", "график"), ("net", "сеть"), ("media", "музыка"), ("cal", "календарь")];
pub const SOURCES: &[(&str, &str)] = &[("cpu", "процессор"), ("ram", "память"), ("temp", "температура"), ("gpu", "видеокарта")];
pub const STYLES: &[(&str, &str)] = &[("bars", "столбики"), ("line", "линия"), ("blocks", "клетки"), ("wave", "волна"), ("dots", "точки")];
pub const ANIMS: &[(&str, &str)] = &[("scroll", "бегущий"), ("rise", "рост"), ("flicker", "мерцание"), ("none", "без анимации")];
pub const SIZES: &[(&str, &str)] = &[("s", "маленький"), ("m", "средний"), ("l", "большой")];
pub const COLORS: &[(&str, &str)] = &[("accent", "акцент"), ("bright", "яркий"), ("ok", "зелёный"), ("warn", "жёлтый"), ("text", "белый")];

pub fn name_of(list: &[(&'static str, &'static str)], id: &str) -> &'static str {
    list.iter().find(|x| x.0 == id).map(|x| x.1).unwrap_or("?")
}

pub fn color(t: &Theme, c: &str) -> C {
    match c {
        "bright" => t.ember,
        "ok" => t.ok,
        "warn" => t.warn,
        "text" => t.text,
        _ => t.red,
    }
}

pub fn size_of(w: &Widget) -> (i32, i32) {
    let k = match w.size.as_str() {
        "s" => 0,
        "l" => 2,
        _ => 1,
    };
    let (a, b, c) = match w.kind.as_str() {
        "clock" => ((260, 100), (360, 128), (540, 190)),
        "graph" | "net" => ((220, 96), (300, 124), (440, 180)),
        "media" => ((320, 112), (400, 136), (500, 170)),
        "cal" => ((236, 220), (280, 252), (350, 316)),
        "script" => ((220, 64), (300, 110), (400, 160)),
        _ => ((200, 100), (300, 140), (400, 200)),
    };
    let (w, h) = [a, b, c][k];
    (w + 4, h + 4)
}

pub fn set_opt(w: &mut Widget, key: &str, val: &str) {
    match key {
        "source" => w.source = val.into(),
        "style" => w.style = val.into(),
        "anim" => w.anim = val.into(),
        "size" => w.size = val.into(),
        "color" => w.color = val.into(),
        "frame" => w.frame = !w.frame,
        "label" => w.label = !w.label,
        "seconds" => w.seconds = !w.seconds,
        "date" => w.date = !w.date,
        "on" => w.on = !w.on,
        _ => {}
    }
}

/// меню виджета: всё, что у него настраивается
pub fn widget_menu(w: &Widget) -> Vec<MItem> {
    let id = w.id;
    let pick = |key: &'static str, list: &[(&'static str, &'static str)], cur: &str| -> Vec<MItem> {
        list.iter().map(|(v, n)| MItem::new(n, "", MAct::Widget(id, key, v.to_string())).check(*v == cur)).collect()
    };
    let mut items = vec![];
    match w.kind.as_str() {
        "graph" => {
            items.push(MItem::sub(&crate::tf!("источник: {}", "source: {}", crate::i18n::tr(name_of(SOURCES, &w.source))), "chart", pick("source", SOURCES, &w.source)));
            items.push(MItem::sub(&crate::tf!("вид: {}", "style: {}", crate::i18n::tr(name_of(STYLES, &w.style))), "grid", pick("style", STYLES, &w.style)));
            items.push(MItem::sub(&crate::tf!("анимация: {}", "animation: {}", crate::i18n::tr(name_of(ANIMS, &w.anim))), "eye", pick("anim", ANIMS, &w.anim)));
        }
        "net" => {
            items.push(MItem::sub(&crate::tf!("вид: {}", "style: {}", crate::i18n::tr(name_of(STYLES, &w.style))), "grid", pick("style", STYLES, &w.style)));
            items.push(MItem::sub(&crate::tf!("анимация: {}", "animation: {}", crate::i18n::tr(name_of(ANIMS, &w.anim))), "eye", pick("anim", ANIMS, &w.anim)));
        }
        "media" => {
            items.push(MItem::sub(&crate::tf!("эквалайзер: {}", "equalizer: {}", crate::i18n::tr(name_of(STYLES, &w.style))), "music", pick("style", STYLES, &w.style)));
        }
        "clock" => {
            items.push(MItem::new("секунды", "", MAct::Widget(id, "seconds", String::new())).check(w.seconds));
            items.push(MItem::new("дата", "", MAct::Widget(id, "date", String::new())).check(w.date));
        }
        _ => {}
    }
    items.push(MItem::sub(&crate::tf!("размер: {}", "size: {}", crate::i18n::tr(name_of(SIZES, &w.size))), "max", pick("size", SIZES, &w.size)));
    items.push(MItem::sub(&crate::tf!("цвет: {}", "color: {}", crate::i18n::tr(name_of(COLORS, &w.color))), "image", pick("color", COLORS, &w.color)));
    items.push(MItem::new("подложка", "", MAct::Widget(id, "frame", String::new())).check(w.frame));
    if matches!(w.kind.as_str(), "graph" | "net" | "media") {
        items.push(MItem::new("подпись", "", MAct::Widget(id, "label", String::new())).check(w.label));
    }
    items.push(MItem::sep());
    items.push(MItem::new("спрятать", "eye", MAct::Widget(id, "on", String::new())));
    items.push(MItem::new("убрать совсем", "trash", MAct::WidgetDel(id)).danger());
    items
}



// ── графики ─────────────────────────────────────────────────────────────────
/// график по точкам 0..1 (старые слева). phase 0..1 — доля секунды до следующей точки.
#[allow(clippy::too_many_arguments)]
pub fn graph(ui: &mut Ui, r: R, data: &[f32], style: &str, anim: &str, phase: f32, c: C, mirror_c: Option<(&[f32], C)>) {
    let t = ui.t;
    let col = match style {
        "line" | "dots" => 3,
        "blocks" => 6,
        _ => 4,
    };
    let n = (r.w / col) as usize + 2;
    let pts: Vec<f32> = data.iter().rev().take(n).rev().copied().collect();
    // бегущий график: сдвиг на долю столбца, по 2 px
    let shift = if anim == "scroll" { ((phase * col as f32 / 2.0).floor() as i32) * 2 } else { 0 };
    // рост: свежая точка вырастает ступенями
    let grow = if anim == "rise" { ((phase * 3.0).ceil() / 3.0).min(1.0) } else { 1.0 };
    let cnt = pts.len();
    let base = r.bottom();
    ui.cv.push_clip(r);
    // сетка: редкие точки
    for gy in (r.y..r.bottom()).step_by(8) {
        for gx in (r.x..r.right()).step_by(8) {
            ui.cv.fill(R::new(gx, gy, 1, 1), t.border.fade(0.5));
        }
    }
    let x_of = |i: usize| r.right() - ((cnt - i) as i32) * col - shift + col;
    let val = |i: usize| -> f32 {
        let v = pts[i].clamp(0.0, 1.0);
        if i + 1 == cnt && grow < 1.0 && i > 0 {
            pts[i - 1] + (v - pts[i - 1]) * grow
        } else {
            v
        }
    };
    let mut prev_y: Option<i32> = None;
    for i in 0..cnt {
        let x = x_of(i);
        let v = val(i);
        let newest = i + 1 == cnt;
        let mut cc = c;
        if anim == "flicker" {
            // CRT: свежий столбец вспыхивает, старые тлеют
            let age = (cnt - i) as f32 / cnt as f32;
            cc = c.lerp(t.bg, age * 0.55);
            if newest && (phase * 6.0) as i32 % 2 == 0 {
                cc = t.ember;
            }
        } else if newest {
            cc = c.lerp(WHITE, 0.25);
        }
        let h = (v * r.h as f32).round() as i32;
        match style {
            "line" => {
                let y = base - h.max(1);
                ui.cv.fill(R::new(x, y - 1, col, 2), cc);
                if let Some(py) = prev_y {
                    let (a, b) = (py.min(y), py.max(y));
                    ui.cv.fill(R::new(x, a - 1, 2, b - a + 2), cc);
                }
                // заливка под линией — шахматкой
                for yy in (y + 2..base).step_by(2) {
                    let off = if (yy / 2) % 2 == 0 { 0 } else { 1 };
                    for xx in (x + off..x + col).step_by(2) {
                        ui.cv.fill(R::new(xx, yy, 1, 1), cc.fade(0.35));
                    }
                }
                prev_y = Some(y);
            }
            "dots" => {
                let y = base - h.max(2);
                ui.cv.fill(R::new(x, y, 2, 2), cc);
            }
            "blocks" => {
                let cells = (h / 5).max(if v > 0.01 { 1 } else { 0 });
                for k in 0..cells {
                    let cy = base - (k + 1) * 5;
                    let top = k + 1 == cells;
                    ui.cv.fill(R::new(x, cy + 1, col - 1, 4), if top { cc.lerp(WHITE, 0.2) } else { cc.fade(0.85) });
                }
            }
            "wave" => {
                let mid = r.y + r.h / 2;
                let hh = (h / 2).max(1);
                ui.cv.fill(R::new(x, mid - hh, col - 1, hh * 2), cc.fade(0.85));
                ui.cv.fill(R::new(x, mid - hh, col - 1, 1), cc.lerp(WHITE, 0.3));
            }
            _ => {
                if h > 0 {
                    ui.cv.fill(R::new(x, base - h, col - 1, h), cc.fade(0.8));
                    ui.cv.fill(R::new(x, base - h, col - 1, 2.min(h)), cc.lerp(WHITE, 0.3));
                }
            }
        }
    }
    // вторая серия (сеть: отдача) — контуром поверх
    if let Some((d2, c2)) = mirror_c {
        let p2: Vec<f32> = d2.iter().rev().take(n).rev().copied().collect();
        let cnt2 = p2.len();
        let mut py: Option<i32> = None;
        for i in 0..cnt2 {
            let x = r.right() - ((cnt2 - i) as i32) * col - shift + col;
            let y = base - (p2[i].clamp(0.0, 1.0) * r.h as f32).round().max(1.0) as i32;
            ui.cv.fill(R::new(x, y - 1, col, 2), c2);
            if let Some(p) = py {
                let (a, b) = (p.min(y), p.max(y));
                ui.cv.fill(R::new(x, a - 1, 2, b - a + 2), c2);
            }
            py = Some(y);
        }
    }
    ui.cv.pop_clip();
    ui.cv.fill(R::new(r.x, r.bottom(), r.w, 1), t.border);
}

/// эквалайзер: полосы 0..1
pub fn viz(ui: &mut Ui, r: R, bands: &[f32], style: &str, c: C, idle: bool) {
    let t = ui.t;
    let n = bands.len().max(1) as i32;
    let bw = (r.w / n).max(2);
    for i in 0..n {
        let v = if idle { 0.04 } else { bands.get(i as usize).copied().unwrap_or(0.0) };
        let x = r.x + i * bw;
        let h = ((v * r.h as f32).round() as i32).max(2);
        match style {
            "wave" => {
                let mid = r.y + r.h / 2;
                ui.cv.fill(R::new(x, mid - h / 2, bw - 1, h.max(2)), c);
            }
            "dots" => {
                ui.cv.fill(R::new(x, r.bottom() - h, bw - 1, 2), c);
            }
            "blocks" => {
                let cells = (h / 4).max(1);
                for k in 0..cells {
                    let top = k + 1 == cells;
                    ui.cv.fill(R::new(x, r.bottom() - (k + 1) * 4 + 1, bw - 1, 3), if top { t.ember } else { c });
                }
            }
            "line" => {
                let y = r.bottom() - h;
                ui.cv.fill(R::new(x, y, bw, 2), c);
            }
            _ => {
                ui.cv.fill(R::new(x, r.bottom() - h, bw - 1, h), c);
                ui.cv.fill(R::new(x, r.bottom() - h, bw - 1, 2), t.ember);
            }
        }
    }
}

fn panel(ui: &mut Ui, w: &Widget, r: R) {
    let t = ui.t;
    if w.frame {
        ui.cv.pxbox(r, &BoxStyle::new(t.glass_bg(), t.border).shadow(4));
    }
}

/// средняя яркость обоев (0..1) под прямоугольником r монитора ow×oh. Картинка обоев ложится на
/// монитор «как обложка» (wall::cover) — тем же пересчётом; берётся сетка ~48×24 точек
pub fn wall_luma(img: &crate::gfx::Img, ow: i32, oh: i32, r: R) -> Option<f32> {
    if img.w <= 0 || img.h <= 0 || r.w <= 0 || r.h <= 0 {
        return None;
    }
    let k = (ow as f32 / img.w as f32).max(oh as f32 / img.h as f32);
    let (offx, offy) = ((ow as f32 - img.w as f32 * k) / 2.0, (oh as f32 - img.h as f32 * k) / 2.0);
    let (sx, sy) = ((r.w / 48).max(1), (r.h / 24).max(1));
    let (mut sum, mut n) = (0.0f32, 0u32);
    for y in (r.y..r.bottom()).step_by(sy as usize) {
        for x in (r.x..r.right()).step_by(sx as usize) {
            let ix = (((x as f32 - offx) / k) as i32).clamp(0, img.w - 1);
            let iy = (((y as f32 - offy) / k) as i32).clamp(0, img.h - 1);
            let p = img.px[(iy * img.w + ix) as usize];
            let (rr, g, b) = ((p >> 16) & 255, (p >> 8) & 255, p & 255);
            sum += (0.299 * rr as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0;
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f32)
}

/// виджет прямо на обоях: тема под их яркость (светлая — тёмный текст) и тень у букв в
/// противоположный тон. Решает замер App::widget_contrast; с подложкой — тема шелла как есть
pub fn draw(ui: &mut Ui, st: &State, id: u32, sw: i32, sh: i32) {
    let Some(w) = st.set.widgets.iter().find(|x| x.id == id) else { return };
    let (saved, bright) = (ui.t, st.widget_bg.get(&id).copied().filter(|_| !w.frame && st.set.widget_contrast));
    if let Some(light) = bright {
        let s = &st.set;
        if light != saved.light {
            ui.t = Theme::new(s.accent_rgb(), s.tint, s.glass, s.lift, s.inner, light);
        }
        ui.cv.text_shade = Some(if light { C(0xffffffff).alpha(0.55) } else { C(0xff000000).alpha(0.6) });
    }
    draw_widget(ui, st, w, sw, sh);
    ui.t = saved;
    ui.cv.text_shade = None;
}

fn draw_widget(ui: &mut Ui, st: &State, w: &Widget, sw: i32, sh: i32) {
    let id = w.id;
    let t = ui.t;
    let r = R::new(0, 0, sw - 4, sh - 4);
    let c = color(&t, &w.color);
    let phase = st.sys_phase();
    match w.kind.as_str() {
        "clock" => {
            panel(ui, w, r);
            let tm = local_now();
            let big = match w.size.as_str() {
                "s" => 48,
                "l" => 96,
                _ => 64,
            };
            let face = font::px(big);
            let pad = if w.frame { 14 } else { 4 };
            let mut x = pad;
            let y = pad;
            x += ui.text(face, x, y, &format!("{:02}", tm.hour), t.text);
            let colon = if w.seconds && tm.sec % 2 == 1 { t.dim } else { c };
            x += ui.text(face, x, y, ":", colon);
            ui.text(face, x, y, &format!("{:02}", tm.min), t.text);
            let mut yy = y + ui.f.line(face) + 6;
            if w.date {
                let s = crate::tf!("{}, {} {}", "{}, {} {}", crate::i18n::tr(RU_DAY_FULL[tm.wday as usize]), tm.day, crate::i18n::tr(RU_MON[(tm.mon - 1) as usize]));
                ui.text(PX, pad, yy, &s, t.text_dim);
                yy += 24;
            }
            if w.seconds {
                // секунды: 12 клеток по 5 секунд
                let cw = ((r.w - 2 * pad) / 12 - 2).clamp(6, 22);
                for i in 0..12 {
                    let on = i < (tm.sec / 5) as i32 + 1;
                    ui.cv.fill(R::new(pad + i * (cw + 2), yy, cw, 4), if on { c } else { t.border.fade(0.7) });
                }
            }
        }
        "graph" => {
            panel(ui, w, r);
            let (ser, full) = st.sys.series(&w.source);
            let data: Vec<f32> = ser.tail(160).iter().map(|v| v / full).collect();
            let (label, val) = match w.source.as_str() {
                "ram" => ("память", crate::tf!("{:.0}%  {:.1}/{:.0} ГБ", "{:.0}%  {:.1}/{:.0} GB", st.sys.mem * 100.0, st.sys.mem_used, st.sys.mem_total)),
                "temp" => ("температура", format!("{:.0}°C", st.sys.temp)),
                "gpu" if st.sys.gpu_sleep => ("видеокарта", "сон".into()),
                "gpu" => ("видеокарта", format!("{:.0}°C  {:.0}%", st.sys.gpu_temp, st.sys.gpu_load * 100.0)),
                _ => ("процессор", format!("{:.0}%", st.sys.cpu * 100.0)),
            };
            let pad = if w.frame { 10 } else { 2 };
            let mut gy = pad;
            if w.label {
                ui.cv.icon(match w.source.as_str() {
                    "ram" => "ram",
                    "temp" | "gpu" => "temp",
                    _ => "cpu",
                }, pad, pad + 1, 2, c);
                ui.text_in(PX, R::new(pad + 20, pad, r.w / 2, 18), label, t.text_dim);
                ui.text_right(PX, R::new(pad, pad, r.w - 2 * pad, 18), &val, t.text);
                gy += 24;
            }
            graph(ui, R::new(pad, gy, r.w - 2 * pad, r.h - gy - pad), &data, &w.style, &w.anim, phase, c, None);
            if w.anim != "none" {
                ui.anim = false;
            }
        }
        "net" => {
            panel(ui, w, r);
            let full = st.sys.h_rx.max().max(st.sys.h_tx.max()).max(64.0 * 1024.0);
            let rx: Vec<f32> = st.sys.h_rx.tail(160).iter().map(|v| v / full).collect();
            let tx: Vec<f32> = st.sys.h_tx.tail(160).iter().map(|v| v / full).collect();
            let pad = if w.frame { 10 } else { 2 };
            let mut gy = pad;
            if w.label {
                ui.cv.icon("net", pad, pad + 1, 2, c);
                let s = crate::tf!("↓{}/с", "↓{}/s", crate::sys::rate(st.sys.rx));
                let s2 = crate::tf!("↑{}/с", "↑{}/s", crate::sys::rate(st.sys.tx));
                let w1 = ui.text_in(PX, R::new(pad + 20, pad, 120, 18), &s, c);
                ui.text_in(PX, R::new(pad + 32 + w1, pad, 120, 18), &s2, t.text_dim);
                ui.text_right(PX, R::new(pad, pad, r.w - 2 * pad, 18), &st.sys.iface, t.text_faint);
                gy += 24;
            }
            graph(ui, R::new(pad, gy, r.w - 2 * pad, r.h - gy - pad), &rx, &w.style, &w.anim, phase, c, Some((&tx, t.text_dim)));
        }
        "media" => {
            panel(ui, w, r);
            media_card(ui, st, R::new(10, 10, r.w - 20, r.h - 20), &w.style, c, w.label, true);
        }
        "cal" => {
            panel(ui, w, r);
            let pad = if w.frame { 12 } else { 2 };
            calendar(ui, st, R::new(pad, pad, r.w - 2 * pad, r.h - 2 * pad), 0, c, false);
        }
        _ => {}
    }

    // режим расстановки: рамка, весь виджет — ручка; ПКМ — настройки
    if st.editing {
        ui.cv.fill(r, t.red.fade(0.08));
        ui.cv.outline(r, t.red, 2);
        let tag = R::new(r.x + 2, r.y + 2, 120, 22);
        ui.cv.fill(tag, t.red);
        ui.text_in(PX, R::new(tag.x + 6, tag.y, tag.w, tag.h), name_of(KINDS, &w.kind), t.on_accent);
        ui.zone(r, Act::WDrag(id), Cur::Grab);
        ui.button(R::new(r.right() - 28, r.y + 4, 24, 22), "", "gear", Tone::Primary, false, true, Act::WMenu(id));
    }
}

/// карточка музыки: обложка, трек, полоса, кнопки, эквалайзер
/// плеер: обложка (show_art; без неё строки начинаются от края), название, исполнитель, кнопки,
/// полоса позиции и под ними эквалайзер стилем style ("off" — без него)
#[allow(clippy::too_many_arguments)]
pub fn media_card(ui: &mut Ui, st: &State, r: R, style: &str, c: C, label: bool, show_art: bool) {
    let t = ui.t;
    let m = &st.media;
    if m.title.is_empty() {
        ui.cv.icon("music", r.x, r.y + 2, 2, t.text_faint);
        ui.text_in(PX, R::new(r.x + 22, r.y, r.w, 18), "тишина — ничего не играет", t.text_faint);
        if style != "off" {
            viz(ui, R::new(r.x, r.bottom() - 24, r.w, 24), &[], style, t.border, true);
        }
        return;
    }
    let art = st.art.get(&m.art).cloned().flatten();
    let side = (r.h - 30).clamp(32, 64);
    let mut x = r.x;
    if show_art {
        let a = R::new(r.x, r.y, side, side);
        match &art {
            Some(img) => {
                ui.cv.image_scaled(img, a, 1.0);
                ui.cv.outline(a, t.border, 1);
            }
            None => {
                ui.cv.pxbox(a, &BoxStyle::new(t.surface, t.border));
                ui.cv.icon_in("music", a, 3, t.dim);
            }
        }
        x += side + 10;
    }
    let tw = r.right() - x;
    ui.text_in(PX, R::new(x, r.y, tw, 18), &m.title, t.text);
    if label || !m.artist.is_empty() {
        ui.text_in(PX, R::new(x, r.y + 20, tw, 18), if m.artist.is_empty() { &m.player } else { &m.artist }, t.text_dim);
    }
    // кнопки
    let by = r.y + side - 22;
    ui.button(R::new(x, by, 26, 22), "", "prev", Tone::Ghost, false, true, Act::MediaPrev);
    ui.button(R::new(x + 30, by, 30, 22), "", if m.playing() { "pause" } else { "play" }, Tone::Primary, false, true, Act::MediaPlay);
    ui.button(R::new(x + 64, by, 26, 22), "", "next", Tone::Ghost, false, true, Act::MediaNext);
    // полоса позиции: щёлкнуть или потянуть — перемотка (пока тянут, бегунок там, где курсор)
    if m.length > 0.0 {
        let px0 = x + 100;
        let pw = r.right() - px0;
        if pw > 40 {
            let k = st.seek.unwrap_or((m.now_pos() / m.length) as f32).clamp(0.0, 1.0);
            let zr = R::new(px0, by, pw, 22);
            let hov = ui.hover(zr) || st.seek.is_some();
            ui.cv.fill(R::new(px0, by + 10, pw, 2), if hov { t.text_faint } else { t.border });
            ui.cv.fill(R::new(px0, by + 10, (pw as f32 * k) as i32, 2), c);
            ui.cv.fill(R::new(px0 + (pw as f32 * k) as i32 - 2, by + if hov { 7 } else { 8 }, 4, if hov { 8 } else { 6 }), t.ember);
            ui.zone(zr, Act::MediaSeek, Cur::Pointer);
            if m.playing() {
                ui.anim = false;
            }
        }
    }
    let vy = r.y + side + 6;
    if style != "off" && r.bottom() - vy >= 8 {
        viz(ui, R::new(r.x, vy, r.w, r.bottom() - vy), &st.viz, style, c, st.viz.is_empty() || !m.playing());
    }
}

/// месяц сеткой: сегодня — в акценте, выходные — тусклее
pub fn calendar(ui: &mut Ui, _st: &State, r: R, month_shift: i32, c: C, nav: bool) {
    let t = ui.t;
    let now = local_now();
    let mut y = now.year;
    let mut m = now.mon as i32 - 1 + month_shift;
    y += m.div_euclid(12);
    m = m.rem_euclid(12);
    let title = format!("{} {}", crate::i18n::tr(RU_MON_NOM[m as usize]), y);
    let head = R::new(r.x, r.y, r.w, 22);
    if nav {
        ui.button(R::new(head.x, head.y, 24, 22), "", "left", Tone::Ghost, false, true, Act::CalMonth(-1));
        ui.button(R::new(head.right() - 24, head.y, 24, 22), "", "right", Tone::Ghost, false, true, Act::CalMonth(1));
        ui.text_center(PX, head, &title, if month_shift == 0 { t.text } else { c });
        ui.zone(R::new(head.x + 30, head.y, head.w - 60, head.h), Act::CalMonth(0), Cur::Pointer);
    } else {
        ui.text_in(PX, head, &title, t.text);
    }
    let cw = r.w / 7;
    let ch = ((r.h - 50) / 6).max(16);
    for (i, d) in ["пн", "вт", "ср", "чт", "пт", "сб", "вс"].iter().enumerate() {
        ui.text_center(PX, R::new(r.x + i as i32 * cw, r.y + 26, cw, 18), d, if i >= 5 { t.red.fade(0.7) } else { t.text_faint });
    }
    // первый день месяца
    let first = days_from_civil(y, m as u32 + 1, 1);
    let wd = ((first + 3).rem_euclid(7)) as i32; // 0 = пн
    let dim = days_in_month(y, m as u32 + 1);
    let gy = r.y + 48;
    for d in 1..=dim as i32 {
        let cell = d - 1 + wd;
        let (cx, cy) = (r.x + (cell % 7) * cw, gy + (cell / 7) * ch);
        let rr = R::new(cx + 1, cy, cw - 2, ch - 2);
        let today = month_shift == 0 && d == now.day as i32;
        if today {
            ui.cv.pxbox(rr, &BoxStyle::new(c, t.ember).bevel(true));
        }
        let col = if today { t.on_accent } else if cell % 7 >= 5 { t.red.lerp(t.text, 0.4) } else { t.text };
        ui.text_center(PX, rr, &d.to_string(), col);
    }
}

pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub fn draw_editbar(ui: &mut Ui, _st: &State, w: i32, _h: i32) {
    let t = ui.t;
    let r = R::new(0, 0, w - 4, 40);
    ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), t.red).shadow(4));
    ui.zone(r, Act::Block, Cur::Default);
    ui.cv.icon("grid", 12, 13, 2, t.red);
    ui.text_in(PX, R::new(34, 0, 300, 40), "расстановка: тащи, ⚙ — настройки виджета", t.text);
    let mut x = r.right() - 6;
    for (label, icon, act, tone) in [("готово", "check", Act::EditDone, Tone::Primary), ("виджеты", "gear", Act::SEdit, Tone::Plain)] {
        let bw = ui.w(PX, label) + 34;
        x -= bw;
        ui.button(R::new(x, 8, bw, 24), label, icon, tone, false, true, act);
        x -= 6;
    }
}

impl State {
    /// доля секунды с последнего замера датчиков — для бегущих графиков
    pub fn sys_phase(&self) -> f32 {
        self.sys.at.map(|a| a.elapsed().as_secs_f32().min(1.0)).unwrap_or(0.0)
    }
}

impl App {
    /// какие виджеты без подложки стоят на светлых обоях: замер по готовой картинке обоев под
    /// каждым (после новых обоев, перестановки, смены настроек). Порог с запасом в обе стороны,
    /// чтобы виджет на границе не мигал темой при каждом пересчёте
    pub fn widget_contrast(&mut self) {
        let out = self.focus_out();
        let (ow, oh) = self.output_size(&out);
        let mut res = std::collections::HashMap::new();
        for s in self.surfs.values() {
            let SKind::Widget(id) = s.kind else { continue };
            let r = R::new(s.margin.0, s.margin.1, s.w, s.h);
            let luma = match &self.st.wall {
                Some(img) => wall_luma(img, ow, oh, r),
                None => Some(if self.st.t.light { 0.9 } else { 0.1 }),
            };
            let Some(l) = luma else { continue };
            let was = self.st.widget_bg.get(&id).copied();
            let light = match was {
                Some(true) => l > 0.45,
                Some(false) => l > 0.58,
                None => l > 0.52,
            };
            res.insert(id, light);
        }
        if res != self.st.widget_bg {
            self.st.widget_bg = res;
            self.dirty_where(|k| matches!(k, SKind::Widget(_)));
        }
    }

    /// поверхности виджетов — по списку из настроек
    pub fn sync_widgets(&mut self) {
        let out = self.focus_out();
        if out.is_empty() {
            return;
        }
        let (ow, oh) = self.output_size(&out);
        let want: Vec<Widget> = self.st.set.widgets.iter().filter(|w| w.on && !self.st.locked).cloned().collect();
        // лишние — закрыть
        let have: Vec<(u32, u32)> = self.surfs.iter().filter_map(|(sid, s)| if let SKind::Widget(id) = s.kind { if s.closing { None } else { Some((*sid, id)) } } else { None }).collect();
        for (sid, id) in &have {
            if !want.iter().any(|w| w.id == *id) {
                self.close(*sid);
            }
        }
        let top = self.bar_bottom(&out);
        let mut auto_y = top + 40;
        for w in want {
            let (ww, wh) = size_of(&w);
            let (mut x, mut y) = (w.x, w.y);
            if x < 0 || y < 0 {
                // место по умолчанию
                x = 48;
                y = auto_y;
                auto_y += wh + 16;
            }
            x = x.clamp(0, (ow - ww).max(0));
            y = y.clamp(top, (oh - wh).max(top));
            let raised = self.st.editing;
            let sid = have.iter().find(|(_, id)| *id == w.id).map(|(s, _)| *s);
            match sid {
                None => {
                    let s = self.mk(SKind::Widget(w.id), &out, if raised { Layer::Top } else { Layer::Bottom }, "yami-widget", Anchor::TOP | Anchor::LEFT, (ww, wh), (y, 0, 0, x), -1, KeyboardInteractivity::None);
                    if let Some(s) = self.surfs.get_mut(&s) {
                        s.margin = (x, y);
                        s.applied = (x, y);
                    }
                }
                Some(sid) => {
                    // этот виджет сейчас тащат — его ведёт on_drag
                    let dragged = self.press.as_ref().map(|p| p.sid == sid && p.moved).unwrap_or(false);
                    if !dragged {
                        self.move_surface(sid, x, y);
                    }
                    if let Some(s) = self.surfs.get_mut(&sid) {
                        if s.w != ww || s.h != wh {
                            s.layer.set_size(ww as u32, wh as u32);
                        }
                        s.layer.set_layer(if raised { Layer::Top } else { Layer::Bottom });
                        s.layer.commit();
                        s.dirty = true;
                    }
                }
            }
        }
        self.widget_contrast();
    }

    pub fn set_editing(&mut self, on: bool) {
        self.st.editing = on;
        if on {
            let out = self.focus_out();
            if self.find(&SKind::EditBar).is_empty() {
                let top = self.bar_bottom(&out) + 12;
                self.mk(SKind::EditBar, &out, Layer::Overlay, "yami-menu", Anchor::TOP, (520, 52), (top, 0, 0, 0), -1, KeyboardInteractivity::Exclusive);
            }
        } else {
            self.close_kind(&SKind::EditBar);
        }
        self.sync_widgets();
        self.dirty_where(|k| matches!(k, SKind::Widget(_)));
    }

    pub fn open_widget_menu(&mut self, id: u32) {
        let Some(w) = self.st.set.widgets.iter().find(|w| w.id == id).cloned() else { return };
        let (x, y) = self.surfs.values().find(|s| s.kind == SKind::Widget(id)).map(|s| s.margin).unwrap_or((100, 100));
        let (ww, _) = size_of(&w);
        let out = self.focus_out();
        let title = crate::tf!("виджет: {}", "widget: {}", crate::i18n::tr(name_of(KINDS, &w.kind)));
        self.open_menu(MenuState::new(&out, x + ww - 30, y + 30, &title, widget_menu(&w)));
    }

    pub fn widget_key(&mut self, e: &KeyEvent) {
        if e.keysym == Keysym::Escape && self.st.editing {
            self.set_editing(false);
        }
    }
}
