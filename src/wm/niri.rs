// niri по IPC-сокету: поток событий в отдельной нити, действия — короткими запросами.
//
// «Стол» — все окна плавают, заголовки рисует шелл. yami-niri (наш форк) узнаётся по версии: ему
// можно писать правила, которых нет у стокового (allow-tearing).

use super::{Cast, CastTarget, Do, Ev, Kb, Layout, Win, Wm, Ws};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

pub const NAME: &str = "niri";
/// где посмотреть app-id окна
pub const APPID_HINT: &str = "niri msg windows";
/// событие niri как есть: разбирает Wm::handle
pub type Raw = Value;

/// номер потока показа экрана: stream_id (или id — у разных версий niri)
fn cast_id(v: &Value) -> Option<u64> {
    v["stream_id"].as_u64().or_else(|| v["id"].as_u64())
}

fn cast_of(v: &Value) -> Option<Cast> {
    let t = &v["target"];
    let target = if let Some(n) = t["Output"]["name"].as_str() {
        CastTarget::Output(n.to_string())
    } else if let Some(id) = t["Window"]["id"].as_u64() {
        CastTarget::Window(id)
    } else {
        CastTarget::Nothing
    };
    Some(Cast { id: cast_id(v)?, pid: v["pid"].as_u64().map(|p| p as u32), target, dynamic: v["is_dynamic_target"].as_bool().unwrap_or(false) })
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
pub fn spawn_stream(send: impl Fn(Raw) + Send + 'static) {
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

/// поменялись только размеры и места окон: столы и док перерисовывать незачем
pub fn layout_only(v: &Raw) -> bool {
    v.get("WindowLayoutsChanged").is_some()
}

fn action(a: Value) {
    if let Some(Value::Object(m)) = request(&json!({ "Action": a })) {
        if let Some(e) = m.get("Err") {
            eprintln!("niri: {a} → {e}");
        }
    }
}

/// действие шелла — действиями niri (зовётся из нити действий)
pub fn exec(a: &Do) {
    match a {
        Do::Focus(id) => action(json!({"FocusWindow": {"id": id}})),
        Do::Close(id) => action(json!({"CloseWindow": {"id": id}})),
        Do::FocusOutput(o) => action(json!({"FocusMonitor": {"output": o}})),
        Do::FocusWs(ws) => action(json!({"FocusWorkspace": {"reference": {"Id": ws.id}}})),
        Do::WsStep(up) => action(if *up { json!({"FocusWorkspaceUp": {}}) } else { json!({"FocusWorkspaceDown": {}}) }),
        Do::MoveToWs(id, ws) => action(json!({"MoveWindowToWorkspace": {"window_id": id, "reference": {"Id": ws.id}, "focus": false}})),
        // на последний стол этого монитора, он у niri всегда пустой
        Do::Minimize { id, last } => action(json!({"MoveWindowToWorkspace": {"window_id": id, "reference": {"Index": last}, "focus": false}})),
        Do::Restore { id, ws, .. } => {
            let to = match ws {
                Some(ws) => json!({"Id": ws.id}),
                None => json!({"Index": 1}),
            };
            action(json!({"MoveWindowToWorkspace": {"window_id": id, "reference": to, "focus": false}}));
            action(json!({"FocusWindow": {"id": id}}));
        }
        Do::Maximize { id, floating } => {
            if *floating {
                action(json!({"MaximizeWindowToEdges": {"id": id}}));
            } else {
                action(json!({"FocusWindow": {"id": id}}));
                action(json!({"MaximizeColumn": {}}));
            }
        }
        Do::Fullscreen(id) => action(json!({"FullscreenWindow": {"id": id}})),
        Do::ToggleFloat(id) => action(json!({"ToggleWindowFloating": {"id": id}})),
        Do::Center(id) => {
            action(json!({"FocusWindow": {"id": id}}));
            action(json!({"CenterWindow": {"id": id}}));
        }
        Do::Kb(k) => {
            let l = match k {
                Kb::Next => json!("Next"),
                Kb::Prev => json!("Prev"),
                Kb::Index(i) => json!({"Index": i}),
            };
            action(json!({"SwitchLayout": {"layout": l}}));
        }
        Do::Quit => action(json!({"Quit": {"skip_confirmation": true}})),
        Do::MonitorsOff => action(json!({"PowerOffMonitors": {}})),
        // niri будит мониторы сам, от любого ввода
        Do::MonitorsOn => {}
    }
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

impl Wm {
    // ── поток событий ───────────────────────────────────────────────────────
    pub fn handle(&mut self, ev: Raw) -> Vec<Ev> {
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
                self.forget(id);
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
            // весь список — в начале потока, дальше по одному: начался (или сменился) и кончился
            "CastsChanged" => {
                self.casts = d["casts"].as_array().map(|a| a.iter().filter_map(cast_of).collect()).unwrap_or_default();
                self.casting = !self.casts.is_empty();
            }
            "CastStartedOrChanged" => {
                if let Some(c) = cast_of(&d["cast"]) {
                    self.casts.retain(|x| x.id != c.id);
                    self.casts.push(c);
                }
                self.casting = !self.casts.is_empty();
            }
            "CastStopped" => {
                let id = cast_id(&d);
                self.casts.retain(|x| Some(x.id) != id);
                self.casting = !self.casts.is_empty();
            }
            "OverviewOpenedOrClosed" => {
                self.overview = d["is_open"].as_bool().unwrap_or(false);
            }
            _ => {}
        }
        out
    }
}
