// Скриншоты: экран замораживается (grim), поверх — оверлей на каждом мониторе в пиксельном
// стиле шелла: вуаль со строками развёртки, выделение «бегущими муравьями» с ручками, лупа
// с пиксельной сеткой и пипеткой, панель инструментов (перо, рамка, стрелка, маркер,
// мозаика, номерки), палитра из темы. Enter — готово: в буфер и/или в папку, уведомление
// с миниатюрой. Режимы: область, окно (по раскладке niri), экран. Таймер — 3/5/10 с.
//
//   yami-shell msg screenshot [area|window|screen]   (Print, Alt+Print, Ctrl+Print)

pub use crate::snapdraw::{draw, paint};
use crate::app::{App, Msg, SKind};
use crate::gfx::{Canvas, Img, C, R};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Area,
    Window,
    Screen,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Pen,
    Rect,
    Arrow,
    Marker,
    Mosaic,
    Count,
    /// пипетка: щелчок копирует цвет точки как #rrggbb
    Pick,
}

pub const TOOLS: &[(Tool, &str, &str, &str)] = &[
    (Tool::Select, "cursor", "t:select", "выделение"),
    (Tool::Pen, "pen", "t:pen", "перо"),
    (Tool::Rect, "rect", "t:rect", "рамка"),
    (Tool::Arrow, "arrow", "t:arrow", "стрелка"),
    (Tool::Marker, "marker", "t:marker", "маркер"),
    (Tool::Mosaic, "mosaic", "t:mosaic", "мозаика"),
    (Tool::Count, "count", "t:count", "номерок"),
    (Tool::Pick, "drop", "t:pick", "пипетка"),
];
pub(crate) const COLOR_ACTS: [&str; 6] = ["c:0", "c:1", "c:2", "c:3", "c:4", "c:5"];

#[derive(Clone, Debug)]
pub struct Mark {
    pub tool: Tool,
    pub pts: Vec<(i32, i32)>,
    pub color: C,
    pub size: i32,
    pub n: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Drag {
    New(i32, i32),
    Move(i32, i32, R),
    /// ручка: 0..8 по часовой с левого верхнего угла, исходный прямоугольник
    Handle(u8, R),
    Draw,
}

pub struct Snap {
    pub imgs: HashMap<String, Rc<Img>>,
    pub mode: Mode,
    pub sel: Option<(String, R)>,
    pub(crate) drag: Option<Drag>,
    pub tool: Tool,
    pub color: usize,
    pub size: i32,
    pub marks: Vec<Mark>,
    pub redo: Vec<Mark>,
    pub ptr: Option<(String, i32, i32)>,
    pub flash: Option<Instant>,
    /// окна на видимых рабочих столах: монитор, рамка, заголовок
    pub wins: Vec<(String, R, String)>,
    pub busy: bool,
    pub hint: Option<(String, Instant)>,
    /// есть tesseract / zbarimg: кнопки «текст» и «QR» в панели
    pub ocr: bool,
    pub qr: bool,
}

impl Snap {
    pub fn colors(t: &crate::theme::Theme) -> [C; 6] {
        [t.red, C::hex(0xffffff), C::hex(0x0b0909), t.ok, t.warn, C::hex(0x3a9fd0)]
    }
    pub(crate) fn cur_sel(&self, out: &str) -> Option<R> {
        self.sel.as_ref().filter(|s| s.0 == out).map(|s| s.1)
    }
}

impl Snap {
    /// для `yami-shell shot`: «рабочий стол» из полос, выделение и пара меток
    /// снимок для `shot`: шахматка с «окном» или настоящий кадр (`shot --frame`, картинки README);
    /// разметка задана для 1280×720 и масштабируется под кадр
    pub fn mock(w: i32, h: i32, edit: bool, t: &crate::theme::Theme, frame: Option<Vec<u32>>) -> Snap {
        let px = frame.unwrap_or_else(|| {
            let mut px = vec![0u32; (w * h) as usize];
            for y in 0..h {
                for x in 0..w {
                    let c = if y < 32 { t.panel } else if (x / 160 + y / 120) % 2 == 0 { t.surface_hi } else { t.bg.lerp(t.red, 0.12) };
                    px[(y * w + x) as usize] = c.0 | 0xff000000;
                }
            }
            for x in 200..1000 {
                for y in 160..520 {
                    px[(y * w + x) as usize] = if y < 190 { t.blood.0 } else { (t.code_bg.0 & 0xffffff) | 0xff000000 } | 0xff000000;
                }
            }
            px
        });
        let k = |v: i32| v * w / 1280;
        let p = |x: i32, y: i32| (k(x), k(y));
        let mut imgs = HashMap::new();
        imgs.insert(String::new(), Rc::new(Img { w, h, px }));
        let colors = Snap::colors(t);
        let marks = if edit {
            vec![
                Mark { tool: Tool::Rect, pts: vec![p(330, 250), p(620, 380)], color: colors[0], size: 4, n: 0 },
                Mark { tool: Tool::Arrow, pts: vec![p(820, 470), p(640, 390)], color: colors[1], size: 4, n: 0 },
                Mark { tool: Tool::Marker, pts: vec![p(260, 420), p(560, 440)], color: colors[4], size: 14, n: 0 },
                Mark { tool: Tool::Count, pts: vec![p(700, 260)], color: colors[0], size: 4, n: 1 },
                Mark { tool: Tool::Mosaic, pts: vec![p(760, 230), p(900, 330)], color: colors[0], size: 4, n: 0 },
            ]
        } else {
            vec![]
        };
        Snap {
            imgs,
            mode: Mode::Area,
            sel: if edit { Some((String::new(), R::new(k(200), k(160), k(800), k(360)))) } else { None },
            drag: None,
            tool: if edit { Tool::Arrow } else { Tool::Select },
            color: 0,
            size: 4,
            marks,
            redo: vec![],
            ptr: Some((String::new(), k(if edit { 1100 } else { 640 }), k(if edit { 600 } else { 400 }))),
            flash: None,
            wins: vec![],
            busy: false,
            hint: None,
            ocr: true,
            qr: true,
        }
    }
}

/// grim -t ppm: P6, ширина, высота, 255, RGB
pub(crate) fn parse_ppm(b: &[u8]) -> Option<Img> {
    let mut fields = vec![];
    let mut i = 0;
    while fields.len() < 4 && i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) == Some(&b'#') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let s = i;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(String::from_utf8_lossy(&b[s..i]).to_string());
    }
    i += 1;
    if fields.first().map(|f| f != "P6").unwrap_or(true) {
        return None;
    }
    let w: i32 = fields.get(1)?.parse().ok()?;
    let h: i32 = fields.get(2)?.parse().ok()?;
    let data = b.get(i..i + (w * h * 3) as usize)?;
    let px = data.chunks_exact(3).map(|p| 0xff000000 | (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
    Some(Img { w, h, px })
}

pub fn grab(out: &str) -> Option<Img> {
    let o = std::process::Command::new("grim").args(["-o", out, "-t", "ppm", "-"]).output().ok()?;
    if !o.status.success() {
        return None;
    }
    parse_ppm(&o.stdout)
}

/// окна видимых рабочих столов в координатах их мониторов
pub(crate) fn niri_windows() -> Vec<(String, R, String)> {
    let run = |what: &str| -> serde_json::Value {
        std::process::Command::new("niri").args(["msg", "-j", what]).output().ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(serde_json::Value::Null)
    };
    let ws = run("workspaces");
    let wins = run("windows");
    let mut active: HashMap<u64, String> = HashMap::new();
    for w in ws.as_array().into_iter().flatten() {
        if w["is_active"].as_bool().unwrap_or(false) {
            if let (Some(id), Some(out)) = (w["id"].as_u64(), w["output"].as_str()) {
                active.insert(id, out.to_string());
            }
        }
    }
    let mut v = vec![];
    for w in wins.as_array().into_iter().flatten() {
        let Some(out) = w["workspace_id"].as_u64().and_then(|i| active.get(&i)) else { continue };
        let l = &w["layout"];
        let (Some(px), Some(py)) = (l["tile_pos_in_workspace_view"][0].as_f64(), l["tile_pos_in_workspace_view"][1].as_f64()) else { continue };
        let ox = l["window_offset_in_tile"][0].as_f64().unwrap_or(0.0);
        let oy = l["window_offset_in_tile"][1].as_f64().unwrap_or(0.0);
        let ww = l["window_size"][0].as_f64().unwrap_or(0.0);
        let wh = l["window_size"][1].as_f64().unwrap_or(0.0);
        let r = R::new((px + ox).round() as i32, (py + oy).round() as i32, ww as i32, wh as i32);
        // сверху — фокусное и плавающие
        let title = w["title"].as_str().unwrap_or("").to_string();
        if w["is_floating"].as_bool().unwrap_or(false) || w["is_focused"].as_bool().unwrap_or(false) {
            v.insert(0, (out.clone(), r, title));
        } else {
            v.push((out.clone(), r, title));
        }
    }
    v
}

pub(crate) fn norm(x0: i32, y0: i32, x1: i32, y1: i32) -> R {
    R::new(x0.min(x1), y0.min(y1), (x1 - x0).abs() + 1, (y1 - y0).abs() + 1)
}

pub(crate) fn handles(r: R) -> [R; 8] {
    let s = 8;
    let (x0, x1, xm) = (r.x - s / 2, r.right() - s / 2, r.x + r.w / 2 - s / 2);
    let (y0, y1, ym) = (r.y - s / 2, r.bottom() - s / 2, r.y + r.h / 2 - s / 2);
    [R::new(x0, y0, s, s), R::new(xm, y0, s, s), R::new(x1, y0, s, s), R::new(x1, ym, s, s), R::new(x1, y1, s, s), R::new(xm, y1, s, s), R::new(x0, y1, s, s), R::new(x0, ym, s, s)]
}

// ── поведение ──────────────────────────────────────────────────────────────
pub fn shots_dir(set: &crate::config::Settings) -> PathBuf {
    let d = set.shot_dir.trim();
    if !d.is_empty() {
        return yami_px::paths::expand(d);
    }
    // XDG_PICTURES_DIR/Screenshots
    yami_px::paths::pictures().join("Screenshots")
}

pub(crate) fn copy_text(s: &str) {
    use std::io::Write;
    if let Ok(mut ch) = std::process::Command::new("wl-copy").stdin(std::process::Stdio::piped()).spawn() {
        if let Some(i) = ch.stdin.as_mut() {
            let _ = i.write_all(s.as_bytes());
        }
        drop(ch.stdin.take());
        let _ = ch.wait();
    }
}

/// вспышка затвора и наибольшее ожидание, пока курсор уйдёт из угла (мс)
pub const FLASH: u128 = 280;
const HOLD: u128 = 4000;

/// курсор у угла экрана (горячий угол niri — пиксель в углу; берём с запасом)
fn in_corner(x: i32, y: i32, w: i32, h: i32) -> bool {
    let near = |v: i32, max: i32| v <= 2 || v >= max - 3;
    near(x, w) && near(y, h)
}

pub(crate) fn file_name() -> String {
    let tm = crate::clock::local_now();
    format!("yami-{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.png", tm.year, tm.mon, tm.day, tm.hour, tm.min, tm.sec)
}

impl App {
    /// начать: таймер (если есть) → снимок всех мониторов в нити → оверлей
    pub fn snap_start(&mut self, mode: Mode, delay: u32) {
        if self.st.locked || self.st.snap.is_some() {
            return;
        }
        self.close_all();
        let outs = self.output_names();
        let tx = self.tx.clone();
        if delay > 0 {
            self.st.snap_countdown = Some((Instant::now(), delay));
            self.show_osd("snap");
        }
        std::thread::spawn(move || {
            if delay > 0 {
                std::thread::sleep(Duration::from_secs(delay as u64));
            } else {
                // меню и всплывашки успевают исчезнуть
                std::thread::sleep(Duration::from_millis(60));
            }
            let imgs: HashMap<String, Img> = outs.iter().filter_map(|o| grab(o).map(|i| (o.clone(), i))).collect();
            let wins = niri_windows();
            let _ = tx.send(Msg::Snap(imgs, wins, mode));
        });
    }

    pub fn on_snap_ready(&mut self, imgs: HashMap<String, Img>, wins: Vec<(String, R, String)>, mode: Mode) {
        self.st.snap_countdown = None;
        if imgs.is_empty() {
            self.toast_note("скриншот не вышел", "нужен grim (пакет grim)");
            return;
        }
        let outs: Vec<String> = imgs.keys().cloned().collect();
        let mut sn = Snap { imgs: imgs.into_iter().map(|(k, v)| (k, Rc::new(v))).collect(), mode, sel: None, drag: None, tool: Tool::Select, color: 0, size: 4, marks: vec![], redo: vec![], ptr: None, flash: None, wins, busy: false, hint: None, ocr: crate::doctor::which("tesseract"), qr: crate::doctor::which("zbarimg") };
        let focus = self.focus_out();
        if mode == Mode::Screen {
            let (w, h) = self.output_size(&focus);
            sn.sel = Some((focus.clone(), R::new(0, 0, w, h)));
        }
        self.st.snap = Some(sn);
        // мгновенный режим: снимок экрана — без оверлея, сразу в буфер и/или в папку
        if mode == Mode::Screen && self.st.set.shot_confirm == "instant" {
            self.snap_done();
            self.snap_close();
            return;
        }
        for o in outs {
            let kb = if o == focus { KeyboardInteractivity::Exclusive } else { KeyboardInteractivity::None };
            self.mk(SKind::Snap, &o, Layer::Overlay, "yami-screenshot", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, kb);
        }
    }

    pub fn snap_close(&mut self) {
        self.st.snap = None;
        for id in self.find(&SKind::Snap) {
            self.destroy(id);
        }
    }

    fn snap_hint(&mut self, s: &str) {
        if let Some(sn) = &mut self.st.snap {
            sn.hint = Some((s.to_string(), Instant::now()));
        }
    }

    pub fn toast_note(&mut self, summary: &str, body: &str) {
        let n = crate::dbus::Incoming { id: 9_000_000 + (self.st.started.elapsed().as_millis() % 1_000_000) as u32, app: "yami".into(), icon: String::new(), summary: crate::i18n::tr(summary).into(), body: crate::i18n::tr(body).into(), actions: vec![], urgency: 1, image: None, timeout: -1 };
        self.on_notif(n);
    }

    /// мышь над оверлеем; true — событие наше
    pub fn snap_pointer(&mut self, sid: u32, kind: u8, x: f64, y: f64, button: u32, on_btn: bool) -> bool {
        let Some(out) = self.surfs.get(&sid).map(|s| s.output.clone()) else { return false };
        let (w, h) = self.surfs.get(&sid).map(|s| (s.w, s.h)).unwrap_or((0, 0));
        let colors = Snap::colors(&self.st.t);
        let Some(sn) = &mut self.st.snap else { return false };
        let (xi, yi) = ((x as i32).clamp(0, w - 1), (y as i32).clamp(0, h - 1));
        if sn.flash.is_some() || sn.busy {
            // снимок сделан: курсор нужен только snap_tick — ушёл ли он из угла
            sn.ptr = Some((out, xi, yi));
            return true;
        }
        sn.ptr = Some((out.clone(), xi, yi));
        self.dirty(&SKind::Snap);
        let Some(sn) = &mut self.st.snap else { return false };
        match kind {
            // движение
            0 => {
                let Some(d) = sn.drag else { return false };
                match d {
                    Drag::New(x0, y0) => sn.sel = Some((out, norm(x0, y0, xi, yi))),
                    Drag::Move(dx, dy, r0) => {
                        let nx = (xi - dx).clamp(0, w - r0.w);
                        let ny = (yi - dy).clamp(0, h - r0.h);
                        sn.sel = Some((out, R::new(nx, ny, r0.w, r0.h)));
                    }
                    Drag::Handle(k, r0) => {
                        let (mut x0, mut y0, mut x1, mut y1) = (r0.x, r0.y, r0.right() - 1, r0.bottom() - 1);
                        match k {
                            0 => (x0, y0) = (xi, yi),
                            1 => y0 = yi,
                            2 => (x1, y0) = (xi, yi),
                            3 => x1 = xi,
                            4 => (x1, y1) = (xi, yi),
                            5 => y1 = yi,
                            6 => (x0, y1) = (xi, yi),
                            _ => x0 = xi,
                        }
                        sn.sel = Some((out, norm(x0, y0, x1, y1)));
                    }
                    Drag::Draw => {
                        if let Some(m) = sn.marks.last_mut() {
                            if m.tool == Tool::Pen {
                                if m.pts.last() != Some(&(xi, yi)) {
                                    m.pts.push((xi, yi));
                                }
                            } else if m.pts.len() >= 2 {
                                m.pts[1] = (xi, yi);
                            } else {
                                m.pts.push((xi, yi));
                            }
                        }
                    }
                }
                true
            }
            // нажатие
            1 => {
                if on_btn {
                    return false;
                }
                if button == 0x111 {
                    // ПКМ: снять выделение / выйти
                    if sn.sel.is_some() && sn.mode != Mode::Screen {
                        sn.sel = None;
                        sn.marks.clear();
                    } else {
                        self.snap_close();
                    }
                    return true;
                }
                let sel = sn.cur_sel(&out);
                if sn.tool == Tool::Pick {
                    // цвет точки снимка: снимок в физических пикселях, масштаб бывает дробным (1.25)
                    if let Some(img) = sn.imgs.get(&out) {
                        let px = ((x * img.w as f64 / w.max(1) as f64) as i32).clamp(0, img.w - 1);
                        let py = ((y * img.h as f64 / h.max(1) as f64) as i32).clamp(0, img.h - 1);
                        let c = img.px[(py * img.w + px) as usize] & 0xffffff;
                        copy_text(&format!("#{c:06x}"));
                        self.snap_hint(&crate::tf!("цвет #{:06x} скопирован", "colour #{:06x} copied", "cor #{:06x} copiada", c));
                    }
                    return true;
                }
                if let Some(r) = sel {
                    if let Some(k) = handles(r).iter().position(|hr| hr.inset(-3).contains(x, y)) {
                        sn.drag = Some(Drag::Handle(k as u8, r));
                        return true;
                    }
                    if r.contains(x, y) {
                        if sn.tool == Tool::Select {
                            sn.drag = Some(Drag::Move(xi - r.x, yi - r.y, r));
                        } else {
                            let n = sn.marks.iter().filter(|m| m.tool == Tool::Count).count() as u32 + 1;
                            sn.marks.push(Mark { tool: sn.tool, pts: vec![(xi, yi)], color: colors[sn.color], size: if sn.tool == Tool::Marker { 14 } else { sn.size }, n });
                            sn.redo.clear();
                            sn.drag = if sn.tool == Tool::Count { None } else { Some(Drag::Draw) };
                        }
                        return true;
                    }
                }
                match sn.mode {
                    Mode::Screen => {
                        sn.sel = Some((out.clone(), R::new(0, 0, w, h)));
                        sn.marks.clear();
                    }
                    _ => {
                        sn.marks.clear();
                        sn.sel = None;
                        sn.drag = Some(Drag::New(xi, yi));
                    }
                }
                true
            }
            // отпускание
            _ => {
                let Some(d) = sn.drag.take() else { return false };
                let picked = matches!(d, Drag::New(..));
                if let Drag::New(x0, y0) = d {
                    // щелчок без протяжки: окно под курсором (в любом режиме), иначе — экран
                    if (x0 - xi).abs() < 4 && (y0 - yi).abs() < 4 {
                        let win = sn.wins.iter().find(|(o, r, _)| *o == out && r.contains(x, y)).map(|(_, r, _)| r.inter(&R::new(0, 0, w, h)));
                        sn.sel = match (sn.mode, win) {
                            (Mode::Window, Some(r)) | (Mode::Area, Some(r)) => Some((out, r)),
                            _ => Some((out, R::new(0, 0, w, h))),
                        };
                    }
                }
                if let Drag::Draw = d {
                    // нулевая рамка/стрелка — убрать
                    if let Some(m) = sn.marks.last() {
                        if m.tool != Tool::Pen && m.tool != Tool::Count && m.pts.len() < 2 {
                            sn.marks.pop();
                        }
                    }
                }
                // мгновенный режим: выделил (протянул или щёлкнул по окну) — снимок готов
                if picked && sn.sel.is_some() && self.st.set.shot_confirm == "instant" {
                    self.snap_done();
                }
                true
            }
        }
    }

    pub fn snap_wheel(&mut self, d: i32) {
        if let Some(sn) = &mut self.st.snap {
            let sizes = [2, 4, 8];
            let i = sizes.iter().position(|&s| s == sn.size).unwrap_or(1) as i32;
            sn.size = sizes[(i - d).clamp(0, 2) as usize];
            self.dirty(&SKind::Snap);
        }
    }

    pub fn snap_btn(&mut self, a: &str) {
        let Some(sn) = &mut self.st.snap else { return };
        if let Some(t) = TOOLS.iter().find(|t| t.2 == a) {
            sn.tool = t.0;
        } else if let Some(c) = a.strip_prefix("c:") {
            sn.color = c.parse().unwrap_or(0);
            if sn.tool == Tool::Select || sn.tool == Tool::Mosaic {
                sn.tool = Tool::Pen;
            }
        } else if let Some(s) = a.strip_prefix("s:") {
            sn.size = s.parse().unwrap_or(4);
        } else {
            match a {
                "m:area" => sn.mode = Mode::Area,
                "m:window" => sn.mode = Mode::Window,
                "m:screen" => sn.mode = Mode::Screen,
                "undo" => {
                    if let Some(m) = sn.marks.pop() {
                        sn.redo.push(m);
                    }
                }
                "delay" => {
                    let d = match self.st.set.shot_delay {
                        0 => 3,
                        3 => 5,
                        5 => 10,
                        _ => 3,
                    };
                    self.st.set.shot_delay = d;
                    self.settings_changed();
                    let mode = self.st.snap.as_ref().map(|s| s.mode).unwrap_or(Mode::Area);
                    self.snap_close();
                    self.snap_start(mode, d);
                    return;
                }
                "copy" => self.snap_finish(true, false, false),
                "save" => {
                    self.snap_done();
                }
                "saveas" => self.snap_finish(false, false, true),
                "ocr" | "qr" => self.snap_read(a == "qr"),
                "close" => self.snap_close(),
                _ => {}
            }
        }
        self.dirty(&SKind::Snap);
    }

    pub fn snap_key(&mut self, e: &KeyEvent) {
        let (ctrl, shift) = (self.mods.ctrl, self.mods.shift);
        let ptr_w = self.st.snap.as_ref().and_then(|s| s.ptr.as_ref()).map(|p| self.output_size(&p.0).0).unwrap_or(1);
        let Some(sn) = &mut self.st.snap else { return };
        // снимок уже сделан (вспышка) или ждёт диалога: второй пробел, enter или автоповтор
        // клавиши не делают второй снимок (#13)
        if sn.flash.is_some() || sn.busy {
            return;
        }
        let raw = e.raw_code;
        match e.keysym {
            Keysym::Escape => {
                if sn.sel.is_some() && sn.mode != Mode::Screen && !sn.marks.is_empty() {
                    sn.marks.clear();
                } else if sn.sel.is_some() && sn.mode == Mode::Area {
                    sn.sel = None;
                } else {
                    self.snap_close();
                    return;
                }
            }
            Keysym::Return | Keysym::KP_Enter | Keysym::space => {
                if sn.sel.is_none() {
                    // окно под курсором или весь монитор
                    let p = sn.ptr.clone();
                    let out = p.as_ref().map(|p| p.0.clone()).unwrap_or_else(|| sn.imgs.keys().next().cloned().unwrap_or_default());
                    let win = p.as_ref().and_then(|(o, x, y)| sn.wins.iter().find(|(wo, r, _)| wo == o && r.contains(*x as f64, *y as f64))).map(|w| w.1);
                    let full = sn.imgs.get(&out).map(|i| R::new(0, 0, i.w, i.h)).unwrap_or_default();
                    sn.sel = Some((out, if sn.mode == Mode::Window { win.unwrap_or(full) } else { full }));
                    if e.keysym == Keysym::space {
                        self.dirty(&SKind::Snap);
                        return;
                    }
                }
                self.snap_done();
                return;
            }
            Keysym::Left | Keysym::Right | Keysym::Up | Keysym::Down => {
                if let Some((_, r)) = &mut sn.sel {
                    let d = if shift { 10 } else { 1 };
                    let (dx, dy) = match e.keysym {
                        Keysym::Left => (-d, 0),
                        Keysym::Right => (d, 0),
                        Keysym::Up => (0, -d),
                        _ => (0, d),
                    };
                    if ctrl {
                        r.w = (r.w + dx).max(4);
                        r.h = (r.h + dy).max(4);
                    } else {
                        r.x += dx;
                        r.y += dy;
                    }
                }
            }
            Keysym::Tab => {
                sn.mode = match sn.mode {
                    Mode::Area => Mode::Window,
                    Mode::Window => Mode::Screen,
                    Mode::Screen => Mode::Area,
                };
                let m = sn.mode;
                self.snap_hint(match m {
                    Mode::Area => "область",
                    Mode::Window => "окно",
                    Mode::Screen => "экран",
                });
            }
            _ if ctrl => match raw {
                44 => {
                    // ctrl+z / ctrl+shift+z
                    if shift {
                        if let Some(m) = sn.redo.pop() {
                            sn.marks.push(m);
                        }
                    } else if let Some(m) = sn.marks.pop() {
                        sn.redo.push(m);
                    }
                }
                21 => {
                    if let Some(m) = sn.redo.pop() {
                        sn.marks.push(m);
                    }
                }
                46 => {
                    if sn.sel.is_some() {
                        self.snap_finish(true, false, false);
                        return;
                    }
                    // без выделения — цвет под курсором
                    if let Some((o, x, y)) = sn.ptr.clone() {
                        if let Some(img) = sn.imgs.get(&o) {
                            let k = img.w as f32 / ptr_w.max(1) as f32;
                            let (cx, cy) = ((x as f32 * k) as i32, (y as f32 * k) as i32);
                            let c = img.px[(cy.clamp(0, img.h - 1) * img.w + cx.clamp(0, img.w - 1)) as usize] & 0xffffff;
                            copy_text(&format!("#{c:06x}"));
                            self.snap_hint(&crate::tf!("цвет #{:06x} скопирован", "colour #{:06x} copied", "cor #{:06x} copiada", c));
                        }
                    }
                }
                31 => {
                    self.snap_finish(false, false, true);
                    return;
                }
                _ => {}
            },
            _ => {
                // 1–8: инструменты
                if (2..=(TOOLS.len() as u32 + 1)).contains(&raw) {
                    sn.tool = TOOLS[(raw - 2) as usize].0;
                } else if raw == 33 && sn.sel.is_none() {
                    // f — весь монитор под курсором
                    sn.mode = Mode::Screen;
                } else if raw == 17 {
                    sn.mode = Mode::Window;
                }
            }
        }
        self.dirty(&SKind::Snap);
    }

    /// собрать картинку: вырезать, нанести метки, закодировать в PNG
    fn snap_render(&mut self) -> Option<(Vec<u8>, Img)> {
        use image::ImageEncoder as _;
        let sn = self.st.snap.as_ref()?;
        let (out, r) = sn.sel.clone()?;
        let img = sn.imgs.get(&out)?.clone();
        // снимок — в физических пикселях, выделение — в точках экрана; масштаб бывает дробным
        // (1.25, 1.5): целая часть отношения вырезала бы не тот кусок
        let (ow, _) = self.output_size(&out);
        let k = img.w as f32 / ow.max(1) as f32;
        let px = |v: i32| (v as f32 * k).round() as i32;
        let (x0, y0) = (px(r.x).clamp(0, img.w), px(r.y).clamp(0, img.h));
        let (x1, y1) = (px(r.right()).clamp(0, img.w), px(r.bottom()).clamp(0, img.h));
        let (w, h) = (x1 - x0, y1 - y0);
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut buf = vec![0u32; (w * h) as usize];
        {
            let mut cv = Canvas::new(&mut buf, w, h);
            cv.blit(&img, -x0, -y0);
            for m in &sn.marks {
                paint(&mut cv, &mut self.res.fonts, m, &img, (r.x, r.y), k, k, self.st.t.on_accent);
            }
        }
        let mut rgba = Vec::with_capacity(buf.len() * 4);
        for p in &buf {
            rgba.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]);
        }
        let mut png = vec![];
        image::codecs::png::PngEncoder::new(&mut png).write_image(&rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8).ok()?;
        // миниатюра для уведомления
        let thumb = image::RgbaImage::from_raw(w as u32, h as u32, rgba).map(|i| crate::icons::fit_in(i, crate::notif::IMG_W, crate::notif::IMG_H));
        Some((png, thumb?))
    }

    /// распознать выделенное: текст (ocr.rs: подготовка картинки и три прогона tesseract) или QR
    /// (zbarimg); результат — в буфер обмена и уведомлением. Оверлей закрывается сразу, работа — в нити.
    pub fn snap_read(&mut self, qr: bool) {
        let Some((png, _)) = self.snap_render() else { return };
        self.snap_close();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            if !qr {
                let text = image::load_from_memory(&png).ok().and_then(|i| crate::ocr::recognize(&i));
                if let Some(t) = &text {
                    copy_text(t);
                }
                let _ = tx.send(Msg::SnapText(false, text));
                return;
            }
            // снимок экрана — только в свой каталог 0700, не в общий /tmp
            let Ok(dir) = yami_px::paths::private_tmp("yami-read") else {
                let _ = tx.send(Msg::SnapText(qr, None));
                return;
            };
            let tmp = dir.join("shot.png");
            if std::fs::write(&tmp, &png).is_err() {
                let _ = std::fs::remove_dir_all(&dir);
                let _ = tx.send(Msg::SnapText(qr, None));
                return;
            }
            let out = std::process::Command::new("zbarimg").args(["--raw", "-q"]).arg(&tmp).output();
            let _ = std::fs::remove_dir_all(&dir);
            let text = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|t| !t.is_empty());
            if let Some(t) = &text {
                copy_text(t);
            }
            let _ = tx.send(Msg::SnapText(qr, text));
        });
    }

    pub fn on_snap_text(&mut self, qr: bool, text: Option<String>) {
        let title = if qr { "QR-код" } else { "текст со снимка" };
        match text {
            Some(t) => {
                let short: String = t.chars().take(160).collect();
                self.toast_note(&crate::tf!("{} — в буфере", "{}: copied", "{}: copiado", crate::i18n::tr(title)), &short);
            }
            None => self.toast_note(title, if qr { "QR-код не найден" } else { "текст не распознан" }),
        }
    }

    /// готово по настройкам: копировать и/или сохранить (без копирования — всегда сохранить)
    fn snap_done(&mut self) {
        let (c, s) = (self.st.set.shot_copy, self.st.set.shot_save);
        self.snap_finish(c, s || !c, false);
    }

    /// готово: copy — в буфер, save — в папку, ask — спросить место диалогом yami-files
    pub fn snap_finish(&mut self, copy: bool, save: bool, ask: bool) {
        // один снимок на один оверлей: кнопка панели, клавиша и мгновенный режим сходятся сюда
        if self.st.snap.as_ref().is_some_and(|s| s.flash.is_some() || s.busy) {
            return;
        }
        let Some((png, thumb)) = self.snap_render() else { return };
        let dir = shots_dir(&self.st.set);
        let name = file_name();
        let tx = self.tx.clone();
        if let Some(sn) = &mut self.st.snap {
            sn.busy = ask;
            if !ask {
                sn.flash = Some(Instant::now());
            }
        }
        if ask {
            // диалог должен быть виден: убираем оверлей сразу
            self.snap_close();
        }
        self.dirty(&SKind::Snap);
        let notify = self.st.set.shot_notify;
        std::thread::spawn(move || {
            let mut path: Option<PathBuf> = None;
            let mut err = None;
            // «сохранить как» спрашивает окно yami-files; без него — в папку скриншотов, как «сохранить»
            if ask && crate::proc::have("yami-files") {
                let _ = std::fs::create_dir_all(&dir);
                match crate::proc::files_pick("save", &dir, &["--name", &name, "--filter", "PNG|*.png"]) {
                    Some(p) => path = Some(PathBuf::from(p)),
                    None => return,
                }
            } else if save || ask {
                path = Some(dir.join(&name));
            }
            if let Some(p) = &path {
                if let Some(d) = p.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                if let Err(e) = std::fs::write(p, &png) {
                    err = Some(e.to_string());
                }
            }
            if copy {
                use std::io::Write;
                if let Ok(mut ch) = std::process::Command::new("wl-copy").args(["-t", "image/png"]).stdin(std::process::Stdio::piped()).spawn() {
                    if let Some(i) = ch.stdin.as_mut() {
                        let _ = i.write_all(&png);
                    }
                    drop(ch.stdin.take());
                    let _ = ch.wait();
                }
            }
            let _ = tx.send(Msg::SnapDone { path, copied: copy, err, thumb: if notify { Some(thumb) } else { None } });
        });
    }

    pub fn on_snap_done(&mut self, path: Option<PathBuf>, copied: bool, err: Option<String>, thumb: Option<Img>) {
        if let Some(e) = err {
            self.toast_note("скриншот не сохранился", &e);
            return;
        }
        let Some(thumb) = thumb else { return };
        let summary = match (&path, copied) {
            (Some(_), true) => crate::i18n::tr("скриншот сохранён и скопирован"),
            (Some(_), false) => crate::i18n::tr("скриншот сохранён"),
            _ => crate::i18n::tr("скриншот в буфере обмена"),
        };
        let body = path.as_ref().map(|p| p.display().to_string().replace(&crate::config::home().to_string_lossy().to_string(), "~")).unwrap_or_default();
        let id = 9_500_000 + (self.st.started.elapsed().as_millis() % 400_000) as u32;
        let actions = if path.is_some() { vec![("default".to_string(), crate::i18n::tr("открыть").to_string()), ("folder".to_string(), crate::i18n::tr("в папке").to_string())] } else { vec![] };
        if let Some(p) = path {
            // действия есть только у карточек в истории (≤ 50): старые пути не храним
            let live: Vec<u32> = self.st.notifs.history.iter().map(|n| n.id).collect();
            self.st.shot_files.retain(|k, _| live.contains(k));
            self.st.shot_files.insert(id, p);
        }
        let n = crate::dbus::Incoming { id, app: "yami-screenshot".into(), icon: String::new(), summary: summary.into(), body, actions, urgency: 1, image: Some(thumb), timeout: -1 };
        self.on_notif(n);
    }

    /// уведомление скриншота: открыть файл или его папку (true — обработано)
    pub fn shot_note_action(&mut self, id: u32, key: &str) -> bool {
        let Some(p) = self.st.shot_files.get(&id).cloned() else { return false };
        match key {
            "folder" => App::open_dir(p.display().to_string()),
            _ => App::spawn(&["xdg-open".into(), p.display().to_string()]),
        }
        true
    }

    /// раз в тик: вспышка догорела — закрыть оверлей. Курсор в углу экрана — подождать, пока уйдёт
    /// (не дольше HOLD): там горячий угол niri, и оверлей, пока он под курсором, его глушит. Без
    /// оверлея первое же движение мыши в углу открыло бы обзор — так и было в мгновенном режиме,
    /// когда выделение тянут до угла экрана (#15)
    pub fn snap_tick(&mut self) {
        let Some(sn) = &self.st.snap else { return };
        let Some(ms) = sn.flash.map(|f| f.elapsed().as_millis()) else { return };
        let cornered = sn.ptr.as_ref().is_some_and(|(o, x, y)| {
            let (w, h) = self.output_size(o);
            in_corner(*x, *y, w, h)
        });
        if ms > FLASH && (!cornered || ms > HOLD) {
            self.snap_close();
        }
    }
}
