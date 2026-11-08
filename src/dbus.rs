// D-Bus: сервер уведомлений (org.freedesktop.Notifications) и трей (StatusNotifierWatcher +
// хост, меню через com.canonical.dbusmenu). Всё в своей нити; в шелл уходят готовые данные.

use crate::app::Msg;
use crate::gfx::Img;
use crate::menu::{MAct, MItem};
use calloop::channel::Sender;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedValue, Value};

static CONN: OnceLock<Connection> = OnceLock::new();
/// кто прислал уведомление (уникальное имя на шине): ответы, действия и закрытие уходят только
/// ему, а не всем на шине — набранный в уведомлении ответ не читает чужая программа
static SENDERS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());
/// потолки для текста от чужого процесса: история (до 500 карточек) не раздуется
const MAX_SUMMARY: usize = 512;
const MAX_BODY: usize = 8192;
const MAX_ACTIONS: usize = 8;

/// не длиннее max байт, по границе символа
fn cap(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut n = max;
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        s.truncate(n);
        s.push('…');
    }
    s
}

/// начало строки не длиннее max байт (до разбора разметки: мегабайты тегов не перебираем)
fn head(s: &str, max: usize) -> &str {
    let mut n = s.len().min(max);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    &s[..n]
}

fn sender_of(id: u32) -> Option<String> {
    SENDERS.lock().ok()?.iter().find(|(i, _)| *i == id).map(|(_, s)| s.clone())
}

/// сигнал программе, приславшей уведомление id; уведомления самого шелла (тосты, скриншоты) — никому
fn emit_to<B: serde::Serialize + zbus::zvariant::DynamicType>(id: u32, name: &str, body: &B) {
    let (Some(c), Some(dst)) = (CONN.get(), sender_of(id)) else { return };
    let _ = c.emit_signal(Some(dst.as_str()), "/org/freedesktop/Notifications", "org.freedesktop.Notifications", name, body);
}

// ── уведомления ─────────────────────────────────────────────────────────────
pub struct Incoming {
    pub id: u32,
    pub app: String,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub urgency: u8,
    pub image: Option<Img>,
    pub timeout: i32,
}

struct Notif {
    tx: Sender<Msg>,
    next: Arc<AtomicU32>,
}

fn strip_markup(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'")
}

fn image_data(v: &Value) -> Option<Img> {
    let v = match v {
        Value::Value(b) => b.as_ref(),
        x => x,
    };
    let Value::Structure(s) = v else { return None };
    let f = s.fields();
    let num = |i: usize| -> i32 {
        match f.get(i) {
            Some(Value::I32(x)) => *x,
            _ => 0,
        }
    };
    let (w, h, stride) = (num(0) as i64, num(1) as i64, num(2) as i64);
    let alpha = matches!(f.get(3), Some(Value::Bool(true)));
    let ch = num(5) as i64;
    // размеры от чужого процесса: без переполнений, строка не короче пикселей, не больше 1024²
    // (иначе «картинка» 10⁵×10⁵ при stride 0 потребовала бы десятки гигабайт)
    if !(3..=4).contains(&ch) || !(1..=1024).contains(&w) || !(1..=1024).contains(&h) || stride < w * ch {
        return None;
    }
    let Some(Value::Array(a)) = f.get(6) else { return None };
    if (a.len() as i64) < stride * (h - 1) + w * ch {
        return None;
    }
    let bytes: Vec<u8> = a.inner().iter().filter_map(|x| if let Value::U8(b) = x { Some(*b) } else { None }).collect();
    if (bytes.len() as i64) < stride * (h - 1) + w * ch {
        return None;
    }
    let mut img = image::RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let o = (y * stride + x * ch) as usize;
            let a = if alpha && ch == 4 { bytes[o + 3] } else { 255 };
            img.put_pixel(x as u32, y as u32, image::Rgba([bytes[o], bytes[o + 1], bytes[o + 2], a]));
        }
    }
    Some(crate::icons::fit(img, 48))
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notif {
    #[allow(clippy::too_many_arguments)]
    fn notify(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, app_name: &str, replaces_id: u32, app_icon: &str, summary: &str, body: &str, actions: Vec<String>, hints: HashMap<String, OwnedValue>, expire_timeout: i32) -> u32 {
        let from = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        // заменить можно только своё уведомление: чужой номер (другой программы или шелла) — новое
        let mine = replaces_id > 0 && sender_of(replaces_id).is_some_and(|s| s == from);
        let id = if mine { replaces_id } else { self.next.fetch_add(1, Ordering::SeqCst) };
        if let Ok(mut v) = SENDERS.lock() {
            v.retain(|(i, _)| *i != id);
            v.push((id, from));
            // история не длиннее 500: старше — не нужны
            if v.len() > 600 {
                v.drain(..100);
            }
        }
        let urgency = match hints.get("urgency").map(|v| &**v) {
            Some(Value::U8(u)) => *u,
            _ => 1,
        };
        let mut image = None;
        for k in ["image-data", "image_data", "icon_data"] {
            if let Some(v) = hints.get(k) {
                image = image_data(v);
                if image.is_some() {
                    break;
                }
            }
        }
        if image.is_none() {
            for k in ["image-path", "image_path"] {
                if let Some(Value::Str(p)) = hints.get(k).map(|v| &**v) {
                    let p = p.as_str().trim_start_matches("file://");
                    image = crate::icons::load_file(std::path::Path::new(p), 48);
                }
            }
        }
        let acts: Vec<(String, String)> = actions.chunks(2).filter(|c| c.len() == 2).take(MAX_ACTIONS).map(|c| (cap(c[0].clone(), 256), cap(c[1].clone(), 128))).collect();
        let _ = self.tx.send(Msg::Notif(Incoming {
            id,
            app: cap(app_name.to_string(), 128),
            icon: cap(app_icon.trim_start_matches("file://").to_string(), 1024),
            summary: cap(strip_markup(head(summary, MAX_SUMMARY * 4)), MAX_SUMMARY),
            body: cap(strip_markup(head(body, MAX_BODY * 4)), MAX_BODY),
            actions: acts,
            urgency,
            image,
            timeout: expire_timeout,
        }));
        id
    }
    /// закрыть можно только своё уведомление
    fn close_notification(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, id: u32) {
        if hdr.sender().is_some_and(|s| sender_of(id).is_some_and(|o| o == s.as_str())) {
            let _ = self.tx.send(Msg::NotifClose(id));
        }
    }
    fn get_capabilities(&self) -> Vec<String> {
        ["actions", "body", "body-markup", "icon-static", "persistence", "inline-reply"].iter().map(|s| s.to_string()).collect()
    }
    fn get_server_information(&self) -> (String, String, String, String) {
        ("yami-lite".into(), "YAMI".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }
}

/// уведомление закрыто (reason: 1 истекло, 2 закрыл человек, 3 закрыли через D-Bus)
pub fn notif_closed(id: u32, reason: u32) {
    emit_to(id, "NotificationClosed", &(id, reason));
    // истекло — карточка ушла в историю, её действия ещё нажимают; закрыта — забыть отправителя
    if reason != 1 {
        if let Ok(mut v) = SENDERS.lock() {
            v.retain(|(i, _)| *i != id);
        }
    }
}
/// ответ из окна ответа (inline-reply)
pub fn notif_replied(id: u32, text: &str) {
    emit_to(id, "NotificationReplied", &(id, text));
}
pub fn notif_action(id: u32, key: &str) {
    emit_to(id, "ActionInvoked", &(id, key));
}

// ── трей ────────────────────────────────────────────────────────────────────
pub struct TrayItem {
    pub service: String,
    pub bus: String,
    pub path: String,
    /// Id программы (nm-applet, steam…), подпись от неё и имя её процесса (discord, telegram-desktop)
    pub id: String,
    pub title: String,
    pub app: String,
    pub status: String,
    pub icon: String,
    /// файл значка программы: IconName путём или из её IconThemePath (найден в нити D-Bus)
    pub file: Option<String>,
    pub pixmap: Option<std::rc::Rc<Img>>,
    pub menu: String,
    pub is_menu: bool,
}

impl TrayItem {
    /// апплет сети (nm-applet и подобные): когда в баре есть модуль `net`, в трее он второй
    /// значок сети (#16)
    pub fn net_applet(&self) -> bool {
        const IDS: &[&str] = &["nm-applet", "nm-tray", "network-manager-applet", "iwgtk", "connman-gtk", "cmst"];
        IDS.contains(&self.id.to_ascii_lowercase().as_str()) || self.icon.starts_with("nm-")
    }
}

pub enum TrayEv {
    Upsert { service: String, bus: String, path: String, id: String, title: String, app: String, status: String, icon: String, file: Option<String>, pixmap: Option<Img>, menu: String, is_menu: bool },
    Remove(String),
}

pub fn apply(list: &mut Vec<TrayItem>, e: TrayEv) {
    match e {
        TrayEv::Remove(s) => list.retain(|i| i.service != s),
        TrayEv::Upsert { service, bus, path, id, title, app, status, icon, file, pixmap, menu, is_menu } => {
            let it = TrayItem { service: service.clone(), bus, path, id, title, app, status, icon, file, pixmap: pixmap.map(std::rc::Rc::new), menu, is_menu };
            if let Some(x) = list.iter_mut().find(|i| i.service == service) {
                *x = it;
            } else {
                list.push(it);
            }
        }
    }
}

/// зарегистрированный значок: key = «шина+путь» (так его зовут хосты), owner — уникальное имя
/// процесса. Сигналы значка (NewIcon, NewStatus…) приходят от уникального имени, а регистрируются
/// программы часто общеизвестным (org.kde.StatusNotifierItem-PID-N): без owner их смена значка
/// терялась, а у чужого наблюдателя — давала второй, дублирующий значок
#[derive(Clone)]
struct Reg {
    bus: String,
    path: String,
    owner: String,
}

impl Reg {
    /// ключ значка и в баре, и в RegisteredStatusNotifierItems: шина и путь
    fn key(&self) -> String {
        format!("{}{}", self.bus, self.path)
    }
}

type Regs = Arc<Mutex<Vec<Reg>>>;

/// добавить или обновить запись; false — потолок (64), значок не берём
fn reg_put(regs: &Regs, r: Reg) -> bool {
    let mut it = regs.lock().unwrap();
    if let Some(x) = it.iter_mut().find(|x| x.bus == r.bus && x.path == r.path) {
        *x = r;
        return true;
    }
    // потолок: процесс, регистрирующий значки в цикле, не забьёт бар и память
    if it.len() >= 64 {
        return false;
    }
    it.push(r);
    true
}

/// значок, о котором сказал чужой наблюдатель: записать и спросить его картинку
fn adopt(c: &Connection, items: &Regs, tx: &Sender<Msg>, service: &str) {
    let (b, pa) = split_service(service);
    if reg_put(items, Reg { owner: name_owner(c, &b), bus: b.clone(), path: pa.clone() }) {
        refresh_item(&b, &pa, tx);
    }
}

/// уникальное имя владельца шины (у уникального имени — оно само)
fn name_owner(c: &Connection, bus: &str) -> String {
    if bus.starts_with(':') {
        return bus.to_string();
    }
    c.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &(bus,))
        .ok()
        .and_then(|m| m.body().deserialize::<String>().ok())
        .unwrap_or_default()
}

struct Watcher {
    items: Regs,
    tx: Sender<Msg>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: &str, #[zbus(header)] hdr: zbus::message::Header<'_>) {
        let sender = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        let (bus, path) = if service.starts_with('/') { (sender.clone(), service.to_string()) } else { (service.to_string(), "/StatusNotifierItem".to_string()) };
        if !reg_put(&self.items, Reg { bus: bus.clone(), path: path.clone(), owner: sender }) {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || refresh_item(&bus, &path, &tx));
    }
    fn register_status_notifier_host(&self, _service: &str) {}
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().iter().map(Reg::key).collect()
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

fn split_service(s: &str) -> (String, String) {
    match s.find('/') {
        Some(i) => (s[..i].to_string(), s[i..].to_string()),
        None => (s.to_string(), "/StatusNotifierItem".to_string()),
    }
}

fn pixmap(v: Option<OwnedValue>) -> Option<Img> {
    let v = v?;
    let Value::Array(a) = &*v else { return None };
    // берём картинку ближе всего к 16 (но не меньше, если есть)
    let mut best: Option<(i32, i32, Vec<u8>)> = None;
    for e in a.inner() {
        let Value::Structure(s) = e else { continue };
        let f = s.fields();
        let (Some(Value::I32(w)), Some(Value::I32(h)), Some(Value::Array(d))) = (f.first(), f.get(1), f.get(2)) else { continue };
        let better = match &best {
            None => true,
            Some((bw, _, _)) => (*bw < 16 && *w > *bw) || (*w >= 16 && *w < *bw),
        };
        if better {
            best = Some((*w, *h, d.inner().iter().filter_map(|x| if let Value::U8(b) = x { Some(*b) } else { None }).collect()));
        }
    }
    let (w, h, d) = best?;
    // размеры от чужого процесса: не больше 1024² и без переполнения
    if !(1..=1024).contains(&w) || !(1..=1024).contains(&h) || d.len() < (w * h * 4) as usize {
        return None;
    }
    let mut img = image::RgbaImage::new(w as u32, h as u32);
    for (i, px) in d.chunks_exact(4).take((w * h) as usize).enumerate() {
        // ARGB32 в сетевом порядке
        img.put_pixel(i as u32 % w as u32, i as u32 / w as u32, image::Rgba([px[1], px[2], px[3], px[0]]));
    }
    Some(crate::icons::fit(img, 16))
}

fn refresh_item(bus: &str, path: &str, tx: &Sender<Msg>) {
    let Some(c) = CONN.get() else { return };
    let Ok(p) = Proxy::new(c, bus.to_string(), path.to_string(), "org.kde.StatusNotifierItem") else { return };
    // программа уже ушла или объекта нет (зарегистрировалась и не ответила): без значка-призрака
    let Ok(id) = p.get_property::<String>("Id") else { return };
    let s = |k: &str| p.get_property::<String>(k).unwrap_or_default();
    let menu = p.get_property::<zbus::zvariant::OwnedObjectPath>("Menu").map(|m| m.to_string()).unwrap_or_default();
    let status = s("Status");
    // программа зовёт: свой значок «внимания», если он есть (Discord — непрочитанные)
    let att = status == "NeedsAttention";
    let mut icon = if att { s("AttentionIconName") } else { String::new() };
    let mut pm = if att && icon.is_empty() { pixmap(p.get_property::<OwnedValue>("AttentionIconPixmap").ok()) } else { None };
    if icon.is_empty() && pm.is_none() {
        icon = s("IconName");
    }
    // файл из папки программы рисуется первым; без него — картинка: имени может не быть в теме
    // значков (Steam, Electron). Картинку (все размеры, побайтно) спрашиваем, только если файла нет
    let file = icon_file(&icon, &s("IconThemePath"));
    if pm.is_none() && file.is_none() {
        pm = pixmap(p.get_property::<OwnedValue>("IconPixmap").ok());
    }
    // ни значка, ни картинки, ни имени — показывать нечего
    if icon.is_empty() && pm.is_none() && file.is_none() && id.is_empty() && s("Title").is_empty() {
        return;
    }
    let _ = tx.send(Msg::Tray(TrayEv::Upsert {
        service: format!("{bus}{path}"),
        bus: bus.into(),
        path: path.into(),
        title: { let t = s("Title"); if t.is_empty() { id.clone() } else { t } },
        app: owner_name(c, bus),
        id,
        status,
        icon,
        file,
        pixmap: pm,
        menu,
        is_menu: p.get_property::<bool>("ItemIsMenu").unwrap_or(false),
    }));
}

/// имя процесса, которому принадлежит имя на шине: исполняемый файл (у Electron — каталог программы,
/// не «electron»), иначе comm
fn owner_name(c: &zbus::blocking::Connection, bus: &str) -> String {
    let Ok(d) = zbus::blocking::fdo::DBusProxy::new(c) else { return String::new() };
    let Ok(name) = zbus::names::BusName::try_from(bus) else { return String::new() };
    let Ok(pid) = d.get_connection_unix_process_id(name) else { return String::new() };
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    let base = exe.as_ref().and_then(|e| e.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if matches!(base.as_str(), "electron" | "python3" | "python" | "java" | "node" | "") {
        // у обёрток — каталог программы или comm
        let dir = exe.as_ref().and_then(|e| e.parent()).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string();
        return dir.filter(|d| !matches!(d.as_str(), "bin" | "electron")).unwrap_or(comm);
    }
    base
}

/// файл значка трея: IconName — путь к файлу, или имя в IconThemePath программы (прямо в
/// папке или в её теме: hicolor/22x22/apps/…). Из размеров — ближайший сверху к 16, потом svg
fn icon_file(name: &str, dir: &str) -> Option<String> {
    let ok = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.is_file()).unwrap_or(false);
    if name.starts_with('/') {
        return ok(std::path::Path::new(name)).then(|| name.to_string());
    }
    if name.is_empty() || dir.is_empty() || name.contains('/') {
        return None;
    }
    let (mut best, mut seen): (Option<(i32, std::path::PathBuf)>, usize) = (None, 0);
    let mut stack = vec![(std::path::PathBuf::from(dir), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            // чужая папка: глубина и число записей ограничены
            seen += 1;
            if seen > 4000 {
                break;
            }
            let p = e.path();
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if depth < 4 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            let (Some(stem), Some(ext)) = (p.file_stem().and_then(|x| x.to_str()), p.extension().and_then(|x| x.to_str())) else { continue };
            if stem != name || !matches!(ext, "png" | "svg") {
                continue;
            }
            // размер из пути: …/22x22/… ; без него — как средний
            let n = p.iter().find_map(|c| c.to_str()?.split_once('x')?.0.parse::<i32>().ok());
            let score = match (ext, n) {
                ("svg", _) => 70,
                (_, Some(n)) if n >= 16 => n,
                (_, Some(n)) => 1000 - n,
                _ => 80,
            };
            if best.as_ref().map(|b| score < b.0).unwrap_or(true) {
                best = Some((score, p));
            }
        }
    }
    best.and_then(|b| b.1.to_str().map(String::from))
}

/// ЛКМ / СКМ / колесо по значку трея
pub fn tray_call(bus: &str, path: &str, method: &'static str, x: i32, y: i32) {
    let (bus, path) = (bus.to_string(), path.to_string());
    std::thread::spawn(move || {
        let Some(c) = CONN.get() else { return };
        if let Ok(p) = Proxy::new(c, bus, path, "org.kde.StatusNotifierItem") {
            let _ = match method {
                "Scroll" => p.call_method("Scroll", &(x, "vertical")),
                m => p.call_method(m, &(x, y)),
            };
        }
    });
}

fn menu_item(v: &Value, svc: &str, mpath: &str) -> Option<MItem> {
    let v = match v {
        Value::Value(b) => b.as_ref(),
        x => x,
    };
    let Value::Structure(s) = v else { return None };
    let f = s.fields();
    let Some(Value::I32(id)) = f.first() else { return None };
    let mut props: HashMap<String, Value> = HashMap::new();
    if let Some(Value::Dict(d)) = f.get(1) {
        for (k, val) in d.iter() {
            if let Value::Str(k) = k {
                let val = match val {
                    Value::Value(b) => b.as_ref().try_clone().ok()?,
                    x => x.try_clone().ok()?,
                };
                props.insert(k.to_string(), val);
            }
        }
    }
    let st = |k: &str| match props.get(k) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    };
    let bl = |k: &str, d: bool| match props.get(k) {
        Some(Value::Bool(b)) => *b,
        _ => d,
    };
    if !bl("visible", true) {
        return None;
    }
    if st("type") == "separator" {
        return Some(MItem::sep());
    }
    let mut children = vec![];
    if let Some(Value::Array(a)) = f.get(2) {
        for c in a.inner() {
            if let Some(m) = menu_item(c, svc, mpath) {
                children.push(m);
            }
        }
    }
    let check = if st("toggle-type").is_empty() {
        None
    } else {
        Some(matches!(props.get("toggle-state"), Some(Value::I32(1))))
    };
    // «_Файл» → «Файл»
    let label = st("label").replace("__", "\u{1}").replace('_', "").replace('\u{1}', "_");
    Some(MItem {
        label,
        icon: st("icon-name"),
        check,
        enabled: bl("enabled", true),
        sep: false,
        danger: false,
        act: Some(MAct::Tray(svc.to_string(), mpath.to_string(), *id)),
        children,
    })
}

/// ПКМ по значку: dbusmenu → пиксельное меню
pub fn tray_menu(service: &str, bus: &str, menu: &str, tx: Sender<Msg>) {
    let (svc, bus, menu) = (service.to_string(), bus.to_string(), menu.to_string());
    std::thread::spawn(move || {
        let Some(c) = CONN.get() else { return };
        let Ok(p) = Proxy::new(c, bus.clone(), menu.clone(), "com.canonical.dbusmenu") else { return };
        let _ = p.call_method("AboutToShow", &(0i32,));
        let Ok(m) = p.call_method("GetLayout", &(0i32, -1i32, Vec::<String>::new())) else { return };
        let Ok((_rev, (_, _, kids))): Result<(u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)), _> = m.body().deserialize() else { return };
        let key = format!("{bus}|{menu}");
        let items: Vec<MItem> = kids.iter().filter_map(|k| menu_item(k, &key, &menu)).collect();
        let _ = tx.send(Msg::TrayMenu(svc, items));
    });
}

pub fn tray_menu_click(svc: &str, path: &str, id: i32) {
    let bus = svc.split('|').next().unwrap_or("").to_string();
    let path = path.to_string();
    std::thread::spawn(move || {
        let Some(c) = CONN.get() else { return };
        if let Ok(p) = Proxy::new(c, bus, path, "com.canonical.dbusmenu") {
            let ts = 0u32;
            let _ = p.call_method("Event", &(id, "clicked", Value::from(0i32), ts));
        }
    });
}

/// notif, tray: шелл — сервер уведомлений и хозяин трея (настройки → службы); выключенное
/// отдаётся другим (mako, dunst, waybar)
pub fn spawn(tx: Sender<Msg>, notif: bool, tray: bool) {
    std::thread::Builder::new()
        .name("dbus".into())
        .spawn(move || {
            let Ok(c) = Connection::session() else {
                eprintln!("{}", crate::tf!("dbus: нет сессионной шины", "dbus: no session bus"));
                return;
            };
            let _ = CONN.set(c.clone());
            use zbus::fdo::RequestNameFlags;
            if notif {
                let next = Arc::new(AtomicU32::new(1));
                let _ = c.object_server().at("/org/freedesktop/Notifications", Notif { tx: tx.clone(), next });
                let flags = RequestNameFlags::ReplaceExisting | RequestNameFlags::AllowReplacement;
                if let Err(e) = c.request_name_with_flags("org.freedesktop.Notifications", flags) {
                    eprintln!("{}", crate::tf!("dbus: уведомления заняты другим сервером: {e}", "dbus: notifications are owned by another server: {e}"));
                }
            }
            if !tray {
                // соединение живёт в CONN: меню трея и сигналы уведомлений ходят через него
                return;
            }
            // трей: свой наблюдатель, а если он уже есть — подключаемся к нему хостом
            let items: Regs = Arc::new(Mutex::new(Vec::new()));
            let _ = c.object_server().at("/StatusNotifierWatcher", Watcher { items: items.clone(), tx: tx.clone() });
            let own = c.request_name_with_flags("org.kde.StatusNotifierWatcher", RequestNameFlags::DoNotQueue.into()).is_ok();
            let host = format!("org.kde.StatusNotifierHost-{}", std::process::id());
            let _ = c.request_name(host.as_str());
            if !own {
                if let Ok(p) = Proxy::new(&c, "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher", "org.kde.StatusNotifierWatcher") {
                    let _ = p.call_method("RegisterStatusNotifierHost", &(host.as_str(),));
                    for s in p.get_property::<Vec<String>>("RegisteredStatusNotifierItems").unwrap_or_default() {
                        adopt(&c, &items, &tx, &s);
                    }
                }
            }
            // сигналы: значки меняются, программы уходят
            let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).build();
            let Ok(it) = MessageIterator::for_match_rule(rule, &c, Some(256)) else { return };
            for m in it {
                let Ok(m) = m else { continue };
                let h = m.header();
                let iface = h.interface().map(|i| i.to_string()).unwrap_or_default();
                let member = h.member().map(|i| i.to_string()).unwrap_or_default();
                let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
                let path = h.path().map(|p| p.to_string()).unwrap_or_default();
                match (iface.as_str(), member.as_str()) {
                    // подсказку трей не показывает: NewToolTip (часто, у анимированных) — мимо
                    ("org.kde.StatusNotifierItem", sig) if sig != "NewToolTip" => {
                        // значок ищем по уникальному имени отправителя и пути, обновляем под своим ключом
                        let r = items.lock().unwrap().iter().find(|r| r.owner == sender && r.path == path).cloned();
                        if let Some(r) = r {
                            refresh_item(&r.bus, &r.path, &tx);
                        }
                    }
                    ("org.kde.StatusNotifierWatcher", "StatusNotifierItemRegistered") if !own => {
                        if let Ok(s) = m.body().deserialize::<String>() {
                            adopt(&c, &items, &tx, &s);
                        }
                    }
                    ("org.kde.StatusNotifierWatcher", "StatusNotifierItemUnregistered") if !own => {
                        if let Ok(s) = m.body().deserialize::<String>() {
                            let (b, pa) = split_service(&s);
                            let key = format!("{b}{pa}");
                            items.lock().unwrap().retain(|r| r.key() != key);
                            let _ = tx.send(Msg::Tray(TrayEv::Remove(key)));
                        }
                    }
                    ("org.freedesktop.DBus", "NameOwnerChanged") => {
                        if let Ok((name, _old, new)) = m.body().deserialize::<(String, String, String)>() {
                            if new.is_empty() {
                                // ушёл процесс (уникальное имя) или его общеизвестное имя
                                let gone: Vec<String> = {
                                    let mut it = items.lock().unwrap();
                                    let g: Vec<String> = it.iter().filter(|r| r.owner == name || r.bus == name).map(Reg::key).collect();
                                    it.retain(|r| !g.contains(&r.key()));
                                    g
                                };
                                for k in gone {
                                    let _ = tx.send(Msg::Tray(TrayEv::Remove(k)));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .ok();
}
