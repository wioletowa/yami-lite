// Звук через PulseAudio-совместимый слой PipeWire: `pactl subscribe` будит, `pactl -f json`
// отдаёт устройства, громкости и потоки программ. Эквалайзер — свой: parec снимает звук с
// монитора выхода, БПФ раскладывает на полосы. Пишет звук только пока его кто-то видит.

use crate::app::Msg;
use calloop::channel::Sender;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Dev {
    pub name: String,
    pub desc: String,
    pub vol: f64,
    pub muted: bool,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Stream {
    pub idx: u32,
    pub app: String,
    pub title: String,
    /// имена, по которым искать значок, — по порядку (icon_names)
    pub icon: Vec<String>,
    pub vol: f64,
    pub muted: bool,
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct AudioState {
    pub ready: bool,
    pub volume: f64,
    pub muted: bool,
    pub sink: String,
    pub sinks: Vec<Dev>,
    pub source: String,
    pub sources: Vec<Dev>,
    pub mic: f64,
    pub mic_muted: bool,
    pub streams: Vec<Stream>,
    /// кто пишет с микрофона (записи не с мониторов: эквалайзер шелла, cava и пики — мимо)
    pub recorders: Vec<String>,
}

fn pactl_json(args: &[&str]) -> Value {
    // числа в JSON pactl пишет по локали (в ru_RU — «0,000000»): JSON ломается. Числа — по C,
    // названия устройств остаются на языке системы (LC_ALL перекрыл бы LC_NUMERIC — переносим в LANG)
    let mut c = Command::new("pactl");
    if let Ok(all) = std::env::var("LC_ALL") {
        c.env_remove("LC_ALL").env("LANG", all);
    }
    let o = c.env("LC_NUMERIC", "C").arg("-f").arg("json").args(args).output();
    o.ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(Value::Null)
}

fn vol_of(v: &Value) -> f64 {
    let Some(m) = v["volume"].as_object() else { return 0.0 };
    let vals: Vec<f64> = m.values().filter_map(|c| c["value"].as_f64()).collect();
    if vals.is_empty() {
        0.0
    } else {
        vals.iter().sum::<f64>() / vals.len() as f64 / 65536.0
    }
}

fn dev(v: &Value) -> Dev {
    Dev {
        name: v["name"].as_str().unwrap_or("").into(),
        desc: v["description"].as_str().unwrap_or("").into(),
        vol: vol_of(v),
        muted: v["mute"].as_bool().unwrap_or(false),
    }
}

/// как показывать устройство: своё имя из настроек (audioNames) или описание PipeWire
pub fn label(set: &crate::config::Settings, d: &Dev) -> String {
    set.audio_names.get(&d.name).filter(|s| !s.is_empty()).cloned().unwrap_or_else(|| d.desc.clone())
}

fn defaults() -> (String, String) {
    let info = pactl_json(&["info"]);
    (info["default_sink_name"].as_str().unwrap_or("").to_string(), info["default_source_name"].as_str().unwrap_or("").to_string())
}
fn sinks() -> Vec<Dev> {
    pactl_json(&["list", "sinks"]).as_array().map(|a| a.iter().map(dev).collect()).unwrap_or_default()
}
fn sources() -> Vec<Dev> {
    pactl_json(&["list", "sources"]).as_array().map(|a| a.iter().map(dev).filter(|d| !d.name.ends_with(".monitor")).collect()).unwrap_or_default()
}
fn streams() -> Vec<Stream> {
    pactl_json(&["list", "sink-inputs"])
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| {
                    let p = &x["properties"];
                    Stream {
                        idx: x["index"].as_u64().unwrap_or(0) as u32,
                        app: p["application.name"].as_str().or(p["application.process.binary"].as_str()).unwrap_or("?").into(),
                        title: p["media.name"].as_str().unwrap_or("").into(),
                        icon: icon_names(p),
                        vol: vol_of(x),
                        muted: x["mute"].as_bool().unwrap_or(false),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn recorders() -> Vec<String> {
    let monitors: Vec<u64> = pactl_json(&["list", "sources"]).as_array().map(|a| a.iter().filter(|s| s["name"].as_str().is_some_and(|n| n.ends_with(".monitor"))).filter_map(|s| s["index"].as_u64()).collect()).unwrap_or_default();
    let mut out: Vec<String> = vec![];
    for x in pactl_json(&["list", "source-outputs"]).as_array().into_iter().flatten() {
        if x["source"].as_u64().is_some_and(|s| monitors.contains(&s)) {
            continue;
        }
        let p = &x["properties"];
        let name: String = p["application.name"].as_str().or(p["application.process.binary"].as_str()).unwrap_or("?").into();
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// что поменялось (по строкам `pactl subscribe`): переспрашивается только оно
const SINK: u8 = 1;
const SOURCE: u8 = 2;
const STREAM: u8 = 4;
const SERVER: u8 = 8;
const REC: u8 = 16;
const ALL: u8 = SINK | SOURCE | STREAM | SERVER | REC;

/// «Event 'change' on sink-input #12» → что переспросить; записи программ (source-output) — для
/// индикатора микрофона, только появление и уход; клиенты и модули шеллу не нужны
fn facility(l: &str) -> u8 {
    match l.split(" on ").nth(1).and_then(|r| r.split_whitespace().next()) {
        Some("sink") => SINK,
        Some("source") => SOURCE,
        Some("sink-input") => STREAM,
        Some("server") | Some("card") => SERVER,
        Some("source-output") if !l.contains("'change'") => REC,
        _ => 0,
    }
}

/// переспросить отмеченное, остальное взять из прошлого состояния
fn refresh(mut a: AudioState, what: u8) -> AudioState {
    if what & SERVER != 0 {
        (a.sink, a.source) = defaults();
    }
    if what & (SINK | SERVER) != 0 {
        a.sinks = sinks();
    }
    if what & (SOURCE | SERVER) != 0 {
        a.sources = sources();
    }
    if what & STREAM != 0 {
        a.streams = streams();
    }
    if what & REC != 0 {
        a.recorders = recorders();
    }
    let d = a.sinks.iter().find(|s| s.name == a.sink).cloned().unwrap_or_default();
    let m = a.sources.iter().find(|s| s.name == a.source).cloned().unwrap_or_default();
    (a.volume, a.muted, a.mic, a.mic_muted, a.ready) = (d.vol, d.muted, m.vol, m.muted, true);
    a
}

pub fn query() -> AudioState {
    refresh(AudioState::default(), ALL)
}

static SET: OnceLock<mpsc::Sender<Vec<String>>> = OnceLock::new();

/// команды pactl по очереди, подряд идущие одинаковые — схлопываются (ползунок)
fn run_pactl(args: Vec<String>) {
    if let Some(tx) = SET.get() {
        let _ = tx.send(args);
    }
}

pub fn set_volume(v: f64) {
    run_pactl(vec!["set-sink-volume".into(), "@DEFAULT_SINK@".into(), format!("{}%", (v.clamp(0.0, 1.5) * 100.0).round())]);
}
pub fn step_volume(d: i32) {
    run_pactl(vec!["set-sink-volume".into(), "@DEFAULT_SINK@".into(), format!("{}5%", if d > 0 { "+" } else { "-" })]);
}
pub fn toggle_mute() {
    run_pactl(vec!["set-sink-mute".into(), "@DEFAULT_SINK@".into(), "toggle".into()]);
}
pub fn set_mic(v: f64) {
    run_pactl(vec!["set-source-volume".into(), "@DEFAULT_SOURCE@".into(), format!("{}%", (v.clamp(0.0, 1.5) * 100.0).round())]);
}
pub fn toggle_mic() {
    run_pactl(vec!["set-source-mute".into(), "@DEFAULT_SOURCE@".into(), "toggle".into()]);
}
pub fn set_stream(idx: u32, v: f64) {
    run_pactl(vec!["set-sink-input-volume".into(), idx.to_string(), format!("{}%", (v.clamp(0.0, 1.5) * 100.0).round())]);
}
pub fn mute_stream(idx: u32) {
    run_pactl(vec!["set-sink-input-mute".into(), idx.to_string(), "toggle".into()]);
}
pub fn set_sink(name: &str) {
    run_pactl(vec!["set-default-sink".into(), name.into()]);
}
pub fn set_source(name: &str) {
    run_pactl(vec!["set-default-source".into(), name.into()]);
}

pub fn spawn(tx: Sender<Msg>) {
    let (stx, srx) = mpsc::channel::<Vec<String>>();
    let _ = SET.set(stx);
    std::thread::spawn(move || {
        while let Ok(mut a) = srx.recv() {
            // ползунок: берём только последнюю из накопившихся похожих
            while let Ok(b) = srx.try_recv() {
                if b.first() != a.first() || b.get(1) != a.get(1) {
                    let _ = Command::new("pactl").args(&a).status();
                }
                a = b;
            }
            let _ = Command::new("pactl").args(&a).status();
        }
    });
    let tx2 = tx.clone();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            // пауза перед новым pactl: 2 с, а если он раз за разом сразу выходит (звукового сервера
            // нет) — вдвое дольше, до 30 с. Сервер, поднявшийся чуть позже шелла, подхватится быстро
            let mut wait = 2u64;
            loop {
                let t0 = std::time::Instant::now();
                let mut last = query();
                let _ = tx2.send(Msg::Audio(last.clone()));
                let Ok(mut c) = crate::proc::owned(Command::new("pactl").arg("subscribe").stdout(Stdio::piped()).stderr(Stdio::null())).spawn() else {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    continue;
                };
                // stdout у piped-ребёнка есть всегда; паники в потоке звука не будет в любом случае
                let Some(out) = c.stdout.take() else {
                    let _ = c.kill();
                    let _ = c.wait();
                    continue;
                };
                let (ltx, lrx) = mpsc::channel::<u8>();
                std::thread::spawn(move || {
                    for l in BufReader::new(out).lines() {
                        let Ok(l) = l else { break };
                        let f = facility(&l);
                        if f != 0 {
                            let _ = ltx.send(f);
                        }
                    }
                });
                while let Ok(mut what) = lrx.recv() {
                    // пачка событий → один запрос того, что в ней задето
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    while let Ok(f) = lrx.try_recv() {
                        what |= f;
                    }
                    let a = refresh(last.clone(), what);
                    if a != last {
                        last = a.clone();
                        let _ = tx2.send(Msg::Audio(a));
                    }
                }
                let _ = c.kill();
                let _ = c.wait();
                wait = if t0.elapsed().as_secs() < 10 { (wait * 2).min(30) } else { 2 };
                std::thread::sleep(std::time::Duration::from_secs(wait));
            }
        })
        .ok();
    viz_thread(tx);
}

// ── эквалайзер ──────────────────────────────────────────────────────────────
static VIZ_ON: AtomicBool = AtomicBool::new(false);
/// нить эквалайзера спит (park), пока его никто не видит: ни одного пробуждения без музыки
static VIZ_THREAD: OnceLock<std::thread::Thread> = OnceLock::new();

pub fn viz_wanted(on: bool) {
    VIZ_ON.store(on, Ordering::SeqCst);
    if on {
        if let Some(t) = VIZ_THREAD.get() {
            t.unpark();
        }
    }
}

pub const BANDS: usize = 24;
const N: usize = 1024;
const RATE: f32 = 44100.0;

fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ar, ai) = (re[i + k], im[i + k]);
                let (br, bi) = (re[i + k + len / 2] * cr - im[i + k + len / 2] * ci, re[i + k + len / 2] * ci + im[i + k + len / 2] * cr);
                re[i + k] = ar + br;
                im[i + k] = ai + bi;
                re[i + k + len / 2] = ar - br;
                im[i + k + len / 2] = ai - bi;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

fn viz_thread(tx: Sender<Msg>) {
    std::thread::Builder::new()
        .name("viz".into())
        .spawn(move || {
            let _ = VIZ_THREAD.set(std::thread::current());
            // границы полос: логарифмически от 40 Гц до 14 кГц
            let edges: Vec<usize> = (0..=BANDS).map(|i| ((40.0f32 * (14000.0f32 / 40.0).powf(i as f32 / BANDS as f32)) / RATE * N as f32).round() as usize).collect();
            let win: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (N - 1) as f32).cos()).collect();
            const CHUNK: usize = 512;
            let (mut ring, mut re, mut im) = (vec![0f32; N], vec![0f32; N], vec![0f32; N]);
            let mut buf = vec![0u8; CHUNK * 2];
            loop {
                if !VIZ_ON.load(Ordering::SeqCst) {
                    std::thread::park();
                    continue;
                }
                let Ok(mut c) = crate::proc::owned(
                    Command::new("parec")
                        .args(["--raw", "--format=s16le", "--rate=44100", "--channels=1", "--latency-msec=20", "-d", "@DEFAULT_MONITOR@"])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null()),
                )
                .spawn() else {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    continue;
                };
                let Some(mut out) = c.stdout.take() else {
                    let _ = c.kill();
                    let _ = c.wait();
                    continue;
                };
                ring.fill(0.0);
                let mut smooth = vec![0f32; BANDS];
                let mut bands = [0f32; BANDS];
                let mut peak = 1e-3f32;
                let mut last_send = std::time::Instant::now();
                while VIZ_ON.load(Ordering::SeqCst) {
                    if out.read_exact(&mut buf).is_err() {
                        break;
                    }
                    // кольцо: старое влево, новые отсчёты в хвост — без выделений на каждые 12 мс
                    ring.copy_within(CHUNK.., 0);
                    for (d, b) in ring[N - CHUNK..].iter_mut().zip(buf.chunks_exact(2)) {
                        *d = i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0;
                    }
                    // спектр — раз в кадр (~30 мс), а не на каждый кусок: втрое меньше работы
                    if last_send.elapsed().as_millis() < 30 {
                        continue;
                    }
                    last_send = std::time::Instant::now();
                    for i in 0..N {
                        re[i] = ring[i] * win[i];
                        im[i] = 0.0;
                    }
                    fft(&mut re, &mut im);
                    for b in 0..BANDS {
                        let (lo, hi) = (edges[b].max(1), edges[b + 1].max(edges[b].max(1) + 1).min(N / 2));
                        let mut m = 0f32;
                        for k in lo..hi {
                            m = m.max(re[k] * re[k] + im[k] * im[k]);
                        }
                        // верхние полосы тише — подтягиваем
                        bands[b] = m.sqrt() * (1.0 + b as f32 * 0.12);
                    }
                    let mx = bands.iter().copied().fold(0.0, f32::max);
                    peak = (peak * 0.987).max(mx).max(1e-3);
                    for b in 0..BANDS {
                        let v = (bands[b] / peak).sqrt().min(1.0);
                        // быстро вверх, медленно вниз — как стрелочные индикаторы
                        smooth[b] = if v > smooth[b] { v } else { smooth[b] * 0.6 + v * 0.4 };
                    }
                    let _ = tx.send(Msg::Viz(smooth.clone()));
                }
                let _ = c.kill();
                let _ = c.wait();
                let _ = tx.send(Msg::Viz(vec![]));
            }
        })
        .ok();
}

/// имена значка потока по порядку: свой application.icon_name, бинарник (и он же без -bin —
/// zen-bin это zen), имя программы, имя узла PipeWire. Не у всех есть первые: Spotify шлёт одно имя
fn icon_names(p: &serde_json::Value) -> Vec<String> {
    let mut v: Vec<String> = vec![];
    let mut add = |s: &str| {
        let s = s.trim().to_lowercase();
        if !s.is_empty() && !v.contains(&s) {
            v.push(s);
        }
    };
    for k in ["application.icon_name", "application.process.binary"] {
        if let Some(s) = p[k].as_str() {
            add(s);
            for suf in ["-bin", "-wrapped", ".bin"] {
                if let Some(base) = s.strip_suffix(suf) {
                    add(base);
                }
            }
        }
    }
    for k in ["application.name", "node.name"] {
        if let Some(s) = p[k].as_str() {
            add(s);
        }
    }
    v
}
