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
};
use std::time::Instant;
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
                if self.st.tip.is_some() && self.surf_kind(sid) == Some(SKind::Bar) {
                    self.privacy_tip_close();
                }
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
                match after.as_ref().map(|z| (&z.act, z.r)) {
                    Some((Act::Privacy(k), r)) => self.privacy_tip(k, sid, r),
                    _ if self.st.tip.is_some() => self.privacy_tip_close(),
                    _ => {}
                }
                let c = after.as_ref().map(|z| z.cur).unwrap_or(Cur::Default);
                self.set_cursor(c);
                if before.as_ref().map(|z| (&z.act, z.r)) != after.as_ref().map(|z| (&z.act, z.r)) {
                    self.mark(sid);
                    if let Some(z) = &after {
                        self.on_hover(&z.act);
                    }
                }
            }
            PointerEventKind::Press { button, serial, .. } => {
                self.ptr = Some((sid, x, y));
                self.press_serial = serial;
                let z = self.zone_at(sid, x, y);
                // заголовок окна настроек: окно двигает niri, как за его рамку
                if let (Some(Act::MDrag), Some(win), Some(seat)) = (z.as_ref().map(|z| &z.act), self.surfs.get(&sid).and_then(|s| s.layer.window()), self.seat.seats().next()) {
                    if button == 0x110 {
                        win.move_(&seat, serial);
                        return;
                    }
                }
                let (sx, sy) = self.surfs.get(&sid).map(|s| s.applied).unwrap_or((0, 0));
                if let Some(z) = z {
                    // тащим от того места, где поверхность реально стоит
                    let start = (sx, sy);
                    self.press = Some(Press { sid, act: z.act.clone(), button, x, y, zone: z.r, ax: x + sx as f64, ay: y + sy as f64, moved: false, start, origin: (sx, sy) });
                    if matches!(z.act, Act::DeskItem(_) | Act::DeskBg) {
                        self.desk_press(&z.act, button, x as i32, y as i32);
                    }
                    if z.act.is_drag() && button == 0x110 {
                        self.on_drag(x, y);
                    }
                    // рука — у ползунков и окон; значки стола тянутся обычным курсором
                    if z.act.is_drag() && !matches!(z.act, Act::DeskItem(_) | Act::DeskBg) {
                        self.set_cursor(Cur::Grab);
                    }
                } else {
                    // клик мимо панели — по затемнению или ловцу: закрыть
                    match self.surf_kind(sid) {
                        Some(SKind::Dim) if self.st.modal == Some(Modal::Walls) => self.walls_back(),
                        Some(SKind::Dim) => self.close_modal(),
                        Some(SKind::Catcher) => {
                            self.close_popup();
                            self.close_menu();
                            if self.st.modal_anchored {
                                self.close_modal();
                            }
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
                        self.on_click(p.act, button, sid, p.zone);
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
                    let settings = self.surfs.get(&sid).is_some_and(|s| s.kind == SKind::Settings);
                    if matches!(z.act, Act::DropPick(_) | Act::DropScroll) || (settings && matches!(z.act, Act::SFont(_) | Act::SFontScroll)) {
                        self.on_wheel(z.act, d);
                    } else if settings && matches!(z.act, Act::SSideScroll | Act::STab(_)) {
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
        let (press, button) = ((p.x, p.y), p.button);
        if matches!(act, Act::DeskItem(_) | Act::DeskBg) {
            if p.moved && button == 0x110 {
                self.desk_drag(&act, press, x, y);
            }
            return;
        }
        let (sx, sy) = p.origin;
        let ax = x + sx as f64;
        let ay = y + sy as f64;
        let (dx, dy) = (ax - p.ax, ay - p.ay);
        let start = p.start;
        let frac = ((x - zone.x as f64) / zone.w.max(1) as f64).clamp(0.0, 1.0) as f32;
        // вертикальные ползунки панели управления: снизу вверх
        let vfrac = (1.0 - (y - zone.y as f64) / zone.h.max(1) as f64).clamp(0.0, 1.0);
        match act {
            Act::CtlVol => {
                self.ctl_volume(vfrac);
                return;
            }
            Act::CtlBright => {
                self.ctl_brightness(vfrac as f32);
                return;
            }
            Act::FanLevel => {
                let c = crate::fan::cell_at(y - zone.y as f64, zone.h);
                if c != self.st.fan.cell() {
                    self.st.fan.apply(c);
                    self.dirty(&SKind::Popup);
                    self.dirty(&SKind::Bar);
                }
                return;
            }
            Act::SSlider(key, from, to, seg) => {
                let f = (frac * seg as f32).ceil() / seg as f32;
                self.slider_set(key, from + f * (to - from));
            }
            Act::TimerVol(id) => {
                // громкость сигнала таймера — шагами по 5%
                if let Some(w) = self.st.set.widget_mut(id) {
                    w.volume = (frac * 20.0).round() / 20.0;
                }
                self.settings_changed();
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
            Act::WDrag(id) => {
                if p.moved {
                    let (nx, ny) = ((start.0 as f64 + dx).round() as i32, (start.1 as f64 + dy).round() as i32);
                    let out = self.surfs.get(&sid).map(|s| s.output.clone()).unwrap_or_default();
                    let (ow, oh) = self.output_size(&out);
                    let (ww, wh) = self.surfs.get(&sid).map(|s| (s.w, s.h)).unwrap_or((0, 0));
                    // Shift — без магнитов, по 2 px как раньше
                    let (nx, ny, cross) = if self.mods.shift {
                        let (x, y) = crate::widgets::keep_visible(nx / 2 * 2, ny / 2 * 2, ww, wh, ow, oh);
                        (x, y, (false, false))
                    } else {
                        let bars = [self.bar_bottom(&out), oh - self.bar_reserve_bottom(&out)];
                        let others: Vec<crate::gfx::R> = self.surfs.iter().filter(|(k, s)| **k != sid && matches!(s.kind, SKind::Widget(_)) && !s.closing && s.output == out).map(|(_, s)| crate::gfx::R::new(s.margin.0, s.margin.1, s.w, s.h)).collect();
                        crate::widgets::snap(nx, ny, ww, wh, ow, oh, &bars, &others)
                    };
                    if self.st.snap_cross != cross {
                        self.st.snap_cross = cross;
                        self.dirty(&SKind::EditGrid);
                    }
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
        if matches!(p.act, Act::DeskItem(_) | Act::DeskBg) {
            self.desk_drag_end();
        }
        if let Act::MDrag = p.act {
            self.mdrag = None;
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
            // виджет переставили: обои под ним другие; крест гаснет
            self.widget_contrast();
            if self.st.snap_cross != (false, false) {
                self.st.snap_cross = (false, false);
                self.dirty(&SKind::EditGrid);
            }
        }
    }

    pub(crate) fn on_hover(&mut self, a: &Act) {
        match a {
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
                Some(Modal::Power) => self.power_key(e),
                Some(Modal::Reply) => self.reply_key(e),
                Some(Modal::Walls) => self.walls_key(e),
                Some(Modal::Clip) => self.clip_key(e),
                None => {}
            },
            Some(SKind::Settings) => self.settings_key(e),
            Some(SKind::Popup) => self.popup_key(e),
            Some(SKind::Menu) => self.menu_key(e),
            Some(SKind::Widget(id)) => {
                if !self.notes_key(id, e) {
                    self.widget_key(e);
                }
            }
            Some(SKind::Desk) => self.desk_key(e),
            Some(SKind::Info) => {
                if matches!(e.keysym, Keysym::Escape | Keysym::Return) {
                    self.desk_info_close();
                }
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
