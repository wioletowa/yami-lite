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

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    // часы на обоях: картинка «как обложка» в карточке, поверх — вуаль
    let card = R::new(b.x, b.y, b.w, 120);
    ui.cv.pxbox(card, &BoxStyle::new(t.code_bg, t.border));
    let inner = card.inset(2);
    ui.cv.push_clip(inner);
    if let Some(img) = &st.wall {
        let k = (inner.w as f32 / img.w.max(1) as f32).max(inner.h as f32 / img.h.max(1) as f32);
        let (dw, dh) = ((img.w as f32 * k).ceil() as i32, (img.h as f32 * k).ceil() as i32);
        ui.cv.image_scaled(img, R::new(inner.x + (inner.w - dw) / 2, inner.y + (inner.h - dh) / 2, dw, dh), 1.0);
    }
    ui.cv.fill(inner, yami_px::face::veil(&t, Some(0.45)));
    ui.cv.pop_clip();
    let tm = local_now();
    let face = font::px(48);
    let time = format!("{:02}:{:02}", tm.hour, tm.min);
    let tw = ui.w(face, &time);
    let ty = card.y + 14;
    ui.text(face, card.x + (card.w - tw) / 2, ty, &time, t.text_hi);
    let date = crate::tf!("{}, {} {}", "{}, {} {}", crate::i18n::tr(crate::clock::RU_DAY_FULL[tm.wday as usize]), tm.day, crate::i18n::tr(crate::clock::RU_MON[(tm.mon - 1) as usize]));
    let ly = ty + ui.f.line(face) + 2;
    ui.text_center(PX, R::new(card.x, ly, card.w, 18), &date, t.text);
    ui.text_center(PX, R::new(card.x, ly + 20, card.w, 18), &uptime(), t.text_dim);
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
    // плитки: две колонки, ряды по месту
    let gx = b.x + 2 * (sw + 10);
    let gw = b.right() - gx;
    let cols = 2;
    let tsz = (gw - 10) / cols;
    let rows = ((h + 10) / (tsz.min(h) / 2 * 2 + 10)).max(1);
    let th = (h - (rows - 1) * 10) / rows;
    for (i, k) in st.set.control_tiles.iter().take((cols * rows) as usize).enumerate() {
        let Some((_, name, icon)) = TILES.iter().find(|x| x.0 == k) else { continue };
        let (c, r) = (i as i32 % cols, i as i32 / cols);
        let rr = R::new(gx + c * (tsz + 10), y + r * (th + 10), tsz, th);
        let lit = on(st, k);
        let hov = ui.hover(rr);
        ui.cv.pxbox(rr, &BoxStyle::new(if lit { t.red } else if hov { t.surface_hi } else { t.surface }, if lit { t.ember } else if hov { t.red } else { t.border }).bevel(true));
        let fg = if lit { t.on_accent } else { t.text };
        ui.cv.icon(icon, rr.x + rr.w / 2 - 7, rr.y + rr.h / 2 - 16, 2, fg);
        ui.text_center(PX, R::new(rr.x + 4, rr.y + rr.h / 2 + 4, rr.w - 8, 20), name, fg);
        ui.zone(rr, Act::CtlTile(k.clone()), Cur::Pointer);
    }
}

impl App {
    /// ПКМ по пустому месту бара: открыть панель (открыта — закрыть)
    pub fn toggle_control(&mut self, out: &str, x: i32) {
        self.open_popup("control", out, x);
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
            "settings" => {
                self.close_popup();
                self.open_settings();
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
