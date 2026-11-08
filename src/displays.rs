// Экраны (настройки → экраны): расположение мышкой, разрешение, частота, масштаб, поворот, VRR.
// Состояние берём у niri (IPC Outputs), применяем записью ~/.config/niri/yami-outputs.kdl —
// niri перечитывает конфиг сам. После применения 15 с на «оставить», иначе откат.

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Tone, Ui};
use serde_json::Value;
use std::cell::Cell;
use std::time::{Duration, Instant};

const REVERT_S: u64 = 15;
const SCALES: &[f64] = &[1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];
const TRANSFORMS: &[&str] = &["normal", "90", "180", "270"];

#[derive(Clone, Default, PartialEq)]
pub struct Mode {
    pub w: i32,
    pub h: i32,
    pub mhz: i32,
    pub pref: bool,
}

#[derive(Clone, Default)]
pub struct Out {
    pub name: String,
    pub desc: String,
    pub mm: (i64, i64),
    pub modes: Vec<Mode>,
    pub mode: usize,
    pub on: bool,
    /// включён в niri сейчас (on — с правками до «применить»): бар есть только на таких
    pub live: bool,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub transform: String,
    pub vrr_ok: bool,
    /// 0 выкл, 1 всегда, 2 в играх (on-demand: только когда на экране окно с правилом VRR)
    pub vrr: u8,
}

impl Out {
    fn cur(&self) -> Mode {
        self.modes.get(self.mode).cloned().unwrap_or_default()
    }
    /// логический размер: режим с поворотом, делённый на масштаб
    pub fn size(&self) -> (i32, i32) {
        let m = self.cur();
        let (w, h) = if matches!(self.transform.as_str(), "90" | "270" | "flipped-90" | "flipped-270") { (m.h, m.w) } else { (m.w, m.h) };
        ((w as f64 / self.scale).round() as i32, (h as f64 / self.scale).round() as i32)
    }
}

#[derive(Default)]
pub struct Disp {
    pub outs: Vec<Out>,
    pub sel: usize,
    /// есть правки, ещё не применённые
    pub edited: bool,
    /// откуда начали тащить экран
    pub drag: Option<(i32, i32)>,
    /// прежний yami-outputs.kdl (None — его не было) и когда откатить
    pub revert: Option<(Option<String>, Instant)>,
    pub reload_at: Option<Instant>,
    /// масштаб схемы и её начало (замирают, пока тащим)
    view: Cell<(f64, f64, f64)>,
}

fn path() -> std::path::PathBuf {
    crate::config::config_dir().join("niri/yami-outputs.kdl")
}

fn transform_of(v: &str) -> String {
    let s = v.to_lowercase().replace('_', "");
    match s.strip_prefix("flipped") {
        Some(r) if !r.is_empty() => format!("flipped-{}", r.trim_start_matches('-')),
        _ => s,
    }
}

/// VRR «по запросу» по нашему файлу: niri в IPC его не показывает
fn on_demand() -> Vec<String> {
    let t = std::fs::read_to_string(path()).unwrap_or_default();
    let mut v = vec![];
    let mut cur = "";
    for l in t.lines().map(str::trim) {
        if let Some(n) = l.strip_prefix("output \"") {
            cur = n.split('"').next().unwrap_or("");
        } else if l.starts_with("variable-refresh-rate") && l.contains("on-demand=true") {
            v.push(cur.to_string());
        }
    }
    v
}

pub fn load() -> Vec<Out> {
    let od = on_demand();
    let Some(r) = crate::niri::request(&Value::String("Outputs".into())) else { return vec![] };
    let Some(m) = r.pointer("/Ok/Outputs").and_then(|v| v.as_object()) else { return vec![] };
    let mut outs: Vec<Out> = m
        .values()
        .map(|o| {
            let s = |k: &str| o.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let modes: Vec<Mode> = o
                .get("modes")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .map(|m| Mode {
                            w: m["width"].as_i64().unwrap_or(0) as i32,
                            h: m["height"].as_i64().unwrap_or(0) as i32,
                            mhz: m["refresh_rate"].as_i64().unwrap_or(0) as i32,
                            pref: m["is_preferred"].as_bool().unwrap_or(false),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let cur = o.get("current_mode").and_then(|v| v.as_u64()).map(|v| v as usize);
            let lg = o.get("logical").filter(|v| !v.is_null());
            let n = |k: &str| lg.and_then(|l| l.get(k)).and_then(|v| v.as_f64());
            Out {
                name: s("name"),
                desc: format!("{} {}", s("make"), s("model")).trim().to_string(),
                mm: (o["physical_size"][0].as_i64().unwrap_or(0), o["physical_size"][1].as_i64().unwrap_or(0)),
                mode: cur.or_else(|| modes.iter().position(|m| m.pref)).unwrap_or(0),
                modes,
                on: cur.is_some() && lg.is_some(),
                live: cur.is_some() && lg.is_some(),
                x: n("x").unwrap_or(0.0) as i32,
                y: n("y").unwrap_or(0.0) as i32,
                scale: n("scale").unwrap_or(1.0),
                transform: transform_of(lg.and_then(|l| l["transform"].as_str()).unwrap_or("normal")),
                vrr_ok: o["vrr_supported"].as_bool().unwrap_or(false),
                vrr: if od.contains(&s("name")) { 2 } else { o["vrr_enabled"].as_bool().unwrap_or(false) as u8 },
            }
        })
        .collect();
    outs.sort_by(|a, b| (a.x, a.y, &a.name).cmp(&(b.x, b.y, &b.name)));
    outs
}

/// kdl для niri: включённые сдвинуты так, чтобы схема начиналась в 0,0
pub fn kdl(outs: &[Out]) -> String {
    let on: Vec<&Out> = outs.iter().filter(|o| o.on).collect();
    let mx = on.iter().map(|o| o.x).min().unwrap_or(0);
    let my = on.iter().map(|o| o.y).min().unwrap_or(0);
    let mut s = String::from("// yami: written by the shell (settings → screens). Changes there overwrite this file;\n// your own output tweaks go into user.kdl.\n");
    for o in outs {
        s += &format!("\noutput \"{}\" {{\n", o.name);
        if !o.on {
            s += "    off\n}\n";
            continue;
        }
        let m = o.cur();
        s += &format!("    mode \"{}x{}@{:.3}\"\n", m.w, m.h, m.mhz as f64 / 1000.0);
        s += &format!("    scale {}\n", o.scale);
        s += &format!("    transform \"{}\"\n", o.transform);
        s += &format!("    position x={} y={}\n", o.x - mx, o.y - my);
        match o.vrr {
            1 => s += "    variable-refresh-rate\n",
            2 => s += "    variable-refresh-rate on-demand=true\n",
            _ => {}
        }
        s += "}\n";
    }
    s
}

fn hz(mhz: i32) -> String {
    let v = mhz as f64 / 1000.0;
    if (v - v.round()).abs() < 0.05 {
        crate::tf!("{} Гц", "{} Hz", "{} Hz", v.round())
    } else {
        crate::tf!("{:.2} Гц", "{:.2} Hz", "{:.2} Hz", v)
    }
}

/// масштаб и начало схемы для области b
fn fit(d: &Disp, b: R) -> (f64, f64, f64) {
    let on: Vec<&Out> = d.outs.iter().filter(|o| o.on).collect();
    if on.is_empty() {
        return (0.1, 0.0, 0.0);
    }
    let x0 = on.iter().map(|o| o.x).min().unwrap() as f64;
    let y0 = on.iter().map(|o| o.y).min().unwrap() as f64;
    let x1 = on.iter().map(|o| o.x + o.size().0).max().unwrap() as f64;
    let y1 = on.iter().map(|o| o.y + o.size().1).max().unwrap() as f64;
    // запас по краям — чтобы было куда тащить
    let k = ((b.w as f64 - 40.0) / (x1 - x0)).min((b.h as f64 - 24.0) / (y1 - y0)) * 0.7;
    let ox = x0 - ((b.w as f64 / k) - (x1 - x0)) / 2.0;
    let oy = y0 - ((b.h as f64 / k) - (y1 - y0)) / 2.0;
    (k, ox, oy)
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let d = &st.disp;
    let mut y = crate::settings::title(ui, b.x, b.y, b.w, "экраны — тащи мышкой, чтобы расставить");
    if d.outs.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "niri не ответил (NIRI_SOCKET?)", t.text_faint);
        return;
    }
    // схема
    let mapr = R::new(b.x, y, b.w, 150);
    ui.cv.pxbox(mapr, &BoxStyle::new(t.code_bg, t.border));
    if d.drag.is_none() {
        d.view.set(fit(d, mapr));
    }
    let (k, ox, oy) = d.view.get();
    ui.cv.push_clip(R::new(mapr.x + 2, mapr.y + 2, mapr.w - 4, mapr.h - 4));
    for (i, o) in d.outs.iter().enumerate().filter(|(_, o)| o.on) {
        let (w, h) = o.size();
        let r = R::new(mapr.x + ((o.x as f64 - ox) * k) as i32, mapr.y + ((o.y as f64 - oy) * k) as i32, (w as f64 * k) as i32, (h as f64 * k) as i32);
        let sel = d.sel == i;
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(if sel { t.selection } else { t.surface }, if sel { t.ember } else if hov { t.red } else { t.border }).bevel(true).shadow(2));
        ui.cv.text_center(ui.f, PX, R::new(r.x, r.cy() - 18, r.w, 18), &o.name, if sel { t.text } else { t.text_dim });
        ui.cv.text_center(ui.f, PX, R::new(r.x, r.cy(), r.w, 18), &format!("{}×{}", o.cur().w, o.cur().h), if sel { t.text_dim } else { t.text_faint });
        ui.zone(r, Act::DispDrag(i), Cur::Grab);
    }
    ui.cv.pop_clip();
    y += 158;
    // выбор экрана кнопками (выключенные на схеме не видны)
    let n = d.outs.len() as i32;
    let bw = ((b.w - (n - 1) * 6) / n).min(180);
    for (i, o) in d.outs.iter().enumerate() {
        ui.button(R::new(b.x + i as i32 * (bw + 6), y, bw, 24), &o.name, if o.on { "screen" } else { "close" }, Tone::Ghost, d.sel == i, true, Act::DispSel(i));
    }
    y += 32;
    let Some(o) = d.outs.get(d.sel) else { return };
    let size = if o.mm.0 > 0 { format!(" · {}″", (((o.mm.0 * o.mm.0 + o.mm.1 * o.mm.1) as f64).sqrt() / 25.4).round()) } else { String::new() };
    ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w, 20), &format!("{}{}", o.desc, size), t.text_dim);
    y += 26;
    let half = (b.w - 12) / 2;
    let m = o.cur();
    crate::settings::opt_btn(ui, R::new(b.x, y, half, 26), "разрешение", &format!("{}×{}", m.w, m.h), Act::Disp("res"));
    crate::settings::opt_btn(ui, R::new(b.x + half + 12, y, half, 26), "частота", &hz(m.mhz), Act::Disp("hz"));
    y += 32;
    let crisp = (o.scale - o.scale.round()).abs() < 0.01;
    crate::settings::opt_btn(ui, R::new(b.x, y, half, 26), "масштаб", &format!("{}{}", o.scale, if crisp { "" } else { " ~" }), Act::Disp("scale"));
    let tr = if o.transform == "normal" { "нет".to_string() } else { format!("{}°", o.transform.trim_start_matches("flipped-")) };
    crate::settings::opt_btn(ui, R::new(b.x + half + 12, y, half, 26), "поворот", &tr, Act::Disp("transform"));
    y += 34;
    ui.toggle(R::new(b.x, y, half, 26), "включён", "", o.on, Act::Disp("on"));
    // бар — сразу, без «применить»: это настройка шелла, а не niri (#8)
    // по экранам, что включены в niri сейчас (выключенный бар не держит)
    let live = live_names(&d.outs);
    let bar = crate::bar::shown_on(&st.set.bar_outputs, &live, &o.name);
    if o.vrr_ok {
        let v = ["выкл", "всегда", "в играх"][o.vrr.min(2) as usize];
        crate::settings::opt_btn(ui, R::new(b.x + half + 12, y, half, 26), "VRR", v, Act::Disp("vrr"));
    } else {
        ui.text_in(PX, R::new(b.x + half + 38, y + 2, half - 26, 20), "VRR не поддерживается", t.text_faint);
    }
    y += 30;
    if o.live && live.len() > 1 {
        ui.toggle(R::new(b.x, y, half, 26), "бар на этом экране", "", bar, Act::Disp("bar"));
        y += 30;
    }
    // один монитор на двух кабелях: niri видит два экрана рядом
    let twin = o.on && d.outs.iter().any(|x| x.name != o.name && x.on && x.mm == o.mm && o.mm.0 > 0 && x.modes == o.modes);
    if twin {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "похоже, это тот же монитор на втором кабеле — выключи один", t.ember);
        y += 22;
    }
    ui.text_in(PX, R::new(b.x, y, b.w, 20), if crisp { "масштаб целый — пиксели чёткие" } else { "дробный масштаб размывает пиксельный шрифт" }, if crisp { t.text_faint } else { t.ember });
    // применить / откат
    let by = b.bottom() - 28;
    if let Some((_, at)) = &d.revert {
        let left = at.saturating_duration_since(Instant::now()).as_secs();
        ui.text_in(PX, R::new(b.x, by, b.w - 260, 28), &crate::tf!("оставить? откат через {} с", "keep it? reverting in {} s", "manter? revertendo em {} s", left), t.ember);
        ui.button(R::new(b.right() - 250, by, 120, 28), "вернуть", "reboot", Tone::Plain, false, true, Act::Disp("revert"));
        ui.button(R::new(b.right() - 120, by, 120, 28), "оставить", "check", Tone::Primary, false, true, Act::Disp("keep"));
    } else {
        ui.button(R::new(b.right() - 250, by, 120, 28), "сбросить", "reboot", Tone::Plain, false, d.edited, Act::Disp("reset"));
        ui.button(R::new(b.right() - 120, by, 120, 28), "применить", "check", Tone::Primary, false, d.edited, Act::Disp("apply"));
    }
}

fn live_names(outs: &[Out]) -> Vec<String> {
    outs.iter().filter(|o| o.live).map(|o| o.name.clone()).collect()
}

/// новый barOutputs после щелчка по экрану name (all — экраны сейчас); None — не меняется
fn bar_toggle(all: &[String], list: &[String], name: &str) -> Option<Vec<String>> {
    let mut on: Vec<String> = all.iter().filter(|n| crate::bar::shown_on(list, all, n)).cloned().collect();
    if on.iter().any(|n| n == name) {
        if on.len() == 1 {
            return None;
        }
        on.retain(|n| n != name);
    } else {
        on.push(name.to_string());
    }
    Some(if all.iter().all(|n| on.contains(n)) { vec![] } else { on })
}

impl App {
    /// бар на экране i: вкл/выкл. Список хранит экраны с баром; все — пустой список.
    /// Последний бар не убирается: без экранов из списка он и так вернулся бы на все
    fn disp_bar(&mut self, i: usize) {
        let all = live_names(&self.st.disp.outs);
        let Some(name) = self.st.disp.outs.get(i).map(|o| o.name.clone()).filter(|n| all.contains(n)) else { return };
        if let Some(l) = bar_toggle(&all, &self.st.set.bar_outputs, &name) {
            self.st.set.bar_outputs = l;
            self.settings_changed();
        }
    }

    pub fn disp_open(&mut self) {
        if self.st.disp.revert.is_none() {
            self.st.disp.outs = load();
            self.st.disp.edited = false;
            self.st.disp.sel = self.st.disp.sel.min(self.st.disp.outs.len().saturating_sub(1));
        }
    }

    pub fn disp_click(&mut self, a: &Act) {
        let d = &mut self.st.disp;
        let n_on = d.outs.iter().filter(|x| x.on).count();
        match a {
            Act::DispSel(i) | Act::DispDrag(i) => d.sel = *i,
            Act::Disp(op) => {
                let i = d.sel;
                if let Some(o) = d.outs.get_mut(i) {
                    let m = o.cur();
                    match *op {
                        "res" => {
                            let mut rs: Vec<(i32, i32)> = vec![];
                            for x in &o.modes {
                                if !rs.contains(&(x.w, x.h)) {
                                    rs.push((x.w, x.h));
                                }
                            }
                            rs.sort_by_key(|r| -(r.0 * r.1));
                            let j = rs.iter().position(|r| *r == (m.w, m.h)).map(|j| (j + 1) % rs.len()).unwrap_or(0);
                            let (w, h) = rs[j];
                            // та же частота, если есть, иначе самая высокая
                            let same = o.modes.iter().position(|x| x.w == w && x.h == h && (x.mhz - m.mhz).abs() < 500);
                            let best = o.modes.iter().enumerate().filter(|(_, x)| x.w == w && x.h == h).max_by_key(|(_, x)| x.mhz).map(|(j, _)| j);
                            o.mode = same.or(best).unwrap_or(o.mode);
                        }
                        "hz" => {
                            let mut hs: Vec<(usize, i32)> = o.modes.iter().enumerate().filter(|(_, x)| x.w == m.w && x.h == m.h).map(|(j, x)| (j, x.mhz)).collect();
                            hs.sort_by_key(|h| -h.1);
                            let j = hs.iter().position(|h| h.0 == o.mode).map(|j| (j + 1) % hs.len()).unwrap_or(0);
                            o.mode = hs.get(j).map(|h| h.0).unwrap_or(o.mode);
                        }
                        "scale" => {
                            let j = SCALES.iter().position(|s| (s - o.scale).abs() < 0.01).map(|j| (j + 1) % SCALES.len()).unwrap_or(0);
                            o.scale = SCALES[j];
                        }
                        "transform" => {
                            let j = TRANSFORMS.iter().position(|s| *s == o.transform).map(|j| (j + 1) % TRANSFORMS.len()).unwrap_or(0);
                            o.transform = TRANSFORMS[j].into();
                        }
                        "vrr" if o.vrr_ok => o.vrr = (o.vrr + 1) % 3,
                        // последний экран не выключаем
                        "on" if !o.on || n_on > 1 => o.on = !o.on,
                        _ => {}
                    }
                }
                match *op {
                    "res" | "hz" | "scale" | "transform" | "vrr" | "on" => {
                        d.edited = true;
                        self.disp_unoverlap();
                    }
                    "apply" => self.disp_apply(),
                    "keep" => {
                        d.revert = None;
                        // «авто»-шрифт подстраивается под новый экран
                        if self.st.set.font_size == 0 {
                            self.apply_fonts();
                        }
                        let d = &mut self.st.disp;
                        d.reload_at = Some(Instant::now() + Duration::from_millis(600));
                    }
                    "revert" => self.disp_revert(),
                    "reset" => self.disp_open(),
                    "bar" => self.disp_bar(i),
                    _ => {}
                }
            }
            _ => {}
        }
        self.dirty(&SKind::Modal);
    }

    /// тащим экран по схеме: шаг экрана — dx/k, края липнут к соседям
    pub fn disp_drag(&mut self, i: usize, dx: f64, dy: f64) {
        let d = &mut self.st.disp;
        let Some(o) = d.outs.get(i) else { return };
        let (sx, sy) = *d.drag.get_or_insert((o.x, o.y));
        let (k, _, _) = d.view.get();
        let (w, h) = o.size();
        let mut nx = sx + (dx / k) as i32;
        let mut ny = sy + (dy / k) as i32;
        let near = (10.0 / k) as i32;
        for (j, b) in d.outs.iter().enumerate() {
            if j == i || !b.on {
                continue;
            }
            let (bw, bh) = b.size();
            for c in [b.x - w, b.x + bw, b.x, b.x + bw - w] {
                if (nx - c).abs() < near {
                    nx = c;
                }
            }
            for c in [b.y - h, b.y + bh, b.y, b.y + bh - h] {
                if (ny - c).abs() < near {
                    ny = c;
                }
            }
        }
        d.sel = i;
        d.outs[i].x = nx;
        d.outs[i].y = ny;
        d.edited = true;
        self.dirty(&SKind::Modal);
    }

    pub fn disp_drag_end(&mut self) {
        self.st.disp.drag = None;
        self.disp_unoverlap();
        self.dirty(&SKind::Modal);
    }

    /// сменился размер — соседи справа/снизу не должны наезжать
    fn disp_unoverlap(&mut self) {
        let outs = &mut self.st.disp.outs;
        for _ in 0..outs.len() {
            let mut moved = false;
            for a in 0..outs.len() {
                for b in 0..outs.len() {
                    if a == b || !outs[a].on || !outs[b].on {
                        continue;
                    }
                    let (aw, ah) = outs[a].size();
                    let (bw, bh) = outs[b].size();
                    let (ax, ay, bx, by) = (outs[a].x, outs[a].y, outs[b].x, outs[b].y);
                    if ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah && (bx, by) >= (ax, ay) {
                        // сдвигаем b в ту сторону, где перекрытие меньше
                        if ax + aw - bx <= ay + ah - by {
                            outs[b].x = ax + aw;
                        } else {
                            outs[b].y = ay + ah;
                        }
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
    }

    fn disp_apply(&mut self) {
        let p = path();
        let prev = std::fs::read_to_string(&p).ok();
        let _ = crate::config::write_atomic(&p, &kdl(&self.st.disp.outs));
        crate::login::publish(&self.st.set);
        self.st.disp.edited = false;
        self.st.disp.revert = Some((prev, Instant::now() + Duration::from_secs(REVERT_S)));
    }

    fn disp_revert(&mut self) {
        let Some((prev, _)) = self.st.disp.revert.take() else { return };
        let p = path();
        match prev {
            Some(t) => {
                let _ = crate::config::write_atomic(&p, &t);
            }
            None => {
                let _ = std::fs::remove_file(&p);
            }
        }
        crate::login::publish(&self.st.set);
        self.st.disp.reload_at = Some(Instant::now() + Duration::from_millis(800));
    }

    /// раз в тик: отсчёт отката и перечитывание после применения
    pub fn disp_tick(&mut self) {
        if let Some((_, at)) = &self.st.disp.revert {
            if Instant::now() >= *at {
                self.disp_revert();
            }
            self.dirty(&SKind::Modal);
        }
        if self.st.disp.reload_at.map(|t| Instant::now() >= t).unwrap_or(false) {
            self.st.disp.reload_at = None;
            self.disp_open();
            self.dirty(&SKind::Modal);
        }
    }
}

/// два экрана для `yami-shell shot settings:screens`
pub fn mock() -> Vec<Out> {
    let m = |w, h, mhz, pref| Mode { w, h, mhz, pref };
    vec![
        Out { name: "DP-1".into(), desc: "Monitor 27 QHD".into(), mm: (530, 300), modes: vec![m(2560, 1440, 165000, true), m(2560, 1440, 60000, false), m(1920, 1080, 144001, false)], mode: 0, on: true, live: true, x: 0, y: 0, scale: 1.0, transform: "normal".into(), vrr_ok: true, vrr: 2 },
        Out { name: "HDMI-A-1".into(), desc: "Dell P2419H".into(), mm: (527, 296), modes: vec![m(1920, 1080, 60000, true)], mode: 0, on: true, live: true, x: 2560, y: 180, scale: 1.25, transform: "normal".into(), vrr_ok: false, vrr: 0 },
    ]
}

#[cfg(test)]
mod tests {
    use super::bar_toggle;

    #[test]
    fn bar_screens() {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let all = v(&["DP-1", "HDMI-A-1"]);
        // всё включено (пусто) → убрать с HDMI: только DP-1
        assert_eq!(bar_toggle(&all, &[], "HDMI-A-1"), Some(v(&["DP-1"])));
        // последний бар не убирается
        assert_eq!(bar_toggle(&all, &v(&["DP-1"]), "DP-1"), None);
        // вернуть на HDMI: снова все — пустой список
        assert_eq!(bar_toggle(&all, &v(&["DP-1"]), "HDMI-A-1"), Some(vec![]));
        // в списке только чужой экран (ноутбук без монитора): считается «все»
        assert_eq!(bar_toggle(&all, &v(&["eDP-1"]), "DP-1"), Some(v(&["HDMI-A-1"])));
    }
}
