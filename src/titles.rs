// Заголовки окон в режиме «стол» (на любом niri): над каждым плавающим окном — полоса во всю
// ширину, как в DE: значок, название, свернуть / развернуть / закрыть. Тащишь — едет окно,
// двойной клик — развернуть, ПКМ — меню окна. В правом нижнем углу — ручка размера: при
// prefer-no-csd у окон нет своих рамок, а niri тянет размер только по Mod+ПКМ. Части, закрытые
// окнами выше, не рисуются и не ловят клики (порядок — по времени фокуса: на столе фокус только
// по щелчку, а щелчок окно и поднимает, см. keys::kdl).

use crate::app::{App, SKind, State, TITLE_H};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::icons;
use crate::niri::Win;
use crate::theme::CLEAR;
use crate::ui::{Act, Cur, Tone, Ui};
use serde_json::json;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use smithay_client_toolkit::shell::WaylandSurface;

/// плавающие окна видимых столов, снизу вверх
fn stack(st: &State, out: &str) -> Vec<Win> {
    let Some(ws) = st.niri.active_ws(out) else { return vec![] };
    let mut v: Vec<Win> = st.niri.wins.iter().filter(|w| w.ws == Some(ws.id) && w.floating && !st.niri.is_min(w) && w.layout.pos.is_some()).cloned().collect();
    v.sort_by(|a, b| a.focused.cmp(&b.focused).then(a.ts.partial_cmp(&b.ts).unwrap_or(std::cmp::Ordering::Equal)));
    v
}

fn tile(w: &Win) -> R {
    let (x, y) = w.layout.pos.unwrap_or((0.0, 0.0));
    R::new(x.round() as i32, y.round() as i32, w.layout.size.0.round() as i32, w.layout.size.1.round() as i32)
}

/// куски полосы (в её координатах), не закрытые окнами выше
fn visible(bar: R, above: &[R]) -> Vec<R> {
    let mut parts = vec![bar];
    for a in above {
        let mut next = vec![];
        for p in parts {
            let i = p.inter(a);
            if i.empty() {
                next.push(p);
                continue;
            }
            // полоса низкая: делим только по горизонтали, по вертикали — сверху/снизу
            if i.x > p.x {
                next.push(R::new(p.x, p.y, i.x - p.x, p.h));
            }
            if i.right() < p.right() {
                next.push(R::new(i.right(), p.y, p.right() - i.right(), p.h));
            }
            if i.y > p.y {
                next.push(R::new(i.x, p.y, i.w, i.y - p.y));
            }
            if i.bottom() < p.bottom() {
                next.push(R::new(i.x, i.bottom(), i.w, p.bottom() - i.bottom()));
            }
        }
        parts = next.into_iter().filter(|r| !r.empty()).collect();
    }
    parts
}

pub fn draw(ui: &mut Ui, st: &State, id: u64, w: i32, h: i32) {
    let t = ui.t;
    let Some(win) = st.niri.win(id) else { return };
    let r = R::new(0, 0, w, h);
    let focused = win.focused;
    // пиксельная плашка с фаской; активное — акцентная кромка снизу
    ui.cv.pxbox(r, &BoxStyle::new(if focused { t.panel } else { t.bg.lerp(t.panel, 0.5) }, if focused { t.red } else { t.border }).bevel(true));
    if focused {
        ui.cv.fill(R::new(2, h - 3, w - 4, 1), t.ember.fade(0.7));
    }
    ui.zone(r, Act::TDrag(id), Cur::Default);
    let mut x = 6;
    // ручка: точки 2×3
    for i in 0..6 {
        ui.cv.fill(R::new(x + (i % 2) * 4, h / 2 - 5 + (i / 2) * 4, 2, 2), if focused { t.red } else { t.dim });
    }
    x += 14;
    let icon = icons::icon_for(&st.apps, &win.app_id);
    ui.app_icon(&icon, x, (h - 16) / 2, 16, if focused { 1.0 } else { 0.6 });
    x += 22;
    let bw = 24;
    let btns = 3 * (bw + 2);
    let title = if win.title.is_empty() { win.app_id.clone() } else { win.title.clone() };
    ui.text_in(PX, R::new(x, 0, w - x - btns - 10, h - 1), &title, if focused { t.text_hi } else { t.text_dim });
    let mut bx = w - btns - 2;
    for (icon, act, tone) in [("min", Act::TMin(id), Tone::Ghost), ("max", Act::TMax(id), Tone::Ghost), ("close", Act::TClose(id), Tone::Danger)] {
        let br = R::new(bx, 3, bw, h - 6);
        let hov = ui.hover(br);
        if tone == Tone::Danger && hov {
            ui.cv.pxbox(br, &BoxStyle::new(t.blood, t.ember).bevel(true));
            ui.cv.icon_in(icon, br, 2, t.text_hi);
            ui.zone(br, act, Cur::Pointer);
        } else {
            ui.button(br, "", icon, Tone::Ghost, false, true, act);
        }
        bx += bw + 2;
    }
    // закрытые чужими окнами места — прозрачные
    if let Some(s) = st.titles.get(&id) {
        for c in &s.covered {
            ui.cv.set(*c, CLEAR);
        }
    }
}

/// ручка размера: квадрат на углу окна, GRIP_IN пикселей внутри, остальное снаружи
const GRIP: i32 = 14;
const GRIP_IN: i32 = 5;
/// меньше этого окно ручкой не сжать
const MIN_W: i32 = 160;
const MIN_H: i32 = 90;

fn grip_of(tr: R) -> R {
    R::new(tr.right() - GRIP_IN, tr.bottom() - GRIP_IN, GRIP, GRIP)
}

/// ручка: три пиксельные диагонали, как у старых окон; у активного — акцентом
pub fn draw_grip(ui: &mut Ui, st: &State, id: u64, w: i32, h: i32) {
    let t = ui.t;
    let Some(win) = st.niri.win(id) else { return };
    let hov = ui.hover(R::new(0, 0, w, h));
    let c = if hov { t.text_hi } else if win.focused { t.red } else { t.dim };
    for d in 0..3 {
        // диагональ d: точки 2×2 от нижнего края к правому
        for k in 0..=d {
            let (x, y) = (w - 4 - k * 4, h - 4 - (d - k) * 4);
            ui.cv.fill(R::new(x, y, 2, 2), c);
        }
    }
    ui.zone(R::new(0, 0, w, h), Act::TSize(id), Cur::Resize);
    if let Some(s) = st.titles.get(&id) {
        for r in &s.grip_covered {
            ui.cv.set(*r, CLEAR);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TitleGeo {
    pub covered: Vec<R>,
    pub grip_covered: Vec<R>,
}

/// поверхность заголовка или ручки, которую надо поставить
struct Want {
    id: u64,
    out: String,
    r: R,
    vis: Vec<R>,
    cov: Vec<R>,
}

/// что из r видно над окнами выше (в координатах r) и что закрыто
fn piece(id: u64, out: &str, r: R, above: &[R]) -> Option<Want> {
    let vis = visible(r, above);
    if vis.is_empty() {
        return None;
    }
    let local = |x: &R| R::new(x.x - r.x, x.y - r.y, x.w, x.h);
    let cov = above.iter().map(|a| local(&a.inter(&r))).filter(|x| !x.empty()).collect();
    Some(Want { id, out: out.to_string(), r, vis: vis.iter().map(local).collect(), cov })
}

impl App {
    pub fn sync_titles(&mut self) {
        let on = self.st.niri.stacking && self.st.set.titlebars && !self.st.locked && !self.st.niri.overview;
        let (mut titles, mut grips): (Vec<Want>, Vec<Want>) = (vec![], vec![]);
        if on {
            for out in self.output_names() {
                let (ow, oh) = self.output_size(&out);
                let top = self.bar_bottom(&out);
                let wins = stack(&self.st, &out);
                for (i, w) in wins.iter().enumerate() {
                    let tr = tile(w);
                    // во весь экран — без заголовка и ручки
                    if tr.w >= ow && tr.h >= oh - top {
                        continue;
                    }
                    // окно выше вместе со своим заголовком и ручкой
                    let above: Vec<R> = wins[i + 1..].iter().flat_map(|a| {
                        let ar = tile(a);
                        [ar, R::new(ar.x, ar.y - TITLE_H, ar.w, TITLE_H), grip_of(ar)]
                    }).collect();
                    let bar = R::new(tr.x, tr.y - TITLE_H, tr.w, TITLE_H);
                    if bar.bottom() > 0 && bar.y < oh {
                        titles.extend(piece(w.id, &out, bar, &above));
                    }
                    let g = grip_of(tr);
                    if g.x < ow && g.y < oh {
                        grips.extend(piece(w.id, &out, g, &above));
                    }
                }
            }
        }
        // пока тащим — положение того, за что тянут, ведёт шелл: ответы niri запаздывают
        let (mut by_title, mut by_grip) = (None, None);
        match self.press.as_ref().filter(|p| p.moved).map(|p| &p.act) {
            Some(Act::TDrag(id)) => by_title = Some(*id),
            Some(Act::TSize(id)) => by_grip = Some(*id),
            _ => {}
        }
        self.place(true, titles, by_title);
        self.place(false, grips, by_grip);
        let live: Vec<u64> = self.surfs.values().filter_map(|s| if let SKind::Title(id) | SKind::Grip(id) = s.kind { Some(id) } else { None }).collect();
        self.st.titles.retain(|id, _| live.contains(id));
    }

    /// поставить поверхности одного вида (заголовки или ручки) по списку: лишние убрать сразу
    /// (окно ушло — его украшения тоже), новые создать, остальные сдвинуть
    fn place(&mut self, is_title: bool, want: Vec<Want>, held: Option<u64>) {
        let kind: fn(u64) -> SKind = if is_title { SKind::Title } else { SKind::Grip };
        let have: Vec<(u32, u64)> = self
            .surfs
            .iter()
            .filter_map(|(sid, s)| match s.kind {
                SKind::Title(id) if is_title => Some((*sid, id)),
                SKind::Grip(id) if !is_title => Some((*sid, id)),
                _ => None,
            })
            .collect();
        for (sid, id) in &have {
            if !want.iter().any(|w| w.id == *id) {
                self.destroy(*sid);
            }
        }
        for w in want {
            let g = self.st.titles.entry(w.id).or_default();
            if is_title {
                g.covered = w.cov;
            } else {
                g.grip_covered = w.cov;
            }
            if held == Some(w.id) {
                continue;
            }
            let (r, ns) = (w.r, if is_title { "yami-title" } else { "yami-grip" });
            match have.iter().find(|h| h.1 == w.id).map(|h| h.0) {
                None => {
                    let sid = self.mk(kind(w.id), &w.out, Layer::Top, ns, Anchor::TOP | Anchor::LEFT, (r.w, r.h), (r.y, 0, 0, r.x), -1, KeyboardInteractivity::None);
                    if let Some(s) = self.surfs.get_mut(&sid) {
                        s.margin = (r.x, r.y);
                        s.applied = (r.x, r.y);
                        s.input = Some(w.vis);
                    }
                }
                Some(sid) => {
                    self.move_surface(sid, r.x, r.y);
                    if let Some(s) = self.surfs.get_mut(&sid) {
                        // новый размер — коммитом (niri ответит configure); область ввода и картинку
                        // применит отрисовка, а кадр, равный прошлому, она и не отправит
                        if s.w != r.w {
                            s.layer.set_size(r.w as u32, r.h as u32);
                            s.layer.commit();
                        }
                        s.input = Some(w.vis);
                        s.dirty = true;
                    }
                }
            }
        }
    }

    /// тянем ручку: окно растёт на ту же разницу (niri: AdjustFixed к ожидаемому размеру),
    /// ручка едет сразу, не дожидаясь niri
    pub fn grip_drag(&mut self, win: u64, sid: u32, start: (i32, i32), dx: f64, dy: f64) {
        let Some((x0, y0)) = self.st.niri.win(win).and_then(|w| w.layout.pos) else { return };
        // не меньше MIN_W×MIN_H: угол не левее и не выше этого
        let dx = dx.round().max((x0.round() as i32 + MIN_W - GRIP_IN - start.0) as f64);
        let dy = dy.round().max((y0.round() as i32 + MIN_H - GRIP_IN - start.1) as f64);
        let Some((mx, my)) = self.drag_step(win, dx, dy) else { return };
        if mx != 0.0 {
            self.st.niri.act(json!({"SetWindowWidth": {"id": win, "change": {"AdjustFixed": mx as i32}}}));
        }
        if my != 0.0 {
            self.st.niri.act(json!({"SetWindowHeight": {"id": win, "change": {"AdjustFixed": my as i32}}}));
        }
        self.drag_follow(sid, start.0 + dx as i32, start.1 + dy as i32);
        if let Some(g) = self.st.titles.get_mut(&win) {
            g.grip_covered.clear();
        }
    }

    /// шаг перетаскивания заголовка или ручки: сколько добавить к уже посланному niri (None — не
    /// сдвинулось ни на пиксель)
    fn drag_step(&mut self, win: u64, dx: f64, dy: f64) -> Option<(f64, f64)> {
        let sent = self.tdrag.filter(|t| t.0 == win).map(|t| t.1).unwrap_or((0.0, 0.0));
        let (mx, my) = (dx - sent.0, dy - sent.1);
        if mx == 0.0 && my == 0.0 {
            return None;
        }
        self.tdrag = Some((win, (dx, dy)));
        Some((mx, my))
    }

    /// поверхность едет за мышью сразу, не дожидаясь niri; пока тащим, ловит клики всей площадью
    /// (куски под чужими окнами — тоже)
    fn drag_follow(&mut self, sid: u32, x: i32, y: i32) {
        if let Some(s) = self.surfs.get_mut(&sid) {
            if s.input.is_some() {
                s.input = None;
                s.dirty = true;
            }
        }
        self.move_surface(sid, x, y);
    }

    /// тащим заголовок: окно едет на ту же разницу (niri: смещение относительно текущего)
    pub fn title_drag(&mut self, win: u64, sid: u32, start: (i32, i32), dx: f64, dy: f64) {
        let (dx, dy) = (dx.round(), dy.round());
        let Some((mx, my)) = self.drag_step(win, dx, dy) else { return };
        self.st.niri.act(json!({"MoveFloatingWindow": {"id": win, "x": {"AdjustFixed": mx}, "y": {"AdjustFixed": my}}}));
        self.drag_follow(sid, start.0 + dx as i32, (start.1 + dy as i32).max(0));
        if let Some(g) = self.st.titles.get_mut(&win) {
            g.covered.clear();
        }
    }

    pub fn title_drag_end(&mut self, win: u64) {
        if self.tdrag.map(|t| t.0) == Some(win) {
            self.tdrag = None;
        }
        // заголовок не должен уехать под бар: окно — ниже
        if let Some(w) = self.st.niri.win(win) {
            let top = self.win_top(w);
            if let Some((_, y)) = w.layout.pos {
                if (y as i32) < top {
                    self.st.niri.act(json!({"MoveFloatingWindow": {"id": win, "x": {"AdjustFixed": 0.0}, "y": {"AdjustFixed": (top - y as i32) as f64}}}));
                }
            }
        }
        self.sync_titles();
    }

    /// выше этого y заголовок окна залез бы под бар (бар может плавать с отступом)
    fn win_top(&self, w: &Win) -> i32 {
        let out = self.st.niri.ws.iter().find(|x| Some(x.id) == w.ws).map(|x| x.output.clone()).unwrap_or_else(|| self.focus_out());
        self.bar_bottom(&out) + TITLE_H
    }

    /// новое плавающее окно под самым баром — сдвинуть, чтобы заголовок поместился
    pub fn fit_title(&mut self, id: u64) {
        if !(self.st.niri.stacking && self.st.set.titlebars) {
            return;
        }
        if let Some(w) = self.st.niri.win(id) {
            let top = self.win_top(w);
            if let Some((_, y)) = w.layout.pos {
                if (y as i32) < top && w.floating {
                    self.st.niri.act(json!({"MoveFloatingWindow": {"id": id, "x": {"AdjustFixed": 0.0}, "y": {"AdjustFixed": (top - y as i32) as f64}}}));
                }
            }
        }
    }
}
