// Командная строка: `yami-lite` без команды запускает шелл (так его зовёт niri),
// `yami-lite <команда>` — всё остальное. Справка — в цветах палитры, на языке шелла; NO_COLOR —
// без цвета. Команды для настроек — cli.rs.
// i18n: inline pairs

use crate::config::Settings;
use std::io::IsTerminal;

/// раскраска терминала: акцент палитры (truecolor), тусклый, жирный, итоги проверок
pub struct Sty {
    on: bool,
    acc: (u8, u8, u8),
}

impl Sty {
    pub fn new(s: &Settings) -> Sty {
        let on = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);
        let a = s.accent_rgb();
        Sty { on, acc: ((a >> 16) as u8, (a >> 8) as u8, a as u8) }
    }
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn acc(&self, s: &str) -> String {
        self.wrap(&format!("38;2;{};{};{}", self.acc.0, self.acc.1, self.acc.2), s)
    }
    pub fn head(&self, s: &str) -> String {
        self.wrap(&format!("1;38;2;{};{};{}", self.acc.0, self.acc.1, self.acc.2), s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn ok(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub fn warn(&self, s: &str) -> String {
        self.wrap("33", s)
    }
    pub fn bad(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    /// цветной квадратик (образец палитры)
    pub fn swatch(&self, rgb: u32) -> String {
        if self.on {
            format!("\x1b[38;2;{};{};{}m██\x1b[0m", (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
        } else {
            String::new()
        }
    }
    /// «▪ заголовок» и строки «  имя   что делает»
    pub fn section(&self, name: &str) {
        println!("\n {} {}", self.acc("▪"), self.head(&name.to_uppercase()));
    }
    pub fn row(&self, cmd: &str, what: &str) {
        let n = cmd.chars().count();
        // команда не влезла в колонку — описание строкой ниже, а не впритык к ней
        if n >= 24 {
            println!("   {}", self.bold(cmd));
            println!("   {}{}", " ".repeat(24), self.dim(what));
            return;
        }
        println!("   {}{}{}", self.bold(cmd), " ".repeat(24 - n), self.dim(what));
    }
}

fn banner(y: &Sty) {
    println!();
    println!(" {} {}  {}", y.acc("▚▞"), y.head(&format!("YAMI-LITE {}", env!("CARGO_PKG_VERSION"))), y.dim(&crate::tf!("пиксельный шелл для niri", "pixel shell for niri")));
}

fn help(y: &Sty) {
    banner(y);
    println!("\n {}  yami-lite {} {}", y.dim(&crate::tf!("как", "usage")), y.acc(&crate::tf!("<команда>", "<command>")), y.dim(&crate::tf!("[аргументы]", "[args]")));
    y.section(&crate::tf!("сеанс", "session"));
    y.row("", &crate::tf!("без команды — запустить шелл (spawn-at-startup \"yami-lite\")", "no command: run the shell (spawn-at-startup \"yami-lite\")"));
    y.row("msg <cmd>", &crate::tf!("команда запущенному шеллу (yami-lite help msg)", "a command to the running shell (yami-lite help msg)"));
    y.row("restart", &crate::tf!("перезапустить шелл (подождёт, если играешь)", "restart the shell (waits while you play)"));
    y.section(&crate::tf!("вид", "look"));
    y.row("palette [name|#hex]", &crate::tf!("палитры или сменить: blood ash sakura …", "list palettes or switch: blood ash sakura …"));
    y.row("wallpaper [path]", &crate::tf!("свои обои; без пути — пиксельные из палитры", "own picture; no path: the palette's pixel art"));
    y.row("theme save|list|apply", &crate::tf!("свои темы: сохранить вид, список, применить (remove — удалить)", "your themes: keep the look, list, apply (remove deletes)"));
    y.section(&crate::tf!("настройки", "config"));
    y.row("config …", &crate::tf!("все настройки: list get set reset check path", "every setting: list get set reset check path"));
    y.row("version", &crate::tf!("версия", "version"));
    println!("\n {}", y.dim(&crate::tf!("NO_COLOR=1 — без цвета", "NO_COLOR=1 for plain output")));
    println!();
}

fn help_msg(y: &Sty) {
    y.section(&crate::tf!("команды шеллу", "shell commands"));
    for (c, ru, en) in [
        ("settings [tab]", "настройки (на вкладке)", "settings (on a tab)"),
        ("power | lock", "питание / заблокировать", "power menu / lock"),
        (&format!("popup {}", crate::popups::KINDS.join("|")), "всплывашка бара", "a bar popup"),
        ("bar | edit", "бар / правка виджетов", "bar / edit widgets"),
        ("wallpaper [path] | walls", "обои / выбор обоев из папки", "wallpaper / pick from the wallpaper folder"),
        ("media play-pause|next|previous|player", "плееру, что в баре; player — другой плеер", "to the player shown in the bar; player switches to another one"),
        ("night", "ночной свет вкл-выкл", "night light on-off"),
        ("reload | restart | quit", "перечитать настройки / перезапуск / выход", "reload settings / restart / quit"),
        ("ping", "жив ли шелл: ответит pong", "is the shell alive: answers pong"),
    ] {
        y.row(c, crate::i18n::select(ru, en));
    }
    println!();
}

/// команда шеллу понятна ему (App::ipc): опечатка — ошибка здесь, а не молчаливое «ok»
pub fn msg_ok(cmd: &str) -> bool {
    let mut w = cmd.split_whitespace();
    let head = w.next().unwrap_or("");
    let one = |w: &mut std::str::SplitWhitespace| w.next().is_none();
    match head {
        "power" | "lock" | "bar" | "edit" | "reload" | "quit" | "ping" | "restart" | "night" | "walls" => one(&mut w),
        "settings" => w.next().is_none_or(|t| crate::settings::find(t).is_some()) && one(&mut w),
        "popup" => w.next().is_some_and(|k| crate::popups::KINDS.contains(&k)) && one(&mut w),
        "media" => w.next().is_some_and(|a| matches!(a, "play-pause" | "play" | "pause" | "stop" | "next" | "previous" | "player")) && one(&mut w),
        // путь к картинке (с пробелами) или пусто — пиксельные из палитры
        "wallpaper" => true,
        _ => false,
    }
}

fn send(y: &Sty, cmd: &str) -> i32 {
    if crate::ipc::send(cmd) {
        0
    } else {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("шелл не запущен (yami-lite или войди в сеанс niri)", "the shell is not running (run yami-lite, or log into a niri session)"));
        1
    }
}

fn palette(y: &Sty, args: &[&str]) -> i32 {
    let mut s = Settings::load();
    match args.first() {
        None => {
            y.section(&crate::tf!("палитры", "palettes"));
            let cur = s.accent_rgb();
            for p in crate::theme::PRESETS {
                let mark = if p.accent == cur { y.acc("●") } else { " ".into() };
                let name = if crate::i18n::translated() { p.id.to_string() } else { format!("{} · {}", p.id, p.name) }; // i18n-ok: preset names are Russian data
                println!("   {mark} {} {}  {}", y.swatch(p.accent), y.bold(&format!("{:<8}", p.id)), y.dim(&format!("#{:06x}  {name}", p.accent)));
            }
            if s.wall_accent {
                println!("   {} {} {}  {}", y.acc("●"), y.swatch(cur), y.bold(&format!("{:<8}", "wallpaper")), y.dim(&crate::tf!("#{:06x}  из обоев", "#{:06x}  from the wallpaper", cur)));
            } else if crate::theme::PRESETS.iter().all(|p| p.accent != cur) {
                println!("   {} {} {}", y.acc("●"), y.swatch(cur), y.bold(&format!("#{cur:06x}")));
            }
            println!("\n   {}\n", y.dim(&crate::tf!("сменить: yami-lite palette ash  ·  свой цвет: yami-lite palette '#3a9fd0'  ·  из обоев: yami-lite palette wallpaper", "switch: yami-lite palette ash  ·  own colour: yami-lite palette '#3a9fd0'  ·  from the wallpaper: yami-lite palette wallpaper")));
            0
        }
        Some(&"wallpaper") => {
            // акцент из обоев (#26): его достаёт шелл, когда перечитает настройки, и пересчитывает с каждой новой картинкой
            s.wall_accent = true;
            if crate::config::write_atomic(&crate::config::settings_path(), &s.to_json()).is_err() {
                return 1;
            }
            println!(" {} {}", y.bold("wallpaper"), y.dim(&crate::tf!("— палитра пойдёт за обоями", "— the palette follows the wallpaper")));
            0
        }
        Some(v) => {
            if crate::theme::parse_hex(v).is_some() {
                s.accent = v.to_string();
            } else if let Some(p) = crate::theme::PRESETS.iter().find(|p| p.id == *v) {
                s.palette = p.id.into();
                s.accent.clear();
            } else {
                eprintln!(" {} {}", y.bad("✕"), crate::tf!("нет такой палитры: {} (yami-lite palette — список)", "no such palette: {} (yami-lite palette lists them)", v));
                return 2;
            }
            // выбрана руками: обои больше не перекрашивают
            s.wall_accent = false;
            s.accent2.clear();
            if crate::config::write_atomic(&crate::config::settings_path(), &s.to_json()).is_err() {
                return 1;
            }
            println!(" {} {} {}", y.swatch(s.accent_rgb()), y.bold(v), y.dim(&crate::tf!("— шелл перекрасит всё сам", "— the shell recolours everything itself")));
            0
        }
    }
}

fn restart(y: &Sty, force: bool) -> i32 {
    if !force && crate::niri::game_focused() {
        eprintln!(" {} {}", y.warn("!"), crate::tf!("в фокусе игра — не перезапускаю (yami-lite restart --force)", "a game has focus: not restarting (yami-lite restart --force)"));
        return 3;
    }
    send(y, "restart")
}

pub fn run(args: &[String]) -> i32 {
    let set = Settings::load();
    crate::i18n::set(&set.lang);
    let y = Sty::new(&set);
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    // `… | head` не должен падать с паникой
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    match a.first().copied() {
        None | Some("help") | Some("-h") | Some("--help") => {
            match a.get(1).copied() {
                Some("msg") => help_msg(&y),
                Some("config") => {
                    let _ = crate::cli::config(&["help".to_string()]);
                }
                _ => help(&y),
            }
            0
        }
        Some("--version") | Some("-V") | Some("version") => {
            println!("yami-lite {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("msg") => {
            let cmd = a[1..].join(" ");
            if cmd.is_empty() {
                help_msg(&y);
                return 2;
            }
            if !msg_ok(&cmd) {
                eprintln!(" {} {}", y.bad("✕"), crate::tf!("шелл не знает такой команды: {} (yami-lite help msg)", "the shell has no such command: {} (yami-lite help msg)", cmd));
                return 2;
            }
            let rc = send(&y, &cmd);
            if rc == 0 && cmd == "ping" {
                println!("pong");
            }
            rc
        }
        Some("restart") => restart(&y, a.contains(&"--force")),
        Some("theme") => match a.get(1) {
            Some(&w @ ("save" | "list" | "apply" | "remove")) => crate::bundle::run(&y, w, &a[2..]),
            _ => crate::bundle::run(&y, "", &[]),
        },
        Some("palette") => palette(&y, &a[1..]),
        Some("wallpaper") => {
            // шелл живёт в другой папке: путь — абсолютный
            let p = a[1..].join(" ");
            let p = if p.is_empty() { p } else { std::fs::canonicalize(&p).map(|x| x.display().to_string()).unwrap_or(p) };
            send(&y, format!("wallpaper {p}").trim())
        }
        Some("config") => crate::cli::config(&args[1..]),
        Some(c) => {
            eprintln!(" {} {}", y.bad("✕"), crate::tf!("нет такой команды: {} (yami-lite help)", "no such command: {} (yami-lite help)", c));
            2
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn msg_commands() {
        for ok in ["power", "popup cal", "popup media", "settings color", "wallpaper /a b.png", "wallpaper", "night"] {
            assert!(super::msg_ok(ok), "{ok}");
        }
        for bad in ["cal", "popup foo", "settings nope", "power now", "launcher", ""] {
            assert!(!super::msg_ok(bad), "{bad}");
        }
    }
}
