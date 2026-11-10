// Обработчики протоколов Wayland для шелла: кадры, мониторы, layer-поверхности, замок сеанса,
// seat, клавиатура и мышь, а также расширения (дробный масштаб, viewporter, размытие фона,
// простой, перехват сочетаний).

use crate::app::*;

use smithay_client_toolkit::{
    compositor::CompositorHandler,
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_session_lock, delegate_shm, delegate_xdg_shell, delegate_xdg_window,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        pointer::{PointerEvent, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    session_lock::{SessionLock, SessionLockHandler, SessionLockSurface, SessionLockSurfaceConfigure},
    shell::wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    shell::xdg::window::{Window, WindowConfigure, WindowHandler},
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::{ext_background_effect_manager_v1, ext_background_effect_surface_v1};
use wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1};
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1 as frac_mgr, wp_fractional_scale_v1 as frac};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{zwp_keyboard_shortcuts_inhibit_manager_v1 as ksi_mgr, zwp_keyboard_shortcuts_inhibitor_v1 as ksi};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

// ── обработчики Wayland ─────────────────────────────────────────────────────
impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        let Some(sid) = self.surf_of(surface) else { return };
        if sid >= LOCK_BASE {
            self.mark(sid);
            return;
        }
        if let Some(s) = self.surfs.get_mut(&sid) {
            s.frame_pending = false;
            if s.release_pool {
                // обои отрисованы и показаны — память буфера больше не нужна
                s.release_pool = false;
                s.pool = None;
                return;
            }
            if s.anim {
                s.dirty = true;
            }
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.ensure_output_surfaces();
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.ensure_output_surfaces();
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.ensure_output_surfaces();
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        let id = self.surfs.iter().find(|(_, s)| s.layer.is_layer(layer)).map(|(i, _)| *i);
        if let Some(id) = id {
            self.destroy(id);
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        if let Some(s) = self.surfs.values_mut().find(|s| s.layer.is_layer(layer)) {
            let (w, h) = c.new_size;
            if w > 0 {
                s.w = w as i32;
            }
            if h > 0 {
                s.h = h as i32;
            }
            s.configured = true;
            s.dirty = true;
            s.frame_pending = false;
            // после configure нужен коммит, даже если картинка та же
            s.drawn = 0;
        }
    }
}

impl WindowHandler for App {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if self.surfs.values().any(|s| s.layer.is_window(window) && s.kind == SKind::Settings) {
            self.close_settings();
        }
        if self.surfs.values().any(|s| s.layer.is_window(window) && s.kind == SKind::Info) {
            self.desk_info_close();
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window, c: WindowConfigure, _: u32) {
        if let Some(s) = self.surfs.values_mut().find(|s| s.layer.is_window(window)) {
            // размер решает niri; не сказал — какой был (сначала — минимальный)
            if let Some(w) = c.new_size.0 {
                s.w = w.get() as i32;
            }
            if let Some(h) = c.new_size.1 {
                s.h = h.get() as i32;
            }
            s.configured = true;
            s.dirty = true;
            s.frame_pending = false;
            s.drawn = 0;
        }
    }
}

impl SessionLockHandler for App {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {}
    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        self.lock = None;
        self.drop_lock_surfs();
        self.st.locked = false;
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: SessionLockSurface, c: SessionLockSurfaceConfigure, _: u32) {
        if let Some(l) = self.lock_surfs.iter_mut().find(|l| l.s.wl_surface() == surface.wl_surface()) {
            l.w = c.new_size.0 as i32;
            l.h = c.new_size.1 as i32;
            l.dirty = true;
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {
        self.setup_idle();
    }
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            let h = self.handle.clone();
            self.keyboard = self
                .seat
                .get_keyboard_with_repeat(qh, &seat, None, h, Box::new(|app: &mut App, _, e| app.on_key(&e)))
                .ok();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            if let Ok(p) = self.seat.get_pointer(qh, &seat) {
                if let Some(m) = &self.cursor_mgr {
                    self.cursor_dev = Some(m.get_shape_device(&p, qh));
                }
                if let Some(m) = &self.dd_mgr {
                    self.data_dev = Some(m.get_data_device(qh, &seat));
                }
                self.pointer = Some(p);
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
        if cap == Capability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
            self.cursor_dev = None;
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        self.kb_surf = self.surf_of(surface);
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32) {
        let sid = self.surf_of(surface);
        if self.kb_surf == sid {
            self.kb_surf = None;
            // клавиатура ушла от шелла: отпускания Shift и прочих он уже не увидит — не держать
            // их «нажатыми» (иначе виджеты тянутся без магнитов)
            self.mods = Modifiers::default();
        }
        if let Some(sid) = sid {
            if let Some(SKind::Widget(_)) = self.surf_kind(sid) {
                self.st.field_focus = None;
                self.mark(sid);
            }
        }
    }
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, serial: u32, e: KeyEvent) {
        // буфер обмена (Ctrl+C на столе) ставится по серийному номеру последнего ввода
        self.press_serial = serial;
        self.on_key(&e);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.on_key(&e);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, m: Modifiers, _: RawModifiers, _: u32) {
        self.mods = m;
    }
}

impl PointerHandler for App {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            self.on_pointer(e);
        }
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<wayland_client::protocol::wl_callback::WlCallback, (u32, i32, i32)> for App {
    fn event(app: &mut Self, _: &wayland_client::protocol::wl_callback::WlCallback, e: wayland_client::protocol::wl_callback::Event, d: &(u32, i32, i32), _: &Connection, _: &QueueHandle<Self>) {
        if let wayland_client::protocol::wl_callback::Event::Done { .. } = e {
            if let Some(s) = app.surfs.get_mut(&d.0) {
                s.applied = (d.1, d.2);
            }
        }
    }
}

impl Dispatch<ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1, ()> for App {
    fn event(_: &mut Self, _: &ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1, _: ext_background_effect_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1, ()> for App {
    fn event(_: &mut Self, _: &ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1, _: ext_background_effect_surface_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<frac_mgr::WpFractionalScaleManagerV1, ()> for App {
    fn event(_: &mut Self, _: &frac_mgr::WpFractionalScaleManagerV1, _: frac_mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
/// масштаб вывода под поверхностью: id поверхности (LOCK_BASE+i — экран блокировки)
impl Dispatch<frac::WpFractionalScaleV1, u32> for App {
    fn event(app: &mut Self, _: &frac::WpFractionalScaleV1, e: frac::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        let frac::Event::PreferredScale { scale } = e else { return };
        let v = scale as f64 / 120.0;
        if *id >= LOCK_BASE {
            if let Some(l) = app.lock_surfs.get_mut((*id - LOCK_BASE) as usize) {
                if l.scale != v {
                    l.scale = v;
                    l.dirty = true;
                }
            }
        } else if let Some(s) = app.surfs.get_mut(id) {
            if s.scale != v {
                s.scale = v;
                s.dirty = true;
            }
        }
    }
}

impl Dispatch<ksi_mgr::ZwpKeyboardShortcutsInhibitManagerV1, ()> for App {
    fn event(_: &mut Self, _: &ksi_mgr::ZwpKeyboardShortcutsInhibitManagerV1, _: ksi_mgr::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ksi::ZwpKeyboardShortcutsInhibitorV1, ()> for App {
    fn event(_: &mut Self, _: &ksi::ZwpKeyboardShortcutsInhibitorV1, _: ksi::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wp_viewporter::WpViewporter, ()> for App {
    fn event(_: &mut Self, _: &wp_viewporter::WpViewporter, _: wp_viewporter::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<wp_viewport::WpViewport, ()> for App {
    fn event(_: &mut Self, _: &wp_viewport::WpViewport, _: wp_viewport::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for App {
    fn event(_: &mut Self, _: &ext_idle_notifier_v1::ExtIdleNotifierV1, _: ext_idle_notifier_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}
impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, u8> for App {
    fn event(app: &mut Self, _: &ext_idle_notification_v1::ExtIdleNotificationV1, e: ext_idle_notification_v1::Event, kind: &u8, _: &Connection, _: &QueueHandle<Self>) {
        if let ext_idle_notification_v1::Event::Idled = e {
            if *kind == 0 {
                app.lock_now();
            } else {
                app.st.niri.act(serde_json::json!({"PowerOffMonitors": {}}));
            }
        }
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_keyboard!(App);
delegate_pointer!(App);
delegate_layer!(App);
delegate_xdg_shell!(App);
delegate_xdg_window!(App);
delegate_session_lock!(App);
delegate_registry!(App);
