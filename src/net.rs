// Сеть и bluetooth для всплывашек: Wi-Fi через nmcli, bluetooth через BlueZ по системной
// шине (один запрос GetManagedObjects, дальше — по сигналам).

use crate::app::Msg;
use calloop::channel::Sender;
use std::collections::HashMap;
use std::process::Command;
use std::sync::OnceLock;
use zbus::blocking::{Connection, MessageIterator, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

#[derive(Clone, Default, Debug)]
pub struct WifiNet {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub active: bool,
    pub known: bool,
    /// SECURITY из nmcli: «WPA2», «WPA2 WPA3», «WPA2 802.1X», «WEP», «OWE»…
    pub sec: String,
    /// сохранённый профиль этой сети: uuid и key-mgmt (wpa-psk, sae, wpa-eap, none)
    pub uuid: String,
    pub mgmt: String,
}

#[derive(Clone, Default, Debug)]
pub struct NetState {
    pub loaded: bool,
    pub wifi_present: bool,
    pub wifi_on: bool,
    pub nets: Vec<WifiNet>,
    pub active: Vec<(String, String, String)>,
    pub busy: bool,
    pub msg: String,
    /// сети не хватило пароля: всплывашка открывает под ней поле
    pub ask: String,
}

/// nmcli всегда по-английски: разбираем его слова («enabled», «Secrets were required»),
/// а на русской системе он бы ответил по-русски
fn nm() -> Command {
    let mut c = Command::new("nmcli");
    c.env("LC_ALL", "C").env("LANG", "C");
    c
}

fn nmcli(args: &[&str]) -> String {
    nm().args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
}

/// разбор `nmcli -t`: двоеточия внутри полей экранированы обратной чертой
fn fields(l: &str) -> Vec<String> {
    let mut v = vec![String::new()];
    let mut esc = false;
    for c in l.chars() {
        if esc {
            v.last_mut().unwrap().push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == ':' {
            v.push(String::new());
        } else {
            v.last_mut().unwrap().push(c);
        }
    }
    v
}

/// сохранённые Wi-Fi профили: ssid → (uuid, key-mgmt). Имя профиля не обязано совпадать с SSID
/// («Дом 1», переименованный), поэтому SSID спрашиваем у самих профилей — одним вызовом
fn known_wifi() -> HashMap<String, (String, String)> {
    let uuids: Vec<String> = nmcli(&["-t", "-f", "UUID,TYPE", "connection", "show"]).lines().map(fields).filter(|f| f.get(1).is_some_and(|t| t == "802-11-wireless")).map(|f| f[0].clone()).collect();
    let mut out = HashMap::new();
    if uuids.is_empty() {
        return out;
    }
    let mut args: Vec<&str> = vec!["-t", "-f", "connection.uuid,802-11-wireless.ssid,802-11-wireless-security.key-mgmt", "connection", "show"];
    args.extend(uuids.iter().map(|u| u.as_str()));
    let (mut uuid, mut ssid, mut mgmt) = (String::new(), String::new(), String::new());
    let mut flush = |u: &mut String, s: &mut String, m: &mut String| {
        if !u.is_empty() && !s.is_empty() {
            out.entry(std::mem::take(s)).or_insert((std::mem::take(u), std::mem::take(m)));
        }
        u.clear();
        s.clear();
        m.clear();
    };
    for l in nmcli(&args).lines() {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.replace("\\:", ":").replace("\\\\", "\\");
        match k {
            "connection.uuid" => {
                flush(&mut uuid, &mut ssid, &mut mgmt);
                uuid = v;
            }
            "802-11-wireless.ssid" => ssid = v,
            "802-11-wireless-security.key-mgmt" => mgmt = v,
            _ => {}
        }
    }
    flush(&mut uuid, &mut ssid, &mut mgmt);
    out
}

pub fn query_wifi(rescan: bool) -> NetState {
    let mut s = NetState { loaded: true, ..Default::default() };
    s.wifi_on = nmcli(&["-t", "-f", "WIFI", "general"]).trim() == "enabled";
    s.wifi_present = nmcli(&["-t", "-f", "TYPE", "device"]).lines().any(|l| l == "wifi");
    for l in nmcli(&["-t", "-f", "NAME,TYPE,DEVICE", "connection", "show", "--active"]).lines() {
        let f = fields(l);
        if f.len() >= 3 && f[1] != "loopback" {
            let kind = match f[1].as_str() {
                "802-3-ethernet" => "кабель",
                "802-11-wireless" => "wi-fi",
                "wireguard" | "vpn" => "vpn",
                "bridge" => "мост",
                x => x,
            };
            s.active.push((f[0].clone(), kind.to_string(), f[2].clone()));
        }
    }
    if s.wifi_present && s.wifi_on {
        let known = known_wifi();
        let list = nmcli(&["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", if rescan { "yes" } else { "auto" }]);
        let mut seen: HashMap<String, usize> = HashMap::new();
        for l in list.lines() {
            let f = fields(l);
            if f.len() < 4 || f[1].is_empty() {
                continue;
            }
            let sec = if f[3] == "--" { String::new() } else { f[3].trim().to_string() };
            let (uuid, mgmt) = known.get(&f[1]).cloned().unwrap_or_default();
            let n = WifiNet { active: f[0] == "*", ssid: f[1].clone(), signal: f[2].parse().unwrap_or(0), secure: !sec.is_empty() && sec != "OWE", known: !uuid.is_empty(), sec, uuid, mgmt };
            if let Some(&i) = seen.get(&n.ssid) {
                if n.signal > s.nets[i].signal || n.active {
                    s.nets[i] = n;
                }
            } else {
                seen.insert(n.ssid.clone(), s.nets.len());
                s.nets.push(n);
            }
        }
        s.nets.sort_by(|a, b| b.active.cmp(&a.active).then(b.known.cmp(&a.known)).then(b.signal.cmp(&a.signal)));
        s.nets.truncate(12);
    }
    s
}

pub fn refresh_wifi(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Net(query_wifi(false)));
    });
}

/// короткая причина из stderr nmcli (последняя строка без «Error:»)
fn nm_err(e: &str) -> String {
    let l = e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("не вышло");
    let l = l.trim().trim_start_matches("Error:").trim();
    let l = l.rsplit_once("failed: ").map(|(_, r)| r).unwrap_or(l);
    l.chars().take(60).collect()
}

/// нехватка секрета: пароль не сохранён, сохранён у агента (KWallet живой сессии) или неверный
fn needs_secret(e: &str) -> bool {
    ["Secrets were required", "no-secrets", "No secrets", "802-11-wireless-security", "password"].iter().any(|k| e.contains(k))
}

/// радио Wi-Fi и «отключиться»: команды без секретов
pub fn wifi_cmd(args: Vec<String>, tx: Sender<Msg>, mut cur: NetState) {
    cur.busy = true;
    cur.msg = String::new();
    cur.ask.clear();
    let _ = tx.send(Msg::Net(cur));
    std::thread::spawn(move || {
        let o = nm().args(&args).output();
        let mut s = query_wifi(false);
        if let Ok(o) = o {
            if !o.status.success() {
                s.msg = nm_err(&String::from_utf8_lossy(&o.stderr));
            }
        }
        let _ = tx.send(Msg::Net(s));
    });
}

/// key-mgmt новой сети по её SECURITY: WPA1/2 (и переходный WPA2+WPA3) — wpa-psk, только WPA3 — sae
fn mgmt_of(sec: &str) -> &'static str {
    if sec.contains("802.1X") {
        "wpa-eap"
    } else if sec.contains("WPA1") || sec.contains("WPA2") {
        "wpa-psk"
    } else if sec.contains("WPA3") {
        "sae"
    } else if sec.contains("WEP") {
        "none"
    } else {
        ""
    }
}

/// пароль в профиль — по D-Bus (Settings.Connection.Update2), а не аргументом nmcli: командную
/// строку видят все процессы системы. Флаги 0 — пароль хранит NetworkManager, а не агент
/// (KWallet и подобные, которых в сеансе yami нет)
fn nm_set_secret(uuid: &str, wep: bool, pass: &str) -> Result<(), String> {
    let e = |x: zbus::Error| x.to_string();
    let c = system().ok_or("нет системной шины")?;
    let st = Proxy::new(&c, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager/Settings", "org.freedesktop.NetworkManager.Settings").map_err(e)?;
    let path: OwnedObjectPath = st.call("GetConnectionByUuid", &(uuid,)).map_err(e)?;
    let con = Proxy::new(&c, "org.freedesktop.NetworkManager", path, "org.freedesktop.NetworkManager.Settings.Connection").map_err(e)?;
    let mut set: HashMap<String, HashMap<String, OwnedValue>> = con.call("GetSettings", &()).map_err(e)?;
    let sec = set.entry("802-11-wireless-security".into()).or_default();
    let (key, flags) = if wep { ("wep-key0", "wep-key-flags") } else { ("psk", "psk-flags") };
    sec.insert(key.into(), OwnedValue::try_from(Value::from(pass)).map_err(|x| x.to_string())?);
    sec.insert(flags.into(), OwnedValue::from(0u32));
    if wep {
        // 1 — ключ (10/26 hex или 5/13 символов), не парольная фраза
        sec.insert("wep-key-type".into(), OwnedValue::from(1u32));
    }
    // 0x1 — сохранить на диск
    let _: HashMap<String, OwnedValue> = con.call("Update2", &(set, 1u32, HashMap::<String, Value>::new())).map_err(e)?;
    Ok(())
}

/// подключиться к сети. Известная — поднять её профиль (с новым паролем, если дали); новая открытая —
/// `device wifi connect`; новая с паролем — профиль без секрета, секрет по D-Bus, затем подъём.
/// Не хватило пароля — `ask` открывает поле под сетью; профиль, созданный этой попыткой, удаляется
pub fn wifi_connect(n: WifiNet, pass: Option<String>, tx: Sender<Msg>, mut cur: NetState) {
    cur.busy = true;
    cur.msg = "подключаюсь…".into();
    cur.ask.clear();
    let _ = tx.send(Msg::Net(cur));
    std::thread::spawn(move || {
        let done = |msg: String, ask: bool| {
            let mut s = query_wifi(false);
            s.msg = msg;
            if ask {
                s.ask = n.ssid.clone();
            }
            let _ = tx.send(Msg::Net(s));
        };
        let mgmt = if n.known { n.mgmt.clone() } else { mgmt_of(&n.sec).to_string() };
        if mgmt == "wpa-eap" {
            // логин и пароль, сертификаты — это редактору подключений
            return done("сеть с учётной записью: «все подключения»".into(), false);
        }
        if !n.known && (mgmt.is_empty() || !n.secure) {
            let o = nm().args(["-w", "40", "device", "wifi", "connect", &n.ssid]).output();
            return match o {
                Ok(o) if o.status.success() => done(String::new(), false),
                Ok(o) => done(nm_err(&String::from_utf8_lossy(&o.stderr)), false),
                Err(_) => done("nmcli не найден".into(), false),
            };
        }
        // WPA: 8–63 символа (или 64 hex); короче NetworkManager не примет — скажем сразу
        if let Some(p) = pass.as_ref().filter(|p| !p.is_empty()) {
            if mgmt != "none" && !(8..=64).contains(&p.chars().count()) {
                return done("пароль — от 8 до 63 символов".into(), true);
            }
        }
        let mut created = false;
        let uuid = if n.known {
            n.uuid.clone()
        } else {
            if pass.as_ref().is_none_or(|p| p.is_empty()) {
                return done("нужен пароль".into(), true);
            }
            let mut a = vec!["-t", "connection", "add", "type", "wifi", "con-name", &n.ssid, "ssid", &n.ssid, "wifi-sec.key-mgmt", &mgmt];
            a.extend(if mgmt == "none" { ["wifi-sec.wep-key-flags", "0"] } else { ["wifi-sec.psk-flags", "0"] });
            let out = nm().args(&a).output();
            let txt = out.as_ref().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
            // «Connection 'X' (uuid) successfully added.»
            let Some(u) = txt.rsplit_once('(').and_then(|(_, r)| r.split_once(')')).map(|(u, _)| u.to_string()) else {
                return done(out.map(|o| nm_err(&String::from_utf8_lossy(&o.stderr))).unwrap_or_else(|_| "nmcli не найден".into()), false);
            };
            created = true;
            u
        };
        if let Some(p) = pass.as_ref().filter(|p| !p.is_empty()) {
            if let Err(e) = nm_set_secret(&uuid, mgmt == "none", p) {
                if created {
                    let _ = nm().args(["connection", "delete", "uuid", &uuid]).output();
                }
                return done(e.chars().take(60).collect(), false);
            }
        }
        let o = nm().args(["-w", "40", "connection", "up", "uuid", &uuid]).output();
        match o {
            Ok(o) if o.status.success() => done(String::new(), false),
            Ok(o) => {
                let e = String::from_utf8_lossy(&o.stderr).to_string();
                if created {
                    let _ = nm().args(["connection", "delete", "uuid", &uuid]).output();
                }
                if needs_secret(&e) {
                    done(if pass.is_some() { "неверный пароль" } else { "нужен пароль" }.into(), true)
                } else {
                    done(nm_err(&e), false)
                }
            }
            Err(_) => done("nmcli не найден".into(), false),
        }
    });
}

// ── bluetooth ───────────────────────────────────────────────────────────────
#[derive(Clone, Default, Debug)]
pub struct BtDev {
    pub path: String,
    pub name: String,
    pub icon: String,
    pub connected: bool,
    pub paired: bool,
    /// доверенное: подключается само, без вопросов
    pub trusted: bool,
    pub battery: Option<u8>,
    /// сила сигнала, дБм — есть у тех, кого видно при поиске
    pub rssi: Option<i16>,
}

#[derive(Clone, Default, Debug)]
pub struct BtState {
    pub present: bool,
    pub powered: bool,
    pub discovering: bool,
    pub adapter: String,
    pub devices: Vec<BtDev>,
    /// что не вышло в последний раз (сопряжение, подключение)
    pub msg: String,
    /// устройство, с которым сейчас идёт сопряжение или подключение
    pub busy: String,
    /// состояние пришло от действия пользователя (а не от сигнала BlueZ): msg и busy — его
    pub op: bool,
}

static SYS: OnceLock<Connection> = OnceLock::new();

/// одно подключение к системной шине на весь шелл (bluez, polkit, профили питания)
pub fn system() -> Option<Connection> {
    static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = INIT.lock().ok()?;
    if let Some(c) = SYS.get() {
        return Some(c.clone());
    }
    let c = Connection::system().ok()?;
    let _ = SYS.set(c.clone());
    Some(c)
}

type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn query_bt() -> BtState {
    let mut s = BtState::default();
    let Some(c) = SYS.get() else { return s };
    let Ok(p) = Proxy::new(c, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager") else { return s };
    let Ok(m) = p.call_method("GetManagedObjects", &()) else { return s };
    let Ok(objs) = m.body().deserialize::<Objects>() else { return s };
    let b = |m: &HashMap<String, OwnedValue>, k: &str| matches!(m.get(k).map(|v| &**v), Some(Value::Bool(true)));
    let st = |m: &HashMap<String, OwnedValue>, k: &str| match m.get(k).map(|v| &**v) {
        Some(Value::Str(x)) => x.to_string(),
        _ => String::new(),
    };
    for (path, ifs) in &objs {
        if let Some(a) = ifs.get("org.bluez.Adapter1") {
            if s.adapter.is_empty() {
                s.present = true;
                s.adapter = path.to_string();
                s.powered = b(a, "Powered");
                s.discovering = b(a, "Discovering");
            }
        }
        if let Some(d) = ifs.get("org.bluez.Device1") {
            let name = { let n = st(d, "Alias"); if n.is_empty() { st(d, "Name") } else { n } };
            if name.is_empty() {
                continue;
            }
            let battery = ifs.get("org.bluez.Battery1").and_then(|bt| match bt.get("Percentage").map(|v| &**v) {
                Some(Value::U8(x)) => Some(*x),
                _ => None,
            });
            let rssi = match d.get("RSSI").map(|v| &**v) {
                Some(Value::I16(x)) => Some(*x),
                _ => None,
            };
            s.devices.push(BtDev { path: path.to_string(), name, icon: st(d, "Icon"), connected: b(d, "Connected"), paired: b(d, "Paired"), trusted: b(d, "Trusted"), battery, rssi });
        }
    }
    s.devices.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.paired.cmp(&a.paired)).then(a.name.cmp(&b.name)));
    s
}

pub fn refresh_bt(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Bt(query_bt()));
    });
}

pub fn spawn_bt(tx: Sender<Msg>) {
    std::thread::Builder::new()
        .name("bluez".into())
        .spawn(move || {
            let Some(c) = system() else { return };
            let _ = tx.send(Msg::Bt(query_bt()));
            let Ok(rule) = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).sender("org.bluez").map(|r| r.build()) else { return };
            let Ok(it) = MessageIterator::for_match_rule(rule, &c, Some(64)) else { return };
            let mut last = std::time::Instant::now();
            for m in it {
                if m.is_err() {
                    continue;
                }
                // пачки сигналов при поиске устройств — не чаще раза в 300 мс
                if last.elapsed().as_millis() < 300 {
                    continue;
                }
                last = std::time::Instant::now();
                let _ = tx.send(Msg::Bt(query_bt()));
            }
        })
        .ok();
}

pub fn bt_power(adapter: &str, on: bool, tx: Sender<Msg>) {
    let a = adapter.to_string();
    std::thread::spawn(move || {
        if let Some(c) = SYS.get() {
            if let Ok(p) = Proxy::new(c, "org.bluez", a, "org.freedesktop.DBus.Properties") {
                let _ = p.call_method("Set", &("org.bluez.Adapter1", "Powered", Value::from(on)));
            }
        }
        let _ = tx.send(Msg::Bt(BtState { op: true, ..query_bt() }));
    });
}

/// короткая причина ошибки BlueZ: org.bluez.Error.AuthenticationFailed → «AuthenticationFailed»
fn bt_err(e: &zbus::Error) -> String {
    let t = match e {
        zbus::Error::MethodError(name, desc, _) => desc.clone().filter(|d| !d.is_empty()).unwrap_or_else(|| name.rsplit('.').next().unwrap_or("").to_string()),
        x => x.to_string(),
    };
    t.chars().take(60).collect()
}

/// подключить или отключить. Новое устройство сначала сопрягается (Pair) и становится доверенным
/// (Trusted: потом подключается само, без вопросов), иначе Connect у новых наушников не выходит
pub fn bt_toggle_dev(d: BtDev, tx: Sender<Msg>, mut cur: BtState) {
    cur.busy = d.path.clone();
    cur.msg.clear();
    cur.op = true;
    let _ = tx.send(Msg::Bt(cur));
    std::thread::spawn(move || {
        let mut msg = String::new();
        if let Some(c) = SYS.get() {
            let run = || -> Result<(), zbus::Error> {
                let dev = Proxy::new(c, "org.bluez", d.path.as_str(), "org.bluez.Device1")?;
                if d.connected {
                    return dev.call_method("Disconnect", &()).map(|_| ());
                }
                if !d.paired {
                    dev.call_method("Pair", &())?;
                    let props = Proxy::new(c, "org.bluez", d.path.as_str(), "org.freedesktop.DBus.Properties")?;
                    let _ = props.call_method("Set", &("org.bluez.Device1", "Trusted", Value::from(true)));
                }
                dev.call_method("Connect", &()).map(|_| ())
            };
            if let Err(e) = run() {
                msg = bt_err(&e);
            }
        }
        let mut s = query_bt();
        s.msg = msg;
        s.op = true;
        let _ = tx.send(Msg::Bt(s));
    });
}

/// доверие: доверенное подключается само, без вопросов
pub fn bt_trust(d: BtDev, on: bool, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let mut msg = String::new();
        if let Some(c) = SYS.get() {
            let r = Proxy::new(c, "org.bluez", d.path.as_str(), "org.freedesktop.DBus.Properties").and_then(|p| p.call_method("Set", &("org.bluez.Device1", "Trusted", Value::from(on))).map(|_| ()));
            if let Err(e) = r {
                msg = bt_err(&e);
            }
        }
        let _ = tx.send(Msg::Bt(BtState { msg, op: true, ..query_bt() }));
    });
}

/// забыть устройство: сопряжение снимается, из списка уходит (Adapter1.RemoveDevice)
pub fn bt_remove(adapter: &str, d: BtDev, tx: Sender<Msg>) {
    let a = adapter.to_string();
    std::thread::spawn(move || {
        let mut msg = String::new();
        if let (Some(c), Ok(path)) = (SYS.get(), zbus::zvariant::ObjectPath::try_from(d.path.as_str())) {
            let r = Proxy::new(c, "org.bluez", a, "org.bluez.Adapter1").and_then(|p| p.call_method("RemoveDevice", &(path,)).map(|_| ()));
            if let Err(e) = r {
                msg = bt_err(&e);
            }
        }
        let _ = tx.send(Msg::Bt(BtState { msg, op: true, ..query_bt() }));
    });
}

pub fn bt_scan(adapter: &str, tx: Sender<Msg>) {
    let a = adapter.to_string();
    std::thread::spawn(move || {
        let Some(c) = SYS.get() else { return };
        let Ok(p) = Proxy::new(c, "org.bluez", a, "org.bluez.Adapter1") else { return };
        let _ = p.call_method("StartDiscovery", &());
        let _ = tx.send(Msg::Bt(BtState { op: true, ..query_bt() }));
        std::thread::sleep(std::time::Duration::from_secs(12));
        let _ = p.call_method("StopDiscovery", &());
        let _ = tx.send(Msg::Bt(query_bt()));
    });
}
