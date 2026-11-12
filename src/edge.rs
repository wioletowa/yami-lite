// Визуализатор по краям экрана: кольцо пиксельных столбиков вдоль всех четырёх краёв. Спектр — тот
// же, что у эквалайзера (audio.rs: parec с монитора выхода, то есть все звуки, а не только плеер):
// басы в середине низа, выше по частоте — по бокам вверх, самые высокие сходятся в середине верха;
// левая половина — зеркало правой. Одна поверхность во весь экран, рисуется в ней только кайма.
// Столбик — клетки вплотную, в углах столбики низа и боков сливаются в один кусок: у края тёмный
// оттенок акцента, к верхушке ступенями до яркого акцента. Кадры —
// по спектру, ~60 в секунду; в тишине — нижний ряд клеток, кадр не меняется и не перерисовывается.
// Настройки — во вкладке «стол».
// Поверхность — на слое обоев, создана после обоев (над ними) и, как обои, в фоне обзора niri
// (layer-rule place-within-backdrop для yami-wall и yami-edge): в обзоре кайма стоит на месте во
// весь экран, а не уменьшается со столами — одна и та же, без подмены при входе и выходе.

use crate::app::{App, SKind, State};
use crate::gfx::{C, R};
use crate::ui::Ui;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::time::Instant;

/// отсчётов вдоль половины кольца (от середины низа до середины верха)
const K: usize = 192;

pub const BOTTOM: u8 = 0;
pub const TOP: u8 = 1;
pub const LEFT: u8 = 2;
pub const RIGHT: u8 = 3;

#[derive(Default)]
pub struct Edge {
    /// уровни вдоль половины кольца, 0..1
    pub lv: Vec<f32>,
    pub at: Option<Instant>,
}

impl Edge {
    /// новый спектр (полосы снизу вверх по частоте): разложить по кольцу. smooth — плавность спада
    /// 0..1 (доля прежнего уровня за кадр 60 fps). false — кадр тот же (тишина): перерисовывать нечего
    pub fn feed(&mut self, bands: &[f32], smooth: f32) -> bool {
        let now = Instant::now();
        let dt = self.at.map(|a| now.duration_since(a).as_secs_f32()).unwrap_or(0.0).min(0.2);
        self.at = Some(now);
        if self.lv.len() != K {
            self.lv = vec![0.0; K];
        }
        let n = bands.len();
        let get = |i: isize| if n == 0 { 0.0 } else { bands[i.clamp(0, n as isize - 1) as usize] };
        let mut changed = false;
        let keep = smooth.clamp(0.0, 0.95).powf(dt * 60.0);
        for k in 0..K {
            // Катмулл — Ром между полосами: 24 полосы на сотни столбиков без ступенек
            let f = k as f32 / (K - 1) as f32 * (n.max(1) - 1) as f32;
            let i = f.floor() as isize;
            let t = f - i as f32;
            let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
            let raw = (0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 + p3 - 3.0 * p2) * t * t * t)).clamp(0.0, 1.0);
            // вверх сразу, вниз — плавно; совсем низкое — ноль, иначе спад не кончается и тишина
            // перерисовывалась бы вечно
            let v = raw.max(self.lv[k] * keep + raw * (1.0 - keep));
            let v = if v < 0.004 { 0.0 } else { v };
            changed |= v != self.lv[k];
            self.lv[k] = v;
        }
        changed
    }
}

/// толщина каймы по настройке: целое число клеток. Внутри неё поверхность всегда пуста — render
/// чистит и отдаёт композитору только кайму, а не весь экран
pub fn thick(st: &State) -> i32 {
    let c = cell(st);
    (st.set.edge_viz_h.clamp(8, 400) / c).max(2) * c
}

/// клетка: пиксель столбика, без зазоров — кайма одним куском
fn cell(st: &State) -> i32 {
    st.set.num("edge_viz_cell")
}

pub fn on(st: &State) -> bool {
    crate::features::VISUALIZER && st.set.edge_viz
}

impl App {
    /// одна поверхность во весь монитор на каждый. Обои пересоздаются — кайма следом
    /// (ensure_output_surfaces), иначе окажется под ними
    pub fn sync_edge(&mut self) {
        if !on(&self.st) {
            for id in self.find(&SKind::Edge) {
                self.destroy(id);
            }
            self.viz_sync();
            return;
        }
        for out in self.output_names() {
            if self.find_on(&SKind::Edge, &out).is_some() {
                continue;
            }
            // -1: до самого края, мимо места бара (и фон обзора берёт только такие)
            let id = self.mk(SKind::Edge, &out, Layer::Background, "yami-edge", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
            // мышь сквозь: щёлкают по столу под ней
            if let Some(s) = self.surfs.get_mut(&id) {
                s.input = Some(vec![]);
            }
        }
        // толщина могла смениться
        self.dirty(&SKind::Edge);
        self.viz_sync();
    }
}

pub fn draw(ui: &mut Ui, st: &State, sw: i32, sh: i32) {
    let e = &st.edge;
    let (cell, steps) = (cell(st), st.set.num("edge_viz_steps") as f32);
    let t = thick(st).min(sh / 2).max(cell);
    let acc = ui.t.red;
    let dark = acc.lerp(C::hex(0), 0.78).alpha(st.set.edge_viz_alpha);
    // длина половины кольца: полниза, бок между полосами, полверха
    let half = (sw / 2) as f32;
    let len = half * 2.0 + (sh - 2 * t).max(0) as f32;
    let rows = t / cell;
    // столбик края в точке d кольца: сколько клеток залито
    let level = |d: f32| -> i32 {
        let k = ((d / len).clamp(0.0, 1.0) * (K - 1) as f32).round() as usize;
        // в тишине — один ряд у края: рамка вокруг экрана не рвётся
        ((e.lv.get(k).copied().unwrap_or(0.0) * rows as f32).round() as i32).clamp(1, rows)
    };
    // рисуется только кайма: низ и верх во всю ширину, бока между ними. Сетка каждой полосы
    // прижата к своему краю — клетки ровно встают к краю экрана
    for side in [BOTTOM, TOP, LEFT, RIGHT] {
        let (ox, oy, w, h) = match side {
            BOTTOM => (0, sh - t, sw, t),
            TOP => (0, 0, sw, t),
            LEFT => (0, t, t, sh - 2 * t),
            _ => (sw - t, t, t, sh - 2 * t),
        };
        let (from_right, from_bottom) = (side == RIGHT, side != TOP);
        let (nx, ny) = ((w + cell - 1) / cell, (h + cell - 1) / cell);
        for j in 0..ny {
            let y = if from_bottom { h - (j + 1) * cell } else { j * cell };
            for i in 0..nx {
                let x = if from_right { w - (i + 1) * cell } else { i * cell };
                // клетка — часть столбика любого края, до которого достаёт: так столбики низа и
                // боков сходятся в углах одним куском
                let (cx, cy) = ((ox + x) as f32 + cell as f32 / 2.0, (oy + y) as f32 + cell as f32 / 2.0);
                let side_d = half + (sh - t) as f32 - cy;
                let mut best: Option<f32> = None;
                for (dist, d) in [(sh as f32 - cy, (cx - half).abs()), (cy, len - (cx - half).abs()), (cx, side_d), (sw as f32 - cx, side_d)] {
                    let depth = (dist / cell as f32).floor() as i32;
                    if depth < 0 || depth >= rows {
                        continue;
                    }
                    let bar = level(d);
                    if depth < bar {
                        // ступени: от тёмного у края к акценту на верхушке, сколько бы столбик ни вырос
                        let k = if bar <= 1 { 0.0 } else { ((depth as f32 / (bar - 1) as f32) * steps).round() / steps };
                        best = Some(best.map_or(k, |b: f32| b.min(k)));
                    }
                }
                // клетка не вылезает из своей полосы: верх и низ не заходят на бока и наоборот
                let r = R::new(ox + x.max(0), oy + y.max(0), cell + x.min(0), cell + y.min(0));
                if let Some(k) = best {
                    ui.cv.fill(r, dark.lerp(acc, k));
                }
            }
        }
    }
}

// ── вкладка «стол» ──────────────────────────────────────────────────────────
pub fn tab(ui: &mut Ui, st: &State, b: R) {
    use crate::ui::Act;
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = crate::settings::title(ui, b.x, b.y, b.w, "визуализатор");
    ui.toggle(R::new(b.x, y, b.w, 40), "звук по краям экрана", "пиксельные столбики вокруг стола, под окнами — от любого звука", s.edge_viz, Act::SBool("edge_viz"));
    y += 50;
    ui.slider(R::new(b.x, y, b.w, 40), "высота столбиков", (s.edge_viz_h - 24) as f32 / 216.0, &format!("{} px", thick(st)), 54, Act::SSlider("edge_viz_h", 24.0, 240.0, 54));
    y += 50;
    crate::settings::num_btn(ui, R::new(lx, y, half, 26), "размер пикселя", s.num("edge_viz_cell"), "edge_viz_cell");
    crate::settings::num_btn(ui, R::new(rx, y, half, 26), "ступеней цвета", s.num("edge_viz_steps"), "edge_viz_steps");
    y += 38;
    ui.slider(R::new(lx, y, half, 40), "прозрачность у края", s.edge_viz_alpha, &format!("{}%", (s.edge_viz_alpha * 100.0).round()), 20, Act::SSlider("edge_viz_alpha", 0.0, 1.0, 20));
    ui.slider(R::new(rx, y, half, 40), "плавность спада", s.edge_viz_smooth / 0.9, &format!("{}%", (s.edge_viz_smooth * 100.0).round()), 18, Act::SSlider("edge_viz_smooth", 0.0, 0.9, 18));
}
