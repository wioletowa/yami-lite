// Hyprland по своим сокетам ($XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE): .socket.sock —
// запросы (j/clients, dispatch …), один на соединение; .socket2.sock — события строками
// «имя>>данные». В событиях нет всего состояния: нить событий будит, вторая нить пачкой
// перечитывает мониторы, столы, окна и раскладки и шлёт срез целиком (Wm::apply).
//
// id окна — его адрес (0x…) числом. Свёрнутые окна — на спрятанном special:yami. Обзора и
// потоков показа экрана у Hyprland нет; «срочное» приходит только событием — его помним сами.

use super::{Do, Ev, Kb, Layout, Snap, Win, Wm, Ws};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

pub const NAME: &str = "Hyprland";
/// где посмотреть app-id окна (class)
pub const APPID_HINT: &str = "hyprctl clients";
pub type Raw = Snap;

/// спрятанный стол свёрнутых
const MIN_WS: &str = "special:yami";

fn dir() -> Option<PathBuf> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let rt = std::env::var("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    let p = rt.join("hypr").join(&sig);
    // до 0.40 сокеты жили в /tmp/hypr
    Some(if p.join(".socket.sock").exists() { p } else { PathBuf::from("/tmp/hypr").join(sig) })
}

fn request(cmd: &str) -> Option<String> {
    let mut s = UnixStream::connect(dir()?.join(".socket.sock")).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

fn query(what: &str) -> Option<Value> {
    serde_json::from_str(&request(&format!("j/{what}"))?).ok()
}

/// диспетчер Hyprland; ответ не «ok» — в журнал
fn dispatch(args: &str) {
    if let Some(r) = request(&format!("dispatch {args}")) {
        if r.trim() != "ok" {
            eprintln!("hyprland: {args} → {}", r.trim());
        }
    }
}

fn addr(v: &Value) -> u64 {
    v.as_str().and_then(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16).ok()).unwrap_or(0)
}

/// окно в аргументе диспетчера
fn win(id: &u64) -> String {
    format!("address:0x{id:x}")
}

/// имена раскладок по кодам xkb: Hyprland отдаёт полное имя только у текущей
fn layout_name(code: &str) -> String {
    match code.trim() {
        "us" => "English (US)",
        "gb" => "English (UK)",
        "ru" => "Russian",
        "ua" => "Ukrainian",
        "by" => "Belarusian",
        "kz" => "Kazakh",
        "de" => "German",
        "fr" => "French",
        "pl" => "Polish",
        c => c,
    }
    .to_string()
}

/// главная клавиатура (или первая)
fn main_kb(devs: &Value) -> Option<&Value> {
    let kbs = devs["keyboards"].as_array()?;
    kbs.iter().find(|k| k["main"] == true).or(kbs.first())
}

fn keyboard(devs: &Value) -> (Vec<String>, usize) {
    let Some(kb) = main_kb(devs) else { return (vec![], 0) };
    let mut names: Vec<String> = kb["layout"].as_str().unwrap_or("").split(',').filter(|c| !c.trim().is_empty()).map(layout_name).collect();
    let active = kb["active_keymap"].as_str().unwrap_or("").to_string();
    // индекс есть с 0.42; раньше — искать по имени
    let idx = kb["active_layout_index"].as_u64().map(|i| i as usize).or_else(|| names.iter().position(|n| *n == active)).unwrap_or(0);
    if let Some(n) = names.get_mut(idx) {
        if !active.is_empty() {
            *n = active;
        }
    }
    (names, idx)
}

fn snapshot(urgent: &[u64]) -> Option<Snap> {
    let mons = query("monitors")?;
    let mons = mons.as_array()?;
    let active = query("activewindow").map(|v| addr(&v["address"])).filter(|a| *a != 0);
    let mut wins: Vec<(Win, (i64, i64))> = query("clients")?
        .as_array()?
        .iter()
        .filter(|c| c["mapped"] != false)
        .map(|c| {
            let id = addr(&c["address"]);
            let wid = c["workspace"]["id"].as_i64().unwrap_or(-1);
            let p = |v: &Value, i: usize| v[i].as_i64().unwrap_or(0);
            let w = Win {
                id,
                title: c["title"].as_str().unwrap_or("").to_string(),
                app_id: c["class"].as_str().unwrap_or("").to_string(),
                // special-столы (отрицательные) — без стола
                ws: (wid > 0).then_some(wid as u64),
                floating: c["floating"].as_bool().unwrap_or(false),
                focused: Some(id) == active,
                urgent: urgent.contains(&id),
                ts: 0.0,
                layout: Layout { size: (p(&c["size"], 0) as f64, p(&c["size"], 1) as f64), col: None },
            };
            (w, (p(&c["at"], 0), p(&c["at"], 1)))
        })
        .collect();
    // порядок раскладки — по месту на экране: слева направо, сверху вниз
    wins.sort_by_key(|(_, at)| *at);
    let mut n = 0;
    let wins: Vec<Win> = wins
        .into_iter()
        .map(|(mut w, _)| {
            if !w.floating {
                w.layout.col = Some((n, 0));
                n += 1;
            }
            w
        })
        .collect();
    let mut ws: Vec<Ws> = query("workspaces")?
        .as_array()?
        .iter()
        .filter(|w| w["id"].as_i64().unwrap_or(0) > 0)
        .map(|w| {
            let id = w["id"].as_u64().unwrap_or(0);
            let name = w["name"].as_str().unwrap_or("").to_string();
            let output = w["monitor"].as_str().unwrap_or("").to_string();
            let mon = mons.iter().find(|m| m["name"].as_str() == Some(&output));
            let active = mon.is_some_and(|m| m["activeWorkspace"]["id"].as_u64() == Some(id));
            Ws {
                id,
                idx: id as u32,
                name: (name != id.to_string()).then_some(name),
                focused: active && mon.is_some_and(|m| m["focused"] == true),
                active,
                urgent: wins.iter().any(|x| x.urgent && x.ws == Some(id)),
                output,
            }
        })
        .collect();
    ws.sort_by(|a, b| a.output.cmp(&b.output).then(a.idx.cmp(&b.idx)));
    let (kb_names, kb_idx) = query("devices").map(|d| keyboard(&d)).unwrap_or_default();
    Some(Snap { ws, wins, kb_names, kb_idx })
}

/// нити: события только будят (и несут «срочное»), срез собирается пачкой
pub fn spawn_stream(send: impl Fn(Raw) + Send + 'static) {
    let (tx, rx) = mpsc::channel::<Option<u64>>();
    std::thread::Builder::new()
        .name("hypr-events".into())
        .spawn(move || loop {
            if let Some(s) = dir().and_then(|d| UnixStream::connect(d.join(".socket2.sock")).ok()) {
                let _ = tx.send(None);
                for line in BufReader::new(s).lines() {
                    let Ok(line) = line else { break };
                    let urgent = line.strip_prefix("urgent>>").map(|a| addr(&Value::from(a.trim())));
                    let _ = tx.send(urgent);
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        })
        .ok();
    std::thread::Builder::new()
        .name("hypr-state".into())
        .spawn(move || {
            let mut urgent: Vec<u64> = vec![];
            while let Ok(first) = rx.recv() {
                std::thread::sleep(Duration::from_millis(15));
                for u in std::iter::once(first).chain(rx.try_iter()).flatten() {
                    urgent.push(u);
                }
                if let Some(s) = snapshot(&urgent) {
                    // срочное гаснет, когда окно в фокусе или его нет
                    urgent.retain(|u| s.wins.iter().any(|w| w.id == *u && !w.focused));
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

/// в фокусе игра или окно на весь экран
pub fn game_focused() -> bool {
    let Some(w) = query("activewindow") else { return false };
    let id = w["class"].as_str().unwrap_or("");
    // fullscreen — число (режим) с 0.42, раньше — да/нет
    let full = w["fullscreen"].as_u64().map(|f| f > 0).or(w["fullscreen"].as_bool()).unwrap_or(false);
    id.starts_with("steam_app_") || id == "gamescope" || id.ends_with(".exe") || full
}

/// тема курсора: Hyprland ставит её сам
pub fn set_cursor(theme: &str, size: i32) {
    let _ = request(&format!("setcursor {theme} {size}"));
}

/// действие шелла — диспетчером Hyprland (зовётся из нити действий)
pub fn exec(a: &Do) {
    match a {
        Do::Focus(id) => dispatch(&format!("focuswindow {}", win(id))),
        Do::Close(id) => dispatch(&format!("closewindow {}", win(id))),
        Do::FocusOutput(o) => dispatch(&format!("focusmonitor {o}")),
        Do::FocusWs(ws) => dispatch(&format!("workspace {}", ws.id)),
        Do::WsStep(up) => dispatch(if *up { "workspace m-1" } else { "workspace m+1" }),
        Do::MoveToWs(id, ws) => dispatch(&format!("movetoworkspacesilent {},{}", ws.id, win(id))),
        Do::Minimize { id, .. } => dispatch(&format!("movetoworkspacesilent {MIN_WS},{}", win(id))),
        Do::Restore { id, ws, .. } => {
            if let Some(ws) = ws {
                dispatch(&format!("movetoworkspacesilent {},{}", ws.id, win(id)));
            }
            dispatch(&format!("focuswindow {}", win(id)));
        }
        // fullscreen 1 — «развернуть» (бар и отступы остаются), 0 — на весь экран
        Do::Maximize { id, .. } => {
            dispatch(&format!("focuswindow {}", win(id)));
            dispatch("fullscreen 1");
        }
        Do::Fullscreen(id) => {
            dispatch(&format!("focuswindow {}", win(id)));
            dispatch("fullscreen 0");
        }
        Do::ToggleFloat(id) => dispatch(&format!("togglefloating {}", win(id))),
        Do::Center(id) => {
            dispatch(&format!("focuswindow {}", win(id)));
            dispatch("centerwindow");
        }
        Do::Kb(k) => {
            let to = match k {
                Kb::Next => "next".to_string(),
                Kb::Prev => "prev".to_string(),
                Kb::Index(i) => i.to_string(),
            };
            let dev = query("devices").and_then(|d| main_kb(&d).and_then(|k| k["name"].as_str().map(String::from))).unwrap_or_else(|| "all".into());
            let _ = request(&format!("switchxkblayout {dev} {to}"));
        }
        Do::Quit => dispatch("exit"),
        Do::MonitorsOff => dispatch("dpms off"),
        Do::MonitorsOn => dispatch("dpms on"),
    }
}

impl Wm {
    pub fn handle(&mut self, s: Raw) -> Vec<Ev> {
        self.apply(s)
    }
}
