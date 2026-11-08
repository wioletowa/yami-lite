// Вентилятор ThinkPad (модуль `fan` и его всплывашка): ещё один клиент thfn-gtk. Кривую ведёт
// только его демон (`thfn-gtk --daemon`), шелл говорит с ним тем же способом, что окно и CLI:
// поле `enabled` в ~/.config/thfn-gtk/curve.conf (демон перечитывает файл по mtime), состояние
// демона — $XDG_RUNTIME_DIR/thfn-gtk.state. Уровень — прямо в /proc/acpi/ibm/fan, датчики — hwmon
// `thinkpad`. Ручной уровень сначала выключает кривую: демон и рука не дерутся за вентилятор.

use crate::app::State;
use crate::font::PX;
use crate::gfx::{BoxStyle, C, R};
use crate::sys::Series;
use crate::ui::{Act, Cur, Ui};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const PROC_FAN: &str = "/proc/acpi/ibm/fan";

/// клетки шкалы снизу вверх: кривая, авто прошивки, уровни 0…7, полные обороты
pub const CELLS: usize = 11;
const CURVE: usize = 0;
const AUTO: usize = 1;
const FULL: usize = 10;
const NAMES: [&str; CELLS] = ["C", "A", "0", "1", "2", "3", "4", "5", "6", "7", "F"];


/// шкала температуры на графике, °C: 20…100 — по 10° на строку уровня, сетка общая
const T_LO: f32 = 20.0;
const T_HI: f32 = 100.0;
/// обороты «100 %», пока больших не видели (полные у P53 — около 5300)
const RPM_FULL: u32 = 5000;

/// всплывашка: шкала слева на всю высоту, график справа, CPU / GPU / FAN под ним
pub const SIZE: (i32, i32) = (440, 250);
const SLIDER_W: i32 = 58;
const STATS_H: i32 = 42;
/// подписи температур справа от графика
const AXIS_W: i32 = 42;

#[derive(Default)]
pub struct Fan {
    /// есть /proc/acpi/ibm/fan: иначе модуля нет
    pub present: bool,
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub rpm: Option<u32>,
    pub rpm2: Option<u32>,
    /// самые большие обороты, что видели: это 100 % модуля в баре
    rpm_max: u32,
    /// уровень из /proc: «auto», «disengaged», «0»…«7»
    pub level: String,
    /// кривая включена в curve.conf
    pub curve: bool,
    /// демон жив (свежий state-файл)
    pub daemon: bool,
    pub critical: bool,
    /// температура max(CPU, GPU) и уровень 0…8 (8 — полные; -1 — у прошивки, не знаем)
    pub h_temp: Series,
    pub h_level: Series,
    /// загрузка вентиляторов, 0..1 (модуль в баре со стилем «график»)
    pub h_pct: Series,
}

// ── чтение ──────────────────────────────────────────────────────────────────

/// /sys/class/hwmon/hwmonN с name = thinkpad
fn hwmon() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| fs::read_dir("/sys/class/hwmon").ok()?.flatten().map(|e| e.path()).find(|p| fs::read_to_string(p.join("name")).is_ok_and(|n| n.trim() == "thinkpad"))).as_deref()
}

fn read_num(p: &Path) -> Option<f64> {
    fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// tempN_input с подписью label («CPU», «GPU»); 0 — датчик не подключён
fn temp_by_label(dir: &Path, label: &str) -> Option<f32> {
    (1..=16).find(|i| fs::read_to_string(dir.join(format!("temp{i}_label"))).is_ok_and(|l| l.trim() == label)).and_then(|i| read_num(&dir.join(format!("temp{i}_input")))).map(|v| (v / 1000.0) as f32).filter(|v| *v > 0.0)
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"));
    base.join("thfn-gtk/curve.conf")
}

fn state_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp")).join("thfn-gtk.state")
}

/// значение `key=…` из текста k=v
fn kv<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| l.trim().split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim()))
}

/// curve.conf с полем enabled = on; остальные строки — как были (их пишет окно thfn-gtk)
fn with_enabled(text: &str, on: bool) -> String {
    let line = format!("enabled={on}");
    let mut found = false;
    let mut out: Vec<String> = text
        .lines()
        .map(|l| {
            if l.trim().split_once('=').is_some_and(|(k, _)| k.trim() == "enabled") {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        out.push(line);
    }
    out.join("\n") + "\n"
}

/// включить или выключить кривую; файл трогаем, только если поле меняется (демон смотрит mtime)
fn set_curve(on: bool) -> Result<(), String> {
    let path = config_path();
    let text = fs::read_to_string(&path).unwrap_or_default();
    // файла нет — у thfn-gtk кривая и так выключена
    if kv(&text, "enabled").map_or(!on, |v| (v == "true") == on) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, with_enabled(&text, on)).map_err(|e| e.to_string())
}

/// `level` — «auto», «disengaged» или «0»…«7»
fn set_level(level: &str) -> Result<(), String> {
    fs::write(PROC_FAN, format!("level {level}\n")).map_err(|e| format!("{PROC_FAN}: {e}"))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Fan {
    /// раз в секунду, пока модуль в баре или открыта всплывашка
    pub fn sample(&mut self) {
        let Ok(text) = fs::read_to_string(PROC_FAN) else {
            self.present = false;
            return;
        };
        self.present = true;
        self.level = kv(&text.replace(':', "="), "level").unwrap_or("").to_string();
        if let Some(dir) = hwmon() {
            self.cpu = temp_by_label(dir, "CPU");
            self.gpu = temp_by_label(dir, "GPU");
            self.rpm = read_num(&dir.join("fan1_input")).map(|v| v as u32);
            self.rpm2 = read_num(&dir.join("fan2_input")).map(|v| v as u32);
            self.rpm_max = self.rpm_max.max(RPM_FULL).max(self.rpm.unwrap_or(0)).max(self.rpm2.unwrap_or(0));
        }
        let conf = fs::read_to_string(config_path()).unwrap_or_default();
        self.curve = kv(&conf, "enabled") == Some("true");
        let poll: u64 = kv(&conf, "poll").and_then(|v| v.parse().ok()).unwrap_or(2).max(1);
        // демон пишет состояние каждый опрос, даже когда кривая выключена; старое — его нет
        let state = fs::read_to_string(state_path()).unwrap_or_default();
        let ts: u64 = kv(&state, "ts").and_then(|v| v.parse().ok()).unwrap_or(0);
        self.daemon = ts > 0 && now_secs().saturating_sub(ts) <= poll * 3 + 2;
        self.critical = self.daemon && kv(&state, "critical") == Some("true");
        if let Some(t) = self.temp() {
            self.h_temp.push(t);
        }
        self.h_level.push(self.level_num().map_or(-1.0, |l| l as f32));
        let pct = self.pct();
        self.h_pct.push(pct);
    }

    /// как сильно работают вентиляторы: быстрый из двух от самых больших оборотов, 0..1
    pub fn pct(&self) -> f32 {
        self.rpm.max(self.rpm2).map_or(0.0, |r| r as f32 / self.rpm_max.max(1) as f32).min(1.0)
    }

    /// уровень 0…8 (8 — полные обороты); None — ведёт прошивка
    pub fn level_num(&self) -> Option<u8> {
        match self.level.as_str() {
            "disengaged" | "full-speed" | "full" => Some(8),
            l => l.parse().ok().filter(|n| *n <= 7),
        }
    }

    /// вход кривой: горячий из двух
    pub fn temp(&self) -> Option<f32> {
        match (self.cpu, self.gpu) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// клетка шкалы, которая выбрана сейчас
    pub fn cell(&self) -> usize {
        if self.curve {
            return CURVE;
        }
        self.level_num().map_or(AUTO, |l| l as usize + 2)
    }

    /// выбрать клетку: C — кривая демона (если он жив), A — прошивка, 0…7 и F — вручную
    pub fn apply(&mut self, cell: usize) {
        let cell = cell.min(FULL);
        if cell == self.cell() || (cell == CURVE && !self.daemon) {
            return;
        }
        let r = match cell {
            CURVE => set_curve(true),
            _ => set_curve(false).and_then(|_| {
                set_level(&match cell {
                    AUTO => "auto".to_string(),
                    FULL => "disengaged".to_string(),
                    c => (c - 2).to_string(),
                })
            }),
        };
        if let Err(e) = r {
            eprintln!("fan: {e}");
        }
        // показать сразу, не дожидаясь тика (историю не трогаем)
        let hist = (std::mem::take(&mut self.h_temp), std::mem::take(&mut self.h_level), std::mem::take(&mut self.h_pct));
        self.sample();
        (self.h_temp, self.h_level, self.h_pct) = hist;
        if cell == CURVE {
            self.curve = true;
        }
    }

    /// шаг по шкале колесом (C без демона пропускаем)
    pub fn step(&mut self, up: bool) {
        let c = self.cell();
        let next = if up { (c + 1).min(FULL) } else { c.saturating_sub(1) };
        let next = if next == CURVE && !self.daemon { AUTO } else { next };
        self.apply(next);
    }
}

// ── шкала ───────────────────────────────────────────────────────────────────

const GAP: i32 = 2;

/// строки шкалы в дорожке высотой h: (верх, высота) от верха дорожки, по клеткам снизу вверх
fn rows(h: i32) -> [(i32, i32); CELLS] {
    let ch = (h - (CELLS as i32 - 1) * GAP) / CELLS as i32;
    // остаток от деления — сверху, низ шкалы ровно по низу дорожки
    std::array::from_fn(|i| (h - (i as i32 + 1) * ch - i as i32 * GAP, ch))
}

/// клетка под указателем: y от верха дорожки высотой h (между строками — ближайшая)
pub fn cell_at(y: f64, h: i32) -> usize {
    let y = y as i32;
    rows(h).iter().enumerate().min_by_key(|(_, (top, ch))| if y < *top { top - y } else if y >= top + ch { y - top - ch + 1 } else { 0 }).map_or(AUTO, |(i, _)| i)
}

/// пиксельный ползунок снизу вверх, клетками, как громкость и микрофон: горит всё до выбранной.
/// На кривой горит C, а уровень, который сейчас держит демон, — бледнее
fn slider(ui: &mut Ui, f: &Fan, r: R) {
    let t = ui.t;
    let track = R::new(r.x + 8, r.y + 8, r.w - 16, r.h - 16);
    let hov = ui.hover(track);
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, if hov { t.red } else { t.border }).bevel(true));
    let sel = f.cell();
    let lit = if hov { t.ember } else { t.red };
    let empty = if hov { t.surface_hi } else { t.surface.lerp(t.border, 0.3) };
    let real = f.level_num().map_or(0, |l| l as usize + 2);
    for (i, (top, ch)) in rows(track.h).iter().enumerate() {
        let row = R::new(track.x, track.y + top, track.w, *ch);
        let off = i == CURVE && !f.daemon;
        let on = if sel == CURVE { i == CURVE } else { i <= sel };
        let c = if f.critical && on { t.ember } else if on { lit } else if sel == CURVE && (AUTO + 1..=real).contains(&i) { lit.fade(0.4) } else if off { empty.fade(0.5) } else { empty };
        let tc = if off { t.text_faint.fade(0.5) } else if i == sel { t.text } else { t.text_faint };
        ui.text_in(PX, R::new(row.x, row.y, 12, row.h), NAMES[i], tc);
        ui.cv.fill(R::new(row.x + 16, row.y, row.w - 16, row.h), c);
    }
    ui.zone(track, Act::FanLevel, Cur::Pointer);
}

// ── график ──────────────────────────────────────────────────────────────────

/// линия по точкам 0..1 (старые слева) клетками по 2 px; отрицательные — пропуск
fn line(ui: &mut Ui, r: R, pts: &[f32], c: C) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let step = r.w as f32 / (n - 1) as f32;
    let y_of = |v: f32| r.bottom() - 1 - ((v.clamp(0.0, 1.0) * (r.h - 2) as f32).round() as i32);
    let mut prev: Option<i32> = None;
    for (i, v) in pts.iter().enumerate() {
        if *v < 0.0 {
            prev = None;
            continue;
        }
        let x = r.x + (i as f32 * step).round() as i32;
        let w = ((i + 1) as f32 * step).round() as i32 - (i as f32 * step).round() as i32;
        let y = y_of(*v);
        ui.cv.fill(R::new(x, y - 1, w.max(2), 2), c);
        if let Some(py) = prev {
            let (a, b) = (py.min(y), py.max(y));
            ui.cv.fill(R::new(x, a - 1, 2, b - a + 2), c);
        }
        prev = Some(y);
    }
}

fn graph(ui: &mut Ui, f: &Fan, r: R) {
    let t = ui.t;
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, t.border));
    let g = R::new(r.x + 6, r.y + 6, r.w - 12, r.h - 12);
    // сетка: строка на уровень редкими точками; она же — температура через 10°, подписи справа
    for l in 0..=8 {
        let y = g.bottom() - 1 - l * (g.h - 2) / 8;
        for x in (g.x..g.right()).step_by(6) {
            ui.cv.fill(R::new(x, y, 1, 1), t.border);
        }
        if l % 2 == 0 {
            ui.cv.fill(R::new(r.right(), y, 3, 1), t.border);
            ui.text_right(PX, R::new(r.right() + 4, y - 9, AXIS_W - 4, 18), &format!("{}°", T_LO as i32 + l * 10), t.text_faint);
        }
    }
    let n = crate::sys::HIST;
    let temp: Vec<f32> = f.h_temp.tail(n).iter().map(|v| if *v == 0.0 { -1.0 } else { (v - T_LO) / (T_HI - T_LO) }).collect();
    // недостающие в начале точки tail() добивает нулями: у уровня это «0», а не «нет данных»
    let empty = n.saturating_sub(f.h_level.v.len());
    let lvl: Vec<f32> = f.h_level.tail(n).iter().enumerate().map(|(i, v)| if i < empty || *v < 0.0 { -1.0 } else { v / 8.0 }).collect();
    ui.cv.push_clip(g);
    line(ui, g, &temp, t.red);
    line(ui, g, &lvl, t.accent2);
    ui.cv.pop_clip();
}

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let f = &st.fan;
    slider(ui, f, R::new(b.x, b.y, SLIDER_W, b.h));
    let gx = b.x + SLIDER_W + 10;
    let gr = R::new(gx, b.y, b.right() - AXIS_W - gx, b.h - STATS_H);
    graph(ui, f, gr);
    let deg = |v: Option<f32>| v.map_or("—".to_string(), |v| format!("{}°", v.round()));
    let rpm = |v: Option<u32>| v.map_or("—".to_string(), |r| r.to_string());
    let cols: [(&str, String, bool); 4] = [
        ("CPU", deg(f.cpu), f.cpu.is_some_and(|v| v > 80.0)),
        ("GPU", deg(f.gpu), f.gpu.is_some_and(|v| v > 80.0)),
        ("FAN1", rpm(f.rpm), f.critical),
        ("FAN2", rpm(f.rpm2), f.critical),
    ];
    let cw = gr.w / cols.len() as i32;
    let y = gr.bottom() + 4;
    for (i, (label, val, hot)) in cols.iter().enumerate() {
        let c = R::new(gr.x + i as i32 * cw, y, cw, 18);
        ui.text_center(PX, c, label, t.text_faint);
        ui.text_center(PX, R::new(c.x, y + 18, cw, 20), val, if *hot { t.ember } else { t.text });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_is_rewritten_in_place() {
        let conf = "# thfn-gtk\nenabled=false\ncurve=30:1,45:2\npoll=2\n";
        assert_eq!(with_enabled(conf, true), "# thfn-gtk\nenabled=true\ncurve=30:1,45:2\npoll=2\n");
        assert_eq!(with_enabled("", true), "enabled=true\n");
        assert_eq!(kv(conf, "curve"), Some("30:1,45:2"));
    }

    #[test]
    fn rows_cover_the_track_bottom_up() {
        let h = 200;
        let r = rows(h);
        assert_eq!(r[CURVE].0 + r[CURVE].1, h);
        assert!(r[FULL].0 >= 0);
        for i in 1..CELLS {
            assert!(r[i].0 < r[i - 1].0, "{i}");
        }
        assert_eq!(cell_at((h - 1) as f64, h), CURVE);
        assert_eq!(cell_at(0.0, h), FULL);
        for (i, (top, ch)) in r.iter().enumerate() {
            assert_eq!(cell_at((top + ch / 2) as f64, h), i);
        }
    }

    #[test]
    fn cells_follow_the_proc_level() {
        let mut f = Fan { level: "auto".into(), ..Default::default() };
        assert_eq!(f.cell(), AUTO);
        f.level = "disengaged".into();
        assert_eq!(f.cell(), FULL);
        f.level = "3".into();
        assert_eq!(f.cell(), 5);
        f.curve = true;
        assert_eq!(f.cell(), CURVE);
    }
}
