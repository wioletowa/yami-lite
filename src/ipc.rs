// Команды шеллу: `yami-lite msg power` → сокет $XDG_RUNTIME_DIR/yami-lite.sock.
// Ими пользуются бинды niri. Второй экземпляр шелла сюда же и упирается — и выходит.
// Безопасность: сокет 0600, говорим только с процессами своего пользователя (SO_PEERCRED),
// строка не длиннее 4 КБ и ждём её не дольше 2 с — зависший клиент не заткнёт шелл.

use crate::app::{Modal, Msg, SKind};
use crate::config::Settings;
use calloop::channel::Sender;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub fn path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    PathBuf::from(dir).join("yami-lite.sock")
}

/// команда шеллу; true — шелл её принял (ответил). Ответа ждём не дольше 5 с: зависший шелл
/// не должен вешать терминал
pub fn send(cmd: &str) -> bool {
    match UnixStream::connect(path()) {
        Ok(mut s) => {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let _ = s.set_write_timeout(Some(std::time::Duration::from_secs(5)));
            if writeln!(s, "{cmd}").is_err() {
                return false;
            }
            let mut r = String::new();
            BufReader::new(s).read_line(&mut r).map(|n| n > 0).unwrap_or(false)
        }
        Err(_) => false,
    }
}

pub fn serve(tx: Sender<Msg>) {
    let p = path();
    if UnixStream::connect(&p).is_ok() {
        eprintln!("{}", crate::tf!("yami-lite уже запущен", "yami-lite is already running"));
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&p);
    let Ok(l) = UnixListener::bind(&p) else {
        eprintln!("{}", crate::tf!("ipc: не создать {}", "ipc: cannot create {}", p.display()));
        return;
    };
    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for s in l.incoming().flatten() {
                if !same_user(&s) {
                    continue;
                }
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new((&s).take(4096)).read_line(&mut line).is_err() {
                    continue;
                }
                let _ = s.set_read_timeout(None);
                let line = line.trim();
                if !line.is_empty() {
                    let _ = tx.send(Msg::Ipc(line.to_string()));
                }
                let _ = (&s).write_all(b"ok\n");
            }
        })
        .ok();
}

/// собеседник — процесс того же пользователя
fn same_user(s: &UnixStream) -> bool {
    let mut cred = libc::ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt пишет не больше len байт в cred
    let r = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len) };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}

impl crate::app::App {
    // ── команды (IPC) ───────────────────────────────────────────────────────
    pub fn ipc(&mut self, cmd: &str) {
        match cmd.trim() {
            "night" => self.night_toggle(),
            "power" => self.toggle_modal(Modal::Power),
            "settings" => self.toggle_settings(),
            "walls" => self.walls_open(false, false),
            // калькулятор: под баром посередине экрана; открыт — закрыть
            "calculator" => {
                let out = self.focus_out();
                let (ow, _) = self.output_size(&out);
                self.open_popup("calc", &out, ow / 2);
            }
            "lock" => self.lock_now(),
            // приватность окна: прицел niri, щелчок прячет приложение из трансляции (или возвращает)
            "hide" if crate::features::PRIVACY => self.hide_pick(),
            "bar" => self.toggle_bar(),
            // проверка без мыши: как будто курсор продержали у края над полноэкранным (peek.rs)
            "peek" => self.peek_edge(true),
            // проверка без мыши (nest.sh): щелчок в точку экрана — по верхней поверхности шелла там
            c if c.starts_with("click ") => {
                let v: Vec<i32> = c[6..].split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if let [x, y, ..] = v[..] {
                    let button = if v.get(2) == Some(&3) { crate::click::RMB } else { 0x110 };
                    self.test_click(x, y, button);
                }
            }
            // раскрыть или свернуть островок (бар островком)
            "island" if crate::island::on(&self.st.set) => {
                let out = self.focus_out();
                self.open_popup("island", &out, 0);
            }
            // сразу большой вид плитки: календарь, музыка, нагрузка
            c if c.starts_with("island ") && crate::island::on(&self.st.set) => {
                let Some(k) = crate::island::big_kind(c[7..].trim()) else { return };
                if !self.st.popup_is("island") {
                    let out = self.focus_out();
                    self.open_popup("island", &out, 0);
                }
                self.island_click(&crate::ui::Act::IslBig(k));
            }
            "edit" => {
                self.close_all();
                self.set_editing(!self.st.editing);
            }
            "reload" => {
                self.st.set = Settings::load();
                self.st.apps = crate::icons::load_apps();
                self.settings_changed();
            }
            "quit" => self.quit = true,
            // «жив ли шелл»: ответ сокета, ничего не делает
            "ping" => {}
            "restart" => self.restart = true,
            // медиаклавиши и бар: команда показанному плееру; player — другой плеер (#24)
            c if c.starts_with("media ") => match c[6..].trim() {
                "player" => self.media_step(1),
                a @ ("play-pause" | "play" | "pause" | "stop" | "next" | "previous") => self.media_ctl(a),
                _ => {}
            },
            c if c.starts_with("settings ") => self.open_settings_tab(c[9..].trim()),
            c if c.starts_with("popup ") => {
                let kind = crate::popups::KINDS.iter().copied().find(|k| *k == &c[6..]).unwrap_or("sys");
                // из терминала или хоткея: под правым краем бара
                let out = self.focus_out();
                let (ow, _) = self.output_size(&out);
                let side = crate::bar::geo(&self.st.set, ow).1;
                self.open_popup(kind, &out, ow - side - 260);
            }
            // свои обои: путь; без пути — пиксельные из палитры
            c if c == "wallpaper" || c.starts_with("wallpaper ") => self.set_wall(&c["wallpaper".len()..]),
            _ => {}
        }
    }

    /// щелчок в точку экрана (msg click): поверхность сверху (всплывашка, меню, бар), её зона
    fn test_click(&mut self, x: i32, y: i32, button: u32) {
        let out = self.focus_out();
        let order = [SKind::Menu, SKind::Popup, SKind::Bar, SKind::Catcher];
        let hit = order.iter().find_map(|k| {
            self.surfs.iter().filter(|(_, s)| &s.kind == k && !s.closing && s.output == out).find_map(|(id, s)| {
                let (lx, ly) = ((x - s.margin.0) as f64, (y - s.margin.1) as f64);
                (lx >= 0.0 && ly >= 0.0 && lx < s.w as f64 && ly < s.h as f64).then_some((*id, lx, ly))
            })
        });
        let Some((sid, lx, ly)) = hit else { return };
        // настоящий курсор сперва входит на поверхность — как если бы он там был
        if matches!(self.surf_kind(sid), Some(SKind::Bar | SKind::Popup | SKind::Menu)) {
            self.peek_bar(true);
        }
        self.ptr = Some((sid, lx, ly));
        if let Some(z) = self.zone_at(sid, lx, ly) {
            self.on_click(z.act.clone(), button, sid, z.r);
        }
    }

    /// настройки на вкладке (`yami-lite msg settings color`)
    pub fn open_settings_tab(&mut self, tab: &str) {
        let Some(t) = crate::settings::find(tab).map(|x| x.id) else { return };
        self.st.stab = t;
        if !self.st.settings_shown {
            self.open_settings();
        } else {
            self.tab_probe();
        }
        self.dirty_panels();
    }
}
