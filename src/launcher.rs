// Меню программ и буфер обмена: шапка, поиск, список, строка подсказок.
// Tab переключает режим. Частые программы — выше. В поиске ещё и действия системы
// («сон», «заблокировать», «настройки»…), открытые окна, места в настройках («vrr» → экраны)
// и калькулятор («2+2»). Префиксы: «=» считать, «>» команда в терминале, «/» файлы в домашней
// папке, «?» только настройки. В буфере: закреплённое сверху (хранится копией, переживает
// очистку и ротацию cliphist), Ctrl+P — закрепить, Del — удалить. Ctrl+A выделяет поиск,
// Ctrl+C копирует выделенное (или ответ калькулятора).

use crate::app::{App, Modal, Msg, SKind, State};
use crate::config;
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, R};
use crate::icons::{exec_argv, App as DApp};
use crate::niri::Win;
use crate::ui::{Act, Cur, Field, Tone, Ui};
use calloop::channel::Sender;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const SIZE: (i32, i32) = (648, 478);
const ROW: i32 = 30;
pub const ROWS: usize = 11;
/// миниатюра картинки из буфера в строке списка
const TW: i32 = 48;
const TH: i32 = 24;
/// больше картинок за одно открытие не декодируем
const MAX_THUMBS: usize = 60;

#[derive(Clone, Debug)]
pub enum Kind {
    App(usize),
    Clip(String),
    Pin(String),
    Sys(&'static str),
    /// ответ калькулятора (текст, который копируется)
    Calc(String),
    /// открытое окно niri
    Win(u64),
    /// вкладка настроек
    Setting(&'static str),
    /// команда в терминале
    Cmd(String),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub sub: String,
    pub icon: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Pin {
    pub id: String,
    pub file: String,
    pub mime: String,
    pub image: bool,
    pub name: String,
    pub ts: i64,
}

#[derive(Default)]
pub struct LState {
    pub field: Field,
    pub cur: usize,
    pub scroll: usize,
    pub clip: bool,
    pub clip_lines: Vec<String>,
    pub pins: Vec<Pin>,
    pub confirm_wipe: Option<Instant>,
    pub items: Vec<Item>,
    pub counts: HashMap<String, u32>,
    /// найденные файлы для «/»: запрос, на который они найдены, и сами пути
    pub files: (String, Vec<PathBuf>),
    /// номер поиска файлов: старые нити видят, что их запрос устарел, и бросают обход
    pub fgen: Arc<AtomicU64>,
    /// миниатюры картинок буфера (ключ — thumb_key; None — не картинка или не декодируется).
    /// Только в памяти и только пока лаунчер открыт: на диске картинка пережила бы `cliphist wipe`
    pub thumbs: HashMap<String, Option<Img>>,
    /// номер нити миниатюр: закрыли лаунчер — старая нить бросает работу
    pub tgen: Arc<AtomicU64>,
}

/// откуда взять картинку для миниатюры
enum Src {
    Line(String),
    File(PathBuf),
}

/// у картинки из cliphist в списке вместо текста «[[ binary data 23 KiB png 1920x1080 ]]»
fn is_binary(name: &str) -> bool {
    name.starts_with("[[ binary")
}

/// «[[ binary data 23 KiB png 1920x1080 ]]» → «png 1920x1080 · 23 KiB»
fn binary_label(name: &str) -> Option<String> {
    let inner = name.trim().strip_prefix("[[ binary data")?.strip_suffix("]]")?.trim();
    let mut it = inner.split_whitespace();
    let size = format!("{} {}", it.next()?, it.next()?);
    let rest = it.collect::<Vec<_>>().join(" ");
    Some(if rest.is_empty() { size } else { format!("{rest} · {size}") })
}

/// ключ миниатюры строки списка: у записи cliphist — её номер, у закреплённой — «p:» и id
pub fn thumb_key(l: &LState, k: &Kind) -> Option<String> {
    match k {
        Kind::Clip(line) if is_binary(line.split_once('\t').map(|x| x.1).unwrap_or(line)) => Some(line.split('\t').next().unwrap_or("").to_string()),
        Kind::Pin(id) if l.pins.iter().any(|p| p.id == *id && p.image) => Some(format!("p:{id}")),
        _ => None,
    }
}

const SYS: &[(&str, &str, &str)] = &[
    ("lock", "заблокировать", "lock"),
    ("sleep", "сон", "moon"),
    ("logout", "выйти из сеанса", "exit"),
    ("reboot", "перезагрузка", "reboot"),
    ("poweroff", "выключить", "power"),
    ("settings", "настройки шелла", "gear"),
    ("clipboard", "буфер обмена", "copy"),
    ("stacking", "режим стол / лента", "float"),
    ("edit", "расставить виджеты", "grid"),
];

fn clip_dir() -> std::path::PathBuf {
    config::data_dir().join("yami-clip")
}

/// папка закреплённого: только своя (0700) — там бывают пароли и ключи; старая папка с правами
/// по умолчанию закрывается при первой записи
fn clip_dir_private() -> std::path::PathBuf {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let d = clip_dir();
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&d);
    let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    d
}

fn score(hay: &str, q: &str) -> i32 {
    let hay = hay.to_lowercase();
    if q.is_empty() {
        return 1;
    }
    if hay.starts_with(q) {
        return 100;
    }
    if let Some(i) = hay.find(q) {
        let prev = hay[..i].chars().last();
        if matches!(prev, Some(' ') | Some('-') | Some('_') | Some('.')) {
            return 80;
        }
        return 60;
    }
    let qc: Vec<char> = q.chars().collect();
    let mut j = 0;
    for ch in hay.chars() {
        if j < qc.len() && ch == qc[j] {
            j += 1;
        }
    }
    if j == qc.len() {
        20
    } else {
        0
    }
}

impl LState {
    pub fn open(&mut self, clip: bool, apps: &[DApp], wins: &[Win]) {
        self.field.clear();
        self.cur = 0;
        self.scroll = 0;
        self.clip = clip;
        self.confirm_wipe = None;
        self.counts = std::fs::read_to_string(config::state_dir().join("launch.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        self.load_pins();
        self.files = (String::new(), vec![]);
        self.refresh(apps, wins);
    }
    pub fn set_mode(&mut self, clip: bool, apps: &[DApp], wins: &[Win]) {
        self.clip = clip;
        self.cur = 0;
        self.scroll = 0;
        self.refresh(apps, wins);
    }
    /// выбранная строка всегда в видимой части списка
    pub fn keep_visible(&mut self) {
        if self.cur < self.scroll {
            self.scroll = self.cur;
        }
        if self.cur >= self.scroll + ROWS {
            self.scroll = self.cur + 1 - ROWS;
        }
    }
    /// миниатюры картинок, которых ещё нет: одна нить по очереди, сначала закреплённые
    pub fn thumbs_start(&mut self, tx: Sender<Msg>) {
        let pins = self.pins.iter().filter(|p| p.image).map(|p| (format!("p:{}", p.id), Src::File(clip_dir().join(&p.file))));
        let lines = self.clip_lines.iter().filter_map(|l| thumb_key(self, &Kind::Clip(l.clone())).map(|k| (k, Src::Line(l.clone()))));
        let jobs: Vec<(String, Src)> = pins.chain(lines).filter(|(k, _)| !self.thumbs.contains_key(k)).take(MAX_THUMBS).collect();
        if jobs.is_empty() {
            return;
        }
        let my = self.tgen.fetch_add(1, Ordering::Relaxed) + 1;
        let gen = self.tgen.clone();
        let _ = std::thread::Builder::new().name("clip-thumbs".into()).spawn(move || {
            for (k, src) in jobs {
                if gen.load(Ordering::Relaxed) != my {
                    break;
                }
                let data = match &src {
                    Src::Line(l) => cliphist(&["decode"], l),
                    Src::File(p) => yami_px::paths::read_capped(p, crate::icons::MAX_FILE).unwrap_or_default(),
                };
                let img = mime_of(&data).0.starts_with("image/").then(|| crate::icons::decode(&data, crate::icons::MAX_DECODE)).flatten().map(|d| {
                    // «как обложка»: середина картинки в пропорциях клетки
                    let (x, y, cw, ch) = crate::wall::cover_rect(d.width(), d.height(), TW as u32, TH as u32);
                    crate::icons::from_rgba(&d.crop_imm(x, y, cw, ch).thumbnail_exact(TW as u32, TH as u32).to_rgba8())
                });
                drop(data);
                if tx.send(Msg::ClipThumb(k, img)).is_err() {
                    break;
                }
            }
            yami_px::trim_heap();
        });
    }
    /// лаунчер закрыт: миниатюры — из памяти, нить — бросить
    pub fn thumbs_free(&mut self) {
        self.tgen.fetch_add(1, Ordering::Relaxed);
        self.thumbs = HashMap::new();
    }

    pub fn load_pins(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let d = clip_dir();
        // папка от старых версий (права по умолчанию) закрывается и без новой записи
        if std::fs::metadata(&d).is_ok_and(|m| m.permissions().mode() & 0o077 != 0) {
            let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
        }
        self.pins = std::fs::read_to_string(d.join("index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    }

    pub fn refresh(&mut self, apps: &[DApp], wins: &[Win]) {
        let q = self.field.text.trim().to_lowercase();
        let mut out: Vec<Item> = vec![];
        if !self.clip {
            if let Some(items) = self.prefixed(&q) {
                self.items = items;
                self.cur = self.cur.min(self.items.len().saturating_sub(1));
                return;
            }
        }
        if self.clip {
            for p in &self.pins {
                if q.is_empty() || p.name.to_lowercase().contains(&q) {
                    out.push(Item { kind: Kind::Pin(p.id.clone()), name: p.name.clone(), sub: if p.image { "картинка".into() } else { String::new() }, icon: "pin".into() });
                }
            }
            let same = |n: &str| self.pins.iter().any(|p| p.name == n || (n.chars().count() > 40 && p.name.starts_with(&n.chars().take(40).collect::<String>())));
            for l in &self.clip_lines {
                let name = l.split_once('\t').map(|x| x.1).unwrap_or(l).split_whitespace().collect::<Vec<_>>().join(" ");
                if (!q.is_empty() && !name.to_lowercase().contains(&q)) || same(&name) {
                    continue;
                }
                let img = is_binary(&name);
                out.push(Item { kind: Kind::Clip(l.clone()), name, sub: if img { "картинка".into() } else { String::new() }, icon: String::new() });
                if out.len() > 120 {
                    break;
                }
            }
        } else {
            let mut scored: Vec<(f32, usize)> = vec![];
            for (i, a) in apps.iter().enumerate() {
                let s = (score(&a.name, &q) as f32)
                    .max(score(&a.generic, &q) as f32 * 0.8)
                    .max(score(&a.keywords, &q) as f32 * 0.6)
                    .max(score(&a.comment, &q) as f32 * 0.3)
                    .max(score(&a.id, &q) as f32 * 0.5);
                if s > 0.0 {
                    let used = *self.counts.get(&a.id).unwrap_or(&0) as f32;
                    scored.push((s + used.min(40.0) * if q.is_empty() { 1.0 } else { 0.25 }, i));
                }
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(apps[a.1].name.cmp(&apps[b.1].name)));
            if !q.is_empty() {
                if crate::calc::looks_like(&q) {
                    out.extend(calc_item(&q));
                }
                for (id, name, icon) in SYS {
                    if score(name, &q) >= 60 || score(crate::i18n::tr(name), &q) >= 60 {
                        out.push(Item { kind: Kind::Sys(id), name: name.to_string(), sub: "система".into(), icon: icon.to_string() });
                    }
                }
            }
            for (_, i) in scored.into_iter().take(80) {
                let a = &apps[i];
                out.push(Item { kind: Kind::App(i), name: a.name.clone(), sub: if a.generic.is_empty() { a.comment.clone() } else { a.generic.clone() }, icon: a.icon.clone() });
            }
            if !q.is_empty() {
                // открытые окна — по заголовку и программе; после программ: запуск чаще
                for w in wins.iter().filter(|w| score(&w.title, &q) >= 60 || score(&w.app_id, &q) >= 60).take(8) {
                    out.push(Item { kind: Kind::Win(w.id), name: w.title.clone(), sub: crate::tf!("окно · {}", "window · {}", "janela · {}", w.app_id), icon: w.app_id.clone() });
                }
                out.extend(settings_items(&q, 60).into_iter().take(3));
            }
        }
        self.items = out;
        self.cur = self.cur.min(self.items.len().saturating_sub(1));
    }
}

impl LState {
    /// строки для запроса с префиксом (= > / ?); None — префикса нет, обычный поиск
    fn prefixed(&self, q: &str) -> Option<Vec<Item>> {
        let mut c = q.chars();
        let p = c.next()?;
        let rest = c.as_str().trim();
        Some(match p {
            '=' => calc_item(q).into_iter().collect(),
            '>' if !rest.is_empty() => {
                // команду показываем как набрана (без нижнего регистра)
                let cmd = self.field.text.trim().trim_start_matches('>').trim().to_string();
                vec![Item { kind: Kind::Cmd(cmd.clone()), name: cmd, sub: "выполнить в терминале".into(), icon: "term".into() }]
            }
            '>' => vec![],
            '?' => settings_items(rest, 20),
            '/' => {
                if self.files.0 != rest {
                    return Some(vec![]);
                }
                self.files.1.iter().map(|f| {
                    let shown = yami_px::paths::pretty(f);
                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    Item { kind: Kind::File(f.clone()), name, sub: shown, icon: if f.is_dir() { "folder".into() } else { "copy".into() } }
                }).collect()
            }
            _ => return None,
        })
    }
}

fn calc_item(q: &str) -> Option<Item> {
    let v = crate::calc::eval(q)?;
    let s = crate::calc::format(v);
    Some(Item { kind: Kind::Calc(s.clone()), name: format!("= {s}"), sub: "Enter — скопировать".into(), icon: "calc".into() })
}

/// вкладки настроек, подходящие к запросу: по названию (на обоих языках) и словам вкладки
fn settings_items(q: &str, min: i32) -> Vec<Item> {
    let mut hits: Vec<(i32, Item)> = vec![];
    for tb in crate::settings::tabs() {
        let (id, name, icon, words) = (&tb.id, tb.name, tb.icon, tb.find);
        let s = if q.is_empty() {
            1
        } else {
            score(name, q).max(score(crate::i18n::tr(name), q)).max(words.split(' ').map(|w| score(w, q)).max().unwrap_or(0))
        };
        if s >= min {
            hits.push((s, Item { kind: Kind::Setting(id), name: crate::tf!("настройки → {}", "settings → {}", "configurações → {}", crate::i18n::tr(name)), sub: String::new(), icon: icon.to_string() }));
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().map(|h| h.1).collect()
}

/// поиск файлов по имени в домашней папке: обход без скрытых папок, не глубже 8 уровней и не
/// больше 60 тысяч записей; результат — Msg::LFiles. Новый запрос гасит старый обход.
pub fn find_files(tx: Sender<Msg>, gen: Arc<AtomicU64>, q: String) {
    let my = gen.fetch_add(1, Ordering::Relaxed) + 1;
    if q.is_empty() {
        let _ = tx.send(Msg::LFiles(q, vec![]));
        return;
    }
    std::thread::spawn(move || {
        // чуть подождать: пока набирают слово, обходить диск незачем
        std::thread::sleep(std::time::Duration::from_millis(120));
        let ql = q.to_lowercase();
        let mut found: Vec<(i32, PathBuf)> = vec![];
        let mut stack = vec![(config::home(), 0u32)];
        let mut seen = 0u32;
        while let Some((dir, depth)) = stack.pop() {
            if gen.load(Ordering::Relaxed) != my {
                return;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                seen += 1;
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let s = score(&name, &ql);
                if s >= 60 {
                    found.push((s, e.path()));
                }
                // ссылки на папки не обходим: петли и чужие диски
                if depth < 8 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push((e.path(), depth + 1));
                }
            }
            if seen > 60_000 || found.len() > 400 {
                break;
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.as_os_str().len().cmp(&b.1.as_os_str().len())));
        if gen.load(Ordering::Relaxed) == my {
            let _ = tx.send(Msg::LFiles(q, found.into_iter().take(60).map(|f| f.1).collect()));
        }
    });
}

/// команда в терминале (Terminal=true в .desktop и «>» в поиске)
pub fn in_term(argv: Vec<String>) -> Vec<String> {
    let t = term();
    let mut pre = vec![t.clone()];
    match t.as_str() {
        "kitty" => {}
        "wezterm" => pre.extend(["start".into(), "--".into()]),
        _ => pre.push("-e".into()),
    }
    pre.extend(argv);
    pre
}

pub fn load_clip(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let o = std::process::Command::new("cliphist").arg("list").output();
        let lines = o.map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default();
        let _ = tx.send(Msg::Clip(lines));
    });
}

fn mime_of(d: &[u8]) -> (&'static str, &'static str) {
    if d.starts_with(b"\x89PNG") {
        ("image/png", "png")
    } else if d.starts_with(&[0xff, 0xd8, 0xff]) {
        ("image/jpeg", "jpg")
    } else if d.starts_with(b"GIF8") {
        ("image/gif", "gif")
    } else if d.len() > 12 && &d[..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        ("image/webp", "webp")
    } else {
        ("text/plain;charset=utf-8", "txt")
    }
}

fn cliphist(args: &[&str], input: &str) -> Vec<u8> {
    use std::io::Write;
    let Ok(mut c) = std::process::Command::new("cliphist").args(args).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn() else { return vec![] };
    if let Some(mut i) = c.stdin.take() {
        let _ = i.write_all(input.as_bytes());
    }
    c.wait_with_output().map(|o| o.stdout).unwrap_or_default()
}

fn save_pins(p: &[Pin]) {
    let _ = config::write_atomic(&clip_dir_private().join("index.json"), &serde_json::to_string_pretty(p).unwrap_or_default());
}

fn wl_copy(data: Vec<u8>, mime: &str) {
    use std::io::Write;
    if let Ok(mut c) = std::process::Command::new("setsid").args(["-f", "wl-copy", "--type", mime]).stdin(std::process::Stdio::piped()).spawn() {
        if let Some(mut i) = c.stdin.take() {
            let _ = i.write_all(&data);
        }
        let _ = c.wait();
    }
}

impl App {
    fn lrefresh(&mut self) {
        self.st.launcher.refresh(&self.st.apps, &self.st.niri.wins);
        self.dirty(&SKind::Modal);
    }

    pub fn launcher_run(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::App(ai) => {
                if let Some(a) = self.st.apps.get(ai).cloned() {
                    *self.st.launcher.counts.entry(a.id.clone()).or_insert(0) += 1;
                    let _ = config::write_atomic(&config::state_dir().join("launch.json"), &serde_json::to_string(&self.st.launcher.counts).unwrap_or_default());
                    let mut argv = exec_argv(&a.exec);
                    if a.terminal {
                        argv = in_term(argv);
                    }
                    App::spawn(&argv);
                }
            }
            Kind::Clip(line) => {
                std::thread::spawn(move || {
                    let data = cliphist(&["decode"], &line);
                    let (mime, _) = mime_of(&data);
                    wl_copy(data, mime);
                });
            }
            Kind::Pin(id) => {
                if let Some(p) = self.st.launcher.pins.iter().find(|p| p.id == id) {
                    let path = clip_dir().join(&p.file);
                    let mime = p.mime.clone();
                    std::thread::spawn(move || {
                        if let Ok(d) = std::fs::read(path) {
                            wl_copy(d, &mime);
                        }
                    });
                }
            }
            Kind::Sys(a) => {
                self.close_modal();
                self.sys_action(a);
                return;
            }
            Kind::Calc(s) => {
                std::thread::spawn(move || wl_copy(s.into_bytes(), "text/plain;charset=utf-8"));
            }
            Kind::Win(id) => self.st.niri.act(serde_json::json!({"FocusWindow": {"id": id}})),
            Kind::Setting(tab) => {
                self.close_modal();
                self.toggle_modal(Modal::Settings, false);
                self.settings_click(&Act::STab(tab));
                self.dirty(&SKind::Modal);
                return;
            }
            Kind::Cmd(cmd) => {
                // окно не закрывается само: ответ команды виден, Enter закрывает
                let line = format!("{cmd}; printf '\\n[Enter]'; read _");
                App::spawn(&in_term(vec!["sh".into(), "-c".into(), line]));
            }
            Kind::File(p) => {
                App::spawn(&["xdg-open".into(), p.to_string_lossy().to_string()]);
            }
        }
        self.close_modal();
    }

    pub fn sys_action(&mut self, a: &str) {
        match a {
            "lock" => self.lock_now(),
            "sleep" => {
                self.lock_now();
                App::sh("sleep 0.6; systemctl suspend");
            }
            "logout" => self.st.niri.act(serde_json::json!({"Quit": {"skip_confirmation": true}})),
            "reboot" => App::spawn(&["systemctl".into(), "reboot".into()]),
            "poweroff" => App::spawn(&["systemctl".into(), "poweroff".into()]),
            "settings" => self.toggle_modal(Modal::Settings, false),
            "clipboard" => self.toggle_modal(Modal::Launcher, true),
            "stacking" => self.ipc("stacking"),
            "edit" => self.set_editing(true),
            _ => {}
        }
    }

    pub fn launcher_pin(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::Pin(id) => {
                let mut pins = self.st.launcher.pins.clone();
                if let Some(p) = pins.iter().find(|p| p.id == id) {
                    let _ = std::fs::remove_file(clip_dir().join(&p.file));
                }
                pins.retain(|p| p.id != id);
                save_pins(&pins);
            }
            Kind::Clip(line) => {
                let data = cliphist(&["decode"], &line);
                if data.is_empty() {
                    return;
                }
                let id = format!("{:016x}", yami_px::fnv(yami_px::FNV0, &data));
                let mut pins = self.st.launcher.pins.clone();
                if pins.iter().any(|p| p.id == id) {
                    return;
                }
                let (mime, ext) = mime_of(&data);
                let file = format!("{id}.{ext}");
                let _ = std::fs::write(clip_dir_private().join(&file), &data);
                let name = if ext == "txt" {
                    String::from_utf8_lossy(&data).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect()
                } else {
                    line.split_once('\t').map(|x| x.1.trim().to_string()).unwrap_or_default()
                };
                let ts = unsafe { libc::time(std::ptr::null_mut()) };
                pins.insert(0, Pin { id, file, mime: mime.into(), image: ext != "txt", name, ts });
                save_pins(&pins);
            }
            _ => return,
        }
        self.st.launcher.load_pins();
        self.st.launcher.thumbs_start(self.tx.clone());
        self.lrefresh();
    }

    pub fn launcher_del(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::Pin(_) => self.launcher_pin(i),
            Kind::Clip(line) => {
                cliphist(&["delete"], &line);
                self.st.launcher.clip_lines.retain(|l| *l != line);
                self.lrefresh();
            }
            _ => {}
        }
    }

    pub fn launcher_wipe(&mut self) {
        match self.st.launcher.confirm_wipe {
            Some(t) if t.elapsed().as_secs() < 3 => {
                self.st.launcher.confirm_wipe = None;
                cliphist(&["wipe"], "");
                self.st.launcher.clip_lines.clear();
                self.lrefresh();
            }
            _ => {
                self.st.launcher.confirm_wipe = Some(Instant::now());
                self.dirty(&SKind::Modal);
            }
        }
    }

    pub fn launcher_key(&mut self, e: &KeyEvent) {
        let l = &mut self.st.launcher;
        let n = l.items.len();
        match e.keysym {
            Keysym::Escape => self.close_modal(),
            Keysym::Return | Keysym::KP_Enter => {
                let c = l.cur;
                self.launcher_run(c);
            }
            Keysym::Down => l.cur = (l.cur + 1).min(n.saturating_sub(1)),
            Keysym::Up => l.cur = l.cur.saturating_sub(1),
            Keysym::Page_Down => l.cur = (l.cur + 10).min(n.saturating_sub(1)),
            Keysym::Page_Up => l.cur = l.cur.saturating_sub(10),
            Keysym::Tab | Keysym::ISO_Left_Tab => {
                let c = !l.clip;
                l.set_mode(c, &self.st.apps, &self.st.niri.wins);
                if c {
                    load_clip(self.tx.clone());
                }
            }
            Keysym::Delete if l.clip && l.field.cur == l.field.text.chars().count() => {
                let c = l.cur;
                self.launcher_del(c);
            }
            // Ctrl+P и Ctrl+C — в любой раскладке (keys::latin_letter)
            _ if self.mods.ctrl && l.clip && crate::keys::latin_letter(e) == Some('P') => {
                let c = l.cur;
                self.launcher_pin(c);
            }
            // Ctrl+C: выделенный поиск или ответ калькулятора
            _ if self.mods.ctrl && crate::keys::latin_letter(e) == Some('C') => {
                let text = l.field.selected().map(String::from).or_else(|| match l.items.get(l.cur).map(|i| &i.kind) {
                    Some(Kind::Calc(s)) => Some(s.clone()),
                    _ => None,
                });
                if let Some(t) = text {
                    std::thread::spawn(move || wl_copy(t.into_bytes(), "text/plain;charset=utf-8"));
                }
            }
            _ => {
                let mods = self.mods;
                if App::edit_field(&mut self.st.launcher.field, e, &mods) {
                    self.st.launcher.cur = 0;
                    self.st.launcher.scroll = 0;
                    let q = self.st.launcher.field.text.trim().to_string();
                    if !self.st.launcher.clip && q.starts_with('/') {
                        let rest = q[1..].trim().to_lowercase();
                        if rest != self.st.launcher.files.0 {
                            find_files(self.tx.clone(), self.st.launcher.fgen.clone(), rest);
                        }
                    }
                    self.st.launcher.refresh(&self.st.apps, &self.st.niri.wins);
                }
            }
        }
        self.st.launcher.keep_visible();
        self.dirty(&SKind::Modal);
    }
}

pub fn term() -> String {
    for t in ["kitty", "foot", "alacritty", "wezterm", "ghostty", "konsole", "xterm"] {
        if std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(t).exists())).unwrap_or(false) {
            return t.into();
        }
    }
    "xterm".into()
}


pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let l = &st.launcher;
    let fr = R::new(0, 0, w - 8, h - 8);
    ui.frame(fr, true);
    // шапка
    let head = R::new(1, 1, fr.w - 2, 30);
    ui.header(head, "", if l.clip { "буфер обмена" } else { "программы" }, &l.items.len().to_string());
    let mut bx = head.right() - 6;
    let mut btn = |ui: &mut Ui, label: &str, icon: &str, tone: Tone, sel: bool, act: Act| {
        let bw = if label.is_empty() { 22 } else { ui.w(PX, label) + 12 + if icon.is_empty() { 0 } else { 18 } };
        bx -= bw;
        ui.button(R::new(bx, head.y + 5, bw, 20), label, icon, tone, sel, true, act);
        bx -= 6;
    };
    btn(ui, "", "close", Tone::Danger, false, Act::CloseModal);
    btn(ui, "БУФЕР", "", Tone::Plain, l.clip, Act::LTab(true));
    btn(ui, "ПРОГРАММЫ", "", Tone::Plain, !l.clip, Act::LTab(false));
    if l.clip {
        let wiping = l.confirm_wipe.map(|t| t.elapsed().as_secs() < 3).unwrap_or(false);
        btn(ui, if wiping { "точно? ещё раз" } else { "очистить" }, "trash", Tone::Danger, false, Act::LWipe);
    }

    // поиск
    let fx = 14;
    let field = R::new(fx, head.bottom() + 13, fr.w - 28, 32);
    ui.field(field, &l.field, if l.clip { "ищи в буфере… Enter — скопировать" } else { "напиши… Enter — запустить" }, true, false, Act::Block);

    // список
    let list = R::new(fx, field.bottom() + 8, fr.w - 28, fr.h - field.bottom() - 8 - 34);
    let rows = (list.h / ROW).max(1) as usize;
    let scroll = l.scroll.min(l.items.len().saturating_sub(rows));
    ui.cv.push_clip(list);
    for (vi, it) in l.items.iter().enumerate().skip(scroll).take(rows + 1) {
        let r = R::new(list.x, list.y + (vi - scroll) as i32 * ROW, list.w, ROW);
        let cur = vi == l.cur;
        let hov = ui.hover(r);
        if cur || hov {
            ui.cv.pxbox(R::new(r.x, r.y, r.w, r.h - 2), &BoxStyle::new(if cur { t.selection } else { t.surface_hi }, if cur { t.red } else { t.border }).bevel(cur));
        }
        if cur {
            ui.cv.fill(R::new(r.x, r.y + 2, 2, r.h - 6), t.ember);
        }
        ui.zone(r, Act::LRun(vi), Cur::Pointer);
        let mut x = r.x + 10;
        let cy = r.y + (r.h - 2) / 2;
        match &it.kind {
            Kind::Pin(_) => {
                ui.cv.icon("pin", x, cy - 7, 2, t.red);
                x += 24;
            }
            Kind::Sys(_) | Kind::Calc(_) | Kind::Setting(_) | Kind::Cmd(_) | Kind::File(_) => {
                ui.cv.icon(&it.icon, x, cy - 7, 2, t.red);
                x += 26;
            }
            Kind::App(_) | Kind::Win(_) => {
                ui.app_icon(&it.icon, x, cy - 8, 16, 1.0);
                x += 26;
            }
            Kind::Clip(_) => {}
        }
        // картинка из буфера: миниатюра (пока не готова или не декодируется — значок)
        let thumb = l.clip.then(|| thumb_key(l, &it.kind)).flatten();
        if let Some(k) = &thumb {
            let b = R::new(x, cy - TH / 2 - 1, TW + 2, TH + 2);
            ui.cv.fill(b, t.border);
            match l.thumbs.get(k) {
                Some(Some(img)) => ui.cv.blit(img, b.x + 1, b.y + 1),
                _ => {
                    ui.cv.fill(R::new(b.x + 1, b.y + 1, TW, TH), t.surface);
                    ui.cv.icon("image", b.x + (b.w - 14) / 2, b.y + (b.h - 14) / 2, 2, t.text_faint);
                }
            }
            x += TW + 10;
        }
        let label = thumb.as_ref().and_then(|_| binary_label(&it.name));
        let name = label.as_deref().unwrap_or(&it.name);
        let maxw = if l.clip { list.w - (x - r.x) - 70 } else { list.w - 230 };
        // имена программ, окон, файлов и команды — как есть, без перевода интерфейса
        let raw = !matches!(it.kind, Kind::Sys(_));
        let nw = if raw { ui.f.width(PX, name) } else { ui.w(PX, name) }.min(maxw);
        let nc = if cur { t.text_hi } else { t.text };
        if raw {
            ui.cv.text_in(ui.f, PX, R::new(x, r.y, nw, r.h - 2), name, nc);
        } else {
            ui.text_in(PX, R::new(x, r.y, nw, r.h - 2), name, nc);
        }
        if !it.sub.is_empty() {
            let sx = x + nw + 10;
            let sr = R::new(sx, r.y, r.right() - sx - 16, r.h - 2);
            let sc = if cur { t.text } else { t.text_faint };
            if matches!(it.kind, Kind::App(_) | Kind::File(_)) {
                ui.cv.text_in(ui.f, PX, sr, &it.sub, sc);
            } else {
                ui.text_in(PX, sr, &it.sub, sc);
            }
        }
        // в буфере: закрепить и удалить на выбранной строке
        if l.clip && (cur || hov) {
            let pinned = matches!(it.kind, Kind::Pin(_));
            ui.button(R::new(r.right() - 54, r.y + 3, 24, 22), "", "pin", Tone::Ghost, pinned, true, Act::LPin(vi));
            ui.button(R::new(r.right() - 28, r.y + 3, 24, 22), "", "trash", Tone::Ghost, false, true, Act::LDel(vi));
        }
    }
    ui.cv.pop_clip();
    if l.items.is_empty() {
        let q = l.field.text.trim();
        let msg = if l.clip {
            "буфер пуст"
        } else if q.starts_with('/') && q.len() > 1 && l.files.0 != q[1..].trim().to_lowercase() {
            "ищу файлы…"
        } else if q == "/" {
            "/имя — файлы в домашней папке"
        } else if q == ">" {
            ">команда — выполнить в терминале"
        } else {
            "ничего не нашлось"
        };
        ui.text_center(PX, R::new(list.x, list.y + 20, list.w, 20), msg, t.text_faint);
    }

    // строка подсказок внизу
    let hint = if l.clip { "Enter — скопировать · Ctrl+P — закрепить · Del — удалить · Tab — программы" } else { "Enter — запустить · = считать · > команда · / файлы · ? настройки · Tab — буфер" };
    ui.text_in(PX, R::new(fx, fr.h - 30, fr.w - 28, 20), hint, t.text_faint);
}
