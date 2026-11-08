// Панель управления: ПКМ по пустому месту бара (всплывашка «control»). Сверху — часы на обоях под
// лёгкой вуалью и аптайм системы; снизу — два вертикальных пиксельных ползунка (громкость и яркость,
// как в OSD: клетки, значок, процент) и плитки на выбор (controlTiles, от 1 до 6).

use crate::app::{App, State};
use crate::clock::local_now;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Ui};

pub const SIZE: (i32, i32) = (420, 336);

/// плитки: ключ, подпись, значок
pub const TILES: &[(&str, &str, &str)] = &[
    ("wifi", "Wi-Fi", "wifi"),
    ("bt", "Bluetooth", "bt"),
    ("dnd", "не беспокоить", "bellx"),
    ("night", "ночной свет", "moon"),
    ("mic", "микрофон", "mic"),
    ("theme", "светлая тема", "sun"),
    ("mute", "звук", "speaker"),
    ("settings", "настройки", "gear"),
    ("lock", "заблокировать", "lock"),
    ("power", "питание", "power"),
];

/// плитка включена (горит)
fn on(st: &State, k: &str) -> bool {
    match k {
        "wifi" => st.net.wifi_on,
        "bt" => st.bt.powered,
        "dnd" => st.quiet(),
        "night" => st.night.on,
        "mic" => !st.audio.mic_muted,
        "theme" => st.set.light,
        "mute" => !st.audio.muted,
        _ => false,
    }
}

/// аптайм системы из /proc/uptime
fn uptime() -> String {
    let s = std::fs::read_to_string("/proc/uptime").ok().and_then(|t| t.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(0.0) as u64;
    let (d, h, m) = (s / 86400, s / 3600 % 24, s / 60 % 60);
    if d > 0 {
        crate::tf!("работает {} д {} ч {} мин", "up {} d {} h {} min", d, h, m)
    } else {
        crate::tf!("работает {} ч {} мин", "up {} h {} min", h, m)
    }
}

/// вертикальный ползунок: клетки снизу вверх, значок и процент под ними
fn vslider(ui: &mut Ui, r: R, icon: &str, v: f32, off: bool, act: Act) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, if hov { t.red } else { t.border }).bevel(true));
    let track = R::new(r.x + 10, r.y + 10, r.w - 20, r.h - 64);
    let cells = 12;
    let ch = (track.h - (cells - 1) * 2) / cells;
    let lit = if off { 0 } else { (v.clamp(0.0, 1.0) * cells as f32).round() as i32 };
    for i in 0..cells {
        let y = track.bottom() - (i + 1) * ch - i * 2;
        ui.cv.fill(R::new(track.x, y, track.w, ch), if i < lit { t.red } else { t.surface_hi.lerp(t.border, 0.45) });
    }
    ui.cv.icon(icon, r.x + r.w / 2 - 7, r.bottom() - 48, 2, if off { t.text_faint } else { t.red });
    let pct = if off { "—".to_string() } else { format!("{}%", (v * 100.0).round()) };
    ui.text_center(PX, R::new(r.x, r.bottom() - 28, r.w, 20), &pct, if off { t.text_faint } else { t.text });
    ui.zone(R::new(track.x - 6, track.y, track.w + 12, track.h), act, Cur::Pointer);
}

/// значок и подпись плитки: у темы — по тому, какая сейчас
fn face_of(st: &State, k: &str, name: &'static str, icon: &'static str) -> (&'static str, &'static str) {
    match k {
        "theme" if st.set.light => ("sun", "светлая тема"),
        "theme" => ("moon", "тёмная тема"),
        "dnd" if !st.quiet() => ("bell", name),
        "mute" if st.audio.muted => ("mute", name),
        _ => (icon, name),
    }
}

/// сетка плиток: одна — на всё место, две — рядом, больше — в два ряда; неполный ряд растягивается
fn tile_rects(area: R, n: usize) -> Vec<R> {
    const GAP: i32 = 10;
    let rows = if n <= 2 { 1 } else { 2 };
    let per = n.div_ceil(rows);
    let th = (area.h - (rows as i32 - 1) * GAP) / rows as i32;
    let mut out = vec![];
    for r in 0..rows {
        let cnt = per.min(n - r * per);
        if cnt == 0 {
            break;
        }
        let tw = (area.w - (cnt as i32 - 1) * GAP) / cnt as i32;
        for c in 0..cnt {
            out.push(R::new(area.x + c as i32 * (tw + GAP), area.y + r as i32 * (th + GAP), tw, th));
        }
    }
    out
}

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    // часы на обоях: картинка «как обложка» в карточке, поверх — вуаль; текст — от левого края
    let card = R::new(b.x, b.y, b.w, 120);
    ui.cv.pxbox(card, &BoxStyle::new(t.code_bg, t.border));
    let inner = card.inset(2);
    ui.cv.push_clip(inner);
    if let Some(img) = &crate::wall::shown(st) {
        let k = (inner.w as f32 / img.w.max(1) as f32).max(inner.h as f32 / img.h.max(1) as f32);
        let (dw, dh) = ((img.w as f32 * k).ceil() as i32, (img.h as f32 * k).ceil() as i32);
        ui.cv.image_scaled(img, R::new(inner.x + (inner.w - dw) / 2, inner.y + (inner.h - dh) / 2, dw, dh), 1.0);
    }
    ui.cv.fill(inner, yami_px::face::veil(&t, Some(0.45)));
    ui.cv.pop_clip();
    // щелчок по обоям — выбор обоев
    if ui.hover(card) {
        ui.cv.outline(card, t.red, 2);
        ui.cv.icon("image", card.right() - 26, card.y + 10, 2, t.text);
    }
    ui.zone(card, Act::CtlWall, Cur::Pointer);
    let tm = local_now();
    let face = font::px(48);
    // формат — как у часов бара (настройки → бар → вид: часы с секундами)
    let time = if st.set.clock_seconds { format!("{:02}:{:02}:{:02}", tm.hour, tm.min, tm.sec) } else { format!("{:02}:{:02}", tm.hour, tm.min) };
    let (lx, ty) = (card.x + 16, card.y + 14);
    ui.text(face, lx, ty, &time, t.text_hi);
    let date = crate::tf!("{}, {} {}", "{}, {} {}", crate::i18n::tr(crate::clock::RU_DAY_FULL[tm.wday as usize]), tm.day, crate::i18n::tr(crate::clock::RU_MON[(tm.mon - 1) as usize]));
    let ly = ty + ui.f.line(face) + 2;
    ui.text_in(PX, R::new(lx, ly, card.w - 32, 18), &date, t.text);
    ui.text_in(PX, R::new(lx, ly + 20, card.w - 32, 18), &uptime(), t.text_dim);
    // низ: два ползунка и плитки
    let y = card.bottom() + 12;
    let h = b.bottom() - y;
    let sw = 64;
    let a = &st.audio;
    vslider(ui, R::new(b.x, y, sw, h), if a.muted { "mute" } else { "speaker" }, a.volume as f32, a.muted, Act::CtlVol);
    match st.leds.bright {
        Some(v) => vslider(ui, R::new(b.x + sw + 10, y, sw, h), "sun", v, false, Act::CtlBright),
        None => vslider(ui, R::new(b.x + sw + 10, y, sw, h), "sun", 0.0, true, Act::Block),
    }
    let gx = b.x + 2 * (sw + 10);
    let tiles: Vec<(&String, &str, &str)> = st.set.control_tiles.iter().filter_map(|k| TILES.iter().find(|x| x.0 == k).map(|x| (k, x.1, x.2))).collect();
    // три и больше в ряд — подписей нет ни у одной: значки одного размера, без подписи у части
    let per_row = if tiles.len() <= 2 { tiles.len() } else { tiles.len().div_ceil(2) };
    let labels = per_row < 3;
    for (rr, (k, name, icon)) in tile_rects(R::new(gx, y, b.right() - gx, h), tiles.len()).into_iter().zip(tiles) {
        let (icon, name) = face_of(st, k, name, icon);
        let lit = on(st, k);
        let hov = ui.hover(rr);
        ui.cv.pxbox(rr, &BoxStyle::new(if lit { t.red } else if hov { t.surface_hi } else { t.surface }, if lit { t.ember } else if hov { t.red } else { t.border }).bevel(true));
        let fg = if lit { t.on_accent } else { t.text };
        // подпись — если влезает по ширине и есть место под значком; нет — один значок по центру
        let label = crate::i18n::tr(name);
        let fits = labels && ui.f.width(PX, label) <= rr.w - 12 && rr.h >= 64;
        let cell = if rr.w.min(rr.h) >= 120 { 4 } else if rr.w.min(rr.h) >= 64 { 3 } else { 2 };
        let iy = if fits { rr.cy() - cell * 7 / 2 - 10 } else { rr.cy() - cell * 7 / 2 };
        ui.cv.icon(icon, rr.x + (rr.w - cell * 7) / 2, iy, cell, fg);
        if fits {
            ui.text_center(PX, R::new(rr.x + 4, iy + cell * 7 + 6, rr.w - 8, 20), name, fg);
        }
        ui.zone(rr, Act::CtlTile(k.clone()), Cur::Pointer);
    }
}

impl App {
    /// ПКМ по пустому месту бара: открыть панель (открыта — закрыть)
    pub fn toggle_control(&mut self, out: &str, x: i32) {
        self.open_popup("control", out, x);
    }

    /// ползунок громкости: на экране сразу, системе — следом
    pub fn ctl_volume(&mut self, v: f64) {
        let v = v.clamp(0.0, 1.0);
        self.st.audio.volume = v;
        self.st.ctl_hold = Some(std::time::Instant::now() + std::time::Duration::from_millis(1200));
        crate::audio::set_volume(v);
        self.dirty(&crate::app::SKind::Popup);
    }

    pub fn ctl_brightness(&mut self, v: f32) {
        let v = v.clamp(0.01, 1.0);
        self.st.leds.bright = Some(v);
        self.st.ctl_hold = Some(std::time::Instant::now() + std::time::Duration::from_millis(1200));
        crate::leds::set_brightness(v);
        self.dirty(&crate::app::SKind::Popup);
    }

    /// ползунок панели ещё «держит» значения
    pub fn ctl_holding(&self) -> bool {
        self.st.ctl_hold.is_some_and(|t| std::time::Instant::now() < t)
    }

    pub fn control_tile(&mut self, k: &str) {
        match k {
            "wifi" => {
                let on = !self.st.net.wifi_on;
                crate::net::wifi_cmd(vec!["radio".into(), "wifi".into(), if on { "on" } else { "off" }.into()], self.tx.clone(), self.st.net.clone());
            }
            "bt" => crate::net::bt_power(&self.st.bt.adapter.clone(), !self.st.bt.powered, self.tx.clone()),
            "dnd" => {
                self.st.set.dnd = !self.st.set.dnd;
                self.settings_changed();
            }
            "night" => self.night_toggle(),
            "mic" => crate::audio::toggle_mic(),
            "mute" => crate::audio::toggle_mute(),
            "theme" => {
                self.st.set.light = !self.st.set.light;
                self.settings_changed();
            }
            // как кнопка в баре: открыто — закрыть
            "settings" => {
                self.close_popup();
                self.toggle_settings();
            }
            "lock" => {
                self.close_popup();
                self.lock_now();
            }
            "power" => {
                self.close_popup();
                self.toggle_modal(crate::app::Modal::Power);
            }
            _ => {}
        }
    }
}
