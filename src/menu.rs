// Пиксельное меню у курсора: меню трея (dbusmenu, по уровням), меню окна (свернуть,
// во весь экран, плавающим, поверх всех…), меню виджета в режиме расстановки.

use crate::app::{App, SKind, State};
use crate::font::{Fonts, PX};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::shell::WaylandSurface;

#[derive(Clone, Debug, PartialEq)]
pub enum MAct {
    Win(u64, &'static str),
    Tray(String, String, i32),
    Widget(u32, &'static str, String),
    WidgetDel(u32),
    Dock(&'static str, &'static str),
}

#[derive(Clone, Debug, Default)]
pub struct MItem {
    pub label: String,
    pub icon: String,
    pub check: Option<bool>,
    pub enabled: bool,
    pub sep: bool,
    pub danger: bool,
    pub act: Option<MAct>,
    pub children: Vec<MItem>,
}

impl MItem {
    pub fn new(label: &str, icon: &str, act: MAct) -> MItem {
        MItem { label: label.into(), icon: icon.into(), enabled: true, act: Some(act), ..Default::default() }
    }
    pub fn sep() -> MItem {
        MItem { sep: true, ..Default::default() }
    }
    pub fn check(mut self, on: bool) -> MItem {
        self.check = Some(on);
        self
    }
    pub fn danger(mut self) -> MItem {
        self.danger = true;
        self
    }
    pub fn sub(label: &str, icon: &str, children: Vec<MItem>) -> MItem {
        MItem { label: label.into(), icon: icon.into(), enabled: true, children, ..Default::default() }
    }
}

pub struct MenuState {
    pub output: String,
    pub x: i32,
    pub y: i32,
    pub stack: Vec<(String, Vec<MItem>)>,
    pub cur: Option<usize>,
}

impl MenuState {
    pub fn new(output: &str, x: i32, y: i32, title: &str, items: Vec<MItem>) -> MenuState {
        MenuState { output: output.into(), x, y, stack: vec![(title.into(), items)], cur: None }
    }
    fn top(&self) -> &(String, Vec<MItem>) {
        self.stack.last().unwrap()
    }
    /// строки меню: «назад» на подуровнях
    fn rows(&self) -> Vec<MItem> {
        let mut v = vec![];
        if self.stack.len() > 1 {
            v.push(MItem { label: "назад".into(), icon: "up".into(), enabled: true, ..Default::default() });
        }
        v.extend(self.top().1.iter().cloned());
        v
    }
}

const W_MIN: i32 = 230;

pub fn size(m: &MenuState, f: &mut Fonts) -> (i32, i32) {
    let rows = m.rows();
    let mut w = W_MIN;
    for r in &rows {
        w = w.max(f.width(PX, crate::i18n::tr(&r.label)) + 60);
    }
    let mut h = 8 + if m.top().0.is_empty() { 0 } else { 20 };
    for r in &rows {
        h += if r.sep { 7 } else { 22 };
    }
    (w.min(420) + 4, h + 4)
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let Some(m) = &st.menu else { return };
    let fr = R::new(0, 0, w - 4, h - 8);
    ui.cv.pxbox(fr, &BoxStyle::new(t.panel, t.red).shadow(4));
    ui.zone(fr, Act::Block, Cur::Default);
    let mut y = 4;
    let title = &m.top().0;
    if !title.is_empty() {
        ui.text_in(PX, R::new(10, y, fr.w - 20, 20), title, t.red);
        y += 20;
    }
    for (i, it) in m.rows().iter().enumerate() {
        if it.sep {
            ui.cv.fill(R::new(8, y + 3, fr.w - 16, 1), t.border);
            y += 7;
            continue;
        }
        let r = R::new(4, y, fr.w - 8, 22);
        let hov = it.enabled && (ui.hover(r) || m.cur == Some(i));
        if hov {
            ui.cv.fill(r, if it.danger { t.blood } else { t.surface_hi });
        }
        let ic = R::new(r.x + 6, r.y, 14, r.h);
        if let Some(on) = it.check {
            // флажок: квадрат с «лампочкой» (px.rs toggle)
            let b = R::new(ic.x + 1, r.cy() - 6, 12, 12);
            ui.cv.pxbox(b, &BoxStyle::new(t.code_bg, t.border));
            if on {
                ui.cv.fill(R::new(b.x + 3, b.y + 3, 6, 6), t.ember);
            }
        } else if !it.icon.is_empty() {
            if crate::gfx::icon7(&it.icon).is_some() {
                ui.cv.icon_in(&it.icon, ic, 2, if it.danger { t.ember } else { t.text_dim });
            } else {
                let name = it.icon.clone();
                ui.app_icon(&name, ic.x - 1, r.cy() - 8, 16, 1.0);
            }
        }
        let c = if !it.enabled { t.text_faint } else if hov { t.text_hi } else { t.text };
        ui.text_in(PX, R::new(r.x + 28, r.y, r.w - 46, r.h), &it.label, c);
        if !it.children.is_empty() {
            ui.text_right(PX, R::new(r.x, r.y, r.w - 6, r.h), "▸", t.text_dim);
        }
        if it.enabled {
            ui.zone(r, if i == 0 && m.stack.len() > 1 { Act::MenuBack } else { Act::Menu(i) }, Cur::Pointer);
        }
        y += 22;
    }
}

impl App {
    pub fn menu_pick(&mut self, i: usize) {
        let Some(m) = self.st.menu.as_mut() else { return };
        let rows = m.rows();
        let Some(it) = rows.get(i).cloned() else { return };
        if m.stack.len() > 1 && i == 0 {
            m.stack.pop();
            m.cur = None;
            self.resize_menu();
            return;
        }
        if !it.children.is_empty() {
            m.stack.push((it.label.clone(), it.children.clone()));
            m.cur = None;
            self.resize_menu();
            return;
        }
        let act = it.act.clone();
        self.close_menu();
        if let Some(a) = act {
            self.menu_act(a);
        }
    }

    fn resize_menu(&mut self) {
        let Some(m) = &self.st.menu else { return };
        let (w, h) = size(m, &mut self.res.fonts);
        for s in self.surfs.values_mut() {
            if s.kind == SKind::Menu && !s.closing {
                s.layer.set_size(w as u32, (h + 4) as u32);
                s.layer.commit();
                s.dirty = true;
            }
        }
    }

    pub fn menu_act(&mut self, a: MAct) {
        match a {
            MAct::Win(id, op) => self.window_op(id, op),
            MAct::Tray(svc, path, item) => {
                crate::dbus::tray_menu_click(&svc, &path, item);
                // пункт выбран из меню во всплывашке трея — закрыть и её
                if self.st.popup_is("tray") {
                    self.close_popup();
                }
            }
            MAct::Widget(id, key, val) => {
                if let Some(w) = self.st.set.widget_mut(id) {
                    crate::widgets::set_opt(w, key, &val);
                }
                self.settings_changed();
            }
            MAct::Dock(id, op) => self.dock_op(id, op),
            MAct::WidgetDel(id) => {
                self.st.set.widgets.retain(|w| w.id != id);
                self.settings_changed();
            }
        }
    }

    pub fn menu_key(&mut self, e: &KeyEvent) {
        let Some(m) = self.st.menu.as_mut() else { return };
        let rows = m.rows();
        let n = rows.len();
        let ok = |i: usize| !rows[i].sep && rows[i].enabled;
        match e.keysym {
            Keysym::Escape => self.close_menu(),
            Keysym::Down | Keysym::Up => {
                let mut i = m.cur.unwrap_or(if e.keysym == Keysym::Down { n - 1 } else { 0 });
                for _ in 0..n {
                    i = if e.keysym == Keysym::Down { (i + 1) % n } else { (i + n - 1) % n };
                    if ok(i) {
                        break;
                    }
                }
                m.cur = Some(i);
            }
            Keysym::Return | Keysym::KP_Enter | Keysym::Right => {
                if let Some(c) = m.cur {
                    self.menu_pick(c);
                }
            }
            Keysym::Left | Keysym::BackSpace => {
                if m.stack.len() > 1 {
                    m.stack.pop();
                    m.cur = None;
                    self.resize_menu();
                }
            }
            _ => {}
        }
        self.dirty(&SKind::Menu);
    }

    /// меню окна у курсора (ПКМ по окну в баре или по заголовку)
    pub fn open_window_menu(&mut self, id: u64, out: &str, x: i32, y: i32) {
        let Some(w) = self.st.niri.win(id).cloned() else { return };
        let min = self.st.niri.is_min(&w);
        let mut items = vec![
            MItem::new(if min { "вернуть" } else { "свернуть" }, if min { "up" } else { "min" }, MAct::Win(id, "minimize")),
            MItem::new("развернуть", "max", MAct::Win(id, "maximize")),
            MItem::new("во весь экран", "max", MAct::Win(id, "fullscreen")),
            MItem::new(if w.floating { "в ленту" } else { "плавающим" }, if w.floating { "tile" } else { "float" }, MAct::Win(id, "float")),
            MItem::new("по центру", "grid", MAct::Win(id, "center")),
        ];
        // на другой стол
        let mut wsm = vec![];
        for ws in self.st.niri.ws.iter().filter(|x| x.output == out) {
            let name = ws.name.clone().unwrap_or_else(|| crate::tf!("стол {}", "workspace {}", "área de trabalho {}", ws.idx));
            wsm.push(MItem::new(&name, "right", MAct::Win(id, Box::leak(format!("ws{}", ws.idx).into_boxed_str()))));
        }
        if !wsm.is_empty() {
            items.push(MItem::sub("на стол", "right", wsm));
        }
        items.push(MItem::sep());
        items.push(MItem::new("закрыть", "close", MAct::Win(id, "close")).danger());
        let title = if w.app_id.is_empty() { w.title.clone() } else { w.app_id.clone() };
        self.close_popup();
        self.open_menu(MenuState::new(out, x, y, &title, items));
    }

    pub fn window_op(&mut self, id: u64, op: &str) {
        use serde_json::json;
        let n = &mut self.st.niri;
        match op {
            "minimize" => n.toggle_minimize(id),
            "maximize" => {
                if n.win(id).map(|w| w.floating).unwrap_or(false) {
                    n.act(json!({"MaximizeWindowToEdges": {"id": id}}));
                } else {
                    n.act(json!({"FocusWindow": {"id": id}}));
                    n.act(json!({"MaximizeColumn": {}}));
                }
            }
            "fullscreen" => n.act(json!({"FullscreenWindow": {"id": id}})),
            "float" => n.act(json!({"ToggleWindowFloating": {"id": id}})),
            "center" => {
                n.act(json!({"FocusWindow": {"id": id}}));
                n.act(json!({"CenterWindow": {"id": id}}));
            }
            "close" => n.close_window(id),
            "focus" => {
                if n.win(id).map(|w| n.is_min(w)).unwrap_or(false) {
                    n.restore(id);
                } else {
                    n.act(json!({"FocusWindow": {"id": id}}));
                }
            }
            o if o.starts_with("ws") => {
                if let Ok(idx) = o[2..].parse::<u32>() {
                    n.act(json!({"MoveWindowToWorkspace": {"window_id": id, "reference": {"Index": idx}, "focus": false}}));
                }
            }
            _ => {}
        }
    }

    pub fn open_tray_menu(&mut self, svc: &str, items: Vec<MItem>) {
        if items.is_empty() {
            return;
        }
        let (out, x, y) = self.tray_anchor.clone().unwrap_or_else(|| {
            let out = self.focus_out();
            // бар снизу — меню встанет над ним (open_menu прижимает к нему)
            let y = if crate::bar::low(&self.st.set) { i32::MAX / 2 } else { self.bar_bottom(&out) + 2 };
            (out, 100, y)
        });
        let title = self.st.tray.iter().find(|t| t.service == svc).map(|t| t.title.clone()).unwrap_or_default();
        self.open_menu(MenuState::new(&out, x, y, &title, items));
    }
}
