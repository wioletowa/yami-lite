// Свой агент polkit: окно пароля в стиле шелла вместо чужого Qt/GTK-диалога.
// Агент живёт на системной шине (org.freedesktop.PolicyKit1.AuthenticationAgent) для нашего
// сеанса; пароль проверяет polkit-agent-helper-1 (сокет /run/polkit/agent-helper.socket,
// а на старых polkit — setuid-помощник). Нить на каждый запрос — только пока он идёт.

use crate::app::{App, Modal, Msg, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Field, Tone, Ui};
use calloop::channel::Sender;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Instant;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};

pub const SIZE: (i32, i32) = (446, 248);
const PATH: &str = "/org/yami/PolkitAgent";
pub const SOCKET: &str = "/run/polkit/agent-helper.socket";
pub const HELPERS: &[&str] = &["/usr/lib/polkit-1/polkit-agent-helper-1", "/usr/libexec/polkit-agent-helper-1", "/usr/lib/policykit-1/polkit-agent-helper-1", "/usr/libexec/polkit-1/polkit-agent-helper-1"];

/// помощник polkit: сначала setuid-обёртка NixOS (/run/wrappers/bin — копия из PATH без setuid
/// пароль не проверит), потом пути дистрибутивов, потом PATH (#1)
pub fn helper() -> Option<std::path::PathBuf> {
    let wrap = std::path::Path::new("/run/wrappers/bin/polkit-agent-helper-1");
    if wrap.exists() {
        return Some(wrap.to_path_buf());
    }
    HELPERS.iter().map(std::path::PathBuf::from).find(|p| p.exists()).or_else(|| yami_px::paths::find_exe("polkit-agent-helper-1"))
}
const TRIES: u32 = 3;

/// ответы окна нитям запросов: cookie → пароль (None — отмена)
static REPLY: Mutex<Vec<(String, mpsc::Sender<Option<String>>)>> = Mutex::new(Vec::new());

pub enum Ev {
    Begin { cookie: String, message: String, action: String, user: String },
    Prompt { cookie: String, prompt: String, echo: bool },
    Info { cookie: String, text: String },
    Fail { cookie: String },
    End { cookie: String },
}

pub struct Req {
    pub cookie: String,
    pub message: String,
    pub action: String,
    pub user: String,
    pub prompt: String,
    pub echo: bool,
    pub info: String,
    pub fails: u32,
    /// пароль ушёл помощнику, ждём ответа
    pub busy: bool,
    pub field: Field,
    pub shake: Option<Instant>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.PolicyKit1.Error")]
enum PkError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Failed(String),
    Cancelled(String),
}

struct Agent {
    tx: Sender<Msg>,
}

fn user_name(uid: u32) -> Option<String> {
    unsafe {
        let pw = libc::getpwuid(uid);
        (!pw.is_null()).then(|| std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy().into_owned())
    }
}

/// за кого входить: сам пользователь, если он в списке, иначе первый (обычно root)
fn pick(ids: &[(String, HashMap<String, OwnedValue>)]) -> Option<String> {
    let me = unsafe { libc::getuid() };
    let uids: Vec<u32> = ids
        .iter()
        .filter(|(k, _)| k == "unix-user")
        .filter_map(|(_, d)| match d.get("uid").map(|v| &**v) {
            Some(Value::U32(u)) => Some(*u),
            Some(Value::I32(u)) => Some(*u as u32),
            _ => None,
        })
        .collect();
    let uid = if uids.contains(&me) { me } else { *uids.first()? };
    user_name(uid)
}

enum Out {
    Ok,
    Bad,
    Cancel,
}

/// один заход PAM через помощника: строки PAM_* туда-сюда до SUCCESS/FAILURE
fn session(user: &str, cookie: &str, tx: &Sender<Msg>, rx: &mpsc::Receiver<Option<String>>) -> Out {
    let (mut w, r): (Box<dyn Write>, Box<dyn std::io::Read>) = if let Ok(s) = std::os::unix::net::UnixStream::connect(SOCKET) {
        let Ok(s2) = s.try_clone() else { return Out::Bad };
        let mut w: Box<dyn Write> = Box::new(s);
        if writeln!(w, "{user}\n{cookie}").is_err() {
            return Out::Bad;
        }
        (w, Box::new(s2))
    } else {
        let Some(h) = helper() else { return Out::Bad };
        let Ok(mut ch) = std::process::Command::new(h).arg(user).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn() else { return Out::Bad };
        let (Some(mut i), Some(o)) = (ch.stdin.take(), ch.stdout.take()) else { return Out::Bad };
        if writeln!(i, "{cookie}").is_err() {
            return Out::Bad;
        }
        // помощник сам выйдет, когда закроем ему вход; ждём его в фоне, чтобы не было зомби
        std::thread::spawn(move || {
            let _ = ch.wait();
        });
        (Box::new(i), Box::new(o))
    };
    let send = |e: Ev| {
        let _ = tx.send(Msg::Auth(e));
    };
    for line in BufReader::new(r).lines() {
        let Ok(line) = line else { break };
        let line = line.trim_end();
        let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim().to_string();
        match head {
            "PAM_PROMPT_ECHO_OFF" | "PAM_PROMPT_ECHO_ON" => {
                send(Ev::Prompt { cookie: cookie.into(), prompt: rest, echo: head.ends_with("ON") });
                match rx.recv() {
                    Ok(Some(mut p)) => {
                        let ok = writeln!(w, "{p}").and_then(|_| w.flush()).is_ok();
                        yami_px::ui::wipe(&mut p);
                        if !ok {
                            return Out::Bad;
                        }
                    }
                    _ => return Out::Cancel,
                }
            }
            "PAM_ERROR_MSG" | "PAM_TEXT_INFO" => send(Ev::Info { cookie: cookie.into(), text: rest }),
            "SUCCESS" => return Out::Ok,
            "FAILURE" => return Out::Bad,
            _ => {}
        }
    }
    Out::Bad
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, #[zbus(connection)] conn: &zbus::Connection, action_id: String, message: String, _icon_name: String, _details: HashMap<String, String>, cookie: String, identities: Vec<(String, HashMap<String, OwnedValue>)>) -> Result<(), PkError> {
        // окно пароля открывает только сам polkitd: чужой процесс на шине не нарисует «фишинг»
        if !from_polkit(&hdr, conn).await {
            return Err(PkError::Failed("not polkitd".into()));
        }
        let Some(user) = pick(&identities) else { return Err(PkError::Failed("no unix-user identity".into())) };
        let (ptx, prx) = mpsc::channel();
        REPLY.lock().unwrap().push((cookie.clone(), ptx));
        let (done_tx, done_rx) = async_channel::bounded::<Result<(), PkError>>(1);
        let tx = self.tx.clone();
        let _ = tx.send(Msg::Auth(Ev::Begin { cookie: cookie.clone(), message, action: action_id, user: user.clone() }));
        std::thread::spawn(move || {
            let mut res = Err(PkError::Failed("authentication failed".into()));
            for _ in 0..TRIES {
                match session(&user, &cookie, &tx, &prx) {
                    Out::Ok => {
                        res = Ok(());
                        break;
                    }
                    Out::Bad => {
                        let _ = tx.send(Msg::Auth(Ev::Fail { cookie: cookie.clone() }));
                    }
                    Out::Cancel => {
                        res = Err(PkError::Cancelled("cancelled".into()));
                        break;
                    }
                }
            }
            REPLY.lock().unwrap().retain(|(c, _)| *c != cookie);
            let _ = tx.send(Msg::Auth(Ev::End { cookie }));
            let _ = done_tx.send_blocking(res);
        });
        done_rx.recv().await.unwrap_or(Err(PkError::Cancelled("agent stopped".into())))
    }

    async fn cancel_authentication(&self, #[zbus(header)] hdr: zbus::message::Header<'_>, #[zbus(connection)] conn: &zbus::Connection, cookie: String) {
        if from_polkit(&hdr, conn).await {
            reply(&cookie, None);
        }
    }
}

/// вызов пришёл от владельца имени org.freedesktop.PolicyKit1
async fn from_polkit(hdr: &zbus::message::Header<'_>, conn: &zbus::Connection) -> bool {
    let Some(sender) = hdr.sender() else { return false };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(conn).await else { return false };
    let Ok(name) = zbus::names::BusName::try_from("org.freedesktop.PolicyKit1") else { return false };
    dbus.get_name_owner(name).await.map(|o| o.as_str() == sender.as_str()).unwrap_or(false)
}

/// пароль (или None — отмена) нити запроса
pub fn reply(cookie: &str, pass: Option<String>) {
    if let Some((_, s)) = REPLY.lock().unwrap().iter().find(|(c, _)| c == cookie) {
        let _ = s.send(pass);
    }
}

fn session_id(c: &Connection) -> Option<String> {
    if let Ok(id) = std::env::var("XDG_SESSION_ID") {
        if !id.is_empty() {
            return Some(id);
        }
    }
    // шелл запущен из niri.service, вне области сеанса: берём графический сеанс пользователя
    let p = zbus::blocking::Proxy::new(c, "org.freedesktop.login1", "/org/freedesktop/login1/user/self", "org.freedesktop.login1.User").ok()?;
    p.get_property::<(String, zbus::zvariant::OwnedObjectPath)>("Display").ok().map(|d| d.0).filter(|s| !s.is_empty())
}

/// зарегистрироваться агентом сеанса; если агент уже есть (чужой) — уступаем ему.
/// Выключен в настройках (службы → пароли) — окна пароля рисует другой агент (polkit-gnome, KDE)
pub fn spawn(tx: Sender<Msg>, on: bool) {
    if !on {
        return;
    }
    std::thread::Builder::new()
        .name("polkit".into())
        .spawn(move || {
            let Some(c) = crate::net::system() else { return };
            let Some(sid) = session_id(&c) else { return };
            if c.object_server().at(PATH, Agent { tx }).is_err() {
                return;
            }
            let mut d: HashMap<&str, Value> = HashMap::new();
            d.insert("session-id", Value::from(sid.as_str()));
            let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
            let r = c.call_method(Some("org.freedesktop.PolicyKit1"), "/org/freedesktop/PolicyKit1/Authority", Some("org.freedesktop.PolicyKit1.Authority"), "RegisterAuthenticationAgent", &(("unix-session", d), locale.as_str(), PATH));
            match r {
                Ok(_) => {}
                Err(e) => eprintln!("{}", crate::tf!("polkit: агент не зарегистрирован: {e}", "polkit: agent not registered: {e}", "polkit: agente não registrado: {e}")),
            }
        })
        .ok();
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let Some(r) = st.auth.first() else { return };
    let mut fr = R::new(0, 0, w - 6, h - 6);
    if let Some(s) = r.shake {
        let e = s.elapsed().as_millis() as i32;
        if e < 420 {
            fr.x += [6, -6, 4, -4, 2, -2, 0][(e / 60) as usize % 7];
            ui.anim = true;
        }
    }
    ui.frame(fr, true);
    let more = if st.auth.len() > 1 { crate::tf!("ещё {}", "{} more", "mais {}", st.auth.len() - 1) } else { String::new() };
    ui.header(R::new(fr.x + 1, 1, fr.w - 2, 30), "lock", "нужен пароль", &more);
    let x = fr.x + 16;
    let iw = fr.w - 32;
    // сообщение polkit уже на языке сеанса: не переводим
    ui.cv.text_wrap(ui.f, PX, R::new(x, 42, iw, 40), &r.message, t.text, 2);
    ui.cv.text_in(ui.f, PX, R::new(x, 84, iw, 18), &r.action, t.text_faint);
    ui.cv.icon("user", x, 112, 2, t.red);
    ui.cv.text_in(ui.f, PX, R::new(x + 22, 108, iw - 22, 20), &r.user, t.text);
    let fl = R::new(x, 134, iw, 32);
    ui.cv.pxbox(R::new(fl.x - 4, fl.y - 4, fl.w + 8, fl.h + 8), &BoxStyle::new(t.glass_panel(), t.border));
    let ph = if r.prompt.is_empty() || r.prompt.to_lowercase().starts_with("password") { "пароль" } else { r.prompt.as_str() };
    ui.field(fl, &r.field, ph, !r.busy, !r.echo, Act::AuthField);
    let (status, sc) = if r.busy {
        ("проверяю…".to_string(), t.text_faint)
    } else if !r.info.is_empty() {
        (r.info.clone(), t.ember)
    } else if r.fails > 0 {
        (crate::tf!("неверно ×{}", "wrong ×{}", "incorreta ×{}", r.fails), t.ember)
    } else {
        ("enter — подтвердить".to_string(), t.text_faint)
    };
    ui.text_in(PX, R::new(x, 176, iw, 20), &status, sc);
    let by = fr.h - 40;
    ui.button(R::new(fr.x + fr.w - 16 - 220, by, 104, 28), "отмена", "close", Tone::Plain, false, true, Act::AuthCancel);
    ui.button(R::new(fr.x + fr.w - 16 - 104, by, 104, 28), "ок", "check", Tone::Primary, false, !r.busy, Act::AuthOk);
    ui.zone(fl, Act::AuthField, Cur::Text);
}

impl App {
    pub fn on_auth(&mut self, e: Ev) {
        match e {
            Ev::Begin { cookie, message, action, user } => {
                self.st.auth.push(Req { cookie, message, action, user, prompt: String::new(), echo: false, info: String::new(), fails: 0, busy: true, field: Field::default(), shake: None });
                if self.st.auth.len() == 1 {
                    self.auth_show();
                }
            }
            Ev::Prompt { cookie, prompt, echo } => {
                if let Some(r) = self.st.auth.iter_mut().find(|r| r.cookie == cookie) {
                    (r.prompt, r.echo, r.busy) = (prompt, echo, false);
                }
            }
            Ev::Info { cookie, text } => {
                if let Some(r) = self.st.auth.iter_mut().find(|r| r.cookie == cookie) {
                    r.info = text;
                }
            }
            Ev::Fail { cookie } => {
                if let Some(r) = self.st.auth.iter_mut().find(|r| r.cookie == cookie) {
                    r.fails += 1;
                    r.field.clear();
                    r.info.clear();
                    r.shake = Some(Instant::now());
                }
            }
            Ev::End { cookie } => {
                let first = self.st.auth.first().map(|r| r.cookie == cookie).unwrap_or(false);
                self.st.auth.retain(|r| r.cookie != cookie);
                if first && self.st.modal == Some(Modal::Auth) {
                    if self.st.auth.is_empty() {
                        self.close_modal();
                    }
                } else if first && !self.st.auth.is_empty() {
                    self.auth_show();
                }
            }
        }
        self.dirty(&SKind::Modal);
    }

    /// окно поверх всего (после разблокировки экрана, если он заперт)
    pub fn auth_show(&mut self) {
        if self.st.auth.is_empty() || self.st.locked || self.st.modal == Some(Modal::Auth) {
            return;
        }
        self.close_all();
        let out = self.focus_out();
        self.st.modal = Some(Modal::Auth);
        self.st.field_focus = Some("auth");
        self.mk(SKind::Dim, &out, Layer::Overlay, "yami-dim", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
        self.mk(SKind::Modal, &out, Layer::Overlay, "yami-auth", Anchor::empty(), (SIZE.0, SIZE.1 + 12), (0, 0, 0, 0), -1, KeyboardInteractivity::Exclusive);
    }

    /// окно закрыли (Esc, клик мимо, «отмена»): отказ текущему запросу
    pub fn auth_cancel(&mut self) {
        if let Some(r) = self.st.auth.first() {
            reply(&r.cookie, None);
        }
    }

    pub fn auth_submit(&mut self) {
        let Some(r) = self.st.auth.first_mut() else { return };
        if r.busy {
            return;
        }
        r.busy = true;
        r.info.clear();
        let p = r.field.text.clone();
        r.field.wipe();
        reply(&r.cookie, Some(p));
        self.dirty(&SKind::Modal);
    }

    pub fn auth_key(&mut self, e: &KeyEvent) {
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter => self.auth_submit(),
            Keysym::Escape => self.auth_cancel(),
            _ => {
                let mods = self.mods;
                if let Some(r) = self.st.auth.first_mut() {
                    if !r.busy {
                        App::edit_field(&mut r.field, e, &mods);
                    }
                }
            }
        }
        self.dirty(&SKind::Modal);
    }
}
