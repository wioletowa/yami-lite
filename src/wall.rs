// Обои: своя картинка или пиксельные из палитры (generate: свечение акцента дизерингом). Своя
// готовится в нити — декод, обрезка или вписывание под экран, клетки, окраска в палитру,
// затемнение — и приходит в Msg::Wall уже готовой: экран рисует её без пересчёта. Здесь же
// вкладка «обои».

use crate::app::{App, Msg, SKind, State};
use crate::config::Settings;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, Img, C, R};
use crate::settings::{num_btn, opt_btn, title};
use crate::theme::Theme;
use crate::ui::{Act, Cur, Tone, Ui};
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
        // цвета темы — в ключе, только когда они меняют картинку: свои обои «как есть», без
        // затемнения и полей от темы не зависят. Иначе новый акцент из этих же обоев запускал бы их
        // подготовку заново, и смена обоев обрывалась на середине
        let themed = !self.own || !self.tone.is_empty() || self.dim > 0.0 || self.fit;
        let theme = if themed { format!("{:x?}|{:x}|{}", self.ramp, self.border, self.light) } else { String::new() };
        // и «палитра из обоев»: включили её на той же картинке — акцент надо вычислить заново
        format!("{:?}|{m:?}|{}|{}x{}|{}|{}|{}|{}|{}|{theme}", self.path, self.own, self.w, self.h, self.cell, self.fit, self.tone, self.dim, self.accent)
    }
}

/// готовая картинка и цвета из неё (candidates; у сгенерированных — нет)
pub fn prepare(j: &Job) -> (Option<Img>, Vec<u32>) {
    let bg = C(j.ramp[0]);
    if !j.own {
        // сгенерированные уже в палитре и в четверть экрана: только затемнение
        let mut i = generate(bg, C(j.ramp[3]), C(j.border), j.light);
        dim(&mut i, bg, j.dim);
        return (Some(crate::icons::from_rgba(&i)), vec![]);
    }
    let Ok(src) = image::open(&j.path) else { return (None, vec![]) };
    let src = src.to_rgba8();
    // цвета нужны и без «палитры из обоев»: вкладка цвета показывает их образцами
    let accent = candidates(&image::imageops::thumbnail(&src, 64, 64));
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

/// серый акцент для бесцветных обоев: читается на тёмных панелях, без оттенка
const GREY: u32 = 0xb4b4b4;

/// корзины по 15° оттенка: вес и взвешенная сумма цвета. Пиксель голосует, если он не темнее
/// min_v и не серее min_s; вес — насыщенность² × яркость (яркость — от самой светлой точки
/// картинки, чтобы тёмные обои голосовали наравне со светлыми). Второе — сколько пикселей цветные
fn hue_bins(i: &RgbaImage, min_v: f32, min_s: f32, top_v: f32) -> ([(f32, [f32; 3]); 24], usize) {
    let mut bins = [(0f32, [0f32; 3]); 24];
    let mut coloured = 0;
    for p in i.pixels() {
        let [r, g, b] = [p.0[0] as f32 / 255.0, p.0[1] as f32 / 255.0, p.0[2] as f32 / 255.0];
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let s = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
        if mx < min_v || s < min_s {
            continue;
        }
        coloured += 1;
        let h = hue(r, g, b, mx, mn);
        let wt = s * s * (mx / top_v.max(1e-3)).min(1.0);
        let bin = &mut bins[((h / 15.0) as usize).min(23)];
        bin.0 += wt;
        for (c, v) in bin.1.iter_mut().zip([r, g, b]) {
            *c += v * wt;
        }
    }
    (bins, coloured)
}

/// цвета картинки по весу, до 5: тона среди насыщенных пикселей, каждый доведён до яркости, на
/// которой акцент читается на тёмных панелях. Первый — главный. Тёмные обои (красный узор по
/// чёрному) — второй проход с низким порогом яркости; цвета нет вовсе — серый; цвета мало — серый
/// последним вариантом
fn candidates(i: &RgbaImage) -> Vec<u32> {
    let n = (i.width() * i.height()).max(1) as usize;
    let top_v = i.pixels().map(|p| p.0[0].max(p.0[1]).max(p.0[2])).max().unwrap_or(0) as f32 / 255.0;
    // обычный проход: тёмное и серое не голосует
    let (mut bins, mut coloured) = hue_bins(i, 0.15, 0.2, 1.0);
    // цветных почти нет — тёмные обои: голосует всё, что не чёрное и заметно окрашено
    if coloured * 100 < n {
        (bins, coloured) = hue_bins(i, 0.03, 0.3, top_v);
    }
    if coloured * 100 < n {
        return vec![GREY];
    }
    // соседние корзины вместе: тон на границе не делится пополам
    let mut order: Vec<(usize, f32)> = (0..24).map(|k| (k, bins[(k + 23) % 24].0 * 0.5 + bins[k].0 + bins[(k + 1) % 24].0 * 0.5)).collect();
    order.sort_by(|a, b| b.1.total_cmp(&a.1));
    let top = order[0].1;
    // картинка в основном серая — серый тоже вариант, последним
    let grey = coloured * 4 < n;
    let room = if grey { 4 } else { 5 };
    let mut taken: Vec<usize> = vec![];
    let mut out = vec![];
    for (k, wt) in order {
        if out.len() == room || wt < top * 0.04 {
            break;
        }
        // соседка уже взятой — тот же тон, не новый кандидат
        if bins[k].0 <= 0.0 || taken.iter().any(|&t| (k + 24 - t) % 24 <= 1 || (t + 24 - k) % 24 <= 1) {
            continue;
        }
        taken.push(k);
        let sum = bins[k];
        let [r, g, b] = sum.1.map(|v| v / sum.0);
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let h = hue(r, g, b, mx, mn);
        let s = ((mx - mn) / mx.max(1e-3)).clamp(0.55, 0.9);
        let v = mx.clamp(0.7, 0.92);
        out.push(hsv(h, s, v));
    }
    if grey || out.is_empty() {
        out.push(GREY);
    }
    out
}

/// второй акцент к выбранному кандидату: самый весомый из далёких по тону (от 60°); таких нет —
/// None, тема возьмёт противоположный
pub fn second(cands: &[u32], pick: usize) -> Option<u32> {
    let h = |c: u32| {
        let [r, g, b] = [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0];
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        hue(r, g, b, mx, mn)
    };
    let base = h(*cands.get(pick)?);
    cands.iter().enumerate().filter(|(i, _)| *i != pick).map(|(_, c)| *c).find(|c| {
        let d = (h(*c) - base).abs();
        d.min(360.0 - d) >= 60.0
    })
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

/// картинка в другом размере ближайшим соседом (прошлые обои для смены, если были уменьшены)
fn stretch(img: &Img, w: i32, h: i32) -> Img {
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        let sy = ((y as i64 * img.h as i64) / h.max(1) as i64) as i32;
        let row = (sy.min(img.h - 1) * img.w) as usize;
        for x in 0..w {
            let sx = ((x as i64 * img.w as i64) / w.max(1) as i64) as usize;
            px.push(img.px[row + sx.min(img.w as usize - 1)]);
        }
    }
    Img { w, h, px }
}

/// обои на весь экран «как обложка» (картинка уже в пропорциях экрана — без обрезки)
pub fn cover(ui: &mut Ui, img: &Img, w: i32, h: i32) {
    yami_px::face::cover(ui, img, w, h);
}

/// узоры смены обоев: ключ, подпись
pub const FADES: &[(&str, &str)] = &[("dither", "дизеринг"), ("noise", "шум"), ("scan", "развёртка"), ("wipe", "диагональ"), ("blocks", "крупные блоки"), ("none", "сразу")];

/// шагов смены: клетки открываются ступенями, около 30 в секунду — длинная смена не дробится
/// на редкие кадры, короткая не меньше 12 ступеней
fn fade_steps(st: &State) -> i32 {
    ((fade_secs(st) * 30.0).round() as i32).clamp(12, 120)
}

/// сколько идёт смена обоев, с
pub fn fade_secs(st: &State) -> f32 {
    st.set.wall_fade_time.clamp(0.3, 4.0)
}

/// доля смены обоев 0..1 по времени; None — не идёт
pub fn fade_k(st: &State) -> Option<f32> {
    let t = st.wall_fade?;
    st.wall_prev.as_ref()?;
    Some((t.elapsed().as_secs_f32() / fade_secs(st)).min(1.0))
}

/// считать смену законченной (последний шаг), не рисуя её
pub fn finish_fade(st: &State) {
    st.wall_step.set(fade_steps(st));
}

/// смена закончилась: показан последний шаг
pub fn fade_done(st: &State) -> bool {
    st.wall_step.get() >= fade_steps(st)
}

fn hash(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// порог клетки 0..256 по узору смены
fn threshold(style: &str, cx: i32, cy: i32, cols: i32, rows: i32) -> i32 {
    const BAYER: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let b = BAYER[(cy & 3) as usize][(cx & 3) as usize];
    match style {
        "noise" => (hash(cx, cy) & 255) as i32,
        "scan" => [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15][(cy & 15) as usize] * 16 + (cx * 7 / cols.max(1)).min(15),
        "wipe" => ((cx + cy) * 220 / (cols + rows).max(1)) + b * 2,
        _ => b * 16 + (hash(cx, cy) & 15) as i32,
    }
}

/// что сейчас на обоях: во время смены — новые, проступающие клетками сквозь старые, иначе просто
/// картинка. Тот же кадр берёт карточка панели управления
pub fn shown(st: &State) -> Option<std::rc::Rc<Img>> {
    let new = st.wall.clone()?;
    let (Some(k), Some(old)) = (fade_k(st), st.wall_prev.as_ref()) else { return Some(new) };
    if old.w != new.w || old.h != new.h || st.set.wall_fade == "none" {
        st.wall_step.set(fade_steps(st));
        return Some(new);
    }
    let steps = fade_steps(st);
    // по времени, но не больше шага за кадр: медленный кадр растягивает смену, а не выбрасывает ступени
    let step = ((k * steps as f32) as i32).min(st.wall_step.get() + 1).max(1);
    if step >= steps {
        st.wall_step.set(steps);
        return Some(new);
    }
    if let Some((s, img)) = st.wall_mix.borrow().as_ref() {
        if *s == step {
            return Some(img.clone());
        }
    }
    st.wall_step.set(step);
    // клетка — в точках экрана, а картинка обоев уменьшена в cell раз
    let big = if st.set.wall_fade == "blocks" { 4 } else { 1 };
    let cell = ((st.set.num("wall_fade_cell") * big) / cell(st.set.wall_pixel) as i32).max(1);
    let (cols, rows) = ((new.w + cell - 1) / cell, (new.h + cell - 1) / cell);
    let lvl = step * 256 / steps;
    let style = st.set.wall_fade.as_str();
    let mut px = Vec::with_capacity(new.px.len());
    // пороги — по ряду клеток один раз, дальше копирование кусками
    let mut show = vec![false; cols as usize];
    for y in 0..new.h {
        if y % cell == 0 {
            for (cx, v) in show.iter_mut().enumerate() {
                *v = threshold(style, cx as i32, y / cell, cols, rows) < lvl;
            }
        }
        let row = (y * new.w) as usize;
        for (cx, &v) in show.iter().enumerate() {
            let a = row + cx * cell as usize;
            let e = (a + cell as usize).min(row + new.w as usize);
            px.extend_from_slice(if v { &new.px[a..e] } else { &old.px[a..e] });
        }
    }
    let img = std::rc::Rc::new(Img { w: new.w, h: new.h, px });
    *st.wall_mix.borrow_mut() = Some((step, img.clone()));
    Some(img)
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    ui.cv.fill(R::new(0, 0, w, h), t.bg);
    if let Some(img) = shown(st) {
        cover(ui, &img, w, h);
    }
    if fade_k(st).is_some() && !fade_done(st) {
        ui.anim = true;
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
    // щелчок по превью — выбрать картинку
    let hov = ui.hover(pv);
    if hov {
        ui.cv.outline(pv, t.red, 2);
        let band = R::new(inner.x, inner.bottom() - 20, inner.w, 20);
        ui.cv.fill(band, t.bg.alpha(0.8));
        ui.text_center(PX, band, "выбрать картинку", t.text);
    }
    ui.zone(pv, Act::SWall("pick"), Cur::Pointer);
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
    ui.button(R::new(rx, ry, rw, 28), "выбрать папку…", "folder", Tone::Primary, false, true, Act::SWall("dir"));
    ry += 34;
    ui.button(R::new(rx, ry, rw, 28), "пиксельные из палитры", "image", Tone::Plain, false, own(s).is_some(), Act::SWall("reset"));
    ry += 34;
    let folder = if s.wall_dir.trim().is_empty() { crate::i18n::tr("папка обоев: ~/Pictures/Wallpapers или папка картинки").to_string() } else { crate::tf!("папка обоев: {}", "wallpaper folder: {}", s.wall_dir) };
    ui.cv.text_in(ui.f, PX, R::new(rx, ry, rw, 18), &folder, t.text_faint);
    y = pv.bottom() + 10;
    let focused = st.field_focus == Some("wall");
    ui.field(R::new(b.x, y, b.w - 130, 30), &st.wall_field, "или путь к файлу", focused, false, Act::SField("wall"));
    ui.button(R::new(b.right() - 120, y + 2, 120, 26), "применить", "check", Tone::Primary, false, true, Act::SCycle("wall_apply"));
    y += 44;
    y = title(ui, b.x, y, b.w, "как показывать");
    opt_btn(ui, R::new(b.x, y, half, 26), "подгонка", name_of(FITS, &s.wall_fit), Act::Drop("wall_fit"));
    opt_btn(ui, R::new(b.x + half + 20, y, half, 26), "окраска", name_of(TONES, &s.wall_tone), Act::Drop("wall_tone"));
    y += 38;
    let c = cell(s.wall_pixel);
    let ct = if c == 1 { crate::i18n::tr("как есть").to_string() } else { format!("{c} px") };
    ui.slider(R::new(b.x, y, half, 40), "пиксельные клетки", (s.wall_pixel as f32 - 1.0) / 7.0, &ct, 7, Act::SSlider("wall_pixel", 1.0, 8.0, 7));
    ui.slider(R::new(b.x + half + 20, y, half, 40), "затемнение", s.wall_dim / 0.8, &format!("{:.0}%", s.wall_dim * 100.0), 16, Act::SSlider("wall_dim", 0.0, 0.8, 16));
    y += 54;
    y = title(ui, b.x, y, b.w, "цвета");
    ui.toggle(R::new(b.x, y, half, 40), "палитра из обоев", "акцент из картинки", s.wall_accent, Act::SWall("accent"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "иероглиф на обоях", "в правом нижнем углу", s.wall_kanji, Act::SBool("wall_kanji"));
    y += 50;
    y = title(ui, b.x, y, b.w, "смена обоев");
    opt_btn(ui, R::new(b.x, y, half, 26), "узор", name_of(FADES, &s.wall_fade), Act::Drop("wall_fade"));
    num_btn(ui, R::new(b.x + half + 20, y, half, 26), "клетка", s.num("wall_fade_cell"), "wall_fade_cell");
    y += 34;
    if s.wall_fade != "none" {
        ui.slider(R::new(b.x, y, b.w, 40), "длительность", (s.wall_fade_time - 0.3) / 3.7, &crate::tf!("{:.1} с", "{:.1} s", s.wall_fade_time), 37, Act::SSlider("wall_fade_time", 0.3, 4.0, 37));
        y += 48;
    }
    // Wallhaven: модуль бара; ключ — для NSFW и большего числа запросов
    y = title(ui, b.x, y, b.w, "Wallhaven");
    let kf = st.field_focus == Some("wh_key");
    let kh = if s.wallhaven_key.is_empty() { "ключ API (wallhaven.cc → настройки аккаунта)" } else { "ключ сохранён" };
    ui.field(R::new(b.x, y, b.w - 130, 28), &st.wh_key_field, kh, kf, !kf, Act::SField("wh_key"));
    ui.button(R::new(b.right() - 120, y + 1, 120, 26), "сохранить", "check", Tone::Primary, false, true, Act::SCycle("wh_key_apply"));
    y += 36;
    let df = st.field_focus == Some("wh_dir");
    let dh = if s.wallhaven_dir.is_empty() { "куда качать: папка обоев" } else { s.wallhaven_dir.as_str() };
    ui.field(R::new(b.x, y, b.w - 130, 28), &st.wh_dir_field, dh, df, false, Act::SField("wh_dir"));
    ui.button(R::new(b.right() - 120, y + 1, 120, 26), "применить", "check", Tone::Plain, false, true, Act::SCycle("wh_dir_apply"));
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
        self.wall_job_path = job.path.clone();
        let (n, tx) = (self.wall_job, self.tx.clone());
        std::thread::spawn(move || {
            let (img, accent) = prepare(&job);
            let _ = tx.send(Msg::Wall(n, img, accent));
        });
    }

    pub fn wall_ready(&mut self, n: u64, img: Option<Img>, cands: Vec<u32>) {
        if n != self.wall_job {
            return;
        }
        let new = img.map(std::rc::Rc::new);
        // новые обои с «палитрой из обоев»: сначала их акцент. Если картинка зависит от темы
        // (тон, затемнение, поля), под новый акцент она уже готовится заново — смена начнётся
        // сразу с неё, без подмены цели посередине; эту не показываем
        if self.wall_job_path != self.wall_shown_path && !cands.is_empty() && accent_from_image(&self.st.set) {
            self.st.wall_cands = cands.clone();
            let job = self.wall_job;
            let pick = self.st.set.wall_pick as usize;
            self.wall_pick(pick);
            if self.wall_job != job {
                return;
            }
        }
        let same = self.wall_job_path == self.wall_shown_path;
        self.wall_shown_path = self.wall_job_path.clone();
        // та же картинка, перекрашенная (тон, затемнение от новой темы), пока идёт смена: подменить
        // цель, а смену вести дальше с того же шага — не начинать заново с середины
        let fading = self.st.wall_fade.is_some() && self.st.wall_prev.is_some() && !fade_done(&self.st);
        if same && fading {
            if let (Some(n), Some(o)) = (&new, &self.st.wall_prev) {
                if n.w == o.w && n.h == o.h {
                    self.st.wall = new;
                    *self.st.wall_mix.borrow_mut() = None;
                    self.wall_shown();
                    return;
                }
            }
        }
        // смена обоев: новые проступают клетками сквозь прошлые. Прошлые могли быть отложены в
        // уменьшенной копии (wall_park, пока листали выбор обоев) — растянуть её до размера новых
        // отложенную копию тех же обоев перечитали в полный размер — это не смена, проявлять нечего
        let reload = same && self.wall_reload;
        // посреди прошлой смены — от того, что сейчас на экране (её кадр), а не от её цели: иначе
        // ещё не проступившие клетки перескочили бы разом
        let from = if fading { shown(&self.st) } else { None };
        let cur = self.st.wall.take();
        let old = from.or(cur).filter(|_| !reload && self.st.set.anim() && self.st.set.wall_fade != "none").and_then(|o| {
            let n = new.as_ref()?;
            if o.w == n.w && o.h == n.h {
                Some(o)
            } else {
                Some(std::rc::Rc::new(stretch(&o, n.w, n.h)))
            }
        });
        self.st.wall_fade = old.is_some().then(std::time::Instant::now);
        self.st.wall_step.set(0);
        self.st.wall_prev = old;
        *self.st.wall_mix.borrow_mut() = None;
        self.st.wall = new;
        self.wall_shown();
        self.st.wall_cands = cands;
        if accent_from_image(&self.st.set) {
            let pick = self.st.set.wall_pick as usize;
            self.wall_pick(pick);
        }
    }

    /// новая картинка обоев на месте: поверхности, блокировка, панели, виджеты — перерисовать
    fn wall_shown(&mut self) {
        self.st.wall_parked = false;
        // карточка часов панели управления — на тех же обоях
        self.dirty(&SKind::Popup);
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
        self.dirty_panels();
        // новые обои: виджеты на них — в тон
        self.widget_contrast();
    }

    /// акцент — i-й по весу цвет обоев (номер запоминается для следующих), второй — к нему
    pub fn wall_pick(&mut self, i: usize) {
        let cands = &self.st.wall_cands;
        if cands.is_empty() {
            return;
        }
        let at = i.min(cands.len() - 1);
        let hex = format!("#{:06x}", cands[at]);
        let hex2 = second(cands, at).map(|c| format!("#{c:06x}")).unwrap_or_default();
        let s = &mut self.st.set;
        let changed = hex != s.accent || hex2 != s.accent2 || !s.wall_accent || s.wall_pick != i as u32;
        s.wall_accent = true;
        s.wall_pick = i as u32;
        s.accent = hex.clone();
        s.accent2 = hex2;
        if changed {
            self.st.hex.set(&hex);
            self.settings_changed();
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
            // сетка картинок папки обоев (walls.rs): картинку или саму папку
            "pick" | "dir" => {
                self.walls_open(true, *k == "dir");
                return true;
            }
            "reset" => {
                self.set_wall("");
                return true;
            }
            "accent" => {
                s.wall_accent = !s.wall_accent;
                if !s.wall_accent {
                    s.accent2.clear();
                }
            }
            _ => return false,
        }
        self.settings_changed();
        true
    }
}

#[cfg(test)]
mod tests {
    use yami_px::gfx::{Img, C};

    #[test]
    fn wallpaper_colours_by_weight_and_a_far_second() {
        // три четверти оранжевого, четверть синего, серое не голосует
        let img = image::RgbaImage::from_fn(64, 64, |x, y| match (x < 48, y < 60) {
            (_, false) => image::Rgba([120, 120, 120, 255]),
            (true, _) => image::Rgba([230, 140, 40, 255]),
            (false, _) => image::Rgba([40, 110, 230, 255]),
        });
        let c = super::candidates(&img);
        assert_eq!(c.len(), 2, "{c:x?}");
        let hue = |v: u32| if (v >> 16) & 255 > v & 255 { "warm" } else { "cold" };
        assert_eq!((hue(c[0]), hue(c[1])), ("warm", "cold"));
        assert_eq!(super::second(&c, 0), Some(c[1]));
        assert_eq!(super::second(&c, 1), Some(c[0]));
        assert_eq!(super::second(&c[..1], 0), None);
        // серая картинка — серый акцент, а не «ничего»
        assert_eq!(super::candidates(&image::RgbaImage::from_pixel(8, 8, image::Rgba([90, 90, 90, 255]))), vec![super::GREY]);
        // тёмно-красный узор по чёрному (ярче 13% нет ничего) — красный главный, серый последним
        let dark = image::RgbaImage::from_fn(64, 64, |x, y| if (x * 7 + y * 3) % 5 == 0 { image::Rgba([30, 2, 2, 255]) } else { image::Rgba([3, 0, 0, 255]) });
        let c = super::candidates(&dark);
        assert!(c.len() >= 2 && c[0] != super::GREY && *c.last().unwrap() == super::GREY, "{c:x?}");
        assert!((c[0] >> 16) & 255 > 150 && (c[0] >> 8) & 255 < 60, "{:06x}", c[0]);
    }
    #[test]
    fn generated_wallpaper_glows_in_the_accent() {
        let (bg, acc) = (C(0xff101010), C(0xffd0352d));
        let img = super::generate(bg, acc, C(0xff303030), false);
        assert_eq!(img.dimensions(), (480, 270));
        // правый нижний угол светится акцентом, середина — почти фон
        let corner = img.get_pixel(478, 268).0;
        let mid = img.get_pixel(240, 136).0;
        assert!(corner[0] > mid[0] && corner[0] > 0x10);
        assert!(mid[0] <= 0x30 && mid[1] <= 0x20);
    }

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
