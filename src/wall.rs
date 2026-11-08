// Обои: своя картинка или пиксельные из палитры (generate: свечение акцента дизерингом). Своя
// готовится в нити — декод, обрезка или вписывание под экран, клетки, окраска в палитру,
// затемнение — и приходит в Msg::Wall уже готовой: экран рисует её без пересчёта. Здесь же
// вкладка «обои».

use crate::app::{App, Msg, SKind, State};
use crate::config::{self, Settings};
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, Img, C, R};
use crate::settings::{opt_btn, title};
use crate::theme::Theme;
use crate::ui::{Act, Tone, Ui};
use image::RgbaImage;
use std::path::{Path, PathBuf};

pub const FITS: &[(&str, &str)] = &[("fill", "заполнить"), ("fit", "вписать")];
pub const TONES: &[(&str, &str)] = &[("", "как есть"), ("tint", "оттенок акцента"), ("palette", "тона палитры")];
const BAYER4: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// размер клетки в пикселях экрана для wallPixel 1..8
pub fn cell(p: i32) -> u32 {
    [1, 2, 3, 4, 6, 8, 12, 16][(p.clamp(1, 8) - 1) as usize]
}

/// картинки, которые шелл декодирует сам: обои, выбор обоев
pub const IMAGES: &[&str] = &["png", "jpg", "jpeg", "webp"];

pub fn decodable(p: &Path) -> bool {
    p.extension().is_some_and(|e| IMAGES.contains(&e.to_string_lossy().to_ascii_lowercase().as_str()))
}

/// «как обложка»: наибольший кусок из середины картинки sw×sh с пропорциями tw×th — x, y, ширина, высота
pub fn cover_rect(sw: u32, sh: u32, tw: u32, th: u32) -> (u32, u32, u32, u32) {
    let (cw, ch) = if sw as u64 * th as u64 > sh as u64 * tw as u64 { ((sh as u64 * tw as u64 / th as u64) as u32, sh) } else { (sw, (sw as u64 * th as u64 / tw as u64) as u32) };
    let (cw, ch) = (cw.clamp(1, sw.max(1)), ch.clamp(1, sh.max(1)));
    ((sw - cw.min(sw)) / 2, (sh - ch.min(sh)) / 2, cw, ch)
}

/// своя картинка (с раскрытым ~) или None — пиксельные из палитры
pub fn own(s: &Settings) -> Option<PathBuf> {
    let p = s.wallpaper.trim();
    if p.is_empty() {
        return None;
    }
    Some(yami_px::paths::expand(p))
}

/// что рисовать: своя картинка или None — пиксельные из палитры
pub fn source(st: &State) -> Option<PathBuf> {
    own(&st.set)
}

/// акцент берётся из картинки: «палитра из обоев»
fn accent_from_image(s: &Settings) -> bool {
    s.wall_accent
}

/// пиксельные обои в четверть экрана (шелл растягивает их ×4 без сглаживания): свечение акцента
/// из правого нижнего угла и слабее из левого верхнего, ступенями по Байеру; редкая точечная сетка
pub fn generate(bg: C, accent: C, dot: C, light: bool) -> RgbaImage {
    let (w, h) = (480u32, 270u32);
    let (levels, peak) = (7.0f32, if light { 0.3 } else { 0.2 });
    let diag = ((w * w + h * h) as f32).sqrt();
    let (r1, r2) = (diag * 0.75, diag * 0.45);
    let ch = |c: C| [((c.0 >> 16) & 255) as f32, ((c.0 >> 8) & 255) as f32, (c.0 & 255) as f32];
    let (b, a, d) = (ch(bg), ch(accent), ch(dot));
    RgbaImage::from_fn(w, h, |x, y| {
        let (xf, yf) = (x as f32, y as f32);
        let ga = (1.0 - ((w as f32 - xf).powi(2) + (h as f32 - yf).powi(2)).sqrt() / r1).max(0.0);
        let gb = (1.0 - (xf * xf + yf * yf).sqrt() / r2).max(0.0);
        let g = ga * ga + gb * gb * 0.4;
        let q = (g * levels + BAYER4[((y & 3) * 4 + (x & 3)) as usize] as f32 / 16.0).floor() / levels * peak;
        let mut c = [0.0f32; 3];
        for i in 0..3 {
            c[i] = b[i] + (a[i] - b[i]) * q;
            if x % 10 == 5 && y % 10 == 5 {
                c[i] += (d[i] - c[i]) * 0.6;
            }
        }
        image::Rgba([c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, 255])
    })
}

fn name_of(list: &[(&str, &'static str)], id: &str) -> &'static str {
    list.iter().find(|x| x.0 == id).map(|x| x.1).unwrap_or(list[0].1)
}

fn next_of(list: &[(&'static str, &str)], id: &str) -> &'static str {
    let i = list.iter().position(|x| x.0 == id).unwrap_or(0);
    list[(i + 1) % list.len()].0
}

/// всё, от чего зависит готовая картинка; одинаковый ключ — пересчитывать нечего
pub struct Job {
    path: PathBuf,
    own: bool,
    w: u32,
    h: u32,
    cell: u32,
    fit: bool,
    tone: String,
    dim: f32,
    accent: bool,
    /// тона палитры от фона к тексту
    ramp: [u32; 6],
    /// точки сетки пиксельных обоев и светлая ли тема
    border: u32,
    light: bool,
}

impl Job {
    /// src — картинка (wall::source) или None: пиксельные из палитры
    pub fn new(s: &Settings, t: &Theme, w: i32, h: i32, src: Option<PathBuf>) -> Job {
        let own = src;
        Job {
            own: own.is_some(),
            path: own.unwrap_or_default(),
            w: w.max(1) as u32,
            h: h.max(1) as u32,
            cell: cell(s.wall_pixel),
            fit: s.wall_fit == "fit",
            tone: s.wall_tone.clone(),
            dim: s.wall_dim.clamp(0.0, 0.8),
            accent: accent_from_image(s),
            ramp: [t.bg.0, t.dim.0, t.blood.0, t.red.0, t.ember.0, t.text.0],
            border: t.border.0,
            light: t.light,
        }
    }

    pub fn key(&self) -> String {
        let m = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        // и «палитра из обоев»: включили её на той же картинке — акцент надо вычислить заново
        format!("{:?}|{m:?}|{}|{}x{}|{}|{}|{}|{}|{:x?}|{}|{:x}|{}", self.path, self.own, self.w, self.h, self.cell, self.fit, self.tone, self.dim, self.ramp, self.accent, self.border, self.light)
    }
}

/// готовая картинка и (если просили) акцент из неё
pub fn prepare(j: &Job) -> (Option<Img>, Option<u32>) {
    let bg = C(j.ramp[0]);
    if !j.own {
        // сгенерированные уже в палитре и в четверть экрана: только затемнение
        let mut i = generate(bg, C(j.ramp[3]), C(j.border), j.light);
        dim(&mut i, bg, j.dim);
        return (Some(crate::icons::from_rgba(&i)), None);
    }
    let Ok(src) = image::open(&j.path) else { return (None, None) };
    let src = src.to_rgba8();
    let accent = if j.accent { dominant(&image::imageops::thumbnail(&src, 64, 64)) } else { None };
    let (sw, sh) = src.dimensions();
    let (tw, th) = ((j.w / j.cell).max(1), (j.h / j.cell).max(1));
    let filt = image::imageops::FilterType::Triangle;
    let mut out = if j.fit {
        let k = (tw as f32 / sw as f32).min(th as f32 / sh as f32);
        let (rw, rh) = (((sw as f32 * k).round() as u32).clamp(1, tw), ((sh as f32 * k).round() as u32).clamp(1, th));
        let mut pic = image::imageops::resize(&src, rw, rh, filt);
        drop(src);
        tone(&mut pic, j);
        let (r, g, b) = bg.rgbf();
        let mut o = RgbaImage::from_pixel(tw, th, image::Rgba([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, 255]));
        image::imageops::overlay(&mut o, &pic, ((tw - rw) / 2) as i64, ((th - rh) / 2) as i64);
        o
    } else {
        // «как обложка»: вырезать из середины кусок с пропорциями экрана
        let (cx, cy, cw, ch) = cover_rect(sw, sh, tw, th);
        let crop = image::imageops::crop_imm(&src, cx, cy, cw, ch);
        let mut o = image::imageops::resize(&*crop, tw, th, filt);
        drop(src);
        tone(&mut o, j);
        o
    };
    dim(&mut out, bg, j.dim);
    // большой декод уже не нужен: вернуть память системе
    yami_px::trim_heap();
    (Some(crate::icons::from_rgba(&out)), accent)
}

fn lum(p: &image::Rgba<u8>) -> f32 {
    (0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32) / 255.0
}

/// окраска: яркость картинки → тона палитры (растянутая между 2-м и 98-м процентилем)
fn tone(i: &mut RgbaImage, j: &Job) {
    if j.tone != "tint" && j.tone != "palette" {
        return;
    }
    let mut hist = [0u32; 256];
    for p in i.pixels() {
        hist[(lum(p) * 255.0) as usize] += 1;
    }
    let n = i.width() * i.height();
    let at = |q: f32| {
        let (mut acc, lim) = (0, (n as f32 * q) as u32);
        hist.iter().position(|c| {
            acc += c;
            acc > lim
        })
        .unwrap_or(0) as f32
            / 255.0
    };
    let (lo, hi) = (at(0.02), at(0.98));
    let span = (hi - lo).max(0.1);
    let r = j.ramp.map(C);
    let w = i.width();
    for (k, p) in i.pixels_mut().enumerate() {
        let l = ((lum(p) - lo) / span).clamp(0.0, 1.0);
        let c = if j.tone == "palette" {
            // клетка = пиксель картинки: байеровский дизеринг между соседними тонами
            let (x, y) = (k as u32 % w, k as u32 / w);
            let d = BAYER4[((y & 3) * 4 + (x & 3)) as usize] as f32 / 16.0 - 0.47;
            r[(l * 5.0 + d).round().clamp(0.0, 5.0) as usize]
        } else if l < 0.6 {
            r[0].lerp(r[3], l / 0.6)
        } else {
            r[3].lerp(r[5], (l - 0.6) / 0.4)
        };
        let (cr, cg, cb) = c.rgbf();
        p.0 = [(cr * 255.0) as u8, (cg * 255.0) as u8, (cb * 255.0) as u8, 255];
    }
}

fn dim(i: &mut RgbaImage, bg: C, k: f32) {
    if k <= 0.0 {
        return;
    }
    let (r, g, b) = bg.rgbf();
    let to = [r * 255.0, g * 255.0, b * 255.0];
    for p in i.pixels_mut() {
        for c in 0..3 {
            p.0[c] = (p.0[c] as f32 + (to[c] - p.0[c] as f32) * k).round() as u8;
        }
    }
}

/// главный цвет картинки: самый весомый тон среди насыщенных и не тёмных пикселей,
/// доведённый до яркости, на которой акцент читается на тёмных панелях
fn dominant(i: &RgbaImage) -> Option<u32> {
    let mut bins = [(0f32, [0f32; 3]); 24];
    for p in i.pixels() {
        let [r, g, b] = [p.0[0] as f32 / 255.0, p.0[1] as f32 / 255.0, p.0[2] as f32 / 255.0];
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let s = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
        if mx < 0.15 || s < 0.2 {
            continue;
        }
        let h = hue(r, g, b, mx, mn);
        let wt = s * s * mx;
        let bin = &mut bins[((h / 15.0) as usize).min(23)];
        bin.0 += wt;
        for (c, v) in bin.1.iter_mut().zip([r, g, b]) {
            *c += v * wt;
        }
    }
    let (wt, sum) = bins.iter().enumerate().map(|(k, _)| {
        // соседние корзины вместе: тон на границе не делится пополам
        let (a, b, c) = (bins[(k + 23) % 24], bins[k], bins[(k + 1) % 24]);
        (a.0 * 0.5 + b.0 + c.0 * 0.5, b)
    }).max_by(|a, b| a.0.total_cmp(&b.0))?;
    let total: f32 = bins.iter().map(|b| b.0).sum();
    if wt < 0.5 || total < 1.0 || sum.0 <= 0.0 {
        return None;
    }
    let [r, g, b] = sum.1.map(|v| v / sum.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let h = hue(r, g, b, mx, mn);
    let s = ((mx - mn) / mx.max(1e-3)).clamp(0.55, 0.9);
    let v = mx.clamp(0.7, 0.92);
    Some(hsv(h, s, v))
}

fn hue(r: f32, g: f32, b: f32, mx: f32, mn: f32) -> f32 {
    let d = (mx - mn).max(1e-6);
    let h = if mx == r { (g - b) / d } else if mx == g { 2.0 + (b - r) / d } else { 4.0 + (r - g) / d };
    (h * 60.0).rem_euclid(360.0)
}

fn hsv(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    C::from_f(r + v - c, g + v - c, b + v - c).0 & 0xffffff
}

/// длинная сторона копии обоев, что остаётся в памяти (wall_park)
const PARK: i32 = 640;

/// уменьшение в целое число раз средним по квадрату k×k
fn shrink(img: &Img, max: i32) -> Img {
    if img.w <= 0 || img.h <= 0 || img.px.len() < (img.w * img.h) as usize {
        return Img { w: 1, h: 1, px: vec![0xff00_0000] };
    }
    let k = ((img.w.max(img.h) + max - 1) / max).max(1);
    // по узкой стороне квадрат не шире самой картинки (полоса 4000×3 не выйдет за буфер)
    let (kx, ky) = (k.min(img.w), k.min(img.h));
    let (w, h) = (img.w / kx, img.h / ky);
    let n = (kx * ky) as u32;
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let [mut a, mut r, mut g, mut b] = [0u32; 4];
            for j in 0..ky {
                let row = ((y * ky + j) * img.w + x * kx) as usize;
                for &p in &img.px[row..row + kx as usize] {
                    a += p >> 24;
                    r += (p >> 16) & 255;
                    g += (p >> 8) & 255;
                    b += p & 255;
                }
            }
            px.push((a / n) << 24 | (r / n) << 16 | (g / n) << 8 | b / n);
        }
    }
    Img { w, h, px }
}

/// обои на весь экран «как обложка» (картинка уже в пропорциях экрана — без обрезки)
pub fn cover(ui: &mut Ui, img: &Img, w: i32, h: i32) {
    yami_px::face::cover(ui, img, w, h);
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    ui.cv.fill(R::new(0, 0, w, h), t.bg);
    if let Some(img) = &st.wall {
        cover(ui, img, w, h);
    }
    if st.set.wall_kanji {
        let face = font::jp(32);
        let kw = ui.w(face, "闇");
        ui.text(face, w - 40 - kw, h - 32 - ui.f.line(face), "闇", t.red.fade(0.45));
    }
}

// ── вкладка «обои» ──────────────────────────────────────────────────────────
pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, b.w, "картинка");
    // превью в пропорциях экрана — то, что сейчас на рабочем столе
    let pv = R::new(b.x, y, 256, 144);
    ui.cv.pxbox(pv, &BoxStyle::new(t.code_bg, t.border));
    let inner = pv.inset(3);
    ui.cv.push_clip(inner);
    ui.cv.fill(inner, t.bg);
    if let Some(img) = &st.wall {
        let k = (inner.w as f32 / img.w as f32).max(inner.h as f32 / img.h as f32);
        let (dw, dh) = ((img.w as f32 * k).ceil() as i32, (img.h as f32 * k).ceil() as i32);
        ui.cv.image_scaled(img, R::new(inner.x + (inner.w - dw) / 2, inner.y + (inner.h - dh) / 2, dw, dh), 1.0);
    }
    ui.cv.pop_clip();
    let (rx, rw) = (pv.right() + 16, b.right() - pv.right() - 16);
    let mut ry = y + 2;
    match source(st) {
        Some(p) => {
            // имя файла — как есть, без перевода
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let f = &mut *ui.f;
            ui.cv.push_clip(R::new(rx, ry, rw, 20));
            ui.cv.text(f, PX, rx, ry, &name, t.text);
            ui.cv.pop_clip();
            ry += 20;
            let what = if p.is_file() { "своя картинка" } else { "файл не найден" };
            ui.text_in(PX, R::new(rx, ry, rw, 18), what, if p.is_file() { t.text_faint } else { t.warn });
        }
        None => {
            ui.text_in(PX, R::new(rx, ry, rw, 20), "пиксельные из палитры", t.text);
            ry += 20;
            ui.text_in(PX, R::new(rx, ry, rw, 18), "рисуются заново при смене цвета", t.text_faint);
        }
    }
    ry += 30;
    ui.button(R::new(rx, ry, rw, 28), "выбрать картинку…", "folder", Tone::Primary, false, true, Act::SWall("pick"));
    ry += 34;
    ui.button(R::new(rx, ry, rw, 28), "пиксельные из палитры", "image", Tone::Plain, false, own(s).is_some(), Act::SWall("reset"));
    y = pv.bottom() + 10;
    let focused = st.field_focus == Some("wall");
    ui.field(R::new(b.x, y, b.w - 130, 30), &st.wall_field, "или путь к файлу", focused, false, Act::SField("wall"));
    ui.button(R::new(b.right() - 120, y + 2, 120, 26), "применить", "check", Tone::Primary, false, true, Act::SCycle("wall_apply"));
    y += 44;
    y = title(ui, b.x, y, b.w, "как показывать");
    opt_btn(ui, R::new(b.x, y, half, 26), "подгонка", name_of(FITS, &s.wall_fit), Act::SWall("fit"));
    opt_btn(ui, R::new(b.x + half + 20, y, half, 26), "окраска", name_of(TONES, &s.wall_tone), Act::SWall("tone"));
    y += 38;
    let c = cell(s.wall_pixel);
    let ct = if c == 1 { crate::i18n::tr("как есть").to_string() } else { format!("{c} px") };
    ui.slider(R::new(b.x, y, half, 40), "пиксельные клетки", (s.wall_pixel as f32 - 1.0) / 7.0, &ct, 7, Act::SSlider("wall_pixel", 1.0, 8.0, 7));
    ui.slider(R::new(b.x + half + 20, y, half, 40), "затемнение", s.wall_dim / 0.8, &format!("{:.0}%", s.wall_dim * 100.0), 16, Act::SSlider("wall_dim", 0.0, 0.8, 16));
    y += 54;
    y = title(ui, b.x, y, b.w, "цвета");
    ui.toggle(R::new(b.x, y, half, 40), "палитра из обоев", "акцент из картинки", s.wall_accent, Act::SWall("accent"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "иероглиф на обоях", "в правом нижнем углу", s.wall_kanji, Act::SBool("wall_kanji"));
}

impl App {
    /// пересчитать обои, если изменилось то, от чего они зависят (настройки, палитра, файл,
    /// размер экрана). Пока тянут ползунок обоев — ждём, когда отпустят.
    pub fn sync_wall(&mut self) {
        if matches!(&self.press, Some(p) if matches!(p.act, Act::SSlider(k, ..) if k.starts_with("wall"))) {
            return;
        }
        if self.wall_kanji != self.st.set.wall_kanji {
            self.wall_kanji = self.st.set.wall_kanji;
            self.dirty(&SKind::Wall);
        }
        let (w, h) = self.output_size(&self.focus_out());
        let job = Job::new(&self.st.set, &self.st.t, w, h, source(&self.st));
        let key = job.key();
        if key == self.wall_key {
            return;
        }
        self.wall_key = key;
        self.wall_job += 1;
        let (n, tx) = (self.wall_job, self.tx.clone());
        std::thread::spawn(move || {
            let (img, accent) = prepare(&job);
            let _ = tx.send(Msg::Wall(n, img, accent));
        });
    }

    pub fn wall_ready(&mut self, n: u64, img: Option<Img>, accent: Option<u32>) {
        if n != self.wall_job {
            return;
        }
        self.st.wall = img.map(std::rc::Rc::new);
        self.st.wall_parked = false;
        self.wall_reload = false;
        self.wall_at = std::time::Instant::now();
        self.st.wall_stamp += 1;
        for s in self.surfs.values_mut() {
            if s.kind == SKind::Wall {
                s.dirty = true;
                s.frame_pending = false;
            }
        }
        for l in self.lock_surfs.iter_mut() {
            l.dirty = true;
        }
        self.dirty(&SKind::Modal);
        // новые обои: виджеты на них — в тон
        self.widget_contrast();
        let s = &mut self.st.set;
        if let (Some(a), true) = (accent, accent_from_image(s)) {
            let hex = format!("#{a:06x}");
            if hex != s.accent {
                s.accent = hex.clone();
                self.st.hex.set(&hex);
                self.settings_changed();
            }
        }
    }

    /// обои уже у niri: держать полную картинку незачем (1080p — 8 МБ, 4K — 33 МБ). Остаётся копия
    /// не больше PARK точек по длинной стороне: превью во вкладке, замер под виджетами, первый кадр
    /// блокировки. Полную пересчитывает wall_need, когда она снова нужна
    pub fn wall_park(&mut self) {
        let Some(img) = self.st.wall.as_ref() else { return };
        if self.st.wall_parked || self.st.locked || img.w.max(img.h) <= PARK {
            return;
        }
        // пока обои не показаны (буфер ещё свой) или их только что сменили — рано
        if self.wall_at.elapsed().as_secs() < 20 || self.surfs.values().any(|s| s.kind == SKind::Wall && (s.dirty || s.pool.is_some())) {
            return;
        }
        let small = shrink(img, PARK);
        self.st.wall = Some(std::rc::Rc::new(small));
        self.st.wall_parked = true;
        yami_px::trim_heap();
    }

    /// полная картинка снова нужна (обои перерисовать, блокировка): пересчитать её в нити
    pub fn wall_need(&mut self) {
        if !self.st.wall_parked || self.wall_reload {
            return;
        }
        self.wall_reload = true;
        self.wall_key.clear();
        self.sync_wall();
    }

    /// новый путь (пусто — пиксельные из палитры): из настроек, IPC и диалога выбора
    pub fn set_wall(&mut self, p: &str) {
        let p = p.trim();
        let p = if p.is_empty() { String::new() } else { yami_px::paths::expand(p).to_string_lossy().to_string() };
        self.st.set.wallpaper = p.clone();
        self.st.wall_field.set(&p);
        self.st.field_focus = None;
        self.settings_changed();
    }

    pub fn wall_click(&mut self, a: &Act) -> bool {
        let Act::SWall(k) = a else { return false };
        let s = &mut self.st.set;
        match *k {
            // сетка картинок папки обоев (walls.rs); другой файл и папка — оттуда через yami-files
            "pick" => {
                self.walls_open(true);
                return true;
            }
            "reset" => {
                self.set_wall("");
                return true;
            }
            "fit" => s.wall_fit = next_of(FITS, &s.wall_fit).into(),
            "tone" => s.wall_tone = next_of(TONES, &s.wall_tone).into(),
            "accent" => s.wall_accent = !s.wall_accent,
            _ => return false,
        }
        self.settings_changed();
        true
    }
}

#[cfg(test)]
mod tests {
    use yami_px::gfx::Img;
    #[test]
    fn shrink_odd_sizes() {
        for (w, h) in [(4000, 3), (3, 4000), (1, 1), (1920, 1080), (641, 2)] {
            let img = Img { w, h, px: vec![0xff10_2030; (w * h) as usize] };
            let s = super::shrink(&img, 640);
            assert!(s.w >= 1 && s.h >= 1 && s.w <= 640 && s.h <= 640 && s.px.len() == (s.w * s.h) as usize);
            assert_eq!(s.px[0], 0xff10_2030);
        }
    }
}
