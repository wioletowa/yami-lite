// Поверхность шелла: панель layer-shell (бар, док, всплывашки, OSD) или обычное окно xdg-shell
// (настройки — их niri ставит в ленту, как любую программу). Методы панели у окна ничего не делают:
// его место и размер решает niri.

use smithay_client_toolkit::shell::{
    wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerSurface},
    xdg::window::Window,
    WaylandSurface,
};
use wayland_client::protocol::wl_surface::WlSurface;

pub enum Shell {
    Layer(LayerSurface),
    Window(Window),
}

impl Shell {
    pub fn wl_surface(&self) -> &WlSurface {
        match self {
            Shell::Layer(l) => l.wl_surface(),
            Shell::Window(w) => w.wl_surface(),
        }
    }
    pub fn commit(&self) {
        self.wl_surface().commit();
    }
    pub fn is_layer(&self, l: &LayerSurface) -> bool {
        matches!(self, Shell::Layer(x) if x == l)
    }
    pub fn is_window(&self, w: &Window) -> bool {
        matches!(self, Shell::Window(x) if x == w)
    }
    pub fn window(&self) -> Option<&Window> {
        match self {
            Shell::Window(w) => Some(w),
            Shell::Layer(_) => None,
        }
    }
    pub fn set_size(&self, w: u32, h: u32) {
        if let Shell::Layer(l) = self {
            l.set_size(w, h);
        }
    }
    pub fn set_margin(&self, top: i32, right: i32, bottom: i32, left: i32) {
        if let Shell::Layer(l) = self {
            l.set_margin(top, right, bottom, left);
        }
    }
    pub fn set_anchor(&self, a: Anchor) {
        if let Shell::Layer(l) = self {
            l.set_anchor(a);
        }
    }
    pub fn set_exclusive_zone(&self, z: i32) {
        if let Shell::Layer(l) = self {
            l.set_exclusive_zone(z);
        }
    }
    pub fn set_layer(&self, layer: Layer) {
        if let Shell::Layer(l) = self {
            l.set_layer(layer);
        }
    }
    pub fn set_keyboard_interactivity(&self, k: KeyboardInteractivity) {
        if let Shell::Layer(l) = self {
            l.set_keyboard_interactivity(k);
        }
    }
}
