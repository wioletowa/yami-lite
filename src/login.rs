// Экран входа (yami-greet) в виде шелла: шелл выкладывает в /var/lib/yami-greet/look то, из
// чего тот рисуется, — look.json (палитра и тона, язык, проявление, 闇, шрифт, раскладки,
// мониторы из yami-outputs.kdl, настройки экрана входа greet*) и wall.png (обои: как на столе,
// пиксельные из палитры или никаких — greetWall). Папку отдаёт пользователю установщик; нет её или
// она не наша — ничего не пишется. Экран входа проверяет всё сам: для него это чужие данные.
// Здесь же вкладка «экран входа» и `yami login` (sync, status, log, preview). Как прошёл его
// последний запуск, greet-session пишет в /var/lib/yami-greet/log (читать может каждый).

use crate::app::{App, State};
use crate::config::Settings;
use crate::settings::{opt_btn, title};
use crate::ui::{Act, Ui};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use yami_px::font::PX;
use yami_px::gfx::{Img, R};
use yami_px::ui::Tone;

/// ключи настроек, которые читает экран входа (yami_px::look::Look::from_json, data::Opts, kdl.rs)
const KEYS: [&str; 22] = [
    "palette", "accent", "tint", "glass", "lift", "inner", "light", "lang", "reveal", "revealGrain", "animations", "animSpeed", "lite", "wallKanji", "fontShell", "greetUsers", "greetDim", "greetClock",
    "greetHost", "greetPower", "greetOff", "greetNumlock",
];

/// куда ставит установщик (tools/greet-install.sh)
const GREET: &str = "/usr/local/bin/yami-greet";
const LOG_DIR: &str = "/var/lib/yami-greet/log";

/// папка вида экрана входа; YAMI_GREET_LOOK — для проверок без root
pub fn dir() -> PathBuf {
    std::env::var_os("YAMI_GREET_LOOK").map(PathBuf::from).unwrap_or_else(|| "/var/lib/yami-greet/look".into())
}

/// экран входа установлен и папка вида — наша
pub fn writable() -> bool {
    let Ok(c) = std::ffi::CString::new(dir().to_string_lossy().as_bytes()) else { return false };
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// yami-greet: установленный установщиком, а у сборки пакетного менеджера (Nix) — рядом с шеллом
pub fn greet_bin() -> Option<PathBuf> {
    if Path::new(GREET).exists() {
        return Some(GREET.into());
    }
    let near = yami_px::paths::self_exe().with_file_name("yami-greet");
    (!crate::update::ENABLED && near.is_file()).then_some(near)
}

pub fn installed() -> bool {
    greet_bin().is_some()
}

pub fn look(set: &Settings) -> String {
    let v = serde_json::to_value(set).unwrap_or_default();
    let mut o = serde_json::Map::new();
    o.insert("_".into(), "written by yami-shell for the login screen (yami-greet)".into());
    for k in KEYS {
        if let Some(x) = v.get(k) {
            o.insert(k.into(), x.clone());
        }
    }
    o.insert("kbLayout".into(), crate::keys::layouts(set).into());
    o.insert("kbSwitch".into(), set.kb_switch.clone().into());
    let outs = std::fs::read_to_string(crate::config::config_dir().join("niri/yami-outputs.kdl")).unwrap_or_default();
    o.insert("outputs".into(), outs.chars().take(64 * 1024).collect::<String>().into());
    serde_json::to_string_pretty(&serde_json::Value::Object(o)).unwrap_or_default() + "\n"
}

static LAST: Mutex<String> = Mutex::new(String::new());

/// look.json, если вид изменился (настройки, палитра, мониторы, раскладки)
pub fn publish(set: &Settings) {
    let text = look(set);
    let Ok(mut last) = LAST.lock() else { return };
    if *last == text || !writable() {
        return;
    }
    let p = dir().join("look.json");
    if std::fs::read_to_string(&p).ok().as_deref() != Some(text.as_str()) {
        let _ = crate::config::write_atomic(&p, &text);
    }
    readable(&p);
    *last = text;
}

/// экран входа работает от своего пользователя: файл — на чтение всем, какой бы ни была umask
fn readable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o644)).is_ok()
}

fn key_file() -> PathBuf {
    crate::config::state_dir().join("login-wall.key")
}

/// чем рисовать фон экрана входа: desktop — как на столе, pixel — пиксельные из палитры, plain — цвет темы
pub fn wall_mode(s: &Settings) -> &'static str {
    match s.greet_wall.as_str() {
        "pixel" => "pixel",
        "plain" => "plain",
        _ => "desktop",
    }
}

/// экрану входа нужны эти обои: он есть, а картинку с таким ключом (wall::Job::key: всё, от чего
/// она зависит) ему ещё не писали
pub fn wanted(key: &str) -> bool {
    writable() && std::fs::read_to_string(key_file()).map(|k| k != key).unwrap_or(true)
}

/// обои экрану входа (из нити подготовки обоев): та же картинка, что на рабочем столе
pub fn wall(img: &Img, key: &str) {
    let kf = key_file();
    let rgb: Vec<u8> = img.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
    let Some(buf) = image::RgbImage::from_raw(img.w as u32, img.h as u32, rgb) else { return };
    let (tmp, dst) = (dir().join(".wall.png.tmp"), dir().join("wall.png"));
    if buf.save_with_format(&tmp, image::ImageFormat::Png).is_ok() && readable(&tmp) && std::fs::rename(&tmp, &dst).is_ok() {
        let _ = crate::config::write_atomic(&kf, key);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// обои экрана входа не как на столе (из нити): Some(job) — пиксельные из палитры, None — никаких
/// (фон темы под вуалью; экран входа не держит картинку вовсе)
pub fn wall_other(job: Option<crate::wall::Job>) {
    match job {
        Some(j) => {
            let key = format!("pixel|{}", j.key());
            if wanted(&key) {
                if let (Some(img), _) = crate::wall::prepare(&j) {
                    wall(&img, &key);
                }
            }
        }
        None if wanted("plain") => {
            let _ = std::fs::remove_file(dir().join("wall.png"));
            let _ = crate::config::write_atomic(&key_file(), "plain");
        }
        None => {}
    }
}

/// что известно об установленном экране входа (вкладка открылась — App::tab_probe)
#[derive(Default, Clone)]
pub struct Probe {
    pub installed: bool,
    pub look: bool,
    /// greetd — менеджер входа (или неизвестно: не systemd/OpenRC/runit)
    pub dm: Option<bool>,
    /// последняя строка greet-session: «2026-11-07 10:59:43 ok 412»
    pub last: String,
}

pub fn probe() -> Probe {
    Probe { installed: installed(), look: writable(), dm: dm(), last: last_result() }
}

/// greetd включён как менеджер входа: systemd (display-manager.service), OpenRC (default), runit
pub fn dm() -> Option<bool> {
    if let Ok(p) = std::fs::read_link("/etc/systemd/system/display-manager.service") {
        return Some(p.to_string_lossy().contains("greetd"));
    }
    if Path::new("/run/systemd/system").is_dir() {
        return Some(false);
    }
    if Path::new("/etc/runlevels/default").is_dir() {
        return Some(Path::new("/etc/runlevels/default/greetd").exists());
    }
    for d in ["/var/service", "/etc/runit/runsvdir/default", "/run/runit/service"] {
        if Path::new(d).is_dir() {
            return Some(Path::new(d).join("greetd").exists());
        }
    }
    None
}

pub fn last_result() -> String {
    yami_px::paths::read_capped(&Path::new(LOG_DIR).join("last"), 4096).map(|b| String::from_utf8_lossy(&b).lines().next().unwrap_or("").trim().to_string()).unwrap_or_default()
}

/// «2026-11-07 10:59:43 ok 412» → (время, итог по-людски, хорошо ли)
fn describe(last: &str) -> (String, String, bool) {
    let mut w = last.splitn(4, ' ');
    let (d, t, what, rest) = (w.next().unwrap_or(""), w.next().unwrap_or(""), w.next().unwrap_or(""), w.next().unwrap_or("").trim());
    let when = format!("{d} {}", t.get(..5).unwrap_or(t));
    match what {
        "ok" => (when, crate::tf!("вход, первый кадр через {} мс", "logged in, first frame after {} ms", "login feito, primeiro quadro após {} ms", rest), true),
        "text" => (when, crate::tf!("текстовый вход: {}", "text login: {}", "login em texto: {}", rest), false),
        "fail" => (when, crate::tf!("не поднялся: {}", "did not come up: {}", "não subiu: {}", rest), false),
        "closed" => (when, crate::tf!("закрылся без входа: {}", "closed without a login: {}", "fechada sem login: {}", rest), true),
        _ => (when, last.to_string(), true),
    }
}

/// переключатели вкладки
pub fn toggle(s: &mut Settings, k: &str) -> bool {
    let v = match k {
        "greet_clock" => &mut s.greet_clock,
        "greet_host" => &mut s.greet_host,
        "greet_power" => &mut s.greet_power,
        "greet_numlock" => &mut s.greet_numlock,
        _ => return false,
    };
    *v = !*v;
    true
}

const WALLS: &[(&str, &str)] = &[("desktop", "как на столе"), ("pixel", "пиксельные из палитры"), ("plain", "цвет темы")];

/// settings → система → экран входа
pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let p = &st.login_probe;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = title(ui, b.x, b.y, b.w, "экран входа: greetd + yami-greet");
    if !p.installed {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "не установлен: ./install.sh --login или tools/greet-install.sh", t.warn);
        y += 22;
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "настройки ниже применятся, когда он появится", t.text_faint);
        y += 30;
    } else {
        let (dm, dc) = match p.dm {
            Some(true) => ("greetd — менеджер входа", t.text_dim),
            Some(false) => ("greetd не включён: tools/greet-install.sh", t.warn),
            None => ("стоит; служба greetd — в твоей системе инициализации", t.text_dim),
        };
        ui.text_in(PX, R::new(b.x, y, b.w, 20), dm, dc);
        y += 22;
        if p.last.is_empty() {
            ui.text_in(PX, R::new(b.x, y, b.w, 20), "ещё не запускался с этой версии", t.text_faint);
        } else {
            let (when, what, good) = describe(&p.last);
            let ww = ui.cv.text_in(ui.f, PX, R::new(b.x, y, 160, 20), &when, t.text_faint);
            ui.cv.text_in(ui.f, PX, R::new(b.x + ww + 12, y, b.w - ww - 12, 20), &(ui.tr)(&what).to_string(), if good { t.text_dim } else { t.ember });
        }
        y += 22;
        if !p.look {
            ui.text_in(PX, R::new(b.x, y, b.w, 20), "папка вида не твоя: экран входа не идёт за шеллом", t.warn);
            y += 22;
        }
        y += 8;
    }
    y = title(ui, b.x, y, b.w, "вид: палитра, язык и мониторы — как у шелла");
    let wall = WALLS.iter().find(|w| w.0 == wall_mode(s)).map(|w| w.1).unwrap_or(WALLS[0].1);
    opt_btn(ui, R::new(lx, y + 7, half, 26), "обои", wall, Act::SCycle("greet_wall"));
    ui.slider(R::new(rx, y, half, 40), "вуаль над обоями", (s.greet_dim - 0.2) / 0.7, &format!("{}%", (s.greet_dim * 100.0).round()), 14, Act::SSlider("greet_dim", 0.2, 0.9, 14));
    y += 48;
    ui.toggle(R::new(lx, y, half, 40), "часы и дата", "большие, как на экране блокировки", s.greet_clock, Act::SBool("greet_clock"));
    ui.toggle(R::new(rx, y, half, 40), "имя машины", "в левом верхнем углу", s.greet_host, Act::SBool("greet_host"));
    y += 52;
    y = title(ui, b.x, y, b.w, "кто и как входит");
    let users = if s.greet_users == "name" { "ввод имени" } else { "списком" };
    opt_btn(ui, R::new(lx, y + 7, half, 26), "пользователи", users, Act::SCycle("greet_users"));
    ui.toggle(R::new(rx, y, half, 40), "кнопки питания", "сон, перезагрузка, выключение", s.greet_power, Act::SBool("greet_power"));
    y += 48;
    let off = if s.greet_off == 0 { "никогда".to_string() } else { crate::tf!("{} мин", "{} min", "{} min", s.greet_off) };
    ui.slider(R::new(lx, y, half, 40), "погасить мониторы через", s.greet_off as f32 / 30.0, &off, 15, Act::SSlider("greet_off", 0.0, 30.0, 15));
    ui.toggle(R::new(rx, y, half, 40), "numlock", "цифры на цифровом блоке сразу", s.greet_numlock, Act::SBool("greet_numlock"));
    y += 52;
    y = title(ui, b.x, y, b.w, "проверить");
    let bw = (b.w - 12) / 3;
    ui.button(R::new(b.x, y, bw, 28), "посмотреть", "eye", Tone::Plain, false, p.installed, Act::SCycle("login_preview"));
    ui.button(R::new(b.x + bw + 6, y, bw, 28), "отдать вид сейчас", "reboot", Tone::Plain, false, p.look, Act::SCycle("login_sync"));
    ui.button(R::new(b.x + 2 * (bw + 6), y, bw, 28), "журнал", "text", Tone::Plain, false, p.installed, Act::SCycle("login_log"));
    y += 34;
    ui.text_in(PX, R::new(b.x, y, b.w, 20), "«посмотреть» — экран входа поверх стола: пароль yami или esc", t.text_faint);
}

impl App {
    pub fn login_cycle(&mut self, k: &str) -> bool {
        let s = &mut self.st.set;
        match k {
            "greet_wall" => s.greet_wall = crate::settings::cycle(WALLS, wall_mode(s)).into(),
            "greet_users" => s.greet_users = if s.greet_users == "name" { "list" } else { "name" }.into(),
            "login_sync" => {
                std::thread::spawn(|| {
                    let _ = std::process::Command::new(yami_px::paths::self_exe()).args(["login", "sync"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
                });
                return true;
            }
            "login_preview" => {
                // вид отдаётся сразу: «посмотреть» показывает то, что сейчас в настройках
                publish(&self.st.set);
                self.close_modal();
                App::spawn(&[greet_bin().unwrap_or_else(|| GREET.into()).display().to_string(), "--demo".into()]);
                return true;
            }
            "login_log" => {
                let log = Path::new(LOG_DIR).join("greet.log");
                if log.exists() {
                    self.close_modal();
                    App::spawn(&["xdg-open".into(), log.display().to_string()]);
                }
                return true;
            }
            _ => return false,
        }
        self.settings_changed();
        true
    }
}

/// `yami login [sync|status|log|preview]`
pub fn cli(a: &[&str]) -> i32 {
    match a.first().copied() {
        None | Some("sync") => sync(),
        Some("status") => status(),
        Some("log") => {
            let p = Path::new(LOG_DIR).join("greet.log");
            match yami_px::paths::read_capped(&p, 1 << 20) {
                Some(b) => {
                    print!("{}", String::from_utf8_lossy(&b));
                    0
                }
                None => {
                    eprintln!("{}", crate::tf!("журнала нет: {}", "no log: {}", "sem log: {}", p.display()));
                    1
                }
            }
        }
        Some("preview") => {
            if !installed() {
                eprintln!("{}", crate::tf!("экран входа yami не установлен", "the yami login screen is not installed", "a tela de login do yami não está instalada"));
                return 1;
            }
            publish(&Settings::load());
            std::process::Command::new(greet_bin().unwrap_or_else(|| GREET.into())).arg("--demo").status().map(|s| s.code().unwrap_or(1)).unwrap_or(1)
        }
        _ => {
            eprintln!("yami login [sync|status|log|preview]");
            2
        }
    }
}

fn status() -> i32 {
    let p = probe();
    let yes = |b: bool| if b { "✓" } else { "·" };
    println!("{} yami-greet   {}", yes(p.installed), greet_bin().map(|p| p.display().to_string()).unwrap_or_else(|| "—".into()));
    let dm = match p.dm {
        Some(true) => crate::tf!("greetd — менеджер входа", "greetd is the login manager", "o greetd é o gerenciador de login"),
        Some(false) => crate::tf!("greetd не включён", "greetd is not enabled", "o greetd não está ativado"),
        None => crate::tf!("служба greetd — в твоей системе инициализации", "greetd: see your init system", "greetd: veja o seu sistema de init"),
    };
    println!("{} greetd       {dm}", yes(p.dm != Some(false)));
    println!("{} {}", yes(p.look), dir().display());
    if p.last.is_empty() {
        println!("· {}", crate::tf!("запусков ещё не было", "no start yet", "nenhuma inicialização ainda"));
    } else {
        let (when, what, good) = describe(&p.last);
        println!("{} {when}  {what}", yes(good));
    }
    println!("  {}", crate::tf!("журнал: yami login log ({}/greet.log)", "log: yami login log ({}/greet.log)", "log: yami login log ({}/greet.log)", LOG_DIR));
    i32::from(!p.installed)
}

/// выложить вид сейчас (установщик — до первого входа; шелл делает это сам при каждой смене вида).
/// Обои — под размер первого монитора niri
fn sync() -> i32 {
    if !writable() {
        eprintln!("{}", crate::tf!("экран входа yami не установлен: {} не открыта на запись", "the yami login screen is not installed: {} is not writable", "a tela de login do yami não está instalada: {} não é gravável", dir().display()));
        return 1;
    }
    let set = Settings::load();
    publish(&set);
    let (w, h) = crate::niri::request(&serde_json::json!("Outputs"))
        .and_then(|v| {
            let o = v["Ok"]["Outputs"].as_object()?.values().find_map(|o| o["logical"].as_object().cloned())?;
            Some((o.get("width")?.as_i64()? as i32, o.get("height")?.as_i64()? as i32))
        })
        .unwrap_or((1920, 1080));
    let t = yami_px::theme::Theme::new(set.accent_rgb(), set.tint, set.glass, set.lift, set.inner, set.light).apply();
    match wall_mode(&set) {
        "desktop" => {
            let job = crate::wall::Job::new(&set, &t, w, h, crate::wall::own(&set));
            let key = job.key();
            if let (Some(img), _) = crate::wall::prepare(&job) {
                wall(&img, &key);
            }
        }
        "pixel" => wall_other(Some(crate::wall::Job::new(&set, &t, w, h, None))),
        _ => wall_other(None),
    }
    println!("{}", dir().display());
    0
}
