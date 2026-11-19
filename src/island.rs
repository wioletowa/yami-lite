// Островок: бар узкой плашкой посередине верхнего края. Висит от края с пиксельными ушками или
// плавает с тенью; ширина — по своим модулям, на события (громкость, трек, зарядка…) ненадолго
// раздвигается. Новые края не выезжают, а складываются клетками вразнобой — изнутри наружу, при
// сужении рассыпаются снаружи. По щелчку раскрывается вниз сеткой плиток (всплывашка "island").
// Идеи — из misland (github.com/cublae/misland, MIT), вид и код свои.

use crate::app::{App, State, BAR_H};
use crate::config::{ITile, Settings};
use crate::font::{PX, PX24};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Tone, Ui};
use std::cell::Cell;
use std::time::{Duration, Instant};

const H: i32 = BAR_H;
/// клеток в ширину у раскрытого
pub const COLS: i32 = 4;
pub const CELL: i32 = 96;
pub const GAP: i32 = 8;
pub const PAD: i32 = 12;
/// строк в сетке раскрытого не больше
pub const MAX_ROWS: i32 = 4;
/// ушки висящего островка, px
pub const EAR: i32 = 8;
/// поверхность бара-островка: шире самого широкого события
pub const MAX_W: i32 = 720;
/// клетка сборки краёв, px
const BRICK: i32 = 4;

/// модули, что помещаются в свёрнутый
pub const MODS: &[&str] = &["workspaces", "clock", "kb", "privacy_mic", "privacy_screen", "privacy_cam", "privacy_hidden", "vol", "net", "bt", "bat", "notif", "night"];

/// полоса раскрытого: посередине — часы и индикаторы (пустые не видны), по бокам — свои значки
pub const CENTER: &[&str] = &["privacy_cam", "clock", "privacy_mic", "privacy_screen"];
/// значков с каждой стороны полосы раскрытого не больше
pub const SLOT_MAX: usize = 4;
/// что можно поставить в слоты: ключ модуля бара и значок в редакторе
pub const SLOTS: &[(&str, &str)] = &[("walls", "image"), ("wallhaven", "cloud"), ("tray", "dots"), ("power", "power"), ("privacy_hidden", "lock"), ("night", "moon"), ("kb", "keys"), ("notif", "bell")];

/// плитки раскрытого: ключ, имя, наименьший и наибольший размер в клетках
pub const TILES: &[(&str, &str, (i32, i32), (i32, i32))] = &[
    ("music", "музыка", (1, 1), (4, 2)),
    ("calendar", "календарь", (1, 1), (4, 3)),
    ("clock", "часы", (1, 1), (2, 1)),
    ("load", "нагрузка", (1, 1), (4, 2)),
    ("sound", "звук и яркость", (1, 1), (2, 2)),
    ("notifs", "уведомления", (2, 1), (4, 3)),
    ("power", "питание", (2, 1), (4, 1)),
];

/// счётчики «нагрузки»
pub const COUNTERS: &[(&str, &str)] = &[("cpu", "цп"), ("fan", "вентилятор"), ("ram", "память"), ("temp", "температура"), ("gpu", "видеокарта"), ("disk", "диск")];

/// живые события: ключ, подпись в настройках
pub const EVENTS: &[(&str, &str)] = &[("ws", "рабочие столы"), ("volume", "громкость"), ("bright", "яркость"), ("mic", "микрофон"), ("track", "трек"), ("layout", "раскладка"), ("caps", "Caps Lock"), ("num", "Num Lock"), ("power", "зарядка"), ("bt", "bluetooth")];

pub fn default_tiles() -> Vec<ITile> {
    let t = |kind: &str, x, y, w, h, opts: &[&str]| ITile { kind: kind.into(), x, y, w, h, opts: opts.iter().map(|s| s.to_string()).collect() };
    vec![t("music", 0, 0, 2, 2, &[]), t("calendar", 2, 0, 2, 1, &[]), t("load", 2, 1, 1, 2, &["cpu", "fan", "ram"]), t("sound", 3, 1, 1, 2, &[]), t("power", 0, 2, 2, 1, &[])]
}

pub const EDGES: &[(&str, &str)] = &[("top", "сверху"), ("bottom", "снизу")];

fn edge_name(k: &str) -> &'static str {
    EDGES.iter().find(|e| e.0 == k).map_or("сверху", |e| e.1)
}

/// подпись автоскрытия бара
pub fn hide_name(k: &str) -> &'static str {
    match k {
        "smart" => "умно — виден на пустом столе",
        "always" => "всегда — выезжает у края",
        _ => "никогда",
    }
}

/// значок плитки в палитре редактора
fn tile_icon(kind: &str) -> &'static str {
    match kind {
        "music" => "music",
        "calendar" => "cal",
        "clock" => "clock",
        "load" => "cpu",
        "sound" => "speaker",
        "notifs" => "bell",
        _ => "power",
    }
}

pub fn tile_info(kind: &str) -> Option<&'static (&'static str, &'static str, (i32, i32), (i32, i32))> {
    TILES.iter().find(|t| t.0 == kind)
}

/// островок включён (только у верхнего края)
pub fn on(s: &Settings) -> bool {
    s.bar_island
}

/// настройки из файла: известные модули и плитки, размеры в своих пределах, без наложений
pub fn validate(s: &mut Settings) {
    if s.island_edge != "bottom" {
        s.island_edge = "top".into();
    }
    s.island_mods.retain(|m| MODS.contains(&m.as_str()) && crate::features::module_on(m));
    s.island_mods.dedup();
    s.island_events.retain(|e| EVENTS.iter().any(|x| x.0 == e));
    for side in [&mut s.island_left, &mut s.island_right] {
        side.retain(|m| SLOTS.iter().any(|x| x.0 == m) && crate::features::module_on(m));
        side.dedup();
        side.truncate(SLOT_MAX);
        // трей — вся сторона
        if side.iter().any(|m| m == "tray") {
            *side = vec!["tray".into()];
        }
    }
    let mut ok: Vec<ITile> = vec![];
    for mut t in std::mem::take(&mut s.island_tiles) {
        let Some(info) = tile_info(&t.kind) else { continue };
        t.w = t.w.clamp(info.2 .0, info.3 .0.min(COLS));
        t.h = t.h.clamp(info.2 .1, info.3 .1);
        t.x = t.x.clamp(0, COLS - t.w);
        t.y = t.y.max(0);
        if t.y + t.h > MAX_ROWS {
            continue;
        }
        if t.kind == "load" {
            t.opts.retain(|o| COUNTERS.iter().any(|c| c.0 == o) || matches!(o.as_str(), "graph" | "number"));
        }
        if !ok.iter().any(|o| overlap(o, t.x, t.y, t.w, t.h)) {
            ok.push(t);
        }
    }
    compact(&mut ok);
    s.island_tiles = ok;
}

fn overlap(o: &ITile, x: i32, y: i32, w: i32, h: i32) -> bool {
    x < o.x + o.w && o.x < x + w && y < o.y + o.h && o.y < y + h
}

/// место (x, y, w, h) свободно от всех плиток, кроме skip, и в пределах ширины сетки
pub fn fits(tiles: &[ITile], skip: Option<usize>, x: i32, y: i32, w: i32, h: i32) -> bool {
    x >= 0 && y >= 0 && x + w <= COLS && y + h <= MAX_ROWS && !tiles.iter().enumerate().any(|(i, o)| Some(i) != skip && overlap(o, x, y, w, h))
}

/// пустые строки сетки убрать: всё ниже такой строки поднимается на одну. Плитки остаются там, куда
/// их поставили (в соседних клетках может быть пусто), но целиком пустых строк между ними нет
pub fn compact(tiles: &mut [ITile]) {
    let mut y = 0;
    while y < rows(tiles) {
        if tiles.iter().any(|t| t.y <= y && y < t.y + t.h) {
            y += 1;
            continue;
        }
        if !tiles.iter().any(|t| t.y > y) {
            break;
        }
        for t in tiles.iter_mut().filter(|t| t.y > y) {
            t.y -= 1;
        }
    }
}

/// строк в сетке (не меньше одной)
pub fn rows(tiles: &[ITile]) -> i32 {
    tiles.iter().map(|t| t.y + t.h).max().unwrap_or(1).max(1)
}

/// живое событие: что показать в раздвинутом островке
#[derive(Clone, Debug, PartialEq)]
pub enum Live {
    /// громкость, микрофон, яркость, раскладка, Caps, Num — как OSD
    Osd(&'static str),
    Track(String, String),
    /// зарядку подключили (true) или вынули; заряд
    Power(bool, u8),
    /// устройство: имя, подключилось ли
    Bt(String, bool),
    /// сменился рабочий стол: монитор, на котором он теперь
    Ws(String),
}

impl Live {
    fn key(&self) -> &'static str {
        match self {
            Live::Osd(k) => k,
            Live::Track(..) => "track",
            Live::Power(..) => "power",
            Live::Bt(..) => "bt",
            Live::Ws(_) => "ws",
        }
    }
    fn dur(&self, s: &Settings) -> Duration {
        Duration::from_secs_f32(match self {
            Live::Osd(_) => s.osd_time.max(1.0),
            Live::Track(..) => 4.0,
            Live::Power(..) => 2.5,
            Live::Bt(..) => 3.0,
            Live::Ws(_) => 1.2,
        })
    }
}

/// состояние островка
#[derive(Default)]
pub struct Isl {
    pub live: Option<(Live, Instant)>,
    /// ширина плашки: откуда, куда и когда началась смена
    pub wid: Cell<(i32, i32, Option<Instant>)>,
    /// раскрыт: когда (сборка вниз считается от этого)
    pub open_at: Option<Instant>,
    /// курсор ушёл с раскрытого: когда
    pub leave: Option<Instant>,
    /// какой ползунок плитки звука тянут (IslVol, IslBright) и когда последний раз: пока свежо,
    /// внутри полосы — процент
    pub held: Option<(bool, Instant)>,
    /// плитка, открытая на весь островок (календарь, музыка, нагрузка)
    pub big: Option<Big>,
    /// раскрытый сворачивается: когда начал
    pub closing: Option<Instant>,
    /// большой вид нагрузки показывает вентилятор (fan.rs) вместо системы
    pub fan_view: bool,
    /// большой вид уведомлений: сколько карточек пролистано
    pub notif_skip: usize,
    /// прошлое состояние батареи и устройств bluetooth — события по разнице
    pub plugged: Option<bool>,
    pub bt: Vec<(String, String)>,
}

/// большой вид плитки: какой, когда начал расти (или сворачиваться обратно — back)
pub struct Big {
    pub kind: &'static str,
    pub at: Instant,
    pub back: bool,
}

/// плитки, у которых есть большой вид
const BIG: &[&str] = &["calendar", "music", "load", "notifs"];

/// плитка с большим видом по имени (`yami-lite msg island notifs`)
pub fn big_kind(k: &str) -> Option<&'static str> {
    BIG.iter().copied().find(|b| *b == k)
}

fn big_dur(s: &Settings) -> f32 {
    if s.anim() {
        220.0 * s.anim_speed.max(0.1)
    } else {
        0.0
    }
}

/// доля раскрытия большого вида (0 — плитка, 1 — весь островок) и идёт ли ещё анимация
fn big_k(s: &Settings, b: &Big) -> (f32, bool) {
    let dur = big_dur(s);
    let t = if dur == 0.0 { 1.0 } else { (b.at.elapsed().as_secs_f32() * 1000.0 / dur).min(1.0) };
    let e = 1.0 - (1.0 - t).powi(3);
    (if b.back { 1.0 - e } else { e }, t < 1.0)
}

/// редактор островка во вкладке «бар»
#[derive(Default)]
pub struct IslEd {
    pub drag: Option<IDrag>,
    /// сетка редактора последнего кадра: левый верх, клетка, зазор
    pub grid: Cell<(i32, i32, i32, i32)>,
    pub palette: Cell<R>,
    /// фишки свёрнутого: прямоугольники без той, что в руке, и ряд целиком
    pub chips: std::cell::RefCell<Vec<R>>,
    pub chip_row: Cell<R>,
    /// окошко настроек плитки и список «добавить модуль»
    pub gear: Option<usize>,
    pub add: bool,
    /// список значков для слота: какая сторона (0 — слева, 1 — справа)
    pub slot_add: Option<usize>,
}

#[derive(Clone, Debug)]
pub enum DragWhat {
    /// плитка: двигают или тянут за угол
    Move(usize),
    Size(usize),
    /// новая из палитры
    New(&'static str),
    /// фишка свёрнутого: со своего места или новая
    Mod(Option<usize>, String),
}

pub struct IDrag {
    pub what: DragWhat,
    pub x: i32,
    pub y: i32,
    /// где схватили (от левого верха того, что тащат)
    pub off: (i32, i32),
    /// куда ляжет: клетка и размер для плиток, индекс для фишек; None — мимо (убрать)
    pub cell: Option<(i32, i32, i32, i32)>,
    pub slot: Option<usize>,
}

// ── плашка ──────────────────────────────────────────────────────────────────

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// у какого края экрана островок
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Edge {
    Top,
    Bottom,
}

pub fn edge(s: &Settings) -> Edge {
    match s.island_edge.as_str() {
        "bottom" => Edge::Bottom,
        _ => Edge::Top,
    }
}

/// плашка в своих координатах: u — вдоль края, v — от края вглубь; (x, y) — левый верх её
/// прямоугольника на холсте. Клетки по 2 px; у нижнего края v идёт снизу вверх
#[derive(Clone, Copy, Debug)]
pub struct Orient {
    pub edge: Edge,
    pub x: i32,
    pub y: i32,
}

impl Orient {
    /// клетки [u, u+nu) × [v, v+nv) плашки gw×gh клеток — прямоугольник на холсте
    fn rect(&self, u: i32, v: i32, nu: i32, nv: i32, _gw: i32, gh: i32) -> R {
        match self.edge {
            Edge::Top => R::new(self.x + u * 2, self.y + v * 2, nu * 2, nv * 2),
            Edge::Bottom => R::new(self.x + u * 2, self.y + (gh - v - nv) * 2, nu * 2, nv * 2),
        }
    }
    fn area(&self, gw: i32, gh: i32, shade: i32) -> R {
        R::new(self.x, self.y, gw * 2 + shade, gh * 2 + shade)
    }
}

/// сборка: где кончается готовое ядро (w0×h0) и доля сборки остального; grow = false — рассыпается
#[derive(Clone, Copy)]
pub struct Build {
    pub w0: i32,
    pub h0: i32,
    pub k: f32,
    pub grow: bool,
    /// у ядра свои ушки: оно и есть свёрнутый островок (а не его середина — тот шире раскрытого)
    pub ears: bool,
}

/// плашка шириной w и высотой h, по центру cx от top: висит (ушки сверху, низ скруглён) или
/// плавает (все углы и тень). Клетки по 2 px; за ядром Build — вразнобой по клеткам BRICK.
/// Возвращает занятый прямоугольник (с ушками и тенью)
pub fn plate(ui: &mut Ui, st: &State, o: Orient, w: i32, h: i32, b: Option<Build>) -> (R, Holes) {
    let t = ui.t;
    let hang = st.set.island_hang;
    let rp = (st.set.bar_radius.max(3) * 2).min(h / 2);
    let ear = if hang { EAR } else { 0 };
    let gw = (w + 2 * ear) / 2;
    let gh = h / 2;
    // форма плашки w×h: x от левого края тела (ушки — левее нуля и правее w), y от верха
    let shape = |x: i32, y: i32, w: i32, h: i32, ears: bool| -> bool {
        if y < 0 || y >= h {
            return false;
        }
        let rp = rp.min(h / 2);
        let fx = x as f32;
        let fy = y as f32;
        // ушки: вогнутая четверть круга у верхнего края
        if x < 0 || x >= w {
            if !hang || !ears || y >= ear {
                return false;
            }
            let d = if x < 0 { -fx } else { fx - w as f32 };
            let e = ear as f32;
            return d < e - (e * e - (e - fy) * (e - fy)).max(0.0).sqrt();
        }
        let r = rp as f32;
        let corner = |dy: f32| -> f32 { r - (r * r - dy * dy).max(0.0).sqrt() };
        let mut inset = 0.0;
        if fy > (h - rp) as f32 {
            inset = corner(fy - (h - rp) as f32);
        } else if !hang && fy < rp as f32 {
            inset = corner(rp as f32 - fy);
        }
        fx >= inset && fx < w as f32 - inset
    };
    let xy = |gx: i32, gy: i32| (gx * 2 + 1 - ear, gy * 2 + 1);
    let inside = |gx: i32, gy: i32| -> bool {
        let (x, y) = xy(gx, gy);
        shape(x, y, w, h, true)
    };
    // ядро сборки — сама свёрнутая плашка (скругление, ушки): она есть всегда, и последний кадр
    // сворачивания совпадает с баром под ним
    let small = |gx: i32, gy: i32| -> bool {
        let Some(b) = b else { return false };
        let (x, y) = xy(gx, gy);
        shape(x - (w - b.w0) / 2, y, b.w0, b.h0, b.ears)
    };
    // сборка: клетка за ядром видна, когда её очередь (случайная, ближние к ядру раньше)
    let built = |gx: i32, gy: i32| -> bool {
        let Some(b) = b else { return true };
        let (x, y) = xy(gx, gy);
        let dx = ((x - w / 2).abs() - b.w0 / 2) as f32 / ((w - b.w0) / 2 + ear).max(1) as f32;
        let dy = (y - b.h0) as f32 / (h - b.h0).max(1) as f32;
        // в пределах ядра, но вне его формы (углы, срезанные скруглением) — тоже клетками
        let d = dx.max(dy).max(0.001);
        let r = hash((o.x + gx * 2).div_euclid(BRICK), (o.y + gy * 2).div_euclid(BRICK));
        if b.grow {
            r < b.k * 1.5 - d * 0.5
        } else {
            r >= b.k * 1.5 - (1.0 - d) * 0.5
        }
    };
    // маска — один раз на кадр: в отладочной сборке замыкания на каждого соседа — это сотни
    // тысяч вызовов на кадр раскрытого, и анимация шла бы в два кадра
    let n = (gw * gh) as usize;
    let mut mask = vec![false; n];
    // ядро без клеток сборки: под ним размытие, пока края складываются (россыпь клеток дала бы
    // композитору тысячи прямоугольников)
    let mut core = vec![false; n];
    for gy in 0..gh {
        for gx in 0..gw {
            let i = (gy * gw + gx) as usize;
            let sm = small(gx, gy);
            if sm || inside(gx, gy) {
                mask[i] = sm || built(gx, gy);
                core[i] = b.is_none() || sm;
            }
        }
    }
    let top_row = |gx: i32| hang && gx >= ear / 2 && gx < gw - ear / 2;
    let on = |gx: i32, gy: i32| -> bool {
        if gx < 0 || gx >= gw || gy >= gh {
            false
        } else if gy < 0 {
            top_row(gx)
        } else {
            mask[(gy * gw + gx) as usize]
        }
    };
    let fill = t.glass_panel();
    let runs_of = |m: &[bool]| -> Vec<Vec<(i32, i32)>> {
        (0..gh)
            .map(|gy| {
                let row = &m[(gy * gw) as usize..((gy + 1) * gw) as usize];
                let mut v = vec![];
                let mut gx = 0;
                while gx < gw {
                    if !row[gx as usize] {
                        gx += 1;
                        continue;
                    }
                    let s = gx;
                    while gx < gw && row[gx as usize] {
                        gx += 1;
                    }
                    v.push((s, gx));
                }
                v
            })
            .collect()
    };
    // отрезки рядов: по ним заливка; размытие — по ядру, без углов и ушек
    let runs = runs_of(&mask);
    if fill.a() < 255 {
        let blur = if b.is_some() { runs_of(&core) } else { runs.clone() };
        // ряды с теми же отрезками — одной полосой: прямоугольников немного (углы, ушки, края)
        let mut gy = 0;
        while gy < gh {
            let s = gy;
            while gy + 1 < gh && blur[gy as usize + 1] == blur[s as usize] {
                gy += 1;
            }
            for &(a, e) in &blur[s as usize] {
                ui.cv.blur_rect(o.rect(a, s, e - a, gy - s + 1, gw, gh), 0);
            }
            gy += 1;
        }
    }
    let shade = if hang { 0 } else { crate::bar::SHADE };
    if shade > 0 {
        for gy in 0..gh {
            for gx in 0..gw {
                if on(gx, gy) && !on(gx - shade / 2, gy - shade / 2) {
                    let c = o.rect(gx, gy, 1, 1, gw, gh);
                    ui.cv.fill(R::new(c.x + shade, c.y + shade, 2, 2), t.shade);
                }
            }
        }
    }
    for gy in 0..gh {
        // ряд — отрезками подряд: тысячи заливок по 2 px дороже десятка длинных
        for &(a, e) in &runs[gy as usize] {
            ui.cv.fill(o.rect(a, gy, e - a, 1, gw, gh), fill);
        }
        for gx in 0..gw {
            // ядро — середина более широкого свёрнутого (без своих ушек): рамка у него — от бара
            // под ним, своя дала бы черты посреди полосы
            let mid = b.is_some_and(|b| !b.ears) && core[(gy * gw + gx) as usize];
            if !mid && on(gx, gy) && (!on(gx - 1, gy) || !on(gx + 1, gy) || !on(gx, gy - 1) || !on(gx, gy + 1)) {
                ui.cv.fill(o.rect(gx, gy, 1, 1, gw, gh), t.border);
            }
        }
    }
    // клетки формы, ещё не собранные (или уже рассыпавшиеся): нарисованное поверх в них стирается
    let mut holes = Holes { o, gw, gh, cells: vec![] };
    if b.is_some() {
        for gy in 0..gh {
            for gx in 0..gw {
                let i = (gy * gw + gx) as usize;
                if !mask[i] && inside(gx, gy) {
                    holes.cells.push(i as u32);
                }
            }
        }
    }
    (o.area(gw, gh, shade), holes)
}

/// дыры несобранной плашки (plate): клетки по 2 px
pub struct Holes {
    o: Orient,
    gw: i32,
    gh: i32,
    cells: Vec<u32>,
}

impl Holes {
    /// стереть всё, что нарисовано в дырах (плитки, полоса) — остаётся только собранное
    pub fn punch(&self, ui: &mut Ui) {
        let cv = &mut ui.cv;
        for &i in &self.cells {
            let (gx, gy) = (i as i32 % self.gw, i as i32 / self.gw);
            let c = self.o.rect(gx, gy, 1, 1, self.gw, self.gh);
            let (x, y) = (c.x + cv.ox, c.y + cv.oy);
            for yy in y.max(0)..(y + 2).min(cv.h) {
                for xx in x.max(0)..(x + 2).min(cv.w) {
                    cv.buf[(yy * cv.w + xx) as usize] = 0;
                }
            }
        }
    }
}

/// анимация ширины: показанная ширина и сборка краёв к ширине target
fn width_build(st: &State, target: i32, ui: &mut Ui) -> (i32, Option<Build>) {
    let (from, to, at) = st.island.wid.get();
    let dur = if st.set.anim() { 280.0 * st.set.anim_speed.max(0.1) } else { 0.0 };
    let k_of = |at: Option<Instant>| at.map_or(1.0, |a| if dur == 0.0 { 1.0 } else { (a.elapsed().as_secs_f32() * 1000.0 / dur).min(1.0) });
    let (from, to, at) = if to != target {
        // смена посреди прошлой: от того, что видно сейчас
        let k = k_of(at);
        let cur = if to == 0 { target } else if k >= 1.0 { to } else if to > from { to } else { from };
        let n = (cur, target, Some(Instant::now()));
        st.island.wid.set(n);
        n
    } else {
        (from, to, at)
    };
    let k = k_of(at);
    if k >= 1.0 || from == to {
        return (to, None);
    }
    ui.anim = true;
    if to > from {
        (to, Some(Build { w0: from, h0: H, k, grow: true, ears: true }))
    } else {
        (from, Some(Build { w0: to, h0: H, k, grow: false, ears: true }))
    }
}

// ── свёрнутый: в поверхности бара ───────────────────────────────────────────

const SIDE_PAD: i32 = 14;

/// ширина содержимого свёрнутого: живое событие или модули
fn content_w(ui: &mut Ui, st: &State, out: &str) -> i32 {
    match &st.island.live {
        Some((l, _)) => live_w(ui, st, l),
        None => crate::bar::row(ui, st, out, &mut st.set.island_mods.iter(), 0, 1, false, 0),
    }
}

/// ширина плашки свёрнутого (без ушек)
pub fn collapsed_w(ui: &mut Ui, st: &State, out: &str) -> i32 {
    ((content_w(ui, st, out) + 2 * SIDE_PAD).max(96) + 1) / 2 * 2
}

/// бар-островок шириной поверхности w; возвращает, где ловить мышь
pub fn draw_bar(ui: &mut Ui, st: &State, out: &str, w: i32, h: i32) -> R {
    let e = edge(&st.set);
    let mut target = collapsed_w(ui, st, out).min(w - 2 * EAR - crate::bar::SHADE);
    // раскрыт — свёрнутый не шире раскрытого: иначе его края торчат по бокам «следом»
    // (сворачивается — уже раздвигается обратно, вместе с ним, а не после)
    if st.popup_is("island") && st.island.closing.is_none() {
        target = target.min(panel_wh(st).0);
    }
    let (pw, build) = width_build(st, target, ui);
    let ear = if st.set.island_hang { EAR } else { 0 };
    let sh = if st.set.island_hang { 0 } else { crate::bar::SHADE };
    // посередине края; сверху — от верха поверхности, снизу — до её низа
    let c = (w - crate::bar::SHADE) / 2;
    let o = match e {
        Edge::Top => Orient { edge: e, x: c - pw / 2 - ear, y: 0 },
        Edge::Bottom => Orient { edge: e, x: c - pw / 2 - ear, y: h - H - sh },
    };
    // сворачивается — полоса уже своя: раскрытый шире свёрнутого не бывает, края — только здесь
    let open = st.popup_is("island") && st.island.closing.is_none();
    let (area, holes) = plate(ui, st, o, pw, H, build);
    let shown = build.map_or(pw, |b| if b.grow { b.w0 + ((pw - b.w0) as f32 * b.k) as i32 } else { pw - ((pw - b.w0) as f32 * b.k) as i32 });
    // полоса на холсте: шириной l посередине, высотой H
    let strip_r = |l: i32| R::new(c - l / 2, o.y, l, H);
    // вся плашка раскрывает островок — зона первой, под модулями: щелчок берёт верхняя (у
    // модулей со своим делом — их зона)
    ui.zone(strip_r(pw), Act::Popup("island"), Cur::Pointer);
    // раскрытый рисует ту же полосу сам, поверх: здесь — только плашка
    if !open {
        // обрезка только вдоль края: поперёк полосы — до самого края экрана (подчёркивание часов
        // у островка снизу лежит в нижних пикселях)
        let sr = strip_r(shown);
        ui.cv.push_clip(R::new(sr.x + 2, sr.y, sr.w - 4, sr.h));
        let sr = strip_r(target);
        ui.cv.oy += sr.y;
        content(ui, st, out, R::new(sr.x, 0, sr.w, H));
        ui.cv.oy -= sr.y;
        ui.cv.pop_clip();
    }
    holes.punch(ui);
    area
}

/// полоса свёрнутого: живое событие или модули; щелчок по ним раскрывает островок (столы —
/// переключаются как в баре)
fn content(ui: &mut Ui, st: &State, out: &str, r: R) {
    if let Some((l, _)) = &st.island.live {
        let n0 = ui.zones.len();
        let lw = live_w(ui, st, l);
        live_draw(ui, st, l, R::new(r.x + (r.w - lw) / 2, 0, lw, H));
        for z in ui.zones[n0..].iter_mut() {
            z.act = Act::Popup("island");
        }
        return;
    }
    // по модулю: у кого своё дело — оно (сеть, bluetooth, звук, батарея, замок, ночной свет,
    // раскладка, столы), уведомления — сразу их большим видом в раскрытом; остальное (часы с той
    // же всплывашкой календаря, индикаторы) — раскрыть островок
    let gap = st.set.num("bar_gap");
    let n_all = ui.zones.len();
    // наведение по одной горизонтали, как в баре: курсор у самого края экрана подсвечивает
    // модуль так же, как посередине полосы
    let ptr0 = ui.ptr;
    let oy = ui.cv.oy as f64;
    if let Some((px, py)) = ui.ptr {
        if py >= oy - 2.0 && py < oy + H as f64 + 2.0 {
            ui.ptr = Some((px, oy + ((H - 1) / 2) as f64));
        }
    }
    let cw = crate::bar::row(ui, st, out, &mut st.set.island_mods.iter(), 0, 1, false, 0);
    let mut x = r.x + (r.w - cw) / 2;
    for m in &st.set.island_mods {
        let w = crate::bar::row(ui, st, out, &mut std::iter::once(m), 0, 1, false, 0);
        if w == 0 {
            continue;
        }
        let n0 = ui.zones.len();
        crate::bar::row(ui, st, out, &mut std::iter::once(m), x, 1, true, 0);
        for z in ui.zones[n0..].iter_mut() {
            if m == "notif" {
                z.act = Act::IslBig("notifs");
            } else if !matches!(z.act, Act::Ws(_) | Act::Kb | Act::NightToggle | Act::Popup("net" | "bt" | "sound" | "bat" | "hidden")) {
                z.act = Act::Popup("island");
            }
        }
        x += w + gap;
    }
    // модули ловят мышь на всю высоту полосы — и у самого края экрана, как в баре
    let y0 = ui.cv.oy;
    for z in ui.zones[n_all..].iter_mut() {
        z.r = R::new(z.r.x, y0, z.r.w, H);
    }
    ui.ptr = ptr0;
}

// ── живые события ───────────────────────────────────────────────────────────

const GAUGE_W: i32 = 120;

fn osd_val(st: &State, k: &str) -> (&'static str, f32, String, bool) {
    let pct = |v: f64| format!("{}%", (v * 100.0).round());
    let a = &st.audio;
    match k {
        // выключенный звук или микрофон — просто 0%, без надписи
        "volume" => (if a.muted { "mute" } else { "speaker" }, a.volume as f32, if a.muted { "0%".into() } else { pct(a.volume) }, a.muted),
        "mic" => ("mic", a.mic as f32, if a.mic_muted { "0%".into() } else { pct(a.mic) }, a.mic_muted),
        _ => {
            let b = st.leds.bright.unwrap_or(0.0);
            ("sun", b, pct(b as f64), false)
        }
    }
}

fn live_w(ui: &mut Ui, st: &State, l: &Live) -> i32 {
    match l {
        Live::Osd(k @ ("volume" | "mic" | "bright")) => {
            let (_, _, v, _) = osd_val(st, k);
            14 + 10 + GAUGE_W + 10 + ui.w(PX, &v).max(ui.w(PX, "100%"))
        }
        Live::Osd("caps") => 14 + 10 + ui.w(PX, "Caps Lock") + 10 + ui.w(PX, "выключен"),
        Live::Osd("num") => 14 + 10 + ui.w(PX, "Num Lock") + 10 + ui.w(PX, "выключен"),
        Live::Osd(_) => {
            let name = st.wm.kb_names.get(st.wm.kb_idx).cloned().unwrap_or_default();
            ui.w(PX, &st.wm.kb_short()) + 12 + 10 + ui.w(PX, &name).min(260)
        }
        Live::Track(title, artist) => 16 + 10 + ui.w(PX, title).min(260) + if artist.is_empty() { 0 } else { 14 + ui.w(PX, artist).min(180) } + 10 + 14,
        Live::Power(on, pct) => 14 + 10 + ui.w(PX, if *on { "зарядка" } else { "от батареи" }) + 14 + ui.w(PX, &format!("{pct}%")),
        Live::Bt(name, on) => 14 + 10 + ui.w(PX, name).min(220) + 14 + ui.w(PX, if *on { "подключено" } else { "отключено" }),
        Live::Ws(out) => (st.wm.bar_ws(out).len() as i32 * 18 - 4).max(14),
    }
}

fn live_draw(ui: &mut Ui, st: &State, l: &Live, r: R) {
    let t = ui.t;
    let cy = (H - 1) / 2;
    let x = r.x;
    match l {
        Live::Osd(k @ ("volume" | "mic" | "bright")) => {
            let (icon, v, txt, off) = osd_val(st, k);
            ui.cv.icon(icon, x, cy - 7, 2, if off { t.text_faint } else { t.red });
            let gx = x + 24;
            ui.cv.gauge(R::new(gx, cy - 4, GAUGE_W, 8), 12, if off { 0.0 } else { v.min(1.0) }, if v > 1.0 { t.ember } else { t.red }, t.surface_hi.lerp(t.border, 0.45), 2);
            ui.text_in(PX, R::new(gx + GAUGE_W + 10, 0, r.right() - gx - GAUGE_W - 10, H - 1), &txt, if off { t.text_faint } else { t.text });
        }
        Live::Osd(k @ ("caps" | "num")) => {
            let on = if *k == "caps" { st.leds.caps } else { st.leds.num };
            ui.cv.icon(if *k == "caps" { "up" } else { "calc" }, x, cy - 7, 2, if on { t.red } else { t.text_faint });
            let lw = ui.text_in(PX, R::new(x + 24, 0, r.w, H - 1), if *k == "caps" { "Caps Lock" } else { "Num Lock" }, t.text);
            ui.text_in(PX, R::new(x + 34 + lw, 0, r.w, H - 1), if on { "включён" } else { "выключен" }, if on { t.red } else { t.text_faint });
        }
        Live::Osd(_) => {
            let short = st.wm.kb_short();
            let bw = ui.w(PX, &short) + 12;
            let b = R::new(x, cy - 10, bw, 20);
            ui.cv.pxbox(b, &BoxStyle::new(t.red, t.ember).bevel(true));
            ui.text_center(PX, b, &short, t.on_accent);
            let name = st.wm.kb_names.get(st.wm.kb_idx).cloned().unwrap_or_default();
            ui.text_in(PX, R::new(x + bw + 10, 0, r.right() - x - bw - 10, H - 1), &name, t.text);
        }
        Live::Track(title, artist) => {
            match st.art.get(&st.media.art).cloned().flatten() {
                Some(img) => ui.cv.image_scaled(&img, R::new(x, cy - 8, 16, 16), 1.0),
                None => ui.cv.icon("music", x, cy - 6, 2, t.red),
            }
            let mut tx = x + 26;
            tx += ui.text_in(PX, R::new(tx, 0, 260, H - 1), title, t.text_hi);
            if !artist.is_empty() {
                ui.cv.fill(R::new(tx + 5, cy, 4, 2), t.text_faint);
                tx += 14;
                tx += ui.text_in(PX, R::new(tx, 0, 180, H - 1), artist, t.text_dim);
            }
            eq(ui, st, R::new(tx + 10, cy - 7, 14, 14), 3);
        }
        Live::Power(on, pct) => {
            ui.cv.icon("bolt", x, cy - 7, 2, if *on { t.ok } else { t.text_dim });
            let lw = ui.text_in(PX, R::new(x + 24, 0, r.w, H - 1), if *on { "зарядка" } else { "от батареи" }, t.text);
            ui.text_in(PX, R::new(x + 38 + lw, 0, r.w, H - 1), &format!("{pct}%"), if *on { t.ok } else { t.text_dim });
        }
        // столы точками, активный вырастает в квадрат — та же анимация, что у столов в баре
        Live::Ws(out) => crate::bar::workspace_dots(ui, st, &st.wm.bar_ws(out), x, cy),
        Live::Bt(name, on) => {
            ui.cv.icon("bt", x, cy - 7, 2, if *on { t.red } else { t.text_faint });
            let lw = ui.text_in(PX, R::new(x + 24, 0, 220, H - 1), name, t.text);
            ui.text_in(PX, R::new(x + 38 + lw, 0, r.w, H - 1), if *on { "подключено" } else { "отключено" }, if *on { t.red } else { t.text_faint });
        }
    }
}

/// маленький эквалайзер столбиками по 2 px (спектр, пока играет)
fn eq(ui: &mut Ui, st: &State, r: R, n: i32) {
    let t = ui.t;
    let playing = st.media.playing();
    let step = (r.w / n.max(1)).max(3);
    // спектр на все столбики поровну: столбик — наибольшая из своих полос (раньше брались
    // полосы через пять от начала — дальше конца спектра столбики стояли пустыми)
    let len = st.viz.len() as i32;
    for i in 0..n {
        let (a, b) = (i * len / n.max(1), ((i + 1) * len / n.max(1)).max(i * len / n.max(1) + 1).min(len));
        let v = (a..b).filter_map(|k| st.viz.get(k as usize)).fold(0.0f32, |m, &x| m.max(x));
        let hh = if playing { (v * r.h as f32).round().clamp(2.0, r.h as f32) as i32 / 2 * 2 } else { 2 };
        ui.cv.fill(R::new(r.x + i * step, r.bottom() - hh, step - 1, hh), t.red);
    }
}

// ── раскрытый: всплывашка "island" ──────────────────────────────────────────

/// ширина и высота плашки раскрытого (без ушек и тени)
pub fn panel_wh(st: &State) -> (i32, i32) {
    // (вдоль края, вглубь от него): полоса модулей у края, за ней сетка плиток
    let n = rows(&st.set.island_tiles);
    let (gw, gh) = (COLS * CELL + (COLS - 1) * GAP, n * CELL + (n - 1) * GAP);
    (gw + 2 * PAD, H + gh + PAD + 4)
}

/// размер поверхности раскрытого
pub fn panel_size(st: &State) -> (i32, i32) {
    let (a, d) = panel_wh(st);
    (a + 2 * EAR + crate::bar::SHADE, d + crate::bar::SHADE)
}

pub fn draw_panel(ui: &mut Ui, st: &State, out: &str, w: i32, h: i32) {
    let e = edge(&st.set);
    let (pw, ph) = panel_wh(st);
    let cx = (w - crate::bar::SHADE) / 2;
    let dur = if st.set.anim() { 340.0 * st.set.anim_speed.max(0.1) } else { 0.0 };
    let k = st.island.open_at.map_or(1.0, |a| if dur == 0.0 { 1.0 } else { (a.elapsed().as_secs_f32() * 1000.0 / dur).min(1.0) });
    if k < 1.0 {
        ui.anim = true;
    }
    // ядро сборки — свёрнутый островок, каким он будет сейчас (не по прошлым ширинам: к концу
    // сворачивания плашка должна совпасть с баром под ней)
    let cw0 = collapsed_w(ui, st, out);
    let w0 = cw0.min(pw);
    let ears = cw0 <= pw;
    // сворачивается: те же клетки в обратную сторону — снаружи к свёрнутой полосе
    let kc = st.island.closing.map(|a| if dur == 0.0 { 1.0 } else { (a.elapsed().as_secs_f32() * 1000.0 / (dur * 0.8)).min(1.0) });
    let b = match kc {
        Some(kc) => {
            ui.anim = true;
            Some(Build { w0, h0: H, k: kc, grow: false, ears })
        }
        None => (k < 1.0).then_some(Build { w0, h0: H, k, grow: true, ears }),
    };
    // раскладка по краю: плашка, полоса модулей у края (strip), сетка плиток за ней (area)
    let ear = if st.set.island_hang { EAR } else { 0 };
    let sh = if st.set.island_hang { 0 } else { crate::bar::SHADE };
    let n = rows(&st.set.island_tiles);
    let (gw, gh) = (COLS * CELL + (COLS - 1) * GAP, n * CELL + (n - 1) * GAP);
    let a0 = cx - pw / 2;
    let (o, strip_r, area, line) = match e {
        Edge::Top => (Orient { edge: e, x: a0 - ear, y: 0 }, R::new(a0, 0, pw, H), R::new(a0 + PAD, H + 4, gw, gh), R::new(a0 + PAD, H, pw - 2 * PAD, 2)),
        Edge::Bottom => {
            let y = h - ph - sh;
            (Orient { edge: e, x: a0 - ear, y }, R::new(a0, y + ph - H, pw, H), R::new(a0 + PAD, y + PAD, gw, gh), R::new(a0 + PAD, y + ph - H - 2, pw - 2 * PAD, 2))
        }
    };
    let (_, holes) = plate(ui, st, o, pw, ph, b);
    // полоса у края; щелчок мимо значков — свернуть. Сворачивается — уже свёрнутая полоса, к ней
    // всё и сходится
    ui.zone(strip_r, Act::Popup("island"), Cur::Pointer);
    ui.cv.push_clip(R::new(strip_r.x + 2, strip_r.y, strip_r.w - 4, strip_r.h));
    ui.cv.oy += strip_r.y;
    if kc.is_some() {
        let cw = collapsed_w(ui, st, out);
        content(ui, st, out, R::new(cx - cw / 2, 0, cw, H));
    } else {
        strip(ui, st, out, a0, pw, cx);
    }
    ui.cv.oy -= strip_r.y;
    ui.cv.pop_clip();
    // настройки — в конце полосы, черта между полосой и плитками; при сворачивании их нет сразу:
    // от раскрытого остаётся только свёрнутая полоса
    if kc.is_none() {
        let g = R::new(a0 + pw - PAD - 26, strip_r.y + (H - 22) / 2, 26, 22);
        ui.button(g, "", "gear", Tone::Ghost, st.settings_shown, true, Act::IslSettings);
        ui.cv.fill(line, ui.t.border);
    }
    // плитки проявляются, когда плашка почти собрана
    for tile in &st.set.island_tiles {
        let r = cell_rect(area.x, area.y, CELL, GAP, tile);
        draw_tile(ui, st, tile, r, true);
    }
    if let Some(big) = &st.island.big {
        // растёт из своей плитки; без плитки (уведомления из слота) — из-под полосы посередине
        let from = st.set.island_tiles.iter().find(|t| t.kind == big.kind).map_or(R::new(area.x + area.w / 2 - 24, area.y, 48, 24), |t| cell_rect(area.x, area.y, CELL, GAP, t));
        let (bk, moving) = big_k(&st.set, big);
        if moving {
            ui.anim = true;
        }
        if bk > 0.0 {
            let lerp = |a: i32, b: i32| a + ((b - a) as f32 * bk).round() as i32;
            let r = R::new(lerp(from.x, area.x), lerp(from.y, area.y), lerp(from.w, area.w), lerp(from.h, area.h));
            // плитки под большим видом не ловят мышь и не просвечивают
            ui.zone(r, Act::Block, Cur::Default);
            ui.cv.fill(r, ui.t.panel);
            ui.cv.pxbox(r, &BoxStyle::new(ui.t.surface, ui.t.border).bevel(true));
            // содержимое — по текущей рамке: перестраивается вместе с ней (обложка растёт из
            // плитки), а не обрезается от готовой большой раскладки — та в первом же кадре
            // подменяла плитку чужим куском
            // ближе к плитке — раскладка самой плитки в текущей рамке (в конце она совпадает с
            // настоящей плиткой — без скачка), ближе к островку — большая; на смене одна
            // растворяется в фон плитки, другая проявляется
            let tile = st.set.island_tiles.iter().find(|t| t.kind == big.kind);
            // фон рамки как он выходит на экране: полупрозрачная плитка поверх плотной подложки
            let sf = ui.t.surface;
            let bg = ui.t.panel.lerp(sf.alpha(1.0), sf.a() as f32 / 255.0).alpha(1.0);
            ui.cv.push_clip(r.inset(2));
            match tile {
                Some(tile) if bk < 0.5 => {
                    draw_tile(ui, st, tile, r, false);
                    fade_to(ui, r.inset(3), ((0.5 - bk) / 0.2).min(1.0), bg);
                }
                _ => {
                    big_view(ui, st, big.kind, r.inset(10));
                    if tile.is_some() {
                        fade_to(ui, r.inset(3), ((bk - 0.5) / 0.2).min(1.0), bg);
                    }
                }
            }
            ui.cv.pop_clip();

        }
    }
    // плитки проявляются под конец сборки и гаснут в начале сворачивания
    let kk = match kc {
        Some(kc) => (1.0 - kc * 2.0).clamp(0.0, 1.0),
        None => ((k - 0.45) / 0.55).clamp(0.0, 1.0),
    };
    if kk < 1.0 {
        fade_to(ui, R::new(area.x - 2, area.y - 2, area.w + 4, area.h + 4), kk, ui.t.glass_panel());
    }
    holes.punch(ui);
}

/// полоса раскрытого: часы и индикаторы посередине (щелчок — свернуть), значки слотов по бокам
/// (свои действия: трей, обои, питание…); справа после них — шестерёнка
fn strip(ui: &mut Ui, st: &State, out: &str, x0: i32, pw: i32, cx: i32) {
    let mid: Vec<String> = CENTER.iter().map(|s| s.to_string()).collect();
    // края: слоты от левого края и от шестерёнки; середина — по центру, но сдвигается, чтобы не
    // налезть на слоты; не хватает места — слоты обрезаются по своей половине
    let (left0, right0) = (x0 + PAD + 4, x0 + pw - PAD - 26 - 10);
    let lw = crate::bar::row(ui, st, out, &mut st.set.island_left.iter(), 0, 1, false, 0);
    let rw = crate::bar::row(ui, st, out, &mut st.set.island_right.iter(), 0, 1, false, 0);
    let cw = crate::bar::row(ui, st, out, &mut mid.iter(), 0, 1, false, 0);
    let gap = 12;
    let lo = left0 + lw + if lw > 0 { gap } else { 0 };
    let hi = right0 - rw - if rw > 0 { gap } else { 0 };
    let mx = (cx - cw / 2).clamp(lo, (hi - cw).max(lo));
    let n0 = ui.zones.len();
    crate::bar::row(ui, st, out, &mut mid.iter(), mx, 1, true, 0);
    for z in ui.zones[n0..].iter_mut() {
        z.act = Act::Popup("island");
    }
    let n1 = ui.zones.len();
    ui.cv.push_clip(R::new(x0, 0, (mx - gap / 2 - x0).max(0), H));
    crate::bar::row(ui, st, out, &mut st.set.island_left.iter(), left0, 1, true, 0);
    ui.cv.pop_clip();
    let cr = mx + cw + gap / 2;
    ui.cv.push_clip(R::new(cr, 0, (x0 + pw - cr).max(0), H));
    crate::bar::row(ui, st, out, &mut st.set.island_right.iter(), right0, -1, true, 0);
    ui.cv.pop_clip();
    // всплывашки значков: уведомления — большим видом на всё место островка; трей — обычным
    // списком под стрелкой, островок остаётся; обои, Wallhaven, скрытые окна — своим окном
    // посередине под островком. Обрезанное не ловит мышь
    let (lclip, rclip) = (mx - gap / 2, cr);
    let tail: Vec<_> = ui.zones.drain(n1..).filter(|z| z.r.x + z.r.w / 2 < lclip || z.r.x + z.r.w / 2 >= rclip).collect();
    ui.zones.extend(tail);
    for z in ui.zones.iter_mut().skip(n1) {
        z.act = match &z.act {
            Act::Popup("cal") => Act::IslBig("notifs"),
            Act::Popup("tray") => Act::IslTray,
            Act::Popup(k) => Act::IslOpen(k),
            Act::BarWalls => Act::IslOpen("walls"),
            a => a.clone(),
        };
    }
}

/// сворачивание раскрытого кончилось
pub fn closed(st: &State) -> bool {
    let dur = if st.set.anim() { 340.0 * 0.8 * st.set.anim_speed.max(0.1) } else { 0.0 };
    st.island.closing.is_some_and(|a| a.elapsed().as_secs_f32() * 1000.0 >= dur)
}

/// большой вид плитки на весь островок: стрелка назад и название сверху, ниже — что поместится
fn big_view(ui: &mut Ui, st: &State, kind: &str, b: R) {
    let t = ui.t;
    ui.ghost(R::new(b.x - 2, b.y - 2, 26, 22), "left", Act::IslBack);
    let name = tile_info(kind).map_or("", |x| x.1);
    let tw = ui.text_in(PX, R::new(b.x + 30, b.y, b.w - 30, 18), name, t.text);
    let body = R::new(b.x, b.y + 28, b.w, b.h - 28);
    match kind {
        "calendar" => {
            if body.h >= 160 {
                // дата и месяц на всю ширину, листается стрелками; без колонки уведомлений
                let now = crate::clock::local_now();
                let date = crate::tf!("{}, {} {}", "{}, {} {}", crate::i18n::tr(crate::clock::RU_DAY_FULL[now.wday as usize]), now.day, crate::i18n::tr(crate::clock::RU_MON[(now.mon - 1) as usize]));
                // дата — отдельной строкой под названием, справа от неё место под «через сколько»
                let dy = body.y - 4;
                let dw = ui.text_in(PX, R::new(b.x, dy, b.w, 18), &date, t.text_dim);
                // выбранный день: через сколько он (или сколько прошло), как во всплывашке
                if let Some(day) = st.cal_pick {
                    let left = crate::widgets::days_until(day - crate::widgets::days_from_civil(now.year, now.mon, now.day));
                    let x = b.x + dw + 12;
                    ui.text_in(PX, R::new(x, dy, b.right() - x, 18), &left, t.text_faint);
                }
                crate::widgets::calendar(ui, st, R::new(body.x, body.y + 22, body.w, body.h - 22), st.cal_month, t.red, true);
            } else {
                month(ui, body, crate::clock::local_now());
            }
        }
        "music" => big_music(ui, st, body),
        "notifs" => {
            // «не беспокоить» и «очистить» — справа в строке названия
            let n = st.notifs.history.len();
            ui.button(R::new(b.right() - 58, b.y - 3, 26, 22), "", if st.set.dnd { "bellx" } else { "bell" }, Tone::Ghost, st.set.dnd, true, Act::Dnd);
            ui.button(R::new(b.right() - 26, b.y - 3, 26, 22), "", "trash", Tone::Ghost, false, n > 0, Act::NotifClear);
            if n > 0 {
                ui.text_in(PX, R::new(b.x + 40 + tw, b.y, 60, 18), &n.to_string(), t.text_faint);
                // листается колесом (notif_skip — сколько карточек сверху пропущено)
                let skip = st.island.notif_skip.min(n - 1);
                crate::popups::notif_cards(ui, st, body, body.y, skip, 0);
                if skip > 0 {
                    ui.text_right(PX, R::new(b.x, b.y, b.w - 66, 18), &format!("↑{skip}"), t.text_faint);
                }
            } else {
                ui.cv.icon_in(if st.set.dnd { "bellx" } else { "bell" }, R::new(body.x, body.cy() - 34, body.w, 28), 4, t.border);
                ui.text_center(PX, R::new(body.x, body.cy() + 4, body.w, 20), "тихо. ничего нового", t.text_faint);
            }
        }
        "load" => {
            // справа в строке названия — переключатель на вентилятор (ThinkPad, модуль fan)
            let fan = crate::features::FAN && st.fan.present;
            if fan {
                // одним значком: на системе — вентилятор, на вентиляторе — процессор
                ui.button(R::new(b.right() - 26, b.y - 3, 26, 22), "", if st.island.fan_view { "cpu" } else { "fan" }, Tone::Ghost, false, true, Act::IslFanView);
            }
            if fan && st.island.fan_view {
                crate::fan::draw(ui, st, body);
            } else {
                let up = std::fs::read_to_string("/proc/uptime").ok().and_then(|x| x.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())).unwrap_or(0.0) as u64;
                ui.text_in(PX, R::new(b.x + 40 + tw, b.y, b.w - 40 - tw - 34, 18), &crate::tf!("работает {} ч {} мин", "up {} h {} min", up / 3600, up / 60 % 60), t.text_faint);
                big_sys(ui, st, body);
            }
        }
        _ => {}
    }
}

/// большой плеер: обложка посередине, за ней приглушённый эквалайзер во всю ширину; ниже
/// название и исполнитель, полоса перемотки со временем и кнопки — как в карточке всплывашки,
/// но без своей рамки (растёт из плитки так же, как остальные большие виды)
fn big_music(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let m = &st.media;
    let title = if m.title.is_empty() { crate::i18n::tr("тишина").to_string() } else { m.title.clone() };
    let sub = if m.artist.is_empty() { m.player.clone() } else { m.artist.clone() };
    // снизу вверх: кнопки, перемотка, подписи; обложка — на то, что осталось
    let by = b.bottom() - 30;
    let sy = by - 26;
    let ty = sy - 44;
    let top = R::new(b.x, b.y, b.w, ty - 8 - b.y);
    if top.h > 16 {
        // эквалайзер за обложкой: цвет акцента, приглушённый к фону плитки
        let n = (top.w / 8).max(4);
        let len = st.viz.len() as i32;
        let step = top.w / n;
        let c = t.red.lerp(t.surface, 0.6);
        for i in 0..n {
            let (a, e) = (i * len / n, ((i + 1) * len / n).max(i * len / n + 1).min(len));
            let v = (a..e).filter_map(|k| st.viz.get(k as usize)).fold(0.0f32, |acc, &x| acc.max(x));
            let hh = if m.playing() { (v * top.h as f32).round().clamp(2.0, top.h as f32) as i32 / 2 * 2 } else { 2 };
            ui.cv.fill(R::new(top.x + i * step, top.bottom() - hh, step - 2, hh), c);
        }
        let side = top.h.min(top.w);
        let a = R::new(top.x + (top.w - side) / 2, top.y + (top.h - side) / 2, side, side);
        match st.art.get(&m.art).cloned().flatten() {
            Some(img) => {
                ui.cv.image_scaled(&img, a, 1.0);
                ui.cv.outline(a, t.border, 1);
            }
            None => {
                ui.cv.pxbox(a, &BoxStyle::new(t.code_bg, t.border));
                ui.cv.icon_in("music", a, (side / 24).clamp(2, 5), t.dim);
            }
        }
    }
    ui.text_center(PX24, R::new(b.x, ty, b.w, 26), &title, t.text_hi);
    ui.text_center(PX, R::new(b.x, ty + 26, b.w, 18), &sub, t.text_dim);
    if m.length > 0.0 {
        let at = st.seek.map(|f| f as f64 * m.length).unwrap_or(m.now_pos());
        let lw = ui.text_in(PX, R::new(b.x, sy, 60, 20), &yami_px::clock(at), if st.seek.is_some() { t.red } else { t.text_dim });
        let rw = ui.text_right(PX, R::new(b.x, sy, b.w, 20), &yami_px::clock(m.length), t.text_faint);
        let px0 = b.x + lw + 10;
        let pw = b.right() - rw - 10 - px0;
        if pw > 40 {
            crate::widgets::seek_bar(ui, st, R::new(px0, sy - 1, pw, 22), t.red, true);
        }
    }
    // повтор, назад, пауза, вперёд, перемешать — по центру
    let rep = m.repeat.as_str();
    let sel = |on: bool| if on { Tone::Primary } else { Tone::Ghost };
    let mut x = b.x + (b.w - (5 * 36 + 8 + 4 * 10)) / 2;
    ui.button(R::new(x, by, 36, 30), "", "repeat", sel(matches!(rep, "Playlist" | "Track")), false, !rep.is_empty(), Act::MediaRepeat);
    x += 46;
    ui.button(R::new(x, by, 36, 30), "", "prev", Tone::Ghost, false, true, Act::MediaPrev);
    x += 46;
    ui.button(R::new(x, by, 44, 30), "", if m.playing() { "pause" } else { "play" }, Tone::Primary, false, !m.title.is_empty(), Act::MediaPlay);
    x += 54;
    ui.button(R::new(x, by, 36, 30), "", "next", Tone::Ghost, false, true, Act::MediaNext);
    x += 46;
    ui.button(R::new(x, by, 36, 30), "", "shuffle", sel(m.shuffle == Some(true)), false, m.shuffle.is_some(), Act::MediaShuffle);
}

/// система: процессор, видеокарта, память, сеть — графиками; ниже процессы, сколько влезет
fn big_sys(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.sys;
    let gpu = if s.gpu_sleep { crate::i18n::tr("сон").to_string() } else if s.gpu_temp > 0.0 { format!("{:.0}%  {:.0}°", s.gpu_load * 100.0, s.gpu_temp) } else { "—".into() };
    let full = s.h_rx.max().max(s.h_tx.max()).max(64.0 * 1024.0);
    let cards: [(&str, &str, String, &str); 4] = [
        ("cpu", "CPU", format!("{:.0}%  {:.0}°", s.cpu * 100.0, s.temp), "cpu"),
        ("bolt", "GPU", gpu, "gpu_load"),
        ("ram", "RAM", crate::tf!("{:.1} / {:.0} ГБ", "{:.1} / {:.0} GB", s.mem_used, s.mem_total), "ram"),
        ("net", "NET", format!("↓ {}  ↑ {}", crate::sys::rate(s.rx), crate::sys::rate(s.tx)), "rx"),
    ];
    // высоко — карточки 2×2 и процессы под ними; низко — карточки в ряд
    let two = b.h >= 200;
    let (cols, ch) = if two { (2, ((b.h - 8 - 90) / 2).clamp(56, 84)) } else { (4, b.h.min(84)) };
    let cw = (b.w - (cols - 1) * 10) / cols;
    for (i, (icon, label, val, src)) in cards.iter().enumerate() {
        let r = R::new(b.x + (i as i32 % cols) * (cw + 10), b.y + (i as i32 / cols) * (ch + 8), cw, ch);
        ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
        let inr = r.inset(8);
        ui.cv.icon(icon, inr.x, inr.y + 3, 2, t.red);
        // узкая карточка: значение под подписью
        let narrow = ui.w(PX, label) + 28 + ui.w(PX, val) > inr.w;
        ui.text_in(PX, R::new(inr.x + 20, inr.y, 60, 20), label, t.text_dim);
        let gy = if narrow {
            ui.text_in(PX, R::new(inr.x, inr.y + 20, inr.w, 18), val, t.text);
            inr.y + 42
        } else {
            ui.text_right(PX, R::new(inr.x, inr.y, inr.w, 20), val, t.text);
            inr.y + 24
        };
        let gr = R::new(inr.x, gy, inr.w, inr.bottom() - gy);
        if gr.h < 8 {
            continue;
        }
        let tail = |ser: &crate::sys::Series, k: f32| ser.tail(140).iter().map(|v| v / k).collect::<Vec<f32>>();
        match *src {
            "cpu" => crate::widgets::graph(ui, gr, &tail(&s.h_cpu, 1.0), "line", "scroll", st.sys_phase(), if s.temp > 80.0 { t.ember } else { t.red }, Some((&tail(&s.h_temp, 100.0), t.accent2))),
            "gpu_load" => crate::widgets::graph(ui, gr, &tail(&s.h_gpu_load, 1.0), "line", "scroll", st.sys_phase(), t.red, Some((&tail(&s.h_gpu, 100.0), t.accent2))),
            "rx" => crate::widgets::graph(ui, gr, &tail(&s.h_rx, full as f32), "line", "scroll", st.sys_phase(), t.red, Some((&tail(&s.h_tx, full as f32), t.accent2))),
            _ => {
                let (ser, k) = s.series(src);
                crate::widgets::graph(ui, gr, &tail(ser, k), "bars", "scroll", st.sys_phase(), t.red, None);
            }
        }
    }
    let rows = (4 + cols - 1) / cols;
    let mut y = b.y + rows * (ch + 8) + 2;
    if y + 40 > b.bottom() {
        return;
    }
    let (cx, mx) = (b.right() - 150, b.right() - 70);
    ui.text_in(PX, R::new(b.x, y, 200, 18), "процессы", t.text_faint);
    ui.text_right(PX, R::new(cx, y, 70, 18), "CPU", t.text_faint);
    ui.text_right(PX, R::new(mx, y, 70, 18), "RAM", t.text_faint);
    y += 22;
    for p in &st.procs.top {
        if y + 18 > b.bottom() {
            break;
        }
        ui.text_in(PX, R::new(b.x, y, cx - b.x - 8, 18), &p.name, t.text);
        ui.text_right(PX, R::new(cx, y, 70, 18), &format!("{:.1}%", p.cpu * 100.0), if p.cpu > 0.5 { t.ember } else { t.text_dim });
        ui.text_right(PX, R::new(mx, y, 70, 18), &crate::sys::rate(p.mem), t.text_dim);
        y += 20;
    }
}

/// эквалайзер островка виден: раскрытый с музыкой или раздвинутый на смену трека
pub fn shows_viz(st: &State) -> bool {
    on(&st.set) && ((st.popup_is("island") && st.island.closing.is_none() && (st.set.island_tiles.iter().any(|t| t.kind == "music") || st.island.big.as_ref().is_some_and(|b| b.kind == "music"))) || matches!(st.island.live, Some((Live::Track(..), _))))
}

/// открыт большой вид системы: процессы опрашиваются, как у всплывашки «система»
pub fn shows_sys(st: &State) -> bool {
    st.popup_is("island") && st.island.big.as_ref().is_some_and(|b| b.kind == "load" && !b.back) && !st.island.fan_view
}

/// открыт вентилятор в большом виде нагрузки: опрашивается, как у всплывашки «fan»
pub fn shows_fan(st: &State) -> bool {
    st.popup_is("island") && st.island.big.as_ref().is_some_and(|b| b.kind == "load" && !b.back) && st.island.fan_view
}

/// растворить нарисованное в r в цвет c (доля k видна): клетки по 2 px вразнобой. В отличие от
/// dissolve_rect не пробивает дыры в прозрачное — под плитками плашка, а не стол; пустое
/// (несобранные края) остаётся пустым
fn fade_to(ui: &mut Ui, r: R, k: f32, c: crate::gfx::C) {
    if k >= 1.0 {
        return;
    }
    let cv = &mut ui.cv;
    let r = R::new(r.x + cv.ox, r.y + cv.oy, r.w, r.h).inter(&R::new(0, 0, cv.w, cv.h));
    let fill = c.premul();
    for y in r.y..r.bottom() {
        let row = (y * cv.w) as usize;
        for x in r.x..r.right() {
            let p = &mut cv.buf[row + x as usize];
            if *p >> 24 != 0 && hash(x / 2, y / 2) >= k {
                *p = fill;
            }
        }
    }
}

pub fn cell_rect(x0: i32, y0: i32, cell: i32, gap: i32, t: &ITile) -> R {
    R::new(x0 + t.x * (cell + gap), y0 + t.y * (cell + gap), t.w * cell + (t.w - 1) * gap, t.h * cell + (t.h - 1) * gap)
}

// ── плитки ──────────────────────────────────────────────────────────────────

/// плитка в прямоугольнике r; live = false — в редакторе (без своих кнопок)
pub fn draw_tile(ui: &mut Ui, st: &State, tile: &ITile, r: R, live: bool) {
    let t = ui.t;
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, t.border).bevel(true));
    let n0 = ui.zones.len();
    let b = r.inset(8);
    // щелчок по плитке (мимо её кнопок) — большой вид
    if live {
        if let Some(k) = BIG.iter().find(|k| **k == tile.kind) {
            ui.zone(r, Act::IslBig(k), Cur::Pointer);
        }
    }
    // плитка не вылезает к соседям, как бы ни была мала
    ui.cv.push_clip(r.inset(2));
    match tile.kind.as_str() {
        "music" => music(ui, st, b, tile),
        "calendar" => calendar(ui, b, tile),
        "clock" => clock(ui, b),
        "load" => load(ui, st, b, tile),
        "sound" => sound(ui, st, b),
        "notifs" => notifs(ui, st, b),
        "power" => power(ui, b),
        _ => {}
    }
    ui.cv.pop_clip();
    if !live {
        ui.zones.truncate(n0);
    }
}

fn music(ui: &mut Ui, st: &State, b: R, tile: &ITile) {
    let t = ui.t;
    let m = &st.media;
    let art = st.art.get(&m.art).cloned().flatten();
    let cover = |ui: &mut Ui, a: R| match &art {
        Some(img) => {
            ui.cv.image_scaled(img, a, 1.0);
            ui.cv.outline(a, t.border, 1);
        }
        None => {
            ui.cv.pxbox(a, &BoxStyle::new(t.code_bg, t.border));
            ui.cv.icon_in("music", a, (a.w / 24).clamp(2, 5), t.dim);
        }
    };
    let play = |ui: &mut Ui, r: R| ui.button(r, "", if m.playing() { "pause" } else { "play" }, Tone::Primary, false, !m.title.is_empty(), Act::MediaPlay);
    // назад, пауза, вперёд — от левого края x; 96 px
    let buttons = |ui: &mut Ui, x: i32, y: i32| {
        ui.button(R::new(x, y, 28, 26), "", "prev", Tone::Ghost, false, true, Act::MediaPrev);
        play(ui, R::new(x + 32, y, 32, 26));
        ui.button(R::new(x + 68, y, 28, 26), "", "next", Tone::Ghost, false, true, Act::MediaNext);
    };
    const BTN_W: i32 = 96;
    let title = if m.title.is_empty() { crate::i18n::tr("тишина").to_string() } else { m.title.clone() };
    let sub = if m.artist.is_empty() { m.player.clone() } else { m.artist.clone() };
    if tile.w == 1 && tile.h == 1 {
        // 1×1: обложка и пауза поверх
        cover(ui, b);
        play(ui, R::new(b.right() - 30, b.bottom() - 26, 30, 26));
        return;
    }
    if tile.w > tile.h || tile.h == 1 {
        // в ширину: обложка слева во всю высоту; справа сверху подписи, снизу кнопки, над ними
        // полоса, а между — эквалайзер, если осталось место
        let side = b.h.min(b.w / 2);
        let a = R::new(b.x, b.y + (b.h - side) / 2, side, side);
        cover(ui, a);
        let x = a.right() + 10;
        let r = R::new(x, b.y, b.right() - x, b.h);
        let big = r.h >= 120 && r.w >= 150;
        let mut y = r.y + 2;
        if big {
            ui.text_in(PX24, R::new(r.x, y, r.w, 26), &title, t.text_hi);
            y += 30;
        } else {
            ui.text_in(PX, R::new(r.x, y, r.w, 18), &title, t.text_hi);
            y += 20;
        }
        ui.text_in(PX, R::new(r.x, y, r.w, 18), &sub, t.text_dim);
        y += 20;
        if big && !m.album.is_empty() {
            ui.text_in(PX, R::new(r.x, y, r.w, 18), &m.album, t.text_faint);
            y += 20;
        }
        let by = r.bottom() - 26;
        if r.w >= BTN_W {
            buttons(ui, r.x, by);
        } else if r.w >= 84 {
            // тесно: те же три кнопки уже
            ui.button(R::new(r.x, by, 24, 26), "", "prev", Tone::Ghost, false, true, Act::MediaPrev);
            play(ui, R::new(r.x + 28, by, 28, 26));
            ui.button(R::new(r.x + 60, by, 24, 26), "", "next", Tone::Ghost, false, true, Act::MediaNext);
        } else {
            play(ui, R::new(r.x, by, 32, 26));
        }
        let mut low = by - 4;
        if r.h >= 100 && m.length > 0.0 {
            crate::widgets::seek_bar(ui, st, R::new(r.x, by - 20, r.w, 14), t.red, false);
            low = by - 24;
        }
        if low - y >= 12 {
            eq(ui, st, R::new(r.x, y + 4, r.w, low - y - 4), (r.w / 6).max(3));
        } else if r.w >= BTN_W + 40 {
            eq(ui, st, R::new(r.x + BTN_W + 10, by + 6, r.w - BTN_W - 10, 16), ((r.w - BTN_W - 10) / 6).clamp(3, 12));
        }
        return;
    }
    // в высоту: обложка сверху, под ней подписи, кнопки, полоса и эквалайзер — что поместится
    let below = 18 + 18 + 32 + 16 + 14;
    let side = (b.h - below).min(b.w);
    let mut y = b.y;
    if side >= 32 {
        cover(ui, R::new(b.x + (b.w - side) / 2, y, side, side));
        y += side + 6;
    }
    ui.text_center(PX, R::new(b.x, y, b.w, 18), &title, t.text_hi);
    y += 18;
    ui.text_center(PX, R::new(b.x, y, b.w, 18), &sub, t.text_dim);
    y += 22;
    if y + 26 <= b.bottom() {
        buttons(ui, b.x + (b.w - BTN_W) / 2, y);
        y += 32;
    }
    if m.length > 0.0 && y + 14 <= b.bottom() {
        crate::widgets::seek_bar(ui, st, R::new(b.x, y, b.w, 14), t.red, false);
    }
    y += 16;
    if b.bottom() - y >= 6 {
        eq(ui, st, R::new(b.x, y, b.w, b.bottom() - y), (b.w / 6).max(3));
    }
}

fn calendar(ui: &mut Ui, b: R, tile: &ITile) {
    let t = ui.t;
    let tm = crate::clock::local_now();
    let day = tm.day.to_string();
    let wday = crate::i18n::tr(crate::clock::RU_DAY[tm.wday as usize]);
    if tile.h >= 2 && tile.w >= 2 {
        month(ui, b, tm);
        return;
    }
    if tile.w == 1 {
        // одна клетка: только число, крупно, и день недели в правом верхнем углу
        let face = crate::font::px(if b.h >= 120 { 64 } else { 48 });
        ui.text_right(PX, R::new(b.x, b.y, b.w, 18), wday, t.red);
        ui.text_center(face, R::new(b.x, b.y + 10, b.w, b.h - 10), &day, t.text_hi);
        return;
    }
    ui.text_in(PX24, R::new(b.x, b.y, b.w, 26), &day, t.text_hi);
    let dw = ui.w(PX24, &day);
    ui.text_in(PX, R::new(b.x + dw + 8, b.y + 6, b.w - dw - 8, 18), wday, t.red);
    // неделя: сегодня — в акценте
    let y = b.bottom() - 26;
    let n = 7;
    let cw = (b.w - (n - 1) * 4) / n;
    let wd = (tm.wday as i32 + 6) % 7;
    for i in 0..n {
        let r = R::new(b.x + i * (cw + 4), y, cw, 24);
        let off = i - wd;
        let d = crate::clock::local_tm(crate::clock::unix_now() as i64 + off as i64 * 86_400).day;
        let today = off == 0;
        ui.cv.pxbox(r, &BoxStyle::new(if today { t.red } else { t.code_bg }, if today { t.ember } else { t.border }));
        ui.text_center(PX, r, &d.to_string(), if today { t.on_accent } else { t.text_dim });
    }
}

/// месяц сеткой 7×6 в b: название, дни недели, сегодня — в акценте
fn month(ui: &mut Ui, b: R, tm: crate::clock::Tm) {
    let t = ui.t;
    let name = crate::i18n::tr(crate::clock::RU_MON_NOM[tm.mon as usize - 1]);
    ui.text_in(PX, R::new(b.x, b.y, b.w, 18), &format!("{name} {}", tm.year), t.text);
    let cw = b.w / 7;
    let top = b.y + 22;
    for (i, d) in ["пн", "вт", "ср", "чт", "пт", "сб", "вс"].iter().enumerate() {
        ui.text_center(PX, R::new(b.x + i as i32 * cw, top, cw, 18), crate::i18n::tr(d), if i >= 5 { t.red } else { t.text_faint });
    }
    let first = crate::widgets::days_from_civil(tm.year, tm.mon, 1);
    // 1970-01-01 — четверг: 0 = понедельник
    let lead = (first + 3).rem_euclid(7) as i32;
    let days = crate::widgets::days_in_month(tm.year, tm.mon) as i32;
    let weeks = (lead + days + 6) / 7;
    let ch = ((b.bottom() - top - 20) / weeks).min(24);
    for d in 1..=days {
        let k = lead + d - 1;
        let r = R::new(b.x + (k % 7) * cw, top + 20 + (k / 7) * ch, cw, ch);
        let today = d as u32 == tm.day;
        if today {
            ui.cv.fill(r.inset(1), t.red);
        }
        ui.text_center(PX, r, &d.to_string(), if today { t.on_accent } else if k % 7 >= 5 { t.text_dim } else { t.text });
    }
}

fn clock(ui: &mut Ui, b: R) {
    let t = ui.t;
    let tm = crate::clock::local_now();
    ui.text_center(PX24, R::new(b.x, b.y, b.w, b.h - 18), &format!("{:02}:{:02}", tm.hour, tm.min), t.text_hi);
    ui.text_center(PX, R::new(b.x, b.bottom() - 20, b.w, 18), &format!("{} {:02}.{:02}", crate::i18n::tr(crate::clock::RU_DAY[tm.wday as usize]), tm.day, tm.mon), t.text_dim);
}

/// счётчик «нагрузки»: подпись, значение 0..1, текст, горячо ли
fn counter(st: &State, k: &str) -> Option<(&'static str, f32, String, bool)> {
    let s = &st.sys;
    let pct = |v: f32| format!("{}%", (v * 100.0).round());
    Some(match k {
        "cpu" => ("cpu", s.cpu, pct(s.cpu), s.cpu > 0.85),
        "ram" => ("ram", s.mem, pct(s.mem), s.mem > 0.85),
        "temp" => ("t°", (s.temp / 100.0).min(1.0), format!("{}°", s.temp.round()), s.temp > 80.0),
        "gpu" if s.has_gpu => ("gpu", s.gpu_load, pct(s.gpu_load), s.gpu_load > 0.95),
        "disk" => ("disk", s.disk, pct(s.disk), s.disk > 0.9),
        "fan" if st.fan.present => ("fan", st.fan.pct(), st.fan.rpm.map_or(pct(st.fan.pct()), |r| r.to_string()), st.fan.critical),
        _ => return None,
    })
}

/// сетка счётчиков в прямоугольнике b: столбцов, строк, сколько поместилось. Клетки поровну;
/// из раскладок, где все клетки не меньше MIN, — с самой крупной клеткой
fn counter_grid(b: R, n: i32) -> (i32, i32, i32) {
    const MIN: (i32, i32) = (52, 34);
    let mut best: Option<(i32, i32, i32, i32)> = None;
    for shown in (1..=n).rev() {
        for cols in 1..=shown {
            let rows = (shown + cols - 1) / cols;
            let cw = (b.w - (cols - 1) * 10) / cols;
            let ch = (b.h - (rows - 1) * 6) / rows;
            if cw < MIN.0 || ch < MIN.1 {
                continue;
            }
            // крупнее — у которой меньшая сторона клетки (в долях желанной 90×46) больше
            let score = (cw * 100 / 90).min(ch * 100 / 46);
            if best.is_none_or(|b| score > b.3) {
                best = Some((cols, rows, shown, score));
            }
        }
        if best.is_some() {
            break;
        }
    }
    best.map_or((1, 1, 1), |b| (b.0, b.1, b.2))
}

fn load(ui: &mut Ui, st: &State, b: R, tile: &ITile) {
    let t = ui.t;
    let list: Vec<_> = tile.opts.iter().filter_map(|o| counter(st, o)).collect();
    if list.is_empty() {
        ui.text_center(PX, b, "нет счётчиков", t.text_faint);
        return;
    }
    let number = tile.opts.iter().any(|o| o == "number");
    let (cols, rows, shown) = counter_grid(b, list.len() as i32);
    let cw = (b.w - (cols - 1) * 10) / cols;
    let ch = (b.h - (rows - 1) * 6) / rows;
    // вид один на все клетки: крупное число, в строку с подписью или под ней
    let widest = list.iter().take(shown as usize).map(|(l, _, v, _)| ui.w(PX, l) + 8 + ui.w(PX, v)).max().unwrap_or(0);
    let big = ch >= 58 && cw >= 80;
    let one_line = !big && widest <= cw;
    // высота содержимого клетки: подпись, число, шкала — по середине клетки
    let gauge_h = if number { 0 } else { 8 };
    let body = if big { 48 + gauge_h } else if one_line { 22 + gauge_h } else { 40 + gauge_h };
    let body = if gauge_h > 0 && body > ch { body - gauge_h } else { body };
    let dy = ((ch - body) / 2).max(0);
    // неполная последняя строка — по центру под полными
    let last = shown - (rows - 1) * cols;
    for (i, (label, v, txt, hot)) in list.iter().take(shown as usize).enumerate() {
        let (c, r) = (i as i32 % cols, i as i32 / cols);
        let shift = if r == rows - 1 && last < cols { (cols - last) * (cw + 10) / 2 } else { 0 };
        let cell = R::new(b.x + shift + c * (cw + 10), b.y + r * (ch + 6) + dy, cw, ch - dy);
        let col = if *hot { t.ember } else { t.red };
        let tc = if *hot { t.ember } else { t.text };
        ui.text_in(PX, R::new(cell.x, cell.y, cell.w, 18), label, t.text_faint);
        let gy = if big {
            ui.text_in(PX24, R::new(cell.x, cell.y + 18, cell.w, 26), txt, tc);
            cell.y + 48
        } else if one_line {
            ui.text_right(PX, R::new(cell.x, cell.y, cell.w, 18), txt, tc);
            cell.y + 22
        } else {
            ui.text_in(PX, R::new(cell.x, cell.y + 18, cell.w, 18), txt, tc);
            cell.y + 40
        };
        if !number && gy + 8 <= cell.bottom() {
            ui.cv.gauge(R::new(cell.x, gy, cell.w, 8), (cell.w / 8).max(4), *v, col, t.surface_hi.lerp(t.border, 0.45), 2);
        }
    }
}

fn sound(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let a = &st.audio;
    // звук и яркость (если есть подсветка): значок, значение, ползунок
    let mut list: Vec<(&str, f32, bool, Act, Act)> = vec![(if a.muted { "mute" } else { "speaker" }, a.volume as f32, a.muted, Act::IslVol, Act::CtlVol)];
    if let Some(br) = st.leds.bright {
        list.push(("sun", br, false, Act::IslBright, Act::CtlBright));
    }
    let n = list.len() as i32;
    if b.w > b.h {
        // в ширину — широкие полосы строками; процент — внутри полосы, пока её тянут: над
        // пустой частью акцентом, над заполненной — цветом фона
        let rh = ((b.h - (n - 1) * 8) / n).clamp(18, 30);
        let y0 = b.y + (b.h - (n * rh + (n - 1) * 8)) / 2;
        let held = st.island.held.filter(|h| h.1.elapsed() < Duration::from_millis(1000)).map(|h| h.0);
        for (i, (icon, v, off, act, _)) in list.into_iter().enumerate() {
            let y = y0 + i as i32 * (rh + 8);
            let cy = y + rh / 2;
            ui.cv.icon(icon, b.x, cy - 7, 2, if off { t.text_faint } else { t.text_dim });
            let bright = icon == "sun";
            if !bright {
                ui.zone(R::new(b.x - 2, y, 20, rh), Act::VolMute, Cur::Pointer);
            }
            let bar = R::new(b.x + 22, y, b.w - 22, rh);
            ui.cv.pxbox(bar, &BoxStyle::new(t.code_bg, t.border));
            let inner = bar.inset(2);
            let fw = (inner.w as f32 * v.clamp(0.0, 1.0)).round() as i32 / 2 * 2;
            let fillc = if off { t.text_faint } else { t.red };
            let filled = R::new(inner.x, inner.y, fw, inner.h);
            ui.cv.fill(filled, fillc);
            if held == Some(bright) {
                let txt = format!("{}", (v * 100.0).round());
                ui.cv.push_clip(R::new(inner.x + fw, inner.y, inner.w - fw, inner.h));
                ui.text_center(PX, inner, &txt, fillc);
                ui.cv.pop_clip();
                ui.cv.push_clip(filled);
                ui.text_center(PX, inner, &txt, t.bg);
                ui.cv.pop_clip();
            }
            ui.zone(bar, act, Cur::Pointer);
        }
        return;
    }
    // в высоту — столбики снизу вверх
    let cw = ((b.w - (n - 1) * 10) / n).clamp(14, 36);
    let x0 = b.x + (b.w - (n * cw + (n - 1) * 10)) / 2;
    for (i, (icon, v, off, _, act)) in list.into_iter().enumerate() {
        let x = x0 + i as i32 * (cw + 10);
        let r = R::new(x, b.y, cw, b.h - 22);
        ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
        let fh = ((r.h - 4) as f32 * v.clamp(0.0, 1.0)).round() as i32 / 2 * 2;
        ui.cv.fill(R::new(r.x + 2, r.bottom() - 2 - fh, r.w - 4, fh), if off { t.text_faint } else { t.red });
        ui.zone(r, act, Cur::Pointer);
        ui.cv.icon(icon, x + (cw - 14) / 2, b.bottom() - 16, 2, if off { t.text_faint } else { t.text_dim });
        if icon != "sun" {
            ui.zone(R::new(x, b.bottom() - 20, cw, 20), Act::VolMute, Cur::Pointer);
        }
    }
}

fn notifs(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let list = &st.notifs.history;
    ui.cv.icon(if st.quiet() { "bellx" } else { "bell" }, b.x, b.y + 2, 2, t.red);
    ui.text_in(PX, R::new(b.x + 22, b.y, b.w - 22, 18), &crate::tf!("уведомления · {}", "notifications · {}", list.len()), t.text);
    if list.is_empty() {
        ui.text_center(PX, R::new(b.x, b.y + 20, b.w, b.h - 20), "пусто", t.text_faint);
    }
    let hide = st.auto_quiet();
    let mut y = b.y + 26;
    for n in list.iter().rev() {
        if y + 20 > b.bottom() {
            break;
        }
        // «тихо само» (показ экрана): вместо текста звёздочки той же длины
        let (app, sum) = if hide { (stars(&n.app), stars(&n.summary)) } else { (n.app.clone(), n.summary.clone()) };
        let aw = ui.text_in(PX, R::new(b.x, y, b.w / 3, 18), &app, t.text_faint);
        ui.text_in(PX, R::new(b.x + aw + 8, y, b.w - aw - 8, 18), &sum, t.text_dim);
        y += 20;
    }
}

/// текст звёздочками: столько же знаков, пробелы на месте
fn stars(s: &str) -> String {
    s.chars().map(|c| if c.is_whitespace() { ' ' } else { '*' }).collect()
}

fn power(ui: &mut Ui, b: R) {
    // три кнопки, пока каждая не уже 28 px; теснее — только выключение
    let items: &[(&str, &str)] = if b.w >= 3 * 28 + 12 { &[("lock", "lock"), ("moon", "sleep"), ("power", "poweroff")] } else { &[("power", "poweroff")] };
    let n = items.len() as i32;
    let bw = ((b.w - (n - 1) * 6) / n).min(90);
    let bh = b.h.min(40);
    let x0 = b.x + (b.w - (n * bw + (n - 1) * 6)) / 2;
    for (i, (icon, act)) in items.iter().enumerate() {
        let r = R::new(x0 + i as i32 * (bw + 6), b.y + (b.h - bh) / 2, bw, bh);
        let tone = if *act == "poweroff" { Tone::Danger } else { Tone::Plain };
        ui.button(r, "", icon, tone, false, true, if *act == "poweroff" { Act::Power } else { Act::ISys(act) });
    }
}

// ── события и раскрытие ─────────────────────────────────────────────────────

impl App {
    /// живое событие в островок (если включён и это событие выбрано); true — показано
    pub fn island_live(&mut self, l: Live) -> bool {
        if !on(&self.st.set) || self.st.locked {
            return false;
        }
        if !self.st.set.island_events.iter().any(|e| e == l.key()) {
            // выключенное в островке событие не уходит и в OSD
            return true;
        }
        // раскрыт — там и так всё видно
        if self.st.popup_is("island") {
            return true;
        }
        self.st.island.live = Some((l, Instant::now()));
        self.viz_sync();
        self.dirty(&crate::app::SKind::Bar);
        true
    }

    /// тик: событие отжило своё; курсор давно ушёл с раскрытого; зарядка
    pub fn island_tick(&mut self) {
        // режим выключили (в том числе правкой файла) — раскрытый не оставлять
        if !on(&self.st.set) && (self.st.popup_is("island") || self.st.island.live.is_some()) {
            self.close_island();
        }
        if let Some((l, at)) = &self.st.island.live {
            if at.elapsed() > l.dur(&self.st.set) {
                self.st.island.live = None;
                self.viz_sync();
                self.dirty(&crate::app::SKind::Bar);
            }
        }
        if closed(&self.st) {
            self.close_popup_now();
        }
        // свернувшийся обратно большой вид — забыть
        if self.st.island.big.as_ref().is_some_and(|b| b.back && !big_k(&self.st.set, b).1) {
            self.st.island.big = None;
            self.dirty(&crate::app::SKind::Popup);
        }
        if self.st.island.held.is_some_and(|h| h.1.elapsed() > Duration::from_millis(1000)) {
            self.st.island.held = None;
            self.dirty(&crate::app::SKind::Popup);
        }
        // открыто меню (трея, программы) — не сворачивать, пока его не закроют
        if self.st.island.leave.is_some_and(|t| t.elapsed() > Duration::from_millis(1200)) && self.st.popup_is("island") && self.st.drop.is_none() && self.st.menu.is_none() {
            self.st.island.leave = None;
            self.close_popup();
        }
        let plugged = self.st.sys.bat.map(|b| b.plugged || b.charging);
        if let (Some(old), Some(now)) = (self.st.island.plugged, plugged) {
            if old != now {
                let pct = self.st.sys.bat.map_or(0, |b| b.pct);
                self.island_live(Live::Power(now, pct));
            }
        }
        self.st.island.plugged = plugged;
    }

    /// niri сменил стол в фокусе (old — прошлый id): на каком мониторе теперь
    pub fn island_ws(&mut self, old: Option<u64>) {
        let now = self.st.wm.ws.iter().find(|w| w.focused).map(|w| (w.id, w.output.clone()));
        if let (Some(o), Some((id, out))) = (old, now) {
            if o != id {
                self.island_live(Live::Ws(out));
            }
        }
    }

    /// пришло состояние bluetooth: кто подключился или отвалился
    pub fn island_bt(&mut self) {
        let now: Vec<(String, String)> = self.st.bt.devices.iter().filter(|d| d.connected).map(|d| (d.path.clone(), d.name.clone())).collect();
        let old = std::mem::replace(&mut self.st.island.bt, now.clone());
        if self.st.started.elapsed() < Duration::from_secs(3) {
            return;
        }
        if let Some((_, n)) = now.iter().find(|d| !old.iter().any(|o| o.0 == d.0)) {
            self.island_live(Live::Bt(n.clone(), true));
        } else if let Some((_, n)) = old.iter().find(|d| !now.iter().any(|o| o.0 == d.0)) {
            self.island_live(Live::Bt(n.clone(), false));
        }
    }

    /// сменился трек показанного плеера
    pub fn island_track(&mut self, old_title: &str) {
        let m = &self.st.media;
        if m.title.is_empty() || m.title == old_title || self.st.started.elapsed() < Duration::from_secs(3) {
            return;
        }
        let l = Live::Track(m.title.clone(), m.artist.clone());
        self.island_live(l);
    }
}

// ── редактор во вкладке «бар» ───────────────────────────────────────────────

/// ширина палитры плиток справа от сетки
const PAL_MIN: i32 = 150;
const CHIP_H: i32 = 26;

fn mod_name(m: &str) -> &str {
    crate::bartab::mod_name(m)
}

/// режим бара и вид островка — первая строка вкладки «бар» в обоих режимах
pub fn mode_row(ui: &mut Ui, st: &State, b: R, y: i32) -> i32 {
    let t = ui.t;
    let s = &st.set;
    ui.text_in(PX, R::new(b.x, y, 60, 26), "форма", t.text_dim);
    let mut x = b.x + 60;
    for (label, isl) in [("бар", false), ("островок", true)] {
        let w = ui.w(PX, label) + 24;
        ui.button(R::new(x, y, w, 26), label, "", if s.bar_island == isl { Tone::Primary } else { Tone::Plain }, false, true, Act::SIsland(isl));
        x += w + 2;
    }
    if s.bar_island {
        x += 14;
        ui.cv.fill(R::new(x, y + 4, 2, 18), t.border);
        x += 16;
        for (label, hang) in [("Полуостров", true), ("Остров", false)] {
            let w = ui.w(PX, label) + 24;
            ui.button(R::new(x, y, w, 26), label, "", Tone::Plain, s.island_hang == hang, true, Act::IslHang(hang));
            x += w + 2;
        }
    }
    y + 40
}

/// редактор островка вместо колонок бара; возвращает, где он кончился
pub fn tab(ui: &mut Ui, st: &State, b: R, y: i32) -> i32 {
    let t = ui.t;
    let s = &st.set;
    let ed = &st.isl_ed;
    let drag = ed.drag.as_ref();
    // ── свёрнутый: фишки модулей в ряд
    let mut y = crate::settings::title(ui, b.x, y, b.w, "свёрнутый");
    let from = match drag.map(|d| &d.what) {
        Some(DragWhat::Mod(f, _)) => *f,
        _ => None,
    };
    let slot = drag.filter(|d| matches!(d.what, DragWhat::Mod(..))).and_then(|d| d.slot);
    let items: Vec<(usize, &String)> = s.island_mods.iter().enumerate().filter(|(i, _)| Some(*i) != from).collect();
    // раскладка с переносом: щель под фишку в руке — тоже место
    let held = drag.and_then(|d| match &d.what {
        DragWhat::Mod(_, m) => Some(chip_w(ui, m)),
        _ => None,
    });
    let right = b.right() - 6 - 100;
    let (mut x, mut ly) = (b.x + 6, y + 6);
    let mut place = |w: i32| {
        if x + w > right && x > b.x + 6 {
            x = b.x + 6;
            ly += CHIP_H + 6;
        }
        let r = R::new(x, ly, w, CHIP_H);
        x += w + 6;
        r
    };
    let mut cells: Vec<(Option<(usize, &String)>, R)> = vec![];
    for j in 0..=items.len() {
        if slot == Some(j) {
            cells.push((None, place(held.unwrap_or(60))));
        }
        if let Some(it) = items.get(j) {
            let w = chip_w(ui, it.1);
            cells.push((Some(*it), place(w)));
        }
    }
    let row = R::new(b.x, y, b.w, ly + CHIP_H + 6 - y);
    ui.cv.pxbox(row, &BoxStyle::new(t.code_bg, if held.is_some() { t.red } else { t.border }));
    let mut rects = vec![];
    for (it, r) in cells {
        match it {
            None => ui.cv.ants(r, t.ember, 2, 0),
            Some((i, m)) => {
                chip(ui, r, mod_name(m), false);
                ui.zone(R::new(r.x, r.y, r.w - 22, r.h), Act::IslMod(i), Cur::Grab);
                if drag.is_none() {
                    ui.ghost(R::new(r.right() - 22, r.y + 2, 20, 22), "close", Act::IslModDel(i));
                }
                rects.push(r);
            }
        }
    }
    *ed.chips.borrow_mut() = rects;
    ed.chip_row.set(row);
    let add = R::new(row.right() - 94, row.y + 6, 88, CHIP_H);
    ui.button(add, "модуль", "plus", Tone::Ghost, ed.add, true, Act::IslModAdd);
    y = row.bottom() + 16;

    // ── раскрытый: сетка слева, палитра справа
    y = crate::settings::title(ui, b.x, y, b.w, "раскрытый");
    // клетки как в настоящем островке (CELL, GAP): плитка в редакторе выглядит ровно так же —
    // сколько счётчиков влезло, какая раскладка у музыки. Тесно — палитра уже, потом клетки
    let gap = GAP;
    let gw = (b.w - PAL_MIN - 16).min(COLS * CELL + (COLS - 1) * GAP + 16);
    let cell = ((gw - 16 - (COLS - 1) * gap) / COLS).clamp(48, CELL);
    // полоса раскрытого: слоты слева и справа от часов
    let slots_r = R::new(b.x, y, COLS * cell + (COLS - 1) * gap + 16, 36);
    let slot_add = slot_row(ui, st, slots_r);
    let y = slots_r.bottom() + 8;
    let n = (rows(&s.island_tiles) + 1).min(MAX_ROWS);
    let frame = R::new(b.x, y, COLS * cell + (COLS - 1) * gap + 16, n * cell + (n - 1) * gap + 16);
    ui.cv.pxbox(frame, &BoxStyle::new(t.panel, t.dim).shadow(3));
    let (gx, gy) = (frame.x + 8, frame.y + 8);
    ed.grid.set((gx, gy, cell, gap));
    // пустые клетки
    for cy in 0..n {
        for cx in 0..COLS {
            let r = R::new(gx + cx * (cell + gap), gy + cy * (cell + gap), cell, cell);
            if !s.island_tiles.iter().any(|o| overlap(o, cx, cy, 1, 1)) {
                ui.cv.ants(r, t.border, 2, 0);
                ui.cv.icon("plus", r.x + r.w / 2 - 7, r.y + r.h / 2 - 7, 2, t.text_faint);
            }
        }
    }
    let lifted = match drag.map(|d| &d.what) {
        Some(DragWhat::Move(i)) => Some(*i),
        _ => None,
    };
    for (i, tile) in s.island_tiles.iter().enumerate() {
        if Some(i) == lifted {
            continue;
        }
        let r = cell_rect(gx, gy, cell, gap, tile);
        // во время растягивания — новый размер
        let r = match drag {
            Some(IDrag { what: DragWhat::Size(j), cell: Some((x, y, w, h)), .. }) if *j == i => cell_rect(gx, gy, cell, gap, &ITile { x: *x, y: *y, w: *w, h: *h, ..Default::default() }),
            _ => r,
        };
        ui.cv.push_clip(r);
        draw_tile(ui, st, tile, r, false);
        ui.cv.pop_clip();
        let hov = ui.hover(r) && drag.is_none();
        if hov || ed.gear == Some(i) {
            ui.cv.outline(r, t.red, 2);
        }
        ui.zone(r, Act::IslTile(i), Cur::Grab);
        // угол — размер; шестерёнка и крестик — при наведении
        let hd = R::new(r.right() - 10, r.bottom() - 10, 10, 10);
        ui.cv.fill(hd, if hov { t.ember } else { t.red });
        ui.zone(R::new(hd.x - 4, hd.y - 4, 14, 14), Act::IslSize(i), Cur::Pointer);
        if hov {
            ui.ghost(R::new(r.right() - 22, r.y + 2, 20, 20), "close", Act::IslDel(i));
            if tile.kind == "load" {
                ui.ghost(R::new(r.right() - 44, r.y + 2, 20, 20), "gear", Act::IslGear(i));
            }
            ui.text_in(PX, R::new(r.x + 6, r.bottom() - 20, r.w - 20, 18), &format!("{}×{}", tile.w, tile.h), t.text_faint);
        }
    }
    // куда ляжет
    if let Some(d) = drag {
        if let (Some((x, yy, w, h)), false) = (d.cell, matches!(d.what, DragWhat::Mod(..))) {
            let skip = match d.what {
                DragWhat::Move(i) | DragWhat::Size(i) => Some(i),
                _ => None,
            };
            let ok = fits(&s.island_tiles, skip, x, yy, w, h);
            let r = cell_rect(gx, gy, cell, gap, &ITile { x, y: yy, w, h, ..Default::default() });
            if !matches!(d.what, DragWhat::Size(_)) {
                ui.cv.fill(r, (if ok { t.red } else { t.text_faint }).alpha(0.14));
            }
            ui.cv.ants(r, if ok { t.ember } else { t.text_faint }, 2, 0);
        }
    }
    // палитра
    let pal = R::new(frame.right() + 16, frame.y, b.right() - frame.right() - 16, frame.h.max(TILES.len() as i32 * 46 + 40));
    ed.palette.set(pal);
    ui.cv.pxbox(pal, &BoxStyle::new(t.code_bg, if matches!(drag.map(|d| &d.what), Some(DragWhat::Move(_))) && drag.is_some_and(|d| d.cell.is_none()) { t.red } else { t.border }));
    ui.text_in(PX, R::new(pal.x + 10, pal.y + 6, pal.w - 20, 20), "плитки", t.text);
    let mut py = pal.y + 32;
    for (k, name, mn, mx) in TILES {
        let used = s.island_tiles.iter().any(|x| x.kind == *k);
        let r = R::new(pal.x + 6, py, pal.w - 12, 40);
        let hov = ui.hover(r) && !used && drag.is_none();
        ui.cv.pxbox(r, &BoxStyle::new(if hov { t.surface_hi } else { t.surface }, if hov { t.red } else { t.border }).bevel(!used));
        let c = if used { t.text_faint } else { t.text };
        // значок — что плитка делает, видно сразу
        ui.cv.icon(tile_icon(k), r.x + 18, r.cy() - 7, 2, if used { t.text_faint } else if hov { t.red } else { t.text_dim });
        ui.text_in(PX, R::new(r.x + 40, r.y + 3, r.w - 46, 18), name, c);
        let sub = if used { crate::i18n::tr("стоит").to_string() } else { format!("{}×{} – {}×{}", mn.0, mn.1, mx.0.min(COLS), mx.1) };
        ui.text_in(PX, R::new(r.x + 40, r.y + 20, r.w - 46, 18), &sub, if used { t.ok } else { t.text_faint });
        let gc = if hov { t.red } else { t.text_faint };
        for a in 0..3 {
            for cc in 0..2 {
                ui.cv.fill(R::new(r.x + 6 + cc * 4, r.cy() - 5 + a * 4, 2, 2), gc);
            }
        }
        if !used {
            ui.zone(r, Act::IslNew(k), Cur::Grab);
        }
        py += 46;
    }
    let mut bottom = frame.bottom().max(pal.bottom());
    // события
    // на что островок ненадолго раздвигается (вместо отдельного OSD)
    bottom = crate::settings::title(ui, b.x, bottom + 16, b.w, "OSD");
    let mut ex = b.x;
    let mut ey = bottom;
    for (k, label) in EVENTS {
        let on = s.island_events.iter().any(|e| e == k);
        let w = ui.w(PX, label) + 20;
        if ex + w > b.right() {
            ex = b.x;
            ey += 30;
        }
        ui.button(R::new(ex, ey, w, 24), label, "", Tone::Plain, on, true, Act::IslEvent(k));
        ex += w + 6;
    }
    bottom = crate::settings::title(ui, b.x, ey + 40, b.w, "плашка");
    let cw = (b.w - 24) / 3;
    crate::settings::opt_btn(ui, R::new(b.x, bottom, b.w, 26), "край экрана", edge_name(&s.island_edge), Act::Drop("island_edge"));
    bottom += 34;
    crate::settings::num_btn(ui, R::new(b.x, bottom, cw, 26), "скругление", s.num("bar_radius"), "bar_radius");
    if !s.island_hang {
        crate::settings::num_btn(ui, R::new(b.x + cw + 12, bottom, cw, 26), "отступ сверху", s.num("bar_margin"), "bar_margin");
    }
    bottom += 34;
    ui.toggle(R::new(b.x, bottom, b.w, 40), "поверх полноэкранного", "подержать курсор у верхнего края секунду", s.bar_peek, Act::SBool("bar_peek"));
    bottom += 50;
    crate::settings::opt_btn(ui, R::new(b.x, bottom, b.w, 26), "автоскрытие", hide_name(&s.bar_hide), Act::Drop("bar_hide"));
    bottom += 40;
    // в руке — поверх всего
    if let Some(d) = drag {
        let (px, py) = (d.x - d.off.0, d.y - d.off.1);
        match &d.what {
            DragWhat::Move(i) => {
                if let Some(tile) = s.island_tiles.get(*i) {
                    let r = cell_rect(px, py, cell, gap, &ITile { x: 0, y: 0, ..tile.clone() });
                    ui.cv.push_clip(r);
                    // подложка: клетки сетки под плиткой в руке не просвечивают
                    ui.cv.fill(r, t.panel);
                    draw_tile(ui, st, tile, r, false);
                    ui.cv.pop_clip();
                    ui.cv.outline(r, t.ember, 2);
                }
            }
            DragWhat::New(k) => {
                let name = tile_info(k).map_or("", |x| x.1);
                let r = R::new(px, py, pal.w - 12, 40);
                ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, t.ember).shadow(4));
                ui.text_in(PX, R::new(r.x + 18, r.y + 11, r.w - 24, 18), name, t.text);
            }
            DragWhat::Mod(_, m) => {
                let w = chip_w(ui, m);
                chip(ui, R::new(px, py, w, CHIP_H), mod_name(m), true);
            }
            DragWhat::Size(_) => {}
        }
    }
    // окошки поверх
    if let (Some(side), Some(at)) = (ed.slot_add, slot_add) {
        slot_list(ui, st, b, side, at);
    } else if ed.add {
        add_list(ui, st, b, add);
    } else if let Some(i) = ed.gear {
        if let Some(tile) = s.island_tiles.get(i) {
            gear(ui, st, b, i, tile);
        }
    }
    bottom
}

fn chip_w(ui: &mut Ui, m: &str) -> i32 {
    ui.w(PX, mod_name(m)) + 44
}

fn slot_icon(m: &str) -> &'static str {
    SLOTS.iter().find(|s| s.0 == m).map_or("dots", |s| s.1)
}

/// строка слотов в редакторе: как полоса раскрытого — значки слева и справа, часы посередине.
/// Занятый слот — щелчок убирает, первый пустой — «+», выбрать значок. Возвращает, где «+»
/// открытой стороны (под ним список)
fn slot_row(ui: &mut Ui, st: &State, r: R) -> Option<R> {
    let t = ui.t;
    let s = &st.set;
    ui.cv.pxbox(r, &BoxStyle::new(t.panel, t.dim));
    ui.text_center(PX, R::new(r.x + r.w / 2 - 60, r.y, 120, r.h), "часы", t.text_dim);
    ui.cv.icon("gear", r.right() - 24, r.cy() - 7, 2, t.text_faint);
    let mut add_at = None;
    for side in 0..2 {
        let list = if side == 0 { &s.island_left } else { &s.island_right };
        let tray = list.iter().any(|m| m == "tray");
        // слева — от края к часам, справа — от шестерёнки к часам
        let at = |i: i32| if side == 0 { r.x + 8 + i * 28 } else { r.right() - 34 - 26 - i * 28 };
        if tray {
            let x0 = if side == 0 { at(0) } else { at(SLOT_MAX as i32 - 1) };
            let tr = R::new(x0, r.y + 5, SLOT_MAX as i32 * 28 - 2, 26);
            let hov = ui.hover(tr);
            ui.cv.pxbox(tr, &BoxStyle::new(t.surface, if hov { t.red } else { t.border }).bevel(true));
            ui.text_center(PX, tr, if hov { "убрать трей" } else { "трей" }, if hov { t.red } else { t.text });
            ui.zone(tr, Act::IslSlotDel(side, 0), Cur::Pointer);
            continue;
        }
        for i in 0..SLOT_MAX as i32 {
            let sr = R::new(at(i), r.y + 5, 26, 26);
            let hov = ui.hover(sr);
            match list.get(i as usize) {
                Some(m) => {
                    ui.cv.pxbox(sr, &BoxStyle::new(t.surface, if hov { t.red } else { t.border }).bevel(true));
                    ui.cv.icon(if hov { "close" } else { slot_icon(m) }, sr.x + 6, sr.y + 6, 2, if hov { t.red } else { t.text });
                    ui.zone(sr, Act::IslSlotDel(side, i as usize), Cur::Pointer);
                }
                None if i as usize == list.len() => {
                    let open = st.isl_ed.slot_add == Some(side);
                    ui.cv.ants(sr, if hov || open { t.red } else { t.border }, 2, 0);
                    ui.cv.icon("plus", sr.x + 6, sr.y + 6, 2, if hov || open { t.red } else { t.text_faint });
                    ui.zone(sr, Act::IslSlotAdd(side), Cur::Pointer);
                    if open {
                        add_at = Some(sr);
                    }
                }
                None => ui.cv.ants(sr, t.border.fade(0.5), 2, 0),
            }
        }
    }
    add_at
}

/// значки для слота: всё из SLOTS, чего нет ни слева, ни справа; трей — только в пустую сторону
fn slot_list(ui: &mut Ui, st: &State, b: R, side: usize, at: R) {
    let t = ui.t;
    let s = &st.set;
    let list = if side == 0 { &s.island_left } else { &s.island_right };
    let free: Vec<&(&str, &str)> = SLOTS.iter().filter(|(m, _)| crate::features::module_on(m) && !s.island_left.iter().chain(&s.island_right).any(|x| x == m) && (*m != "tray" || list.is_empty())).collect();
    ui.zone(R::new(b.x - 40, b.y - 40, b.w + 80, b.h + 400), Act::IslClose, Cur::Default);
    let w = 210;
    let h = (free.len().max(1) as i32) * 26 + 8;
    let x = if side == 0 { at.x } else { at.right() - w };
    let r = R::new(x, at.bottom() + 4, w, h);
    ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), t.red).shadow(4));
    ui.zone(r, Act::Block, Cur::Default);
    if free.is_empty() {
        ui.text_in(PX, R::new(r.x + 10, r.y + 4, w - 20, 24), "все уже стоят", t.text_faint);
    }
    let mut y = r.y + 4;
    for (m, icon) in free {
        let row = R::new(r.x + 4, y, w - 8, 24);
        if ui.hover(row) {
            ui.cv.fill(row, t.surface_hi);
        }
        ui.cv.icon(icon, row.x + 4, row.cy() - 7, 2, t.text_dim);
        let name = if *m == "tray" { "трей — вся сторона" } else { mod_name(m) };
        ui.text_in(PX, R::new(row.x + 26, row.y, row.w - 30, row.h), name, t.text);
        ui.zone(row, Act::IslSlotNew(side, m), Cur::Pointer);
        y += 26;
    }
}

/// фишка модуля: ручка, имя (справа место под крестик)
fn chip(ui: &mut Ui, r: R, name: &str, lifted: bool) {
    let t = ui.t;
    let hov = ui.hover(r) && !lifted;
    let stl = BoxStyle::new(if lifted || hov { t.surface_hi } else { t.surface }, if lifted { t.ember } else if hov { t.red } else { t.border }).bevel(true);
    ui.cv.pxbox(r, &if lifted { stl.shadow(4) } else { stl });
    let gc = if hov || lifted { t.red } else { t.text_faint };
    for a in 0..3 {
        for c in 0..2 {
            ui.cv.fill(R::new(r.x + 5 + c * 4, r.cy() - 5 + a * 4, 2, 2), gc);
        }
    }
    ui.text_in(PX, R::new(r.x + 16, r.y, r.w - 40, r.h), name, t.text);
}

/// «добавить модуль» в свёрнутый: всё из MODS, чего там нет
fn add_list(ui: &mut Ui, st: &State, b: R, at: R) {
    let t = ui.t;
    let free: Vec<&str> = MODS.iter().copied().filter(|m| crate::features::module_on(m) && !st.set.island_mods.iter().any(|x| x == m)).collect();
    ui.zone(R::new(b.x - 40, b.y - 40, b.w + 80, b.h + 400), Act::IslClose, Cur::Default);
    let w = 200;
    let h = (free.len().max(1) as i32) * 26 + 8;
    let r = R::new(at.right() - w, at.bottom() + 4, w, h);
    ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), t.red).shadow(4));
    ui.zone(r, Act::Block, Cur::Default);
    if free.is_empty() {
        ui.text_in(PX, R::new(r.x + 10, r.y + 4, w - 20, 24), "все уже стоят", t.text_faint);
    }
    let mut y = r.y + 4;
    for m in free {
        let row = R::new(r.x + 4, y, w - 8, 24);
        if ui.hover(row) {
            ui.cv.fill(row, t.surface_hi);
        }
        ui.cv.icon("plus", row.x + 4, row.cy() - 7, 2, t.text_faint);
        ui.text_in(PX, R::new(row.x + 24, row.y, row.w - 28, row.h), mod_name(m), t.text);
        ui.zone(row, Act::IslModNew(m), Cur::Pointer);
        y += 26;
    }
}

/// настройки плитки: у «нагрузки» — счётчики и вид
fn gear(ui: &mut Ui, st: &State, b: R, i: usize, tile: &ITile) {
    let t = ui.t;
    ui.zone(R::new(b.x - 40, b.y - 40, b.w + 80, b.h + 400), Act::IslClose, Cur::Default);
    let w = 300.min(b.w);
    let d = R::new(b.x + (b.w - w) / 2, b.y + crate::settings::scroll_of(st) + 60, w, 64 + COUNTERS.len() as i32 * 30 + 40);
    ui.cv.pxbox(d, &BoxStyle::new(t.glass_panel(), t.red).shadow(6));
    ui.zone(d, Act::Block, Cur::Default);
    ui.cv.icon("gear", d.x + 12, d.y + 12, 2, t.red);
    let name = tile_info(&tile.kind).map_or("", |x| x.1);
    ui.text_in(PX, R::new(d.x + 34, d.y + 9, d.w - 70, 20), name, t.text);
    ui.ghost(R::new(d.right() - 30, d.y + 6, 24, 22), "close", Act::IslClose);
    ui.cv.fill(R::new(d.x + 1, d.y + 34, d.w - 2, 1), t.border);
    let mut y = d.y + 44;
    ui.text_in(PX, R::new(d.x + 14, y, d.w - 28, 18), "счётчики", t.text_dim);
    y += 22;
    for (k, label) in COUNTERS {
        let on = tile.opts.iter().any(|o| o == k);
        ui.toggle(R::new(d.x + 14, y, d.w - 28, 26), label, "", on, Act::IslOpt(i, k));
        y += 30;
    }
    let number = tile.opts.iter().any(|o| o == "number");
    let bw = (d.w - 28 - 6) / 2;
    ui.button(R::new(d.x + 14, y + 4, bw, 24), "шкала", "", Tone::Plain, !number, true, Act::IslOpt(i, "gauge"));
    ui.button(R::new(d.x + 20 + bw, y + 4, bw, 24), "число", "", Tone::Plain, number, true, Act::IslOpt(i, "number"));
}

impl App {
    /// щелчки редактора островка и переключателя режима; true — обработано
    pub fn island_click(&mut self, a: &Act) -> bool {
        let s = &mut self.st.set;
        match a {
            Act::IslBig(k) => {
                // из свёрнутого (уведомления) — сперва раскрыть
                if !self.st.popup_is("island") || self.st.island.closing.is_some() {
                    let out = self.focus_out();
                    self.open_popup("island", &out, 0);
                }
                self.st.island.big = Some(Big { kind: k, at: Instant::now(), back: false });
                self.st.island.fan_view = false;
                self.st.island.notif_skip = 0;
                // большой плеер без плитки музыки — захвату звука тоже есть для кого работать
                self.viz_sync();
                match *k {
                    "load" => {
                        self.st.procs.sample();
                        // есть ли вентилятор ThinkPad — сейчас, а не по давнему опросу
                        if crate::features::FAN {
                            self.st.fan.sample();
                        }
                    }
                    "calendar" => {
                        self.st.cal_month = 0;
                        self.st.cal_pick = None;
                    }
                    // история показана — прочитано, как в календаре всплывашки
                    "notifs" => self.st.notifs.seen = Some(Instant::now()),
                    _ => {}
                }
                self.dirty(&crate::app::SKind::Popup);
                return true;
            }
            Act::IslTray => {
                // список трея меню под стрелкой: меню живёт рядом с раскрытым, не вместо него
                let Some((sid, px, _)) = self.ptr else { return true };
                let Some(s) = self.surfs.get(&sid) else { return true };
                let (x, out) = (s.margin.0 + px as i32 - 12, s.output.clone());
                let y = s.margin.1 + H + 2;
                let items: Vec<crate::menu::MItem> = crate::popups::tray_items(&self.st).filter(|it| !crate::popups::tray_in_bar(&self.st, it)).map(|it| crate::menu::MItem::new(&crate::popups::tray_name(&self.st, it), "", crate::menu::MAct::TrayOpen(it.service.clone()))).collect();
                if items.is_empty() {
                    return true;
                }
                self.open_menu(crate::menu::MenuState::new(&out, x, y, "трей", items));
                return true;
            }
            Act::IslOpen(k) => {
                // окно значка посередине под островком; островок уступает ему место
                let out = self.st.popup.as_ref().map(|p| p.out.clone()).unwrap_or_else(|| self.focus_out());
                let (ow, _) = self.output_size(&out);
                self.close_popup_now();
                if *k == "walls" {
                    self.walls_at(&out, ow / 2);
                } else {
                    self.open_popup(k, &out, ow / 2);
                }
                return true;
            }
            Act::IslFanView => {
                self.st.island.fan_view = !self.st.island.fan_view;
                if self.st.island.fan_view {
                    self.st.fan.sample();
                } else {
                    self.st.procs.sample();
                }
                self.dirty(&crate::app::SKind::Popup);
                return true;
            }
            Act::IslBack => {
                if let Some(b) = self.st.island.big.as_mut() {
                    b.back = true;
                    b.at = Instant::now();
                }
                self.dirty(&crate::app::SKind::Popup);
                return true;
            }
            Act::IslSettings => {
                // как шестерёнка в баре: настройки открыты — закрыть (островок остаётся), нет —
                // открыть сразу на вкладке «бар», раскрытый уступает им место
                if self.st.settings_shown {
                    self.close_settings();
                } else {
                    self.close_popup();
                    self.open_settings_tab("bar");
                }
                self.dirty(&crate::app::SKind::Popup);
                return true;
            }
            Act::SIsland(on) => {
                if s.bar_island != *on {
                    s.bar_island = *on;
                    // прежний бар рассыпается, новый проявляется — а не перекраивается на месте
                    self.close_kind(&crate::app::SKind::Bar);
                }
            }
            Act::IslHang(h) => s.island_hang = *h,
            Act::IslDel(i) => {
                if *i < s.island_tiles.len() {
                    s.island_tiles.remove(*i);
                }
                compact(&mut s.island_tiles);
                self.st.isl_ed.gear = None;
            }
            Act::IslGear(i) => {
                self.st.isl_ed.add = false;
                self.st.isl_ed.gear = Some(*i);
                self.dirty_panels();
                return true;
            }
            Act::IslOpt(i, k) => {
                let Some(tile) = s.island_tiles.get_mut(*i) else { return true };
                match *k {
                    "gauge" => tile.opts.retain(|o| o != "number"),
                    "number" => {
                        if !tile.opts.iter().any(|o| o == "number") {
                            tile.opts.push("number".into());
                        }
                    }
                    k => match tile.opts.iter().position(|o| o == k) {
                        Some(p) => {
                            tile.opts.remove(p);
                        }
                        None => {
                            // в порядке списка счётчиков
                            let at = COUNTERS.iter().position(|c| c.0 == k).unwrap_or(0);
                            let p = tile.opts.iter().position(|o| COUNTERS.iter().position(|c| c.0 == o).is_some_and(|j| j > at)).unwrap_or(tile.opts.len());
                            tile.opts.insert(p, k.to_string());
                        }
                    },
                }
            }
            Act::IslSlotAdd(side) => {
                self.st.isl_ed.gear = None;
                self.st.isl_ed.add = false;
                self.st.isl_ed.slot_add = if self.st.isl_ed.slot_add == Some(*side) { None } else { Some(*side) };
                self.dirty_panels();
                return true;
            }
            Act::IslSlotNew(side, m) => {
                let l = if *side == 0 { &mut s.island_left } else { &mut s.island_right };
                if *m == "tray" {
                    *l = vec!["tray".into()];
                } else if l.len() < SLOT_MAX {
                    l.push(m.to_string());
                }
                self.st.isl_ed.slot_add = None;
            }
            Act::IslSlotDel(side, i) => {
                let l = if *side == 0 { &mut s.island_left } else { &mut s.island_right };
                if *i < l.len() {
                    l.remove(*i);
                }
            }
            Act::IslClose => {
                self.st.isl_ed.slot_add = None;
                self.st.isl_ed.gear = None;
                self.st.isl_ed.add = false;
                self.dirty_panels();
                return true;
            }
            Act::IslModAdd => {
                self.st.isl_ed.gear = None;
                self.st.isl_ed.add = !self.st.isl_ed.add;
                self.dirty_panels();
                return true;
            }
            Act::IslModNew(m) => {
                s.island_mods.push(m.to_string());
                self.st.isl_ed.add = false;
            }
            Act::IslModDel(i) => {
                if *i < s.island_mods.len() {
                    s.island_mods.remove(*i);
                }
            }
            Act::IslEvent(k) => match s.island_events.iter().position(|e| e == k) {
                Some(p) => {
                    s.island_events.remove(p);
                }
                None => s.island_events.push(k.to_string()),
            },
            _ => return false,
        }
        if !self.st.set.bar_island {
            self.close_island();
        }
        self.settings_changed();
        true
    }

    /// раскрытый островок закрыть (режим выключили)
    fn close_island(&mut self) {
        if self.st.popup_is("island") {
            self.close_popup();
        }
        self.st.island.live = None;
    }

    /// тащат в редакторе: плитку, её угол, новую из палитры или фишку свёрнутого
    pub fn island_drag(&mut self, a: &Act, zone: R, press: (f64, f64), x: f64, y: f64) {
        let (x, y) = (x as i32, y as i32);
        let s = &self.st.set;
        if self.st.isl_ed.drag.is_none() {
            let what = match a {
                Act::IslTile(i) => DragWhat::Move(*i),
                Act::IslSize(i) => DragWhat::Size(*i),
                Act::IslNew(k) => DragWhat::New(k),
                Act::IslMod(i) => DragWhat::Mod(Some(*i), s.island_mods.get(*i).cloned().unwrap_or_default()),
                _ => return,
            };
            // плитку держим за то место, где схватили; новую — за левый верх клетки
            let off = match what {
                DragWhat::New(_) => (12, 12),
                _ => (press.0 as i32 - zone.x, press.1 as i32 - zone.y),
            };
            self.st.isl_ed.gear = None;
            self.st.isl_ed.add = false;
            self.st.isl_ed.drag = Some(IDrag { what, x, y, off, cell: None, slot: None });
        }
        let (gx, gy, cell, gap) = self.st.isl_ed.grid.get();
        let step = cell + gap;
        let n = (rows(&s.island_tiles) + 1).min(MAX_ROWS);
        let grid = R::new(gx - 24, gy - 24, COLS * step + 48, n * step + 48);
        let Some(d) = self.st.isl_ed.drag.as_mut() else { return };
        (d.x, d.y) = (x, y);
        match &d.what {
            DragWhat::Move(_) | DragWhat::New(_) => {
                let (w, h) = match &d.what {
                    DragWhat::Move(i) => s.island_tiles.get(*i).map_or((1, 1), |t| (t.w, t.h)),
                    DragWhat::New(k) => tile_info(k).map_or((1, 1), |t| t.2),
                    _ => (1, 1),
                };
                let (px, py) = (x - d.off.0, y - d.off.1);
                d.cell = grid.contains(x as f64, y as f64).then(|| {
                    let cx = ((px - gx) as f32 / step as f32).round() as i32;
                    let cy = ((py - gy) as f32 / step as f32).round() as i32;
                    (cx.clamp(0, COLS - w), cy.clamp(0, (MAX_ROWS - h).max(0)), w, h)
                });
            }
            DragWhat::Size(i) => {
                if let Some(t) = s.island_tiles.get(*i) {
                    let info = tile_info(&t.kind).map_or(((1, 1), (COLS, 4)), |x| (x.2, x.3));
                    let left = gx + t.x * step;
                    let top = gy + t.y * step;
                    let w = ((x - left + gap) as f32 / step as f32).ceil() as i32;
                    let h = ((y - top + gap) as f32 / step as f32).ceil() as i32;
                    let w = w.clamp(info.0 .0, info.1 .0.min(COLS - t.x));
                    let h = h.clamp(info.0 .1, info.1 .1.min(MAX_ROWS - t.y));
                    d.cell = Some((t.x, t.y, w, h));
                }
            }
            DragWhat::Mod(..) => {
                let row = self.st.isl_ed.chip_row.get();
                let near = R::new(row.x - 20, row.y - 30, row.w + 40, row.h + 60);
                // по строкам: всё выше строки курсора и левее его в ней
                d.slot = near.contains(x as f64, y as f64).then(|| self.st.isl_ed.chips.borrow().iter().filter(|r| r.bottom() < y || (r.y <= y && r.x + r.w / 2 < x)).count());
            }
        }
        self.dirty_panels();
    }

    /// отпустили: положить, если место свободно; плитку или фишку мимо — убрать
    pub fn island_drop(&mut self) {
        let Some(d) = self.st.isl_ed.drag.take() else { return };
        let s = &mut self.st.set;
        match d.what {
            DragWhat::Move(i) | DragWhat::Size(i) => match d.cell {
                Some((x, y, w, h)) => {
                    if fits(&s.island_tiles, Some(i), x, y, w, h) {
                        if let Some(t) = s.island_tiles.get_mut(i) {
                            (t.x, t.y, t.w, t.h) = (x, y, w, h);
                        }
                    }
                }
                None if matches!(d.what, DragWhat::Move(_)) => {
                    if i < s.island_tiles.len() {
                        s.island_tiles.remove(i);
                    }
                }
                None => {}
            },
            DragWhat::New(k) => {
                if let Some((x, y, w, h)) = d.cell {
                    if fits(&s.island_tiles, None, x, y, w, h) {
                        let opts = if k == "load" { vec!["cpu".into(), "fan".into(), "ram".into()] } else { vec![] };
                        s.island_tiles.push(ITile { kind: k.into(), x, y, w, h, opts });
                    }
                }
            }
            DragWhat::Mod(from, name) => {
                if let Some(f) = from {
                    if f < s.island_mods.len() {
                        s.island_mods.remove(f);
                    }
                }
                if let Some(slot) = d.slot {
                    let at = slot.min(s.island_mods.len());
                    s.island_mods.insert(at, name);
                }
            }
        }
        compact(&mut s.island_tiles);
        self.settings_changed();
        self.dirty_panels();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles() {
        let mut s = Settings::default();
        // по умолчанию плитки не налезают друг на друга и влезают в сетку
        let d = s.island_tiles.clone();
        validate(&mut s);
        assert_eq!(s.island_tiles, d);
        let t = |kind: &str, x, y, w, h| ITile { kind: kind.into(), x, y, w, h, opts: vec![] };
        // чужая плитка, наложение, размер сверх предела, ниже последней строки
        s.island_tiles = vec![t("music", 0, 0, 2, 2), t("nope", 2, 0, 1, 1), t("clock", 1, 1, 1, 1), t("sound", 3, 2, 9, 9), t("power", 0, 5, 2, 1)];
        validate(&mut s);
        let kinds: Vec<&str> = s.island_tiles.iter().map(|t| t.kind.as_str()).collect();
        assert_eq!(kinds, ["music", "sound"]);
        // «звук» обрезан до 2×2 и сдвинут в сетку; остаётся в своих строках — пустых между нет
        let snd = &s.island_tiles[1];
        assert_eq!((snd.x, snd.y, snd.w, snd.h), (2, 2, 2, 2));
        assert!(!fits(&s.island_tiles, None, 1, 1, 1, 1));
        assert!(fits(&s.island_tiles, None, 0, 2, 2, 2));
        assert!(!fits(&s.island_tiles, None, 0, 3, 4, 2));
        assert_eq!(rows(&s.island_tiles), 4);
        // целиком пустая строка убирается
        let mut gap = vec![t("music", 0, 0, 2, 1), t("clock", 0, 2, 1, 1)];
        compact(&mut gap);
        assert_eq!(gap[1].y, 1);
    }
}
