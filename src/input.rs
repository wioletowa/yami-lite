// Ввод: мышь (зоны, перетаскивание, колесо; оверлей скриншота — первым) и клавиатура (той
// поверхности, у которой фокус).

use crate::app::*;
use crate::ui::{Act, Cur, Field, Zone};
use crate::audio;

use smithay_client_toolkit::{
    seat::{
        keyboard::{KeyEvent, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind},
    },
    shell::WaylandSurface,
};
use std::time::{Duration, Instant};
use wayland_client::protocol::wl_surface;
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::Shape;

impl App {
    // ── ввод ────────────────────────────────────────────────────────────────
    pub(crate) fn zone_at(&self, sid: u32, x: f64, y: f64) -> Option<Zone> {
        let zones = if sid >= LOCK_BASE {
            self.lock_surfs.get((sid - LOCK_BASE) as usize).map(|l| &l.zones)
        } else {
            self.surfs.get(&sid).map(|s| &s.zones)
        }?;
        zones.iter().rev().find(|z| z.r.contains(x, y)).cloned()
    }

    pub(crate) fn set_cursor(&mut self, c: Cur) {
        let shape = match c {
            Cur::Default => Shape::Default,
            Cur::Pointer => Shape::Pointer,
            Cur::Grab => Shape::Grabbing,
            Cur::Text => Shape::Text,
            Cur::Resize => Shape::SeResize,
        };
        if self.cursor_now == Some(shape) {
            return;
        }
        self.cursor_now = Some(shape);
        if let Some(d) = &self.cursor_dev {
            d.set_shape(self.enter_serial, shape);
        }
    }

    pub(crate) fn surf_of(&self, s: &wl_surface::WlSurface) -> Option<u32> {
        if let Some((i, _)) = self.surfs.iter().find(|(_, x)| x.layer.wl_surface() == s) {
            return Some(*i);
        }
        self.lock_surfs.iter().position(|l| l.s.wl_surface() == s).map(|i| LOCK_BASE + i as u32)
    }

    pub(crate) fn mark(&mut self, sid: u32) {
        if sid >= LOCK_BASE {
            if let Some(l) = self.lock_surfs.get_mut((sid - LOCK_BASE) as usize) {
                l.dirty = true;
            }
        } else if let Some(s) = self.surfs.get_mut(&sid) {
            s.dirty = true;
        }
    }

    pub fn surf_kind(&self, sid: u32) -> Option<SKind> {
        self.surfs.get(&sid).map(|s| s.kind.clone())
    }

    pub(crate) fn on_pointer(&mut self, e: &PointerEvent) {
        let Some(sid) = self.surf_of(&e.surface) else { return };
        let (x, y) = e.position;
        // скриншот: протяжка, рисование и лупа — мимо обычных зон
        if self.surf_kind(sid) == Some(SKind::Snap) {
            let on_btn = self.zone_at(sid, x, y).map(|z| matches!(z.act, Act::SnapBtn(_))).unwrap_or(false);
            let took = match e.kind {
                PointerEventKind::Motion { .. } => self.snap_pointer(sid, 0, x, y, 0, on_btn),
                PointerEventKind::Press { button, .. } => self.snap_pointer(sid, 1, x, y, button, on_btn),
                PointerEventKind::Release { button, .. } if self.press.is_none() => self.snap_pointer(sid, 2, x, y, button, on_btn),
                PointerEventKind::Axis { vertical, .. } => {
                    let d = if vertical.value120 != 0 { vertical.value120.signum() } else { vertical.discrete.signum() };
                    if d != 0 {
                        self.snap_wheel(d);
                    }
                    true
                }
                _ => false,
            };
            if took {
                self.ptr = Some((sid, x, y));
                return;
            }
        }
        match e.kind {
            PointerEventKind::Enter { serial } => {
                if matches!(self.surf_kind(sid), Some(SKind::Dock | SKind::DockEdge)) {
                    self.dock_hover(true);
                }
                self.enter_serial = serial;
                self.cursor_now = None;
                self.ptr = Some((sid, x, y));
                let c = self.zone_at(sid, x, y).map(|z| z.cur).unwrap_or(Cur::Default);
                self.set_cursor(c);
                self.mark(sid);
                if let Some(SKind::Notifs) = self.surf_kind(sid) {
                    self.st.notifs.hover = true;
                }
            }
            PointerEventKind::Leave { .. } => {
                if self.surf_kind(sid) == Some(SKind::Dock) {
                    self.dock_hover(false);
                }
                if self.ptr.map(|p| p.0) == Some(sid) {
                    self.ptr = None;
                }
                self.mark(sid);
                if let Some(SKind::Notifs) = self.surf_kind(sid) {
                    self.st.notifs.hover = false;
                    self.st.notifs.resume();
                }
            }
            PointerEventKind::Motion { .. } => {
                let before = self.ptr.and_then(|p| if p.0 == sid { self.zone_at(sid, p.1, p.2) } else { None });
                self.ptr = Some((sid, x, y));
                if self.press.is_some() {
                    self.on_drag(x, y);
                    return;
                }
                let after = self.zone_at(sid, x, y);
                let c = after.as_ref().map(|z| z.cur).unwrap_or(Cur::Default);
                self.set_cursor(c);
                if before.as_ref().map(|z| (&z.act, z.r)) != after.as_ref().map(|z| (&z.act, z.r)) {
                    self.mark(sid);
                    if let Some(z) = &after {
                        self.on_hover(&z.act);
                    }
                }
            }
            PointerEventKind::Press { button, .. } => {
                self.ptr = Some((sid, x, y));
                let z = self.zone_at(sid, x, y);
                let (sx, sy) = self.surfs.get(&sid).map(|s| s.applied).unwrap_or((0, 0));
                if let Some(z) = z {
                    // тащим от того места, где поверхность реально стоит
                    let start = (sx, sy);
                    self.press = Some(Press { sid, act: z.act.clone(), button, x, y, zone: z.r, ax: x + sx as f64, ay: y + sy as f64, moved: false, start, origin: (sx, sy) });
                    if z.act.is_drag() && button == 0x110 {
                        self.on_drag(x, y);
                    }
                    if z.act.is_drag() {
                        self.set_cursor(if matches!(z.act, Act::TSize(_)) { Cur::Resize } else { Cur::Grab });
                    }
                } else {
                    // клик мимо панели — по затемнению или ловцу: закрыть
                    match self.surf_kind(sid) {
                        Some(SKind::Dim) if self.st.modal == Some(Modal::Walls) => self.walls_back(),
                        Some(SKind::Dim) => self.close_modal(),
                        Some(SKind::Catcher) => {
                            self.close_popup();
                            self.close_menu();
                        }
                        _ => {}
                    }
                }
                self.mark(sid);
            }
            PointerEventKind::Release { button, .. } => {
                let p = self.press.take();
                self.mark(sid);
                if let Some(p) = p {
                    if p.act.is_drag() {
                        self.on_drag_end(&p);
                        if p.moved {
                            return;
                        }
                    }
                    let z = self.zone_at(sid, x, y);
                    if z.map(|z| z.act == p.act).unwrap_or(false) && p.button == button {
                        let double = matches!(&self.last_click, Some((a, t)) if *a == p.act && t.elapsed() < Duration::from_millis(350));
                        self.last_click = Some((p.act.clone(), Instant::now()));
                        self.on_click(p.act, button, sid, p.zone, double);
                    }
                }
            }
            PointerEventKind::Axis { vertical, .. } => {
                let d = if vertical.discrete != 0 { vertical.discrete as f64 } else if vertical.value120 != 0 { vertical.value120 as f64 / 120.0 } else { vertical.absolute / 15.0 };
                if d.abs() < 0.01 {
                    return;
                }
                if let Some(z) = self.zone_at(sid, x, y) {
                    let d = if d > 0.0 { 1 } else { -1 };
                    // окно настроек: колесо листает вкладку, если есть куда (ползунки и числа под
                    // курсором не меняются сами, пока листаешь); листать нечего — как раньше
                    let settings = self.st.modal == Some(crate::app::Modal::Settings) && self.surfs.get(&sid).is_some_and(|s| s.kind == SKind::Modal);
                    if settings && matches!(z.act, Act::SSideScroll | Act::STab(_)) {
                        self.settings_side_scroll(d);
                    } else if !(settings && self.settings_scroll(d)) {
                        self.on_wheel(z.act, d);
                    }
                }
            }
        }
    }

    pub(crate) fn on_drag(&mut self, x: f64, y: f64) {
        let Some(p) = self.press.as_mut() else { return };
        if (x - p.x).abs() + (y - p.y).abs() > 3.0 {
            p.moved = true;
        }
        let sid = p.sid;
        let act = p.act.clone();
        let zone = p.zone;
        let (sx, sy) = p.origin;
        let ax = x + sx as f64;
        let ay = y + sy as f64;
        let (dx, dy) = (ax - p.ax, ay - p.ay);
        let start = p.start;
        let frac = ((x - zone.x as f64) / zone.w.max(1) as f64).clamp(0.0, 1.0) as f32;
        match act {
            Act::SSlider(key, from, to, seg) => {
                let f = (frac * seg as f32).ceil() / seg as f32;
                self.slider_set(key, from + f * (to - from));
            }
            Act::VolSlider => audio::set_volume(frac as f64 * 1.0),
            Act::MicSlider => audio::set_mic(frac as f64),
            Act::AppVol(i) => audio::set_stream(i, frac as f64),
            Act::MediaSeek => {
                // остальные карточки плеера догонят при отпускании (on_drag_end)
                if self.st.seek != Some(frac) {
                    self.st.seek = Some(frac);
                    self.mark(sid);
                }
            }
            Act::TDrag(win) => {
                if p.moved {
                    self.title_drag(win, sid, start, dx, dy);
                }
                return;
            }
            Act::TSize(win) => {
                if p.moved {
                    self.grip_drag(win, sid, start, dx, dy);
                }
                return;
            }
            Act::MDrag => {
                if p.moved {
                    self.modal_drag(sid, dx, dy);
                }
                return;
            }
            Act::SModDrag(..) | Act::SModNew(_) => {
                if p.moved {
                    let press = (p.x, p.y);
                    self.bar_drag(&act, zone, press, x, y);
                }
                return;
            }
            Act::DispDrag(i) => {
                if p.moved {
                    self.disp_drag(i, dx, dy);
                }
                return;
            }
            Act::WDrag(id) => {
                if p.moved {
                    let snap = |v: f64| ((v / 2.0).round() * 2.0) as i32;
                    let nx = snap(start.0 as f64 + dx);
                    let ny = snap(start.1 as f64 + dy);
                    let (ow, oh) = self.surfs.get(&sid).map(|s| self.output_size(&s.output)).unwrap_or((1920, 1080));
                    let top = self.surfs.get(&sid).map(|s| self.bar_bottom(&s.output)).unwrap_or(BAR_H);
                    let (ww, wh) = self.surfs.get(&sid).map(|s| (s.w, s.h)).unwrap_or((0, 0));
                    let nx = nx.clamp(0, (ow - ww).max(0));
                    let low = self.surfs.get(&sid).map(|s| self.bar_reserve_bottom(&s.output)).unwrap_or(0);
                    let ny = ny.clamp(top, (oh - wh - low).max(top));
                    if let Some(w) = self.st.set.widget_mut(id) {
                        w.x = nx;
                        w.y = ny;
                    }
                    // двигаем только эту поверхность, без перерисовки и без записи настроек
                    self.move_surface(sid, nx, ny);
                }
                return;
            }
            _ => {}
        }
        self.mark(sid);
    }

    /// сдвинуть поверхность; положение считается применённым, когда композитор ответит на sync
    pub fn move_surface(&mut self, sid: u32, x: i32, y: i32) {
        let Some(s) = self.surfs.get_mut(&sid) else { return };
        if s.margin == (x, y) {
            return;
        }
        s.layer.set_margin(y, 0, 0, x);
        s.margin = (x, y);
        s.layer.commit();
        self.conn.display().sync(&self.qh, (sid, x, y));
    }

    pub(crate) fn on_drag_end(&mut self, p: &Press) {
        if let Act::TDrag(win) | Act::TSize(win) = p.act {
            self.title_drag_end(win);
        }
        if let Act::MDrag = p.act {
            self.mdrag = None;
        }
        if let Act::SSlider("pixel_level", ..) = p.act {
            self.look_apply();
        }
        if let Act::DispDrag(_) = p.act {
            self.disp_drag_end();
        }
        if matches!(p.act, Act::SModDrag(..) | Act::SModNew(_)) {
            self.bar_drop();
        }
        if let (Act::MediaSeek, Some(f)) = (&p.act, self.st.seek.take()) {
            // одна перемотка на отпускание, а не процесс playerctl на каждое движение
            let m = &mut self.st.media;
            if m.length > 0.0 {
                m.pos = f as f64 * m.length;
                m.at = Instant::now();
                crate::media::seek(&m.inst, m.pos);
            }
            self.dirty_where(|k| matches!(k, SKind::Popup | SKind::Widget(_)));
        }
        if let Act::SSlider(..) = p.act {
            // ползунок отпустили: теперь пересчитать обои (если это был их ползунок)
            self.sync_wall();
        }
        if let Act::WDrag(_) = p.act {
            // виджет переставили: обои под ним другие
            self.widget_contrast();
        }
    }

    pub(crate) fn on_hover(&mut self, a: &Act) {
        match a {
            Act::LRun(i) => {
                if self.st.launcher.cur != *i {
                    self.st.launcher.cur = *i;
                }
            }
            Act::PowerDo(i) => self.st.power_cur = *i,
            Act::Menu(i) => {
                if let Some(m) = self.st.menu.as_mut() {
                    m.cur = Some(*i);
                }
            }
            _ => {}
        }
    }

    // ── клавиатура ──────────────────────────────────────────────────────────
    pub(crate) fn on_key(&mut self, e: &KeyEvent) {
        if self.st.locked {
            self.lock_key(e);
            return;
        }
        let Some(sid) = self.kb_surf else { return };
        match self.surf_kind(sid) {
            Some(SKind::Modal) => match self.st.modal {
                Some(Modal::Launcher) => self.launcher_key(e),
                Some(Modal::Power) => self.power_key(e),
                Some(Modal::Settings) => self.settings_key(e),
                Some(Modal::Auth) => self.auth_key(e),
                Some(Modal::Screen) => self.screen_key(e),
                Some(Modal::Reply) => self.reply_key(e),
                Some(Modal::Walls) => self.walls_key(e),
                None => {}
            },
            Some(SKind::Popup) => self.popup_key(e),
            Some(SKind::Menu) => self.menu_key(e),
            Some(SKind::Widget(_)) => self.widget_key(e),
            Some(SKind::Snap) => {
                self.snap_key(e);
                return;
            }
            Some(SKind::EditBar) => {
                if matches!(e.keysym, Keysym::Escape | Keysym::Return) {
                    self.set_editing(false);
                }
            }
            _ => {}
        }
        self.mark(sid);
    }

    /// общая правка поля: true — клавиша поглощена
    pub fn edit_field(f: &mut Field, e: &KeyEvent, mods: &Modifiers) -> bool {
        match e.keysym {
            Keysym::BackSpace => f.backspace(mods.ctrl),
            Keysym::Delete => f.delete(),
            Keysym::Left => f.left(),
            Keysym::Right => f.right(),
            Keysym::Home => f.home(),
            Keysym::End => f.end(),
            _ => {
                if mods.ctrl {
                    // Ctrl+A/U/V — и в русской раскладке (по положению), и в AZERTY/Dvorak (по букве)
                    match crate::keys::latin_letter(e) {
                        Some('U') => f.clear(),
                        Some('A') => f.select_all(),
                        Some('V') => {
                            if let Some(t) = yami_px::ui::paste() {
                                f.insert(&t);
                            }
                        }
                        _ => return false,
                    }
                    return true;
                }
                match &e.utf8 {
                    Some(s) if !s.is_empty() && !s.chars().any(|c| c.is_control()) => f.insert(s),
                    _ => return false,
                }
            }
        }
        true
    }
}
