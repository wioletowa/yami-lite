// Уведомления: карточки справа под баром, лентой. ЛКМ — действие по
// умолчанию, × — убрать, наведение — не гаснет, срочные висят. Все — в истории (календарь).
// Тут же всплывашка громкости и раскладки (OSD).

use crate::app::{App, SKind, State};
use crate::clock::local_now;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, Img, R};
use crate::ui::{Act, Cur, Tone, Ui};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use smithay_client_toolkit::shell::WaylandSurface;
use std::rc::Rc;
use std::time::Instant;

pub use crate::dbus::Incoming;

pub const W: i32 = 370;
/// самая большая картинка в карточке (превью скриншота); больше — уменьшается при отрисовке
pub const IMG_W: i32 = 112;
pub const IMG_H: i32 = 72;

/// место под картинку уведомления: её размер, вписанный в IMG_W×IMG_H
fn img_size(img: &Img) -> (i32, i32) {
    let (w, h) = (img.w.max(1), img.h.max(1));
    if w <= IMG_W && h <= IMG_H {
        return (w, h);
    }
    let k = (IMG_W as f32 / w as f32).min(IMG_H as f32 / h as f32);
    (((w as f32 * k).round() as i32).max(1), ((h as f32 * k).round() as i32).max(1))
}

pub struct Note {
    pub id: u32,
    pub app: String,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub critical: bool,
    pub image: Option<Rc<Img>>,
    pub born: Instant,
    pub time: String,
    pub life: f32,
    pub left: f32,
}

#[derive(Default)]
pub struct Notifs {
    pub popups: Vec<u32>,
    pub history: Vec<Note>,
    pub hover: bool,
    last: Option<Instant>,
}

impl Notifs {
    pub fn get(&self, id: u32) -> Option<&Note> {
        self.history.iter().find(|n| n.id == id)
    }
    pub fn remove(&mut self, id: u32, from_history: bool) {
        self.popups.retain(|x| *x != id);
        if from_history {
            self.history.retain(|n| n.id != id);
        }
    }
    pub fn resume(&mut self) {
        self.last = Some(Instant::now());
    }
}

/// высота карточки
fn card_h(ui_f: &mut font::Fonts, n: &Note) -> i32 {
    let (iw, ih) = n.image.as_deref().map(img_size).unwrap_or((0, 0));
    let tw = W - 24 - if iw > 0 { iw + 10 } else { 0 };
    let mut h = 18 + 6;
    let mut body = ui_f.wrap_height(PX, &n.summary, tw, 2);
    if !n.body.is_empty() {
        body += 3 + ui_f.wrap_height(PX, &n.body, tw, 4);
    }
    h += body.max(ih);
    if n.actions.iter().any(|a| a.0 != "default") {
        h += 6 + 22;
    }
    h + 22
}

pub fn size(st: &State, f: &mut font::Fonts) -> (i32, i32) {
    let mut h = 0;
    for id in &st.notifs.popups {
        if let Some(n) = st.notifs.get(*id) {
            h += card_h(f, n) + 8;
        }
    }
    (W + 56, h.max(1) + 8)
}

pub fn draw(ui: &mut Ui, st: &State, _w: i32, _h: i32) {
    let t = ui.t;
    let mut y = 0;
    for id in &st.notifs.popups {
        let Some(n) = st.notifs.get(*id) else { continue };
        let ch = card_h(ui.f, n);
        // въезд справа ступенями по 8 px
        let k = if st.set.anim() { (n.born.elapsed().as_secs_f32() / (0.16 * st.set.anim_speed.max(0.1))).min(1.0) } else { 1.0 };
        let step = (k * 4.0).ceil() / 4.0;
        if k < 1.0 {
            ui.anim = true;
        }
        let x0 = 48 - ((step * 48.0 / 8.0).round() as i32) * 8 + 4;
        ui.cv.opacity = step;
        let r = R::new(x0, y, W, ch);
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), if n.critical { t.ember } else if hov { t.red } else { t.border }).shadow(4));
        ui.cv.fill(R::new(r.x + 2, r.y + 2, 3, r.h - 4), if n.critical { t.ember } else { t.red });
        if !n.critical && n.life > 0.0 {
            // остаток — по часам, а не по тику (100 мс): полоса идёт шагами по 2 px каждый кадр;
            // под курсором время стоит, и перерисовывать нечего
            let ticking = !st.notifs.hover;
            let left = n.left - if ticking { st.notifs.last.map(|l| l.elapsed().as_secs_f32()).unwrap_or(0.0) } else { 0.0 };
            let lw = (((r.w - 9) as f32 * (left / n.life).clamp(0.0, 1.0)) / 2.0).round() as i32 * 2;
            ui.cv.fill(R::new(r.x + 5, r.bottom() - 4, lw, 2), t.red.fade(0.6));
            if ticking {
                ui.anim = true;
            }
        }
        ui.zone(r, Act::NotifClick(n.id), Cur::Pointer);
        draw_card(ui, n, R::new(r.x + 14, r.y + 8, r.w - 24, r.h - 16), true);
        y += ch + 8;
    }
    ui.cv.opacity = 1.0;
}

/// содержимое карточки (и в истории внутри календаря)
pub fn draw_card(ui: &mut Ui, n: &Note, r: R, close: bool) {
    let t = ui.t;
    let mut x = r.x;
    if !n.icon.is_empty() {
        let icon = n.icon.clone();
        ui.app_icon(&icon, x, r.y + 1, 16, 1.0);
        x += 22;
    }
    let app = if n.app.is_empty() { "система".to_string() } else { n.app.to_lowercase() };
    let aw = ui.text_in(PX, R::new(x, r.y, r.w / 2, 18), &app, if n.critical { t.ember } else { t.red });
    ui.text_in(PX, R::new(x + aw + 8, r.y, 60, 18), &n.time, t.text_faint);
    if close {
        ui.button(R::new(r.right() - 18, r.y, 18, 18), "", "close", Tone::Ghost, false, true, Act::NotifDel(n.id));
    }
    let mut y = r.y + 24;
    let mut tx = r.x;
    let (iw, ih) = n.image.as_deref().map(img_size).unwrap_or((0, 0));
    if let Some(img) = &n.image {
        if (iw, ih) == (img.w, img.h) {
            ui.cv.image(img, r.x, y, 1.0);
        } else {
            ui.cv.image_scaled(img, R::new(r.x, y, iw, ih), 1.0);
        }
        tx += iw + 10;
    }
    let tw = r.right() - tx;
    y += ui.wrap(PX, R::new(tx, y, tw, 40), &n.summary, t.text, 2);
    if !n.body.is_empty() {
        y += 3;
        y += ui.wrap(PX, R::new(tx, y, tw, 80), &n.body, t.text_dim, 4);
    }
    y = y.max(r.y + 24 + ih);
    let acts: Vec<&(String, String)> = n.actions.iter().filter(|a| a.0 != "default").collect();
    if !acts.is_empty() {
        y += 6;
        let mut ax = r.x;
        for (k, label) in acts {
            let bw = ui.w(PX, label) + 20;
            ui.button(R::new(ax, y, bw, 22), label, "", Tone::Plain, false, true, Act::NotifAct(n.id, k.clone()));
            ax += bw + 6;
        }
    }
}

pub fn draw_osd(ui: &mut Ui, st: &State, w: i32, _h: i32) {
    let t = ui.t;
    let Some((kind, _)) = st.osd else { return };
    let fr = R::new(0, 0, w - 8, 56);
    ui.cv.pxbox(fr, &BoxStyle::new(t.glass_panel(), t.border).shadow(4));
    if kind == "volume" {
        let a = &st.audio;
        ui.cv.icon(if a.muted { "mute" } else { "speaker" }, 12, 11, 2, t.red);
        ui.text_in(PX, R::new(34, 8, 100, 20), "звук", t.text_dim);
        let v = if a.muted { "тишина".to_string() } else { format!("{}%", (a.volume * 100.0).round()) };
        ui.text_right(PX, R::new(12, 8, fr.w - 24, 20), &v, if a.muted { t.text_faint } else { t.text });
        ui.cv.gauge(R::new(12, 34, fr.w - 24, 10), 20, if a.muted { 0.0 } else { a.volume as f32 }, if a.volume > 1.0 { t.ember } else { t.red }, t.surface_hi.lerp(t.border, 0.45), 2);
    } else if kind == "curtain" {
        let on = st.curtain.is_some();
        ui.cv.icon(if on { "lock" } else { "eye" }, 12, 11, 2, t.red);
        ui.text_in(PX, R::new(34, 8, fr.w - 46, 20), if on { "экран скрыт в трансляции" } else { "экран снова виден" }, t.text);
        ui.text_in(PX, R::new(12, 32, fr.w - 24, 18), if on { "зрители видят шторку, ты — экран" } else { "трансляция показывает рабочий стол" }, t.text_faint);
    } else if kind == "snap" {
        let left = st.snap_countdown.map(|(t0, d)| (d as u64).saturating_sub(t0.elapsed().as_secs())).unwrap_or(0);
        ui.cv.icon("camera", 12, 11, 2, t.red);
        ui.text_in(PX, R::new(34, 8, fr.w - 80, 20), "скриншот через", t.text_dim);
        ui.text_right(font::PX24, R::new(12, 4, fr.w - 24, 28), &format!("{left}"), t.text);
        let d = st.snap_countdown.map(|c| c.1).unwrap_or(1).max(1);
        ui.cv.gauge(R::new(12, 34, fr.w - 24, 10), d as i32, left as f32 / d as f32, t.red, t.surface_hi.lerp(t.border, 0.45), 2);
    } else {
        let b = R::new(10, 12, 44, 32);
        ui.cv.pxbox(b, &BoxStyle::new(t.red, t.ember).bevel(true));
        ui.text_center(font::PX24, b, &st.niri.kb_short(), t.on_accent);
        let name = st.niri.kb_names.get(st.niri.kb_idx).cloned().unwrap_or_default();
        ui.text_in(PX, R::new(66, 10, fr.w - 76, 20), &name, t.text);
        let mut x = 66;
        for i in 0..st.niri.kb_names.len() {
            let on = i == st.niri.kb_idx;
            ui.cv.fill(R::new(x, 36, if on { 16 } else { 8 }, 4), if on { t.red } else { t.border });
            x += if on { 20 } else { 12 };
        }
    }
}

impl App {
    pub fn on_notif(&mut self, n: Incoming) {
        let tm = local_now();
        let critical = n.urgency >= 2;
        let life = if n.timeout > 0 { n.timeout as f32 / 1000.0 } else { self.st.set.notif_timeout as f32 };
        let note = Note {
            id: n.id,
            app: n.app,
            icon: n.icon,
            summary: n.summary,
            body: n.body,
            actions: n.actions,
            critical,
            image: n.image.filter(|_| self.st.set.notif_images).map(Rc::new),
            born: Instant::now(),
            time: format!("{:02}:{:02}", tm.hour, tm.min),
            life,
            left: life,
        };
        let id = note.id;
        let quiet = self.quiet();
        let ns = &mut self.st.notifs;
        if let Some(x) = ns.history.iter_mut().find(|x| x.id == id) {
            // замена (replaces_id): обновить на месте, без нового въезда
            let born = x.born;
            *x = note;
            x.born = born;
        } else {
            ns.history.insert(0, note);
            ns.history.truncate(self.st.set.notif_history.clamp(10, 500) as usize);
        }
        if (!quiet || critical) && !self.st.locked {
            ns.popups.retain(|x| *x != id);
            ns.popups.insert(0, id);
            // лишние уходят в историю
            ns.popups.truncate(self.st.set.notif_max.clamp(1, 8) as usize);
            ns.resume();
        }
        self.sync_notifs();
        self.dirty(&SKind::Bar);
        self.dirty(&SKind::Popup);
    }

    /// таймер: жизнь карточек; true — что-то истекло
    pub fn notif_tick(&mut self) -> bool {
        let ns = &mut self.st.notifs;
        if ns.popups.is_empty() {
            ns.last = None;
            return false;
        }
        let now = Instant::now();
        let dt = ns.last.map(|l| now.duration_since(l).as_secs_f32()).unwrap_or(0.0);
        ns.last = Some(now);
        let mut gone = vec![];
        if !ns.hover {
            for id in ns.popups.clone() {
                if let Some(n) = ns.history.iter_mut().find(|n| n.id == id) {
                    if n.critical {
                        continue;
                    }
                    n.left -= dt;
                    if n.left <= 0.0 {
                        gone.push(id);
                    }
                }
            }
        }
        for id in &gone {
            ns.popups.retain(|x| x != id);
            crate::dbus::notif_closed(*id, 1);
        }
        self.dirty(&SKind::Notifs);
        !gone.is_empty()
    }

    /// карточки на своём месте (notifPos): у края, с отступом 10; исключённые зоны бара и дока
    /// niri учитывает сам (exclusive 0), поэтому сверху — под баром, снизу — над доком
    fn notif_place(&self) -> (Anchor, (i32, i32, i32, i32)) {
        // от края — notifOffset; бар niri сам вычитает (исключительная зона), карточки — под ним
        let o = self.st.set.num("notif_offset");
        match self.st.set.notif_pos.as_str() {
            "top-left" => (Anchor::TOP | Anchor::LEFT, (o, 0, 0, 6)),
            "top" => (Anchor::TOP, (o, 0, 0, 0)),
            "bottom-right" => (Anchor::BOTTOM | Anchor::RIGHT, (0, 6, o, 0)),
            "bottom-left" => (Anchor::BOTTOM | Anchor::LEFT, (0, 0, o, 6)),
            "bottom" => (Anchor::BOTTOM, (0, 0, o, 0)),
            _ => (Anchor::TOP | Anchor::RIGHT, (o, 6, 0, 0)),
        }
    }

    /// место или отступ карточек (OSD) поменялись: поверхность — заново, якорь layer-shell на лету
    /// не меняется
    pub fn renew_notif(&mut self, kind: SKind) {
        for id in self.find(&kind) {
            self.destroy(id);
        }
        if kind == SKind::Notifs {
            self.sync_notifs();
        }
    }

    pub fn sync_notifs(&mut self) {
        let empty = self.st.notifs.popups.is_empty() || self.st.locked;
        let ids = self.find(&SKind::Notifs);
        if empty {
            for id in ids {
                self.destroy(id);
            }
            self.st.notifs.hover = false;
            return;
        }
        let (w, h) = size(&self.st, &mut self.res.fonts);
        if ids.is_empty() {
            let out = self.focus_out();
            let (anchor, margin) = self.notif_place();
            self.mk(SKind::Notifs, &out, Layer::Overlay, "yami-notif", anchor, (w, h), margin, 0, KeyboardInteractivity::None);
        } else {
            for id in ids {
                if let Some(s) = self.surfs.get_mut(&id) {
                    if s.h != h || s.w != w {
                        s.layer.set_size(w as u32, h as u32);
                        s.layer.commit();
                    }
                    s.dirty = true;
                }
            }
        }
    }

    pub fn notif_click(&mut self, id: u32, default: bool) {
        if default {
            if let Some(n) = self.st.notifs.get(id) {
                if n.actions.iter().any(|a| a.0 == "default") && !self.shot_note_action(id, "default") {
                    crate::dbus::notif_action(id, "default");
                }
            }
        }
        self.st.notifs.remove(id, false);
        crate::dbus::notif_closed(id, 2);
        self.sync_notifs();
    }
}

/// места карточек уведомлений (notifPos) и OSD (osdPos) по кругу в настройках: значение, подпись
pub const POS: &[(&str, &str)] =
    &[("top-right", "справа сверху"), ("top-left", "слева сверху"), ("top", "сверху по центру"), ("bottom-right", "справа снизу"), ("bottom-left", "слева снизу"), ("bottom", "снизу по центру")];
pub const OSD_POS: &[(&str, &str)] = &[("bottom", "снизу"), ("top", "сверху"), ("center", "по центру")];

impl App {
    /// OSD громкости, яркости, раскладки, таймера снимка (osdPos)
    pub fn show_osd(&mut self, kind: &'static str) {
        if self.st.locked {
            return;
        }
        let fresh = self.st.osd.is_none();
        self.st.osd = Some((kind, Instant::now()));
        if fresh || self.find(&SKind::Osd).is_empty() {
            let out = self.focus_out();
            // osdPos: снизу над доком, сверху под баром, по центру экрана
            let (anchor, margin) = match self.st.set.osd_pos.as_str() {
                "top" => (Anchor::TOP, (self.bar_bottom(&out) + self.st.set.num("osd_offset"), 0, 0, 0)),
                "center" => (Anchor::empty(), (0, 0, 0, 0)),
                _ => (Anchor::BOTTOM, (0, 0, self.bar_reserve_bottom(&out) + self.st.set.num("osd_offset"), 0)),
            };
            let id = self.mk(SKind::Osd, &out, Layer::Overlay, "yami-osd", anchor, (308, 64 + 8), margin, -1, KeyboardInteractivity::None);
            if let Some(s) = self.surfs.get(&id) {
                if let Ok(r) = smithay_client_toolkit::compositor::Region::new(&self.compositor) {
                    s.layer.wl_surface().set_input_region(Some(r.wl_region()));
                }
            }
        }
        self.dirty(&SKind::Osd);
    }
}
