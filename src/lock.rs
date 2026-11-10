// Экран блокировки (ext-session-lock): лицо экрана входа (yami_px::face: обои под вуалью, часы,
// дата), поле пароля. Ошибка — поле вздрагивает ступенями. Остальные
// мониторы — только часы.

use crate::app::{App, State};
use crate::font::PX;
use crate::gfx::BoxStyle;
use std::time::Instant;
use crate::app::*;
use crate::gfx::{Canvas, R};
use crate::ui::{Act, Cur, Ui};

use smithay_client_toolkit::{
    seat::keyboard::{KeyEvent, Keysym},
    shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;


pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32, main: bool) {
    let t = ui.t;
    // то же лицо, что у экрана входа (yami-greet): обои под вуалью, часы, дата
    yami_px::face::backdrop(ui, st.wall.as_deref(), w, h);
    let dy = yami_px::face::clock(ui, w, h) - 30;
    if !main {
        return;
    }
    // поле пароля
    let fw = 380;
    let mut fx = (w - fw) / 2;
    if let Some(s) = st.lock_shake {
        let e = s.elapsed().as_millis() as i32;
        if e < 420 {
            fx += [8, -8, 6, -6, 4, -4, 0][(e / 60) as usize % 7];
            ui.anim = true;
        }
    }
    let fy = dy + 70;
    let user = std::env::var("USER").unwrap_or_default();
    ui.cv.icon("lock", fx, fy - 26, 2, t.red);
    ui.text_in(PX, R::new(fx + 22, fy - 30, fw - 22, 20), &user, t.text);
    let fr = R::new(fx, fy, fw, 36);
    ui.cv.pxbox(R::new(fr.x - 6, fr.y - 6, fr.w + 12, fr.h + 12), &BoxStyle::new(t.glass_panel(), t.border).shadow(4));
    ui.field(fr, &st.lock_field, "пароль", true, true, Act::LockField);
    let status = if !st.lock_status.is_empty() {
        st.lock_status.clone()
    } else if st.lock_fails > 0 {
        crate::tf!("неверно ×{}", "wrong ×{}", st.lock_fails)
    } else {
        "enter — войти".to_string()
    };
    let sc = if st.lock_fails > 0 && st.lock_status.is_empty() { t.ember } else { t.text_faint };
    ui.text_center(PX, R::new(fx, fy + 48, fw, 20), &status, sc);
    ui.zone(R::new(0, 0, w, h), Act::Block, Cur::Default);
    ui.zone(fr, Act::LockField, Cur::Text);
    let mut y = fy + 84;
    if !st.media.title.is_empty() {
        player(ui, st, R::new(fx, y, fw, 64));
        y += 80;
    }
    notes(ui, st, R::new(fx, y, fw, h - y - 24));
}

/// маленький плеер: обложка, название, исполнитель, назад · пауза · вперёд
fn player(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let m = &st.media;
    ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), t.border).shadow(4));
    let a = R::new(r.x + 8, r.y + 8, 48, 48);
    match st.art.get(&m.art).cloned().flatten() {
        Some(img) => {
            ui.cv.image_scaled(&img, a, 1.0);
            ui.cv.outline(a, t.border, 1);
        }
        None => {
            ui.cv.pxbox(a, &BoxStyle::new(t.surface, t.border));
            ui.cv.icon_in("music", a, 3, t.dim);
        }
    }
    let bx = r.right() - 8 - 3 * 30 - 8;
    let tw = bx - a.right() - 18;
    ui.text_in(PX, R::new(a.right() + 10, r.y + 12, tw, 18), &m.title, t.text);
    ui.text_in(PX, R::new(a.right() + 10, r.y + 32, tw, 18), if m.artist.is_empty() { &m.player } else { &m.artist }, t.text_dim);
    let by = r.y + (r.h - 26) / 2;
    ui.button(R::new(bx, by, 28, 26), "", "prev", crate::ui::Tone::Ghost, false, true, Act::MediaPrev);
    ui.button(R::new(bx + 32, by, 30, 26), "", if m.playing() { "pause" } else { "play" }, crate::ui::Tone::Primary, false, true, Act::MediaPlay);
    ui.button(R::new(bx + 66, by, 28, 26), "", "next", crate::ui::Tone::Ghost, false, true, Act::MediaNext);
}

/// уведомления, пришедшие пока заперто: только программа, её значок и время — текста нет
fn notes(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let Some(since) = st.lock_at else { return };
    let list: Vec<&crate::notif::Note> = st.notifs.history.iter().filter(|n| n.born >= since).collect();
    if list.is_empty() || r.h < 40 {
        return;
    }
    let mut y = r.y;
    let more = list.len().saturating_sub(5);
    for n in list.iter().take(5) {
        if y + 30 > r.bottom() {
            break;
        }
        let row = R::new(r.x, y, r.w, 30);
        ui.cv.pxbox(row, &BoxStyle::new(t.glass_panel(), if n.critical { t.ember } else { t.border }));
        let mut x = row.x + 8;
        if !n.icon.is_empty() {
            let icon = n.icon.clone();
            ui.app_icon(&icon, x, row.y + 7, 16, 1.0);
            x += 24;
        }
        let app = if n.app.is_empty() { crate::i18n::tr("система").to_string() } else { n.app.clone() };
        ui.text_in(PX, R::new(x, row.y, row.w - (x - row.x) - 70, row.h), &app, t.text);
        ui.text_right(PX, R::new(row.x, row.y, row.w - 10, row.h), &n.time, t.text_faint);
        y += 34;
    }
    if more > 0 {
        ui.text_center(PX, R::new(r.x, y, r.w, 20), &crate::tf!("и ещё {}", "and {} more", more), t.text_faint);
    }
}

impl App {
    pub fn lock_key(&mut self, e: &KeyEvent) {
        if self.st.lock_status == "проверяю…" {
            return;
        }
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter => {
                let mut pass = self.st.lock_field.text.clone();
                if pass.is_empty() {
                    return;
                }
                self.st.lock_status = "проверяю…".into();
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let ok = crate::pam::check(&pass);
                    yami_px::ui::wipe(&mut pass);
                    let _ = tx.send(crate::app::Msg::Pam(ok));
                });
            }
            Keysym::Escape => self.st.lock_field.clear(),
            _ => {
                let mods = self.mods;
                App::edit_field(&mut self.st.lock_field, e, &mods);
            }
        }
        for l in self.lock_surfs.iter_mut() {
            l.dirty = true;
        }
    }

    pub fn on_pam(&mut self, ok: bool) {
        self.st.lock_status.clear();
        if ok {
            self.st.lock_fails = 0;
            self.unlock();
            self.sync_widgets();
            self.sync_dock();
        } else {
            self.st.lock_fails += 1;
            self.st.lock_field.wipe();
            self.st.lock_shake = Some(Instant::now());
            for l in self.lock_surfs.iter_mut() {
                l.dirty = true;
            }
        }
    }
}

impl App {
    // ── блокировка ──────────────────────────────────────────────────────────
    pub fn lock_now(&mut self) {
        if self.lock.is_some() || self.st.locked {
            return;
        }
        self.close_all();
        self.st.locked = true;
        self.st.lock_at = Some(Instant::now());
        if let Some(en) = self.st.niri.kb_english().filter(|&en| en != self.st.niri.kb_idx) {
            self.st.lock_kb = Some(self.st.niri.kb_idx);
            self.st.niri.act(serde_json::json!({"SwitchLayout": {"layout": {"Index": en}}}));
        }
        // первый кадр — на копии обоев, полная догонит
        self.wall_need();
        self.st.lock_field.wipe();
        self.st.lock_status.clear();
        match self.lock_state.lock(&self.qh) {
            Ok(l) => {
                self.lock = Some(l);
                self.sync_lock_surfs();
            }
            Err(_) => {
                self.st.locked = false;
                self.restore_kb();
            }
        }
    }

    /// экран блокировки на каждом мониторе, и на подключённом уже под блокировкой. Номера
    /// поверхностей — индексы в lock_surfs, поэтому ушедшие мониторы не вынимаются до разблокировки
    pub(crate) fn sync_lock_surfs(&mut self) {
        let Some(l) = self.lock.as_ref() else { return };
        for o in self.outputs.outputs().collect::<Vec<_>>() {
            if self.lock_surfs.iter().any(|ls| ls.wl == o) || self.lock_surfs.len() >= 64 {
                continue;
            }
            let name = self.outputs.info(&o).and_then(|i| i.name.clone()).unwrap_or_default();
            let surface = self.compositor.create_surface(&self.qh);
            let sid = LOCK_BASE + self.lock_surfs.len() as u32;
            let frac = self.frac_mgr.as_ref().map(|m| m.get_fractional_scale(&surface, &self.qh, sid));
            let vp = self.viewporter.as_ref().map(|v| v.get_viewport(&surface, &self.qh, ()));
            let ls = l.create_lock_surface(surface, &o, &self.qh);
            self.lock_surfs.push(LockSurf { s: ls, wl: o, output: name, w: 0, h: 0, pool: None, zones: vec![], dirty: false, scale: 1.0, frac, vp });
        }
    }
    pub fn unlock(&mut self) {
        if let Some(l) = self.lock.take() {
            l.unlock();
        }
        self.drop_lock_surfs();
        self.st.locked = false;
        self.st.lock_field.wipe();
        self.restore_kb();
        let _ = self.conn.roundtrip();
        self.dirty_all();
    }

    /// вернуть раскладку, что была до блокировки, если на экране пароля её не меняли
    fn restore_kb(&mut self) {
        let Some(prev) = self.st.lock_kb.take() else { return };
        if self.st.niri.kb_english() == Some(self.st.niri.kb_idx) {
            self.st.niri.act(serde_json::json!({"SwitchLayout": {"layout": {"Index": prev}}}));
        }
    }

    /// поверхности блокировки — со своими масштабом и viewport
    pub(crate) fn drop_lock_surfs(&mut self) {
        for l in self.lock_surfs.drain(..) {
            if let Some(f) = l.frac {
                f.destroy();
            }
            if let Some(v) = l.vp {
                v.destroy();
            }
        }
    }

    pub(crate) fn draw_lock(&mut self, i: usize) {
        let main = self.lock_surfs[i].output == self.focus_out() || self.lock_surfs.len() == 1;
        let ls = &mut self.lock_surfs[i];
        ls.dirty = false;
        let (w, h) = (ls.w, ls.h);
        if w <= 0 || h <= 0 {
            return;
        }
        let (pw, ph) = if ls.vp.is_none() || ls.scale == 1.0 { (w, h) } else { crate::gfx::phys(w, h, ls.scale) };
        let up = (pw, ph) != (w, h);
        let need = (pw * ph * 4) as usize;
        if ls.pool.as_ref().map(|p| p.len() < need).unwrap_or(true) {
            ls.pool = SlotPool::new(need * 2, &self.shm).ok();
        }
        let Some(pool) = ls.pool.as_mut() else { return };
        let Ok((buffer, bytes)) = pool.create_buffer(pw, ph, pw * 4, wl_shm::Format::Argb8888) else { return };
        let (out_ptr, out_len) = (bytes.as_mut_ptr() as *mut u32, bytes.len() / 4);
        let mut scratch: Vec<u32> = if up { vec![0; (w * h) as usize] } else { vec![] };
        let buf: &mut [u32] = if up { &mut scratch } else { unsafe { std::slice::from_raw_parts_mut(out_ptr, out_len) } };
        let sid = LOCK_BASE + i as u32;
        let ptr = self.ptr.filter(|p| p.0 == sid).map(|p| (p.1, p.2));
        let mut ui = Ui {
            cv: Canvas::new(buf, w, h),
            f: &mut self.res.fonts,
            ic: &mut self.res.icons,
            t: self.st.t,
            zones: vec![],
            ptr,
            down: false,
            anim: false,
            blink: self.st.blink,
            tr: crate::i18n::tr,
        };
        crate::lock::draw(&mut ui, &self.st, w, h, main);
        let anim = ui.anim;
        ls.zones = std::mem::take(&mut ui.zones);
        drop(ui);
        if up {
            crate::gfx::upscale(&scratch, w, h, unsafe { std::slice::from_raw_parts_mut(out_ptr, out_len) }, pw, ph);
        }
        let surf = ls.s.wl_surface().clone();
        if let Some(vp) = &ls.vp {
            vp.set_destination(w, h);
        }
        surf.damage_buffer(0, 0, pw, ph);
        if anim {
            surf.frame(&self.qh, surf.clone());
        }
        let _ = buffer.attach_to(&surf);
        surf.commit();
    }
}
