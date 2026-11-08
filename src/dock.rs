// Док: закреплённые программы и открытые окна на краю экрана (снизу, сверху, слева, справа).
// Значки программ из темы, под ними — пиксельные огоньки окон; наведение — значок
// поднимается и крупнеет, запуск — подпрыгивает ступенями. Скрытие: никогда (занимает
// место), всегда (выезжает у края), умно (виден на пустом столе, иначе — у края).
// ЛКМ — открыть или переключиться (ещё раз — свернуть, несколько окон — по кругу),
// СКМ — новое окно, колесо — окна по кругу, ПКМ — меню: окна, закрепить, закрыть все.

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::icons::{self, App as DApp};
use crate::menu::{MAct, MItem, MenuState};
use crate::niri::Win;
use crate::ui::{Act, Cur, Ui};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use smithay_client_toolkit::shell::WaylandSurface;
use std::time::Instant;

pub const MARGIN: i32 = 6;
/// место над панелью: подсказка и подъём значка
pub const SPARE: i32 = 34;

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub terminal: bool,
    pub pinned: bool,
    pub wins: Vec<Win>,
}

impl Item {
    pub fn focused(&self) -> bool {
        self.wins.iter().any(|w| w.focused)
    }
}

/// программа окна: .desktop с тем же id, StartupWMClass, последний кусок id
pub fn app_for<'a>(apps: &'a [DApp], app_id: &str) -> Option<&'a DApp> {
    let low = app_id.to_lowercase();
    let tail = low.rsplit('.').next().unwrap_or(&low).to_string();
    apps.iter()
        .find(|a| a.id.to_lowercase() == low)
        .or_else(|| apps.iter().find(|a| !a.wm_class.is_empty() && a.wm_class.to_lowercase() == low))
        .or_else(|| apps.iter().find(|a| a.id.to_lowercase().rsplit('.').next() == Some(tail.as_str())))
        .or_else(|| apps.iter().find(|a| a.name.to_lowercase() == low))
}

fn key_of(apps: &[DApp], w: &Win) -> String {
    app_for(apps, &w.app_id).map(|a| a.id.clone()).unwrap_or_else(|| w.app_id.clone())
}

/// закреплённые (по порядку) + открытые, сгруппированные по программе
pub fn items(st: &State) -> Vec<Item> {
    let s = &st.set;
    let mut out: Vec<Item> = vec![];
    for id in &s.dock_pins {
        let a = st.apps.iter().find(|a| &a.id == id);
        // закреплённое, но не установленное — не показываем
        if a.is_none() && !st.niri.wins.iter().any(|w| key_of(&st.apps, w) == *id) {
            continue;
        }
        out.push(Item { id: id.clone(), name: a.map(|a| a.name.clone()).unwrap_or_else(|| id.clone()), icon: a.map(|a| a.icon.clone()).unwrap_or_else(|| id.clone()), exec: a.map(|a| a.exec.clone()).unwrap_or_default(), terminal: a.map(|a| a.terminal).unwrap_or(false), pinned: true, wins: vec![] });
    }
    let mut wins: Vec<Win> = st.niri.wins.iter().filter(|w| !w.app_id.is_empty()).cloned().collect();
    wins.sort_by_key(|w| w.id);
    for w in wins {
        let k = key_of(&st.apps, &w);
        if let Some(it) = out.iter_mut().find(|i| i.id == k) {
            it.wins.push(w);
            continue;
        }
        if !s.dock_running {
            continue;
        }
        let a = app_for(&st.apps, &w.app_id);
        out.push(Item {
            id: k,
            name: a.map(|a| a.name.clone()).unwrap_or_else(|| w.app_id.clone()),
            icon: a.map(|a| a.icon.clone()).unwrap_or_else(|| icons::icon_for(&st.apps, &w.app_id)),
            exec: a.map(|a| a.exec.clone()).unwrap_or_default(),
            terminal: a.map(|a| a.terminal).unwrap_or(false),
            pinned: false,
            wins: vec![w],
        });
    }
    out
}

pub fn vertical(st: &State) -> bool {
    matches!(st.set.dock_side.as_str(), "left" | "right")
}

fn cell(st: &State) -> i32 {
    st.set.dock_size + 12
}

/// число мест: запуск, программы, корзина
fn slots(st: &State, n: usize) -> i32 {
    n as i32 + st.set.dock_launcher as i32 + st.set.dock_trash as i32
}

/// размер поверхности (длина вдоль края, толщина поперёк)
pub fn size(st: &State) -> (i32, i32) {
    let n = items(st).len();
    let len = slots(st, n) * cell(st) + 16 + if st.set.dock_launcher { 9 } else { 0 } + if st.set.dock_trash { 9 } else { 0 };
    let thick = st.set.dock_size + 24 + SPARE;
    if vertical(st) {
        (thick + 140, len)
    } else {
        (len.max(120), thick)
    }
}

/// панель внутри поверхности
pub fn panel(st: &State, w: i32, h: i32) -> R {
    let th = st.set.dock_size + 24;
    match st.set.dock_side.as_str() {
        "top" => R::new(0, 0, w, th),
        "left" => R::new(0, 0, th, h),
        "right" => R::new(w - th, 0, th, h),
        _ => R::new(0, h - th, w, th),
    }
}

fn bounce(st: &State, id: &str) -> i32 {
    let Some(t) = st.dock_bounce.iter().find(|b| b.0 == id).map(|b| b.1) else { return 0 };
    let ms = t.elapsed().as_millis() as i32;
    // ступени прыжка: вверх, выше, вниз, отскок
    const STEPS: [i32; 8] = [4, 8, 10, 8, 4, 0, 3, 0];
    let i = (ms / 70) as usize;
    if i >= STEPS.len() {
        0
    } else {
        STEPS[i]
    }
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let p = panel(st, w, h);
    let vert = vertical(st);
    ui.cv.pxbox(p.inset(0), &BoxStyle::new(t.glass_bg(), t.border).shadow(4));
    ui.zone(p, Act::Block, Cur::Default);
    let list = items(st);
    let c = cell(st);
    let sz = st.set.dock_size;
    let mut pos = 8;
    let mut tip: Option<(R, String)> = None;
    let slot = |pos: i32| -> R {
        if vert {
            R::new(p.x, p.y + pos, p.w, c)
        } else {
            R::new(p.x + pos, p.y, c, p.h)
        }
    };
    let sep = |ui: &mut Ui, pos: i32| {
        if vert {
            ui.cv.fill(R::new(p.x + 8, p.y + pos + 4, p.w - 16, 1), t.border);
        } else {
            ui.cv.fill(R::new(p.x + pos + 4, p.y + 8, 1, p.h - 16), t.border);
        }
    };
    // направление «вверх» для подъёма значка — от края
    let lift = |r: R, d: i32| -> R {
        match st.set.dock_side.as_str() {
            "top" => R::new(r.x, r.y + d, r.w, r.h),
            "left" => R::new(r.x + d, r.y, r.w, r.h),
            "right" => R::new(r.x - d, r.y, r.w, r.h),
            _ => R::new(r.x, r.y - d, r.w, r.h),
        }
    };
    if st.set.dock_launcher {
        let r = slot(pos);
        let hov = ui.hover(r) || st.modal == Some(crate::app::Modal::Launcher);
        let ic = lift(R::new(r.x + (r.w - sz) / 2, r.y + (r.h - sz) / 2, sz, sz), if hov { 3 } else { 0 });
        let q = (sz / 2 - 3).max(4);
        for i in 0..4 {
            let col = if i == 0 || hov { t.red } else { t.dim };
            ui.cv.pxbox(R::new(ic.x + 2 + (i % 2) * (q + 4), ic.y + 2 + (i / 2) * (q + 4), q, q), &BoxStyle::new(col, crate::theme::CLEAR).bevel(true));
        }
        ui.zone(r, Act::DockLauncher, Cur::Pointer);
        if hov {
            tip = Some((r, crate::i18n::tr("программы").to_string()));
        }
        pos += c;
        sep(ui, pos);
        pos += 9;
    }
    // анимация наведения: кто под курсором сейчас и кто был, переход за ~140 мс ступенями
    let first = pos;
    let now_hov = (0..list.len()).find(|i| ui.hover(slot(first + *i as i32 * c)) && st.menu.is_none());
    let (mut cur, mut prev, mut at) = st.dock_hov.get();
    if now_hov != cur {
        (prev, cur, at) = (cur, now_hov, Instant::now());
        st.dock_hov.set((cur, prev, at));
    }
    let dur = if st.set.anim() { 140.0 * st.set.anim_speed.max(0.1) } else { 0.0 };
    let k = if dur <= 0.0 { 1.0 } else { (at.elapsed().as_millis() as f32 / dur).min(1.0) };
    if k < 1.0 {
        ui.anim = true;
    }
    let ease = 1.0 - (1.0 - k) * (1.0 - k);
    let wave = st.set.dock_anim == "wave";
    let weight = |h: Option<usize>, i: usize| -> f32 {
        let Some(h) = h else { return 0.0 };
        match (h as i32 - i as i32).unsigned_abs() {
            0 => 1.0,
            1 if wave => 0.5,
            2 if wave => 0.2,
            _ => 0.0,
        }
    };
    // и пиксельные, и обычные растут на 8 px: пиксельный рисуется сразу нужного размера своей сеткой
    let grow_max = 8;
    for (i, it) in list.iter().enumerate() {
        let r = slot(pos);
        let hov = now_hov == Some(i);
        let amount = weight(cur, i) * ease + weight(prev, i) * (1.0 - ease);
        let focused = it.focused();
        if focused {
            ui.cv.pxbox(r.inset(3), &BoxStyle::new(t.selection.alpha(0.55), crate::theme::CLEAR));
        } else if hov {
            ui.cv.pxbox(r.inset(3), &BoxStyle::new(t.surface_hi.alpha(0.7), crate::theme::CLEAR));
        }
        let (grow, rise) = match st.set.dock_anim.as_str() {
            "none" => (0.0, 0.0),
            "lift" => (0.0, 6.0),
            _ => (grow_max as f32, 4.0),
        };
        let up = bounce(st, &it.id) + (rise * amount).round() as i32;
        // крупнее при наведении — ступенями по 2 px, без сглаживания (значок рисуется сразу нужного размера)
        let isz = sz + ((grow * amount / 2.0).round() as i32) * 2;
        let ic = lift(R::new(r.x + (r.w - isz) / 2, r.y + (r.h - isz) / 2 - if vert { 0 } else { 2 }, isz, isz), up);
        let dim = !it.wins.is_empty() && it.wins.iter().all(|w| st.niri.is_min(w));
        ui.app_icon(&it.icon, ic.x, ic.y, isz, if dim { 0.5 } else { 1.0 });
        // огоньки окон: до трёх квадратиков, фокусное — широкой чертой
        let n = it.wins.len().min(3) as i32;
        if n > 0 {
            let (dw, gap) = (4, 3);
            let total = if focused { 14 } else { n * dw + (n - 1) * gap };
            let (dx, dy) = match st.set.dock_side.as_str() {
                "top" => (r.x + (r.w - total) / 2, r.y + 3),
                "left" => (r.x + 3, r.y + (r.h - total) / 2),
                "right" => (r.right() - 7, r.y + (r.h - total) / 2),
                _ => (r.x + (r.w - total) / 2, r.bottom() - 7),
            };
            if focused {
                if vert {
                    ui.cv.fill(R::new(dx, dy, 4, total), t.ember);
                } else {
                    ui.cv.fill(R::new(dx, dy, total, 4), t.ember);
                }
            } else {
                for k in 0..n {
                    let o = k * (dw + gap);
                    let rr = if vert { R::new(dx, dy + o, 4, 4) } else { R::new(dx + o, dy, 4, 4) };
                    ui.cv.fill(rr, if dim { t.text_faint } else { t.red });
                }
            }
        }
        if it.wins.iter().any(|w| w.urgent) && (st.started.elapsed().as_millis() / 400) % 2 == 0 {
            ui.cv.outline(r.inset(2), t.ember, 2);
        }
        ui.zone(r, Act::DockItem(i), Cur::Pointer);
        if hov && st.set.dock_labels {
            let label = if it.wins.len() == 1 && !it.wins[0].title.is_empty() { it.wins[0].title.clone() } else { it.name.clone() };
            tip = Some((r, label));
        }
        pos += c;
    }
    if st.set.dock_trash {
        sep(ui, pos);
        pos += 9;
        let r = slot(pos);
        let hov = ui.hover(r);
        let full = st.trash_full;
        let k = (sz / 7).max(2);
        let ic = lift(R::new(r.x + (r.w - 7 * k) / 2, r.y + (r.h - 7 * k) / 2, 7 * k, 7 * k), if hov { 3 } else { 0 });
        ui.cv.icon("trash", ic.x, ic.y, k, if hov { t.ember } else if full { t.red } else { t.text_dim });
        if full {
            ui.cv.fill(R::new(ic.right() - k, ic.y - k, k * 2, k * 2), t.ember);
        }
        ui.zone(r, Act::DockTrash, Cur::Pointer);
        if hov {
            tip = Some((r, crate::i18n::tr("корзина").to_string()));
        }
    }
    // подсказка — над значком (или сбоку у вертикального дока)
    if let Some((r, s)) = tip {
        let tw = ui.w(PX, &s).min(260) + 18;
        let tr = match st.set.dock_side.as_str() {
            "top" => R::new((r.x + r.w / 2 - tw / 2).clamp(0, w - tw), p.bottom() + 6, tw, 22),
            "left" => R::new(p.right() + 6, r.y + (r.h - 22) / 2, tw, 22),
            "right" => R::new(p.x - tw - 6, r.y + (r.h - 22) / 2, tw, 22),
            _ => R::new((r.x + r.w / 2 - tw / 2).clamp(0, w - tw), p.y - 28, tw, 22),
        };
        ui.cv.pxbox(tr, &BoxStyle::new(t.panel.alpha(0.97), t.red));
        ui.text_in(PX, R::new(tr.x + 9, tr.y, tr.w - 14, tr.h), &s, t.text);
    }
    if st.dock_bounce.iter().any(|b| b.1.elapsed().as_millis() < 600) {
        ui.anim = true;
    }
}

impl App {
    /// показывать ли док сейчас (без учёта наведения)
    fn dock_shown(&self, out: &str) -> bool {
        match self.st.set.dock_hide.as_str() {
            "never" => true,
            "smart" => self.st.niri.active_ws(out).map(|ws| self.st.niri.windows_on(ws.id).is_empty()).unwrap_or(true) || self.st.dock_hover,
            _ => self.st.dock_hover,
        }
    }

    fn dock_anchor(&self) -> Anchor {
        match self.st.set.dock_side.as_str() {
            "top" => Anchor::TOP,
            "left" => Anchor::LEFT,
            "right" => Anchor::RIGHT,
            _ => Anchor::BOTTOM,
        }
    }

    /// поверхности дока и полоски-ловца у края — по настройкам и окнам
    pub fn sync_dock(&mut self) {
        let out = self.focus_out();
        // под оверлеем скриншота док не убираем: оверлей его и так закрывает, а пересоздание после
        // снимка двигало окна (исключённая зона) и заново проявляло док
        let on = self.st.set.dock && !self.st.locked && !out.is_empty();
        if !on {
            for k in [SKind::Dock, SKind::DockEdge] {
                for id in self.find(&k) {
                    self.destroy(id);
                }
            }
            return;
        }
        // зовётся на каждое событие niri: корзину смотрим, только если док её показывает
        if self.st.set.dock_trash {
            self.st.trash_full = std::fs::read_dir(crate::config::data_dir().join("Trash/files")).map(|mut r| r.next().is_some()).unwrap_or(false);
        }
        let show = self.dock_shown(&out);
        let (w, h) = size(&self.st);
        let anchor = self.dock_anchor();
        let vert = vertical(&self.st);
        // сверху — под баром (он может плавать с отступом), снизу — над нижним баром
        let top_m = if self.st.set.dock_side == "top" { self.bar_bottom(&out) } else { 0 };
        let bot_m = if self.st.set.dock_side == "bottom" { self.bar_reserve_bottom(&out) } else { 0 };
        // отступ от края: док с исключённой зоной niri ставит уже под баром, без неё — от края экрана
        let side = self.st.set.dock_side.clone();
        let margin_of = |excl: i32| {
            let m = MARGIN;
            match side.as_str() {
                "top" => (m + if excl > 0 { 0 } else { top_m }, 0, 0, 0),
                "left" => (0, 0, 0, m),
                "right" => (0, m, 0, 0),
                _ => (0, 0, m + if excl > 0 { 0 } else { bot_m }, 0),
            }
        };
        // на другом мониторе — пересоздать
        for k in [SKind::Dock, SKind::DockEdge] {
            for id in self.find(&k) {
                if self.surfs.get(&id).map(|s| s.output != out).unwrap_or(false) {
                    self.destroy(id);
                }
            }
        }
        let dock = self.find(&SKind::Dock).into_iter().next();
        if show {
            for id in self.find(&SKind::DockEdge) {
                self.destroy(id);
            }
            let thick = self.st.set.dock_size + 24;
            let excl = if self.st.set.dock_hide == "never" { thick + MARGIN } else { -1 };
            let p = panel(&self.st, w, h);
            match dock {
                Some(id) => {
                    if let Some(s) = self.surfs.get_mut(&id) {
                        let mg = margin_of(excl);
                        if s.w != w || s.h != h || s.anchor_side != self.st.set.dock_side || s.margin != (mg.3, mg.0) {
                            s.layer.set_size(w as u32, h as u32);
                            s.layer.set_anchor(anchor);
                            s.layer.set_margin(mg.0, mg.1, mg.2, mg.3);
                            s.layer.set_exclusive_zone(excl);
                            s.layer.commit();
                            s.margin = (mg.3, mg.0);
                            s.applied = s.margin;
                            s.anchor_side = self.st.set.dock_side.clone();
                        }
                        s.input = Some(vec![p]);
                        s.dirty = true;
                    }
                }
                None => {
                    let margin = margin_of(excl);
                    let id = self.mk(SKind::Dock, &out, Layer::Top, "yami-dock", anchor, (w, h), margin, excl, KeyboardInteractivity::None);
                    if let Some(s) = self.surfs.get_mut(&id) {
                        s.input = Some(vec![p]);
                        s.anchor_side = self.st.set.dock_side.clone();
                    }
                }
            }
        } else {
            if let Some(id) = dock {
                self.close(id);
            }
            if self.find(&SKind::DockEdge).is_empty() {
                let (ew, eh) = if vert { (2, h) } else { (w, 2) };
                self.mk(SKind::DockEdge, &out, Layer::Top, "yami-dock-edge", anchor, (ew, eh), (top_m, 0, bot_m, 0), -1, KeyboardInteractivity::None);
            }
        }
    }

    /// курсор у края / над доком / ушёл
    pub fn dock_hover(&mut self, on: bool) {
        if on {
            self.st.dock_leave = None;
            if !self.st.dock_hover {
                self.st.dock_hover = true;
                self.sync_dock();
            }
        } else if self.st.dock_hover && self.st.dock_leave.is_none() {
            self.st.dock_leave = Some(Instant::now());
        }
    }

    /// тик: курсор ушёл давно — спрятать
    pub fn dock_tick(&mut self) {
        if let Some(t) = self.st.dock_leave {
            if t.elapsed().as_millis() > 650 && self.st.menu.is_none() {
                self.st.dock_leave = None;
                self.st.dock_hover = false;
                self.sync_dock();
            }
        }
        self.st.dock_bounce.retain(|b| b.1.elapsed().as_millis() < 700);
    }

    fn dock_launch(&mut self, it: &Item) {
        if it.exec.is_empty() {
            return;
        }
        let mut argv = icons::exec_argv(&it.exec);
        if it.terminal {
            argv.insert(0, "kitty".into());
        }
        App::spawn(&argv);
        self.st.dock_bounce.retain(|b| b.0 != it.id);
        if self.st.set.dock_bounce && self.st.set.anim() {
            self.st.dock_bounce.push((it.id.clone(), Instant::now()));
        }
    }

    pub fn dock_click(&mut self, i: usize, button: u32, sid: u32, zone: R) {
        let list = items(&self.st);
        let Some(it) = list.get(i).cloned() else { return };
        match button {
            0x112 => self.dock_launch(&it),
            0x111 => self.dock_menu(&it, sid, zone),
            _ => {
                if it.wins.is_empty() {
                    self.dock_launch(&it);
                    return;
                }
                let mut ws = it.wins.clone();
                ws.sort_by(|a, b| b.ts.partial_cmp(&a.ts).unwrap_or(std::cmp::Ordering::Equal));
                if let Some(f) = ws.iter().position(|w| w.focused) {
                    if ws.len() == 1 {
                        self.st.niri.toggle_minimize(ws[0].id);
                    } else {
                        // по кругу: следующее по id после фокусного
                        let mut by_id = it.wins.clone();
                        by_id.sort_by_key(|w| w.id);
                        let k = by_id.iter().position(|w| w.id == ws[f].id).unwrap_or(0);
                        let next = by_id[(k + 1) % by_id.len()].id;
                        self.st.niri.toggle_minimize(next);
                    }
                } else {
                    self.st.niri.toggle_minimize(ws[0].id);
                }
            }
        }
    }

    pub fn dock_wheel(&mut self, i: usize, up: bool) {
        let list = items(&self.st);
        let Some(it) = list.get(i) else { return };
        if it.wins.len() < 2 {
            return;
        }
        let mut by_id = it.wins.clone();
        by_id.sort_by_key(|w| w.id);
        let k = by_id.iter().position(|w| w.focused).unwrap_or(0) as i32;
        let n = by_id.len() as i32;
        let next = by_id[((k + if up { -1 } else { 1 }).rem_euclid(n)) as usize].id;
        self.st.niri.act(serde_json::json!({"FocusWindow": {"id": next}}));
    }

    /// где поверхность дока на мониторе: она прибита к середине края, а не к углу
    fn dock_origin(&self, sid: u32) -> (String, i32, i32, R) {
        let Some(s) = self.surfs.get(&sid) else { return (self.focus_out(), 0, 0, R::default()) };
        let (ow, oh) = self.output_size(&s.output);
        let top_m = if self.st.set.dock_side == "top" { self.bar_bottom(&s.output) } else { 0 };
        let bot_m = self.bar_reserve_bottom(&s.output);
        let (x, y) = match self.st.set.dock_side.as_str() {
            "top" => ((ow - s.w) / 2, MARGIN + top_m),
            "left" => (MARGIN, (oh - s.h) / 2),
            "right" => (ow - MARGIN - s.w, (oh - s.h) / 2),
            _ => ((ow - s.w) / 2, oh - MARGIN - s.h - bot_m),
        };
        (s.output.clone(), x, y, panel(&self.st, s.w, s.h))
    }

    fn dock_menu(&mut self, it: &Item, sid: u32, zone: R) {
        let (out, ox, oy, p) = self.dock_origin(sid);
        let id: &'static str = Box::leak(it.id.clone().into_boxed_str());
        let mut items = vec![];
        for w in &it.wins {
            let t = if w.title.is_empty() { it.name.clone() } else { w.title.clone() };
            let t: String = t.chars().take(40).collect();
            items.push(MItem::new(&t, if self.st.niri.is_min(w) { "min" } else { "max" }, MAct::Win(w.id, "focus")));
        }
        if !items.is_empty() {
            items.push(MItem::sep());
        }
        if !it.exec.is_empty() {
            items.push(MItem::new("новое окно", "plus", MAct::Dock(id, "new")));
        }
        items.push(MItem::new(if it.pinned { "открепить" } else { "закрепить" }, "pin", MAct::Dock(id, "pin")));
        if it.pinned {
            items.push(MItem::new("левее", "left", MAct::Dock(id, "left")));
            items.push(MItem::new("правее", "right", MAct::Dock(id, "right")));
        }
        if !it.wins.is_empty() {
            items.push(MItem::sep());
            items.push(MItem::new(if it.wins.len() > 1 { "закрыть все" } else { "закрыть" }, "close", MAct::Dock(id, "close")).danger());
        }
        let mut m = MenuState::new(&out, 0, 0, &it.name, items);
        // меню — у значка: над нижним доком, под верхним, сбоку у боковых
        let (mw, mh) = crate::menu::size(&m, &mut self.res.fonts);
        let cx = ox + zone.x + zone.w / 2;
        let cy = oy + zone.y + zone.h / 2;
        let (x, y) = match self.st.set.dock_side.as_str() {
            "top" => (cx - mw / 2, oy + p.bottom() + 6),
            "left" => (ox + p.right() + 6, cy - mh / 2),
            "right" => (ox + p.x - mw - 6, cy - mh / 2),
            _ => (cx - mw / 2, oy + p.y - mh - 6),
        };
        m.x = x;
        m.y = y;
        self.open_menu(m);
    }

    pub fn dock_op(&mut self, id: &str, op: &str) {
        let list = items(&self.st);
        let it = list.iter().find(|i| i.id == id).cloned();
        let pins = &mut self.st.set.dock_pins;
        match op {
            "new" => {
                if let Some(it) = it {
                    self.dock_launch(&it);
                }
                return;
            }
            "pin" => {
                if let Some(i) = pins.iter().position(|p| p == id) {
                    pins.remove(i);
                } else {
                    pins.push(id.to_string());
                }
            }
            "left" | "right" => {
                if let Some(i) = pins.iter().position(|p| p == id) {
                    let j = if op == "left" { i.saturating_sub(1) } else { (i + 1).min(pins.len() - 1) };
                    pins.swap(i, j);
                }
            }
            "close" => {
                if let Some(it) = it {
                    for w in it.wins {
                        self.st.niri.close_window(w.id);
                    }
                }
                return;
            }
            _ => {}
        }
        self.settings_changed();
        self.sync_dock();
    }
}
