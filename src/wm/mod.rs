// Компоновщик: столы, окна, раскладки и показ экрана — одно состояние для бара, дока и островка.
// С компоновщиком говорит бэкенд, выбранный при сборке: niri (по умолчанию), sway (`--features
// sway`) или Hyprland (`--features hyprland`). Остальной шелл видит только Wm и действия Do.
//
// Сворачивание делает шелл: свёрнутое окно уезжает со стола (у niri — на последний пустой, у
// sway — в scratchpad, у Hyprland — на спрятанный special-стол) и прячется из бара.

#[cfg(all(feature = "sway", feature = "hyprland"))]
compile_error!("features `sway` and `hyprland` are mutually exclusive: build for one compositor");

#[cfg(feature = "hyprland")]
#[path = "hypr.rs"]
mod backend;
#[cfg(all(feature = "sway", not(feature = "hyprland")))]
#[path = "sway.rs"]
mod backend;
#[cfg(not(any(feature = "sway", feature = "hyprland")))]
#[path = "niri.rs"]
mod backend;

pub use backend::{game_focused, layout_only, spawn_stream, Raw, APPID_HINT, NAME};
#[cfg(any(feature = "sway", feature = "hyprland"))]
pub use backend::set_cursor;

use std::sync::mpsc;

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub size: (f64, f64),
    /// колонка и место в ней (лента niri; у sway и Hyprland — порядок на экране)
    pub col: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Win {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub ws: Option<u64>,
    pub floating: bool,
    pub focused: bool,
    pub urgent: bool,
    pub ts: f64,
    pub layout: Layout,
}

#[derive(Clone, Debug, Default)]
pub struct Ws {
    pub id: u64,
    pub idx: u32,
    pub name: Option<String>,
    pub output: String,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
}

/// что показывает поток (потоки знает только niri)
#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CastTarget {
    /// цель не выбрана или сброшена
    #[default]
    Nothing,
    Output(String),
    Window(u64),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct Cast {
    pub id: u64,
    /// программа, если niri её знает (только wlr-screencopy)
    pub pid: Option<u32>,
    pub target: CastTarget,
    /// поток «динамической цели»: его переключают set-dynamic-cast-window / -monitor
    pub dynamic: bool,
}

pub enum Ev {
    /// окно открылось впервые
    NewWindow(u64),
    KbSwitched,
    Changed,
}

/// раскладка клавиатуры
#[derive(Clone, Copy, Debug)]
pub enum Kb {
    Next,
    Prev,
    Index(usize),
}

/// действие над окнами и столами: бэкенд переводит его на язык своего компоновщика (поля
/// нужны не каждому: last — только niri, floating — niri и sway)
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Do {
    Focus(u64),
    Close(u64),
    FocusOutput(String),
    FocusWs(Ws),
    /// соседний стол монитора: вверх — предыдущий
    WsStep(bool),
    /// окно на стол, фокус остаётся
    MoveToWs(u64, Ws),
    /// last — последний номер стола на мониторе окна (у niri он всегда пустой)
    Minimize { id: u64, last: u32 },
    /// ws — активный стол монитора в фокусе; floating — окно плавало до сворачивания
    Restore { id: u64, ws: Option<Ws>, floating: bool },
    Maximize { id: u64, floating: bool },
    Fullscreen(u64),
    ToggleFloat(u64),
    Center(u64),
    Kb(Kb),
    Quit,
    MonitorsOff,
    /// после простоя: niri будит мониторы сам, sway и Hyprland — нет
    MonitorsOn,
}

#[derive(Default)]
pub struct Wm {
    pub ws: Vec<Ws>,
    pub wins: Vec<Win>,
    pub kb_names: Vec<String>,
    pub kb_idx: usize,
    pub overview: bool,
    /// идёт показ экрана (запись, трансляция, демонстрация): потоки niri по номерам и кто
    /// показывает (pid программы, если niri его знает)
    pub casting: bool,
    pub casts: Vec<Cast>,
    /// спрятанные «свёрнутые» окна
    pub hidden: Vec<u64>,
    pub min_order: Vec<u64>,
    /// свёрнутые, что плавали до сворачивания (sway делает плавающим всё в scratchpad)
    min_float: Vec<u64>,
    /// первый срез уже пришёл: окна из него — не «новые»
    #[allow(dead_code)]
    synced: bool,
    tx: Option<mpsc::Sender<Do>>,
}

/// всё состояние сразу — так шлют sway и Hyprland: в их событиях полного состояния нет
#[cfg(any(feature = "sway", feature = "hyprland"))]
pub struct Snap {
    pub ws: Vec<Ws>,
    pub wins: Vec<Win>,
    pub kb_names: Vec<String>,
    pub kb_idx: usize,
}

impl Wm {
    pub fn new() -> Wm {
        let (tx, rx) = mpsc::channel::<Do>();
        std::thread::Builder::new()
            .name("wm-act".into())
            .spawn(move || {
                while let Ok(a) = rx.recv() {
                    backend::exec(&a);
                }
            })
            .ok();
        Wm { tx: Some(tx), ..Default::default() }
    }

    pub fn act(&self, a: Do) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(a);
        }
    }

    // ── запросы ─────────────────────────────────────────────────────────────
    pub fn focused_output(&self) -> String {
        self.ws.iter().find(|w| w.focused).or(self.ws.first()).map(|w| w.output.clone()).unwrap_or_default()
    }
    pub fn active_ws(&self, output: &str) -> Option<&Ws> {
        self.ws.iter().find(|w| w.output == output && w.active)
    }
    pub fn focused_win(&self) -> Option<&Win> {
        self.wins.iter().find(|w| w.focused)
    }
    pub fn win(&self, id: u64) -> Option<&Win> {
        self.wins.iter().find(|w| w.id == id)
    }
    pub fn is_min(&self, w: &Win) -> bool {
        self.hidden.contains(&w.id)
    }
    /// английская раскладка из списка — для ввода пароля на экране блокировки
    pub fn kb_english(&self) -> Option<usize> {
        self.kb_names.iter().position(|n| {
            let n = n.to_lowercase();
            n.starts_with("eng") || n.contains("(us)")
        })
    }
    pub fn kb_short(&self) -> String {
        let n = self.kb_names.get(self.kb_idx).map(|s| s.to_lowercase()).unwrap_or_default();
        if n.starts_with("rus") {
            "RU".into()
        } else if n.starts_with("eng") {
            "EN".into()
        } else {
            n.chars().take(2).collect::<String>().to_uppercase()
        }
    }
    /// столы монитора для бара: занятые и активный; спрятанные (только свёрнутые) — нет
    pub fn bar_ws(&self, output: &str) -> Vec<Ws> {
        let mut v: Vec<Ws> = self
            .ws
            .iter()
            .filter(|ws| {
                if ws.output != output {
                    return false;
                }
                let live = self.wins.iter().any(|w| w.ws == Some(ws.id) && !self.is_min(w));
                let hidden = self.wins.iter().any(|w| w.ws == Some(ws.id) && self.hidden.contains(&w.id));
                ws.active || live || (!hidden && ws.name.is_some())
            })
            .cloned()
            .collect();
        v.sort_by_key(|w| w.idx);
        v
    }
    pub fn has_windows(&self, ws: u64) -> bool {
        self.wins.iter().any(|w| w.ws == Some(ws) && !self.is_min(w))
    }
    /// окна стола в порядке колонок, плавающие — после
    pub fn windows_on(&self, ws: u64) -> Vec<Win> {
        let mut v: Vec<Win> = self.wins.iter().filter(|w| w.ws == Some(ws) && !self.is_min(w)).cloned().collect();
        v.sort_by_key(|w| match w.layout.col {
            Some((c, t)) => c as u64 * 1000 + t as u64,
            None => 1_000_000 + w.id,
        });
        v
    }
    /// цель динамического потока (set-dynamic-cast-*); None — такого потока нет
    pub fn dynamic_cast(&self) -> Option<&CastTarget> {
        self.casts.iter().find(|c| c.dynamic).map(|c| &c.target)
    }
    pub fn minimized(&self) -> Vec<Win> {
        let mut v: Vec<Win> = self.wins.iter().filter(|w| self.is_min(w)).cloned().collect();
        v.sort_by_key(|w| self.min_order.iter().position(|x| *x == w.id).unwrap_or(0));
        v
    }

    // ── действия ────────────────────────────────────────────────────────────
    pub fn focus(&self, id: u64) {
        self.act(Do::Focus(id));
    }
    pub fn close_window(&self, id: u64) {
        self.act(Do::Close(id));
    }
    pub fn focus_ws(&self, ws: &Ws) {
        if ws.output != self.focused_output() {
            self.act(Do::FocusOutput(ws.output.clone()));
        }
        self.act(Do::FocusWs(ws.clone()));
    }
    pub fn kb(&self, k: Kb) {
        self.act(Do::Kb(k));
    }

    pub fn minimize(&mut self, id: u64) {
        let Some(w) = self.win(id).cloned() else { return };
        if self.is_min(&w) {
            return;
        }
        self.min_order.retain(|x| *x != id);
        self.min_order.push(id);
        self.min_float.retain(|x| *x != id);
        if w.floating {
            self.min_float.push(id);
        }
        let out = w.ws.and_then(|i| self.ws.iter().find(|x| x.id == i)).map(|x| x.output.clone()).unwrap_or_else(|| self.focused_output());
        let last = self.ws.iter().filter(|x| x.output == out).map(|x| x.idx).max().unwrap_or(1);
        self.hidden.push(id);
        self.act(Do::Minimize { id, last });
    }
    pub fn restore(&mut self, id: u64) {
        let ws = self.active_ws(&self.focused_output()).cloned();
        let floating = self.min_float.contains(&id);
        self.min_order.retain(|x| *x != id);
        self.hidden.retain(|x| *x != id);
        self.min_float.retain(|x| *x != id);
        self.act(Do::Restore { id, ws, floating });
    }
    pub fn toggle_minimize(&mut self, id: u64) {
        let Some(w) = self.win(id).cloned() else { return };
        if self.is_min(&w) {
            self.restore(id);
        } else if w.focused {
            self.minimize(id);
        } else {
            self.focus(id);
        }
    }

    /// окно закрылось: из списков свёрнутых тоже
    fn forget(&mut self, id: u64) {
        self.hidden.retain(|x| *x != id);
        self.min_order.retain(|x| *x != id);
        self.min_float.retain(|x| *x != id);
    }

    /// новый срез целиком (sway, Hyprland): новые окна и смена раскладки — событиями, время
    /// фокуса — своё (компоновщик его не даёт), свёрнутые закрытые — забыть
    #[cfg(any(feature = "sway", feature = "hyprland"))]
    fn apply(&mut self, s: Snap) -> Vec<Ev> {
        let mut out = vec![Ev::Changed];
        let first = !self.synced;
        self.synced = true;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let mut wins = s.wins;
        for w in wins.iter_mut() {
            match self.wins.iter().find(|o| o.id == w.id) {
                Some(o) => {
                    w.ts = if w.focused && !o.focused { now } else { o.ts };
                }
                None => {
                    if w.focused {
                        w.ts = now;
                    }
                    if !first {
                        out.push(Ev::NewWindow(w.id));
                    }
                }
            }
        }
        let gone: Vec<u64> = self.wins.iter().filter(|o| !wins.iter().any(|w| w.id == o.id)).map(|o| o.id).collect();
        for id in gone {
            self.forget(id);
        }
        if !first && s.kb_idx != self.kb_idx && !s.kb_names.is_empty() {
            out.push(Ev::KbSwitched);
        }
        self.ws = s.ws;
        self.wins = wins;
        self.kb_names = s.kb_names;
        self.kb_idx = s.kb_idx;
        out
    }
}
