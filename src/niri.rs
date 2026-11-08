// niri по IPC-сокету: поток событий в отдельной нити, действия — короткими запросами.
//
// Сворачивание и «стол» делает шелл: свёрнутое окно уезжает на последний (пустой) стол и
// прячется из бара, «стол» — все окна плавают, заголовки рисует шелл. yami-niri (наш форк)
// узнаётся по версии: ему можно писать правила, которых нет у стокового (allow-tearing).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub size: (f64, f64),
    /// колонка и место в ней (лента)
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

#[derive(Default)]
pub struct Niri {
    pub ws: Vec<Ws>,
    pub wins: Vec<Win>,
    pub kb_names: Vec<String>,
    pub kb_idx: usize,
    pub overview: bool,
    /// спрятанные «свёрнутые» окна
    pub hidden: Vec<u64>,
    pub min_order: Vec<u64>,
    tx: Option<mpsc::Sender<Value>>,
}

pub enum Ev {
    /// окно открылось впервые
    NewWindow(u64),
    KbSwitched,
    Changed,
}




fn socket() -> Option<String> {
    std::env::var("NIRI_SOCKET").ok()
}

pub fn request(req: &Value) -> Option<Value> {
    let mut s = UnixStream::connect(socket()?).ok()?;
    let mut line = req.to_string();
    line.push('\n');
    s.write_all(line.as_bytes()).ok()?;
    let mut r = BufReader::new(s);
    let mut out = String::new();
    r.read_line(&mut out).ok()?;
    serde_json::from_str(&out).ok()
}

/// в фокусе игра или окно на весь экран: перезапуск шелла и панели подождут (для CLI и
/// обновлений — у шелла есть свой GameMode)
pub fn game_focused() -> bool {
    let Some(v) = request(&json!("FocusedWindow")) else { return false };
    let w = &v["Ok"]["FocusedWindow"];
    let id = w["app_id"].as_str().unwrap_or("");
    id.starts_with("steam_app_") || id == "gamescope" || id.ends_with(".exe") || w["is_fullscreen"].as_bool().unwrap_or(false)
}

/// нить: читает поток событий, при обрыве переподключается
pub fn spawn_stream(send: impl Fn(Value) + Send + 'static) {
    std::thread::Builder::new()
        .name("niri-events".into())
        .spawn(move || loop {
            if let Some(path) = socket() {
                if let Ok(mut s) = UnixStream::connect(path) {
                    if s.write_all(b"\"EventStream\"\n").is_ok() {
                        let r = BufReader::new(s);
                        for line in r.lines() {
                            let Ok(line) = line else { break };
                            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                                if v.get("Ok").is_none() {
                                    send(v);
                                }
                            }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        })
        .ok();
}

fn parse_win(v: &Value) -> Win {
    let l = &v["layout"];
    let pair = |x: &Value| -> Option<(f64, f64)> { Some((x.get(0)?.as_f64()?, x.get(1)?.as_f64()?)) };
    Win {
        id: v["id"].as_u64().unwrap_or(0),
        title: v["title"].as_str().unwrap_or("").to_string(),
        app_id: v["app_id"].as_str().unwrap_or("").to_string(),
        ws: v["workspace_id"].as_u64(),
        floating: v["is_floating"].as_bool().unwrap_or(false),
        focused: v["is_focused"].as_bool().unwrap_or(false),
        urgent: v["is_urgent"].as_bool().unwrap_or(false),
        ts: ts_of(&v["focus_timestamp"]),
        layout: Layout {
            size: pair(&l["tile_size"]).unwrap_or((0.0, 0.0)),
            col: l["pos_in_scrolling_layout"].get(0).and_then(|c| Some((c.as_u64()? as u32, l["pos_in_scrolling_layout"][1].as_u64()? as u32))),
        },
    }
}

fn ts_of(v: &Value) -> f64 {
    v["secs"].as_f64().unwrap_or(0.0) + v["nanos"].as_f64().unwrap_or(0.0) / 1e9
}

fn parse_ws(v: &Value) -> Ws {
    Ws {
        id: v["id"].as_u64().unwrap_or(0),
        idx: v["idx"].as_u64().unwrap_or(0) as u32,
        name: v["name"].as_str().map(|s| s.to_string()),
        output: v["output"].as_str().unwrap_or("").to_string(),
        active: v["is_active"].as_bool().unwrap_or(false),
        focused: v["is_focused"].as_bool().unwrap_or(false),
        urgent: v["is_urgent"].as_bool().unwrap_or(false),
    }
}

impl Niri {
    pub fn new() -> Niri {
        let (tx, rx) = mpsc::channel::<Value>();
        std::thread::Builder::new()
            .name("niri-act".into())
            .spawn(move || {
                while let Ok(mut a) = rx.recv() {
                    // перетаскивание окна: сдвиги, набежавшие за время запроса, — одним запросом
                    let mut later = None;
                    while let Ok(b) = rx.try_recv() {
                        match (a.get("MoveFloatingWindow"), b.get("MoveFloatingWindow")) {
                            (Some(x), Some(y)) if x["id"] == y["id"] && x["x"].get("AdjustFixed").is_some() && y["x"].get("AdjustFixed").is_some() => {
                                let f = |v: &Value, k: &str| v[k]["AdjustFixed"].as_f64().unwrap_or(0.0);
                                let id = x["id"].clone();
                                let (nx, ny) = (f(x, "x") + f(y, "x"), f(x, "y") + f(y, "y"));
                                a = json!({"MoveFloatingWindow": {"id": id, "x": {"AdjustFixed": nx}, "y": {"AdjustFixed": ny}}});
                            }
                            _ => {
                                later = Some(b);
                                break;
                            }
                        }
                    }
                    let r = request(&json!({ "Action": a }));
                    if let Some(Value::Object(m)) = r {
                        if let Some(e) = m.get("Err") {
                            eprintln!("niri: {a} → {e}");
                        }
                    }
                    if let Some(b) = later {
                        let _ = request(&json!({ "Action": b }));
                    }
                }
            })
            .ok();
        Niri { tx: Some(tx), ..Default::default() }
    }

    pub fn act(&self, a: Value) {
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
    pub fn minimized(&self) -> Vec<Win> {
        let mut v: Vec<Win> = self.wins.iter().filter(|w| self.is_min(w)).cloned().collect();
        v.sort_by_key(|w| self.min_order.iter().position(|x| *x == w.id).unwrap_or(0));
        v
    }

    // ── действия ────────────────────────────────────────────────────────────
    pub fn close_window(&self, id: u64) {
        self.act(json!({"CloseWindow": {"id": id}}));
    }
    pub fn focus_ws(&self, ws: &Ws) {
        if ws.output != self.focused_output() {
            self.act(json!({"FocusMonitor": {"output": ws.output}}));
        }
        self.act(json!({"FocusWorkspace": {"reference": {"Id": ws.id}}}));
    }

    pub fn minimize(&mut self, id: u64) {
        let Some(w) = self.win(id).cloned() else { return };
        if self.is_min(&w) {
            return;
        }
        self.min_order.retain(|x| *x != id);
        self.min_order.push(id);
        // на последний стол этого монитора, он у niri всегда пустой
        let out = w.ws.and_then(|i| self.ws.iter().find(|x| x.id == i)).map(|x| x.output.clone()).unwrap_or_else(|| self.focused_output());
        let last = self.ws.iter().filter(|x| x.output == out).map(|x| x.idx).max().unwrap_or(1);
        self.hidden.push(id);
        self.act(json!({"MoveWindowToWorkspace": {"window_id": id, "reference": {"Index": last}, "focus": false}}));
    }
    pub fn restore(&mut self, id: u64) {
        let out = self.focused_output();
        let idx = self.active_ws(&out).map(|w| w.idx).unwrap_or(1);
        self.min_order.retain(|x| *x != id);
        self.hidden.retain(|x| *x != id);
        self.act(json!({"MoveWindowToWorkspace": {"window_id": id, "reference": {"Index": idx}, "focus": false}}));
        self.act(json!({"FocusWindow": {"id": id}}));
    }
    pub fn toggle_minimize(&mut self, id: u64) {
        let Some(w) = self.win(id).cloned() else { return };
        if self.is_min(&w) {
            self.restore(id);
        } else if w.focused {
            self.minimize(id);
        } else {
            self.act(json!({"FocusWindow": {"id": id}}));
        }
    }


    // ── поток событий ───────────────────────────────────────────────────────
    pub fn handle(&mut self, ev: Value) -> Vec<Ev> {
        let mut out = vec![Ev::Changed];
        let Value::Object(m) = ev else { return out };
        let Some((k, d)) = m.into_iter().next() else { return out };
        match k.as_str() {
            "WorkspacesChanged" => {
                self.ws = d["workspaces"].as_array().map(|a| a.iter().map(parse_ws).collect()).unwrap_or_default();
                self.ws.sort_by(|a, b| a.output.cmp(&b.output).then(a.idx.cmp(&b.idx)));
            }
            "WorkspaceActivated" => {
                let id = d["id"].as_u64().unwrap_or(0);
                let focused = d["focused"].as_bool().unwrap_or(false);
                if let Some(out) = self.ws.iter().find(|w| w.id == id).map(|w| w.output.clone()) {
                    for w in self.ws.iter_mut() {
                        if w.output == out {
                            w.active = w.id == id;
                        }
                        if focused {
                            w.focused = w.id == id;
                        }
                    }
                }
            }
            "WorkspaceUrgencyChanged" => {
                let id = d["id"].as_u64().unwrap_or(0);
                if let Some(w) = self.ws.iter_mut().find(|w| w.id == id) {
                    w.urgent = d["urgent"].as_bool().unwrap_or(false);
                }
            }
            "WindowsChanged" => {
                self.wins = d["windows"].as_array().map(|a| a.iter().map(parse_win).collect()).unwrap_or_default();
            }
            "WindowOpenedOrChanged" => {
                let w = parse_win(&d["window"]);
                let known = self.wins.iter().any(|x| x.id == w.id);
                if w.focused {
                    for x in self.wins.iter_mut() {
                        x.focused = false;
                    }
                }
                if !known {
                    out.push(Ev::NewWindow(w.id));
                }
                if let Some(x) = self.wins.iter_mut().find(|x| x.id == w.id) {
                    *x = w;
                } else {
                    self.wins.push(w);
                }
            }
            "WindowClosed" => {
                let id = d["id"].as_u64().unwrap_or(0);
                self.wins.retain(|x| x.id != id);
                self.hidden.retain(|x| *x != id);
                self.min_order.retain(|x| *x != id);
            }
            "WindowFocusChanged" => {
                let id = d["id"].as_u64();
                for x in self.wins.iter_mut() {
                    x.focused = Some(x.id) == id;
                }
            }
            "WindowFocusTimestampChanged" => {
                let id = d["id"].as_u64().unwrap_or(0);
                if let Some(x) = self.wins.iter_mut().find(|x| x.id == id) {
                    x.ts = ts_of(&d["focus_timestamp"]);
                }
            }
            "WindowUrgencyChanged" => {
                let id = d["id"].as_u64().unwrap_or(0);
                if let Some(x) = self.wins.iter_mut().find(|x| x.id == id) {
                    x.urgent = d["urgent"].as_bool().unwrap_or(false);
                }
            }
            "WindowLayoutsChanged" => {
                if let Some(ch) = d["changes"].as_array() {
                    for c in ch {
                        let id = c[0].as_u64().unwrap_or(0);
                        let tmp = parse_win(&json!({"layout": c[1]}));
                        if let Some(x) = self.wins.iter_mut().find(|x| x.id == id) {
                            x.layout = tmp.layout;
                        }
                    }
                }
            }
            "KeyboardLayoutsChanged" => {
                let k = &d["keyboard_layouts"];
                self.kb_names = k["names"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
                self.kb_idx = k["current_idx"].as_u64().unwrap_or(0) as usize;
            }
            "KeyboardLayoutSwitched" => {
                self.kb_idx = d["idx"].as_u64().unwrap_or(0) as usize;
                out.push(Ev::KbSwitched);
            }
            "OverviewOpenedOrClosed" => {
                self.overview = d["is_open"].as_bool().unwrap_or(false);
            }
            _ => {}
        }
        out
    }
}
