// sway по i3-IPC: сокет $SWAYSOCK, сообщение — «i3-ipc», длина и тип (u32, родной порядок байт)
// и JSON. В событиях sway нет всего состояния, поэтому нить событий только будит, а вторая нить
// пачкой перечитывает столы, дерево окон и раскладки и шлёт срез целиком (Wm::apply).
//
// Свёрнутые окна — в scratchpad (там они плавают: что плавало до этого, шелл помнит сам). Обзора
// и потоков показа экрана у sway нет.

use super::{Do, Ev, Kb, Layout, Snap, Win, Wm, Ws};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

pub const NAME: &str = "sway";
/// где посмотреть app-id окна
pub const APPID_HINT: &str = "swaymsg -t get_tree";
pub type Raw = Snap;

const RUN_COMMAND: u32 = 0;
const GET_WORKSPACES: u32 = 1;
const SUBSCRIBE: u32 = 2;
const GET_TREE: u32 = 4;
const GET_INPUTS: u32 = 100;
/// у событий старший бит типа
const EVENT: u32 = 1 << 31;

fn socket() -> Option<String> {
    std::env::var("SWAYSOCK").ok()
}

fn put(s: &mut UnixStream, ty: u32, body: &str) -> Option<()> {
    let mut m = Vec::with_capacity(14 + body.len());
    m.extend_from_slice(b"i3-ipc");
    m.extend_from_slice(&(body.len() as u32).to_ne_bytes());
    m.extend_from_slice(&ty.to_ne_bytes());
    m.extend_from_slice(body.as_bytes());
    s.write_all(&m).ok()
}

fn get(s: &mut UnixStream) -> Option<(u32, Value)> {
    let mut h = [0u8; 14];
    s.read_exact(&mut h).ok()?;
    if &h[..6] != b"i3-ipc" {
        return None;
    }
    let len = u32::from_ne_bytes(h[6..10].try_into().ok()?) as usize;
    let ty = u32::from_ne_bytes(h[10..14].try_into().ok()?);
    let mut b = vec![0; len];
    s.read_exact(&mut b).ok()?;
    Some((ty, serde_json::from_slice(&b).unwrap_or(Value::Null)))
}

fn request(ty: u32, body: &str) -> Option<Value> {
    let mut s = UnixStream::connect(socket()?).ok()?;
    put(&mut s, ty, body)?;
    get(&mut s).map(|r| r.1)
}

/// команда sway; ошибки — в журнал
fn run(cmd: &str) {
    if let Some(Value::Array(a)) = request(RUN_COMMAND, cmd) {
        for r in a.iter().filter(|r| r["success"] == false) {
            eprintln!("sway: {cmd} → {}", r["error"]);
        }
    }
}

/// строка в кавычках для команды sway
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// стол в команде: по имени, если оно своё, иначе по номеру
fn ws_ref(ws: &Ws) -> String {
    match &ws.name {
        Some(n) => format!("workspace {}", quote(n)),
        None => format!("workspace number {}", ws.idx),
    }
}

fn parse_ws(i: usize, v: &Value) -> Ws {
    let num = v["num"].as_i64().unwrap_or(-1);
    let name = v["name"].as_str().unwrap_or("").to_string();
    Ws {
        id: v["id"].as_u64().unwrap_or(0),
        // столы без номера — после нумерованных
        idx: if num >= 0 { num as u32 } else { 1000 + i as u32 },
        name: (name != num.to_string()).then_some(name),
        output: v["output"].as_str().unwrap_or("").to_string(),
        active: v["visible"].as_bool().unwrap_or(false),
        focused: v["focused"].as_bool().unwrap_or(false),
        urgent: v["urgent"].as_bool().unwrap_or(false),
    }
}

/// окна дерева по порядку на экране: стол — ближайший предок-стол (scratchpad — без стола)
fn walk(n: &Value, ws: Option<u64>, floating: bool, out: &mut Vec<Win>) {
    let ws = if n["type"] == "workspace" { if n["name"] == "__i3_scratch" { None } else { n["id"].as_u64() } } else { ws };
    let empty = |k: &str| n[k].as_array().is_none_or(|a| a.is_empty());
    if empty("nodes") && empty("floating_nodes") && n.get("pid").is_some_and(|p| !p.is_null()) {
        let r = &n["rect"];
        let floating = floating || n["type"] == "floating_con";
        out.push(Win {
            id: n["id"].as_u64().unwrap_or(0),
            title: n["name"].as_str().unwrap_or("").to_string(),
            app_id: n["app_id"].as_str().or(n["window_properties"]["class"].as_str()).unwrap_or("").to_string(),
            ws,
            floating,
            focused: n["focused"].as_bool().unwrap_or(false),
            urgent: n["urgent"].as_bool().unwrap_or(false),
            ts: 0.0,
            layout: Layout { size: (r["width"].as_f64().unwrap_or(0.0), r["height"].as_f64().unwrap_or(0.0)), col: (!floating).then_some((out.len() as u32, 0)) },
        });
        return;
    }
    for k in n["nodes"].as_array().into_iter().flatten() {
        walk(k, ws, floating, out);
    }
    for k in n["floating_nodes"].as_array().into_iter().flatten() {
        walk(k, ws, true, out);
    }
}

/// раскладки первой клавиатуры, где они есть
fn keyboard(v: &Value) -> (Vec<String>, usize) {
    let kb = v.as_array().and_then(|a| a.iter().find(|d| d["type"] == "keyboard" && d["xkb_layout_names"].as_array().is_some_and(|n| !n.is_empty())));
    let Some(kb) = kb else { return (vec![], 0) };
    let names = kb["xkb_layout_names"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
    (names, kb["xkb_active_layout_index"].as_u64().unwrap_or(0) as usize)
}

fn snapshot() -> Option<Snap> {
    let mut ws: Vec<Ws> = request(GET_WORKSPACES, "")?.as_array()?.iter().enumerate().map(|(i, v)| parse_ws(i, v)).collect();
    ws.sort_by(|a, b| a.output.cmp(&b.output).then(a.idx.cmp(&b.idx)));
    let mut wins = vec![];
    walk(&request(GET_TREE, "")?, None, false, &mut wins);
    let (kb_names, kb_idx) = keyboard(&request(GET_INPUTS, "").unwrap_or(Value::Null));
    Some(Snap { ws, wins, kb_names, kb_idx })
}

/// нити: события только будят, срез собирается пачкой (переключение стола — десяток событий)
pub fn spawn_stream(send: impl Fn(Raw) + Send + 'static) {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("sway-events".into())
        .spawn(move || loop {
            if let Some(mut s) = socket().and_then(|p| UnixStream::connect(p).ok()) {
                if put(&mut s, SUBSCRIBE, r#"["workspace","window","input","output"]"#).is_some() {
                    let _ = tx.send(());
                    while let Some((ty, _)) = get(&mut s) {
                        if ty & EVENT != 0 {
                            let _ = tx.send(());
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        })
        .ok();
    std::thread::Builder::new()
        .name("sway-state".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                std::thread::sleep(Duration::from_millis(15));
                while rx.try_recv().is_ok() {}
                if let Some(s) = snapshot() {
                    send(s);
                }
            }
        })
        .ok();
}

/// срез всегда целиком
pub fn layout_only(_: &Raw) -> bool {
    false
}

/// окно в фокусе в дереве
fn focused(n: &Value) -> Option<&Value> {
    if n["focused"] == true && n.get("pid").is_some_and(|p| !p.is_null()) {
        return Some(n);
    }
    n["nodes"].as_array().into_iter().flatten().chain(n["floating_nodes"].as_array().into_iter().flatten()).find_map(focused)
}

/// в фокусе игра или окно на весь экран
pub fn game_focused() -> bool {
    let Some(tree) = request(GET_TREE, "") else { return false };
    let Some(w) = focused(&tree) else { return false };
    let id = w["app_id"].as_str().or(w["window_properties"]["class"].as_str()).unwrap_or("");
    id.starts_with("steam_app_") || id == "gamescope" || id.ends_with(".exe") || w["fullscreen_mode"].as_u64().unwrap_or(0) > 0
}

/// тема курсора: sway ставит её сам (и XCURSOR_* для своих программ)
pub fn set_cursor(theme: &str, size: i32) {
    run(&format!("seat * xcursor_theme {} {size}", quote(theme)));
}

/// действие шелла — командой sway (зовётся из нити действий)
pub fn exec(a: &Do) {
    let c = |id: &u64| format!("[con_id={id}]");
    match a {
        Do::Focus(id) => run(&format!("{} focus", c(id))),
        Do::Close(id) => run(&format!("{} kill", c(id))),
        Do::FocusOutput(o) => run(&format!("focus output {}", quote(o))),
        Do::FocusWs(ws) => run(&ws_ref(ws)),
        Do::WsStep(up) => run(if *up { "workspace prev_on_output" } else { "workspace next_on_output" }),
        Do::MoveToWs(id, ws) => run(&format!("{} move container to {}", c(id), ws_ref(ws))),
        Do::Minimize { id, .. } => run(&format!("{} move scratchpad", c(id))),
        // scratchpad show — на текущий стол, плавающим; в раскладку — если не плавало
        Do::Restore { id, floating, .. } => run(&format!("{} scratchpad show{}, focus", c(id), if *floating { "" } else { ", floating disable" })),
        // «развернуть» у sway — только на весь экран
        Do::Maximize { id, .. } | Do::Fullscreen(id) => run(&format!("{} fullscreen toggle", c(id))),
        Do::ToggleFloat(id) => run(&format!("{} floating toggle", c(id))),
        Do::Center(id) => run(&format!("{} move position center", c(id))),
        Do::Kb(k) => {
            let to = match k {
                Kb::Next => "next".to_string(),
                Kb::Prev => "prev".to_string(),
                Kb::Index(i) => i.to_string(),
            };
            run(&format!("input type:keyboard xkb_switch_layout {to}"));
        }
        Do::Quit => run("exit"),
        Do::MonitorsOff => run("output * power off"),
        Do::MonitorsOn => run("output * power on"),
    }
}

impl Wm {
    pub fn handle(&mut self, s: Raw) -> Vec<Ev> {
        self.apply(s)
    }
}
