// `yami doctor` и `yami status`: что есть в системе и что работает. Шелл без внешних программ не
// падает — просто гаснет то, чему они нужны; doctor говорит, что именно и почему.
// Службы D-Bus — кто держит имя (процесс по pid), а не «есть ли пакет».
// i18n: inline pairs

use yami_px::paths::pids;
use crate::cmd::Sty;
use std::path::Path;

/// программа → для чего (RU, EN); true — без неё не работает основное
const DEPS: &[(&str, &str, &str, bool)] = &[
    ("niri", "композитор", "the compositor", true),
    ("setsid", "запуск программ", "launching programs", true),
    ("grim", "скриншоты, шторка", "screenshots, the curtain", false),
    ("wl-copy", "буфер обмена", "clipboard", false),
    ("wl-paste", "буфер обмена", "clipboard", false),
    ("gio", "телефоны и сеть в файлах (gvfs)", "phones and network in files (gvfs)", false),
    ("cliphist", "история буфера", "clipboard history", false),
    ("pactl", "звук", "sound", false),
    ("parec", "визуализатор музыки", "music visualizer", false),
    ("playerctl", "музыка в баре", "music in the bar", false),
    ("nmcli", "Wi-Fi", "Wi-Fi", false),
    ("brightnessctl", "яркость с клавиш", "brightness keys", false),
    ("python3", "перекраска программ, обои (theme.py)", "recolouring apps, wallpaper (theme.py)", false),
    ("fc-match", "шрифты", "fonts", false),
    ("curl", "обложки музыки из сети", "album covers from the web", false),
    ("xdg-open", "открыть файл настроек", "opening the settings file", false),
    ("ffmpegthumbnailer", "миниатюры видео в файлах", "video thumbnails in files", false),
    ("bsdtar", "архивы в файлах", "archives in files", false),
    ("udisksctl", "флешки и диски", "USB drives", false),
    ("git", "обновления", "updates", false),
    ("wlsunset", "ночной свет", "night light", false),
    ("tesseract", "текст со скриншота", "text from screenshots", false),
    ("zbarimg", "QR-коды со скриншота", "QR codes from screenshots", false),
    ("cargo", "сборка обновлений", "building updates", false),
    ("ssh-keygen", "проверка подписи выпусков", "release signature check", false),
];

pub fn which(c: &str) -> bool {
    yami_px::paths::find_exe(c).is_some()
}

/// сам шелл: yami-shell (или yami) без команды либо с `run`; `yami update apply` и прочие команды
/// CLI — тоже процессы yami-shell, их за шелл не считаем
fn shell_pids() -> Vec<u32> {
    let Ok(rd) = std::fs::read_dir("/proc") else { return vec![] };
    rd.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            let c = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
            let parts: Vec<String> = c.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).to_string()).collect();
            let exe = parts.first().map(|f| f.rsplit('/').next().unwrap_or("").to_string()).unwrap_or_default();
            (exe == "yami-shell" || exe == "yami") && (parts.len() == 1 || (parts.len() == 2 && parts[1] == "run"))
        })
        .collect()
}

fn pss_mb(pid: u32) -> f32 {
    std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok().and_then(|t| crate::sys::kv_num(&t, "Pss:")).map(|k| k as f32 / 1024.0).unwrap_or(0.0)
}

/// кто держит имя на шине: имя процесса или None
fn owner(c: &zbus::blocking::Connection, name: &str) -> Option<String> {
    let p = zbus::blocking::fdo::DBusProxy::new(c).ok()?;
    let n = zbus::names::BusName::try_from(name).ok()?;
    let o = p.get_name_owner(n.clone()).ok()?;
    let pid = p.get_connection_unix_process_id(zbus::names::BusName::from(o)).ok();
    // имя программы: comm обрезан до 15 знаков («xdg-desktop-por»), тогда — из пути (свои
    // процессы) или из argv[0], если comm — его начало (у системных exe не прочитать)
    let prog = |pid: u32| {
        let base = |s: &str| std::path::Path::new(s).file_name().map(|n| n.to_string_lossy().to_string());
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|s| s.trim().to_string())?;
        if comm.len() < 15 {
            return Some(comm);
        }
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok().and_then(|p| base(&p.to_string_lossy()));
        let arg0 = std::fs::read(format!("/proc/{pid}/cmdline")).ok().and_then(|c| c.split(|b| *b == 0).next().and_then(|a| base(&String::from_utf8_lossy(a))));
        exe.or(arg0).filter(|n| n.starts_with(&comm)).or(Some(comm))
    };
    Some(pid.and_then(prog).unwrap_or_else(|| "?".into()))
}
fn activatable(c: &zbus::blocking::Connection, name: &str) -> bool {
    zbus::blocking::fdo::DBusProxy::new(c).ok().and_then(|p| p.list_activatable_names().ok()).map(|l| l.iter().any(|n| n.as_str() == name)).unwrap_or(false)
}

fn line(y: &Sty, mark: &str, what: &str, note: &str) {
    let pad = 22usize.saturating_sub(what.chars().count());
    println!("   {mark} {}{}{}", what, " ".repeat(pad), y.dim(note));
}

/// экран входа: yami-greet стоит, greetd — менеджер входа, вид шелла до него доходит, как прошёл
/// последний запуск (greet-session пишет его в /var/lib/yami-greet/log)
fn login(y: &Sty) {
    let greet = crate::login::installed();
    if !greet && !std::path::Path::new("/usr/share/sddm/themes/yami").exists() {
        return;
    }
    y.section(&crate::tf!("экран входа", "login screen", "tela de login"));
    if !greet {
        line(y, &y.warn("·"), "SDDM", &crate::tf!("тема yami до 2.6; свой экран входа: tools/greet-install.sh", "the yami theme of before 2.6; our own login screen: tools/greet-install.sh", "o tema yami de antes da 2.6; nossa própria tela de login: tools/greet-install.sh"));
        return;
    }
    line(y, &y.ok("✓"), "yami-greet", &crate::login::greet_bin().map(|p| p.display().to_string()).unwrap_or_default());
    match crate::login::dm() {
        Some(false) => line(y, &y.warn("·"), "greetd", &crate::tf!("не менеджер входа: tools/greet-install.sh", "not the login manager: tools/greet-install.sh", "não é o gerenciador de login: tools/greet-install.sh")),
        _ => line(y, &y.ok("✓"), "greetd", &crate::tf!("менеджер входа", "the login manager", "o gerenciador de login")),
    }
    if crate::login::writable() {
        line(y, &y.ok("✓"), &crate::tf!("вид", "look", "visual"), &crate::login::dir().display().to_string());
    } else {
        line(y, &y.warn("·"), &crate::tf!("вид", "look", "visual"), &crate::tf!("папка не твоя: экран входа не идёт за шеллом", "the folder is not yours: the login screen does not follow the shell", "a pasta não é sua: a tela de login não acompanha o shell"));
    }
    let last = crate::login::last_result();
    if !last.is_empty() {
        let bad = last.split(' ').nth(2).is_some_and(|w| w == "fail" || w == "text");
        let mark = if bad { y.warn("·") } else { y.ok("✓") };
        line(y, &mark, &crate::tf!("последний запуск", "last start", "última inicialização"), &format!("{last} · yami login log"));
    }
    if let Some(cmd) = sddm_unused(&last) {
        line(y, &y.warn("·"), "SDDM", &crate::tf!("стоит, но не нужен: убери, если им не пользуется KDE или другой стол", "installed but unused: remove it unless KDE or another desktop uses it", "instalado, mas sem uso: remova se o KDE ou outra área de trabalho não o usar"));
        println!("   {}", y.bold(&cmd));
    }
}

/// SDDM остался после перехода на greetd (до 2.6 его ставил сам установщик): пакет есть, службы нет.
/// Команда удаления — только когда greetd уже менеджер входа и новый экран входа хоть раз впустил
/// (последний запуск «ok»): чужой стол (KDE) решает сам, удалять ли, поэтому только подсказка
fn sddm_unused(last: &str) -> Option<String> {
    let ok = last.split(' ').nth(2) == Some("ok");
    if !ok || crate::login::dm() != Some(true) || !which("sddm") {
        return None;
    }
    let su = if which("sudo") || !which("doas") { "sudo" } else { "doas" };
    let rm = [
        ("pacman", "pacman -Rns sddm"),
        ("dnf", "dnf remove sddm"),
        ("apt-get", "apt autoremove --purge sddm"),
        ("zypper", "zypper remove --clean-deps sddm"),
        ("xbps-remove", "xbps-remove -R sddm"),
        ("apk", "apk del sddm"),
        ("emerge", "emerge --depclean x11-misc/sddm"),
    ];
    rm.iter().find(|(pm, _)| which(pm)).map(|(_, c)| format!("{su} {c}"))
}

/// службы D-Bus: (имя, для чего RU, EN); ours — должен держать шелл (если служба включена)
fn services(y: &Sty, problems: &mut u32) {
    let set = crate::config::Settings::load();
    y.section(&crate::tf!("службы", "services", "serviços"));
    if let Ok(c) = zbus::blocking::Connection::session() {
        for (name, ru, en, want) in [
            ("org.freedesktop.Notifications", "уведомления", "notifications", set.agent_notif.then_some("yami-shell")),
            ("org.kde.StatusNotifierWatcher", "трей", "tray", set.agent_tray.then_some("yami-shell")),
            ("org.freedesktop.portal.Desktop", "порталы", "portals", None),
        ] {
            let label = crate::i18n::select(ru, en);
            match owner(&c, name) {
                Some(who) if want.map(|w| w == who).unwrap_or(true) => line(y, &y.ok("✓"), label, &who),
                Some(who) => line(y, &y.warn("!"), label, &crate::tf!("держит {}, а не шелл", "owned by {}, not the shell", "controlado por {}, não pelo shell", who)),
                None if want.is_none() && activatable(&c, name) => line(y, &y.ok("✓"), label, &crate::tf!("запустится по запросу", "starts on demand", "inicia sob demanda")),
                None => {
                    *problems += 1;
                    line(y, &y.bad("✕"), label, &crate::tf!("никто не держит {}", "nobody owns {}", "ninguém controla {}", name));
                }
            }
        }
        let yp = "org.freedesktop.impl.portal.desktop.yami";
        let run = !pids("yami-files", "--portal").is_empty();
        if run || activatable(&c, yp) {
            line(y, &y.ok("✓"), &crate::tf!("портал yami-files", "yami-files portal", "portal yami-files"), &if run { crate::tf!("работает", "running", "em execução") } else { crate::tf!("запустится по запросу", "starts on demand", "inicia sob demanda") });
        } else if !crate::proc::have("yami-files") {
            // yami-files не выбран при установке: диалоги выбора файлов — от GTK или KDE
            line(y, &y.dim("·"), &crate::tf!("портал yami-files", "yami-files portal", "portal yami-files"), &crate::tf!("yami-files не установлен: диалоги — от GTK/KDE", "yami-files not installed: dialogs come from GTK/KDE", "yami-files não instalado: diálogos do GTK/KDE"));
        } else {
            *problems += 1;
            line(y, &y.bad("✕"), &crate::tf!("портал yami-files", "yami-files portal", "portal yami-files"), &crate::tf!("не установлен: install.sh (шаг «файлы»)", "not installed: install.sh (the «files» step)", "não instalado: install.sh (etapa «arquivos»)"));
        }
    } else {
        *problems += 1;
        line(y, &y.bad("✕"), &crate::tf!("сессионная шина", "session bus", "barramento de sessão"), &crate::tf!("нет", "missing", "ausente"));
    }
    if let Some(c) = crate::net::system() {
        for (name, ru, en) in [
            ("org.freedesktop.PolicyKit1", "polkit", "polkit"),
            ("org.freedesktop.NetworkManager", "сеть", "network"),
            ("net.hadess.PowerProfiles", "профили питания", "power profiles"),
            ("org.bluez", "bluetooth", "bluetooth"),
        ] {
            let label = crate::i18n::select(ru, en);
            match owner(&c, name) {
                Some(who) => line(y, &y.ok("✓"), label, &who),
                None if activatable(&c, name) => line(y, &y.ok("✓"), label, &crate::tf!("запустится по запросу", "starts on demand", "inicia sob demanda")),
                None => line(y, &y.dim("·"), label, &crate::tf!("нет (то, что от неё, в баре скрыто)", "absent (what needs it is hidden)", "ausente (as funções que dependem dele ficam ocultas)")),
            }
        }
    }
    let helper = Path::new(crate::auth::SOCKET).exists() || crate::auth::helper().is_some();
    if set.agent_polkit && !helper {
        *problems += 1;
        line(y, &y.bad("✕"), &crate::tf!("помощник polkit", "polkit helper", "auxiliar do polkit"), &crate::tf!("нет polkit-agent-helper-1: окна пароля не будет", "no polkit-agent-helper-1: no password window", "polkit-agent-helper-1 ausente: a janela de senha não será exibida"));
    }
}

pub fn run(y: &Sty) -> i32 {
    let mut problems = 0u32;
    y.section(&crate::tf!("программы", "programs", "programas"));
    for (c, ru, en, req) in DEPS {
        let what = crate::i18n::select(ru, en);
        if which(c) {
            line(y, &y.ok("✓"), c, what);
        } else if *req {
            problems += 1;
            line(y, &y.bad("✕"), c, &crate::tf!("нужен: {}", "required: {}", "necessário: {}", what));
        } else {
            line(y, &y.warn("·"), c, &crate::tf!("нет — не будет: {}", "missing, no {}", "ausente, sem {}", what));
        }
    }
    if which("python3") {
        let pil = std::process::Command::new("python3").args(["-I", "-c", "import PIL"]).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if !pil {
            line(y, &y.warn("·"), "python3-pillow", &crate::tf!("нет — обои из своей картинки не перекрасятся", "missing, own wallpapers are not recoloured", "ausente, papéis de parede personalizados não são recoloridos"));
        }
    }
    services(y, &mut problems);
    // новым функциям не хватает пакетов (установщик записал, обновление без sudo не поставило)
    if let Some(n) = crate::need::load() {
        y.section(&crate::tf!("не поставлено из выбранного при установке", "not installed of what the install chose", "não instalado do que a instalação escolheu"));
        line(y, &y.warn("·"), &n.what(), "");
        println!("   {}  {}", y.bold(&n.line()), y.dim("yami update need --run"));
    }
    y.section(&crate::tf!("настройки", "config", "configuração"));
    let (bad, unknown) = crate::config::Settings::problems(&crate::cli::file_json());
    if bad.is_empty() && unknown.is_empty() {
        line(y, &y.ok("✓"), "yami-shell.json", &crate::config::settings_path().display().to_string());
    }
    for k in &bad {
        problems += 1;
        line(y, &y.bad("✕"), k, &crate::tf!("негодное значение, взято по умолчанию", "bad value, default used", "valor inválido; usando o padrão"));
    }
    for k in &unknown {
        line(y, &y.warn("·"), k, &crate::tf!("неизвестный ключ, пропущен", "unknown key, ignored", "chave desconhecida, ignorada"));
    }
    if which("niri") {
        let cfg = crate::config::config_dir().join("niri/config.kdl");
        let ok = std::process::Command::new("niri").arg("validate").arg("-c").arg(&cfg).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            line(y, &y.ok("✓"), "niri/config.kdl", &crate::tf!("проходит проверку", "validates", "válido"));
        } else {
            problems += 1;
            line(y, &y.bad("✕"), "niri/config.kdl", &crate::tf!("не проходит: niri validate", "fails: niri validate", "falhou: niri validate"));
        }
    }
    login(y);
    // слабая или виртуальная машина: эффекты съедают её, лёгкий режим их снимает
    if let Some(why) = crate::lite::reason() {
        let set = crate::config::Settings::load();
        y.section(&crate::tf!("машина", "machine", "máquina"));
        let note = if set.lite { crate::tf!("лёгкий режим включён", "lite mode is on", "o modo leve está ativado") } else { crate::tf!("включи лёгкий режим: yami config set lite true (настройки → вид)", "turn on lite mode: yami config set lite true (settings → look)", "ative o modo leve: yami config set lite true (configurações → visual)") };
        line(y, &if set.lite { y.ok("✓") } else { y.warn("·") }, &why, &note);
    }
    // драйвер NVIDIA в initramfs от yami ≤ 2.6 (nvboot): теперь yami в драйвер не лезет, файлы — человеку решать
    let old: Vec<&str> = ["/etc/modprobe.d/yami-nvidia-kms.conf", "/etc/mkinitcpio.conf.d/90-yami-nvidia.conf", "/etc/dracut.conf.d/90-yami-nvidia.conf"]
        .into_iter()
        .filter(|f| std::path::Path::new(f).exists())
        .collect();
    if !old.is_empty() {
        y.section("NVIDIA");
        let rebuild = if std::path::Path::new("/usr/bin/mkinitcpio").exists() { "sudo mkinitcpio -P" } else { "sudo dracut -f --regenerate-all" };
        line(y, &y.dim("·"), &crate::tf!("драйвер в initramfs от старой yami", "the driver in the initramfs from an older yami", "o driver no initramfs de um yami antigo"), &crate::tf!("не нужен, если пароль диска виден и так; убрать: sudo rm {} && {}", "not needed if the disk password shows anyway; to remove: sudo rm {} && {}", "desnecessário se a senha do disco já aparece; para remover: sudo rm {} && {}", old.join(" "), rebuild));
    }
    // кеш сборки лежит на диске, пока его не уберут: не проблема, но гигабайты стоит показать
    let cache = crate::clean::total();
    if cache >= 100 << 20 {
        y.section(&crate::tf!("диск", "disk", "disco"));
        line(y, &y.dim("·"), &crate::tf!("кеш сборки", "build cache", "cache de compilação"), &crate::tf!("{} — yami clean убирает (нужен только следующей сборке)", "{}: yami clean removes it (only the next build needs it)", "{}: yami clean remove (necessário apenas na próxima compilação)", crate::clean::human(cache)));
    }
    snapshots(y);
    println!("   {}", y.dim(&crate::tf!("игры: yami doctor games", "games: yami doctor games", "jogos: yami doctor games")));
    println!();
    if problems == 0 {
        println!(" {} {}\n", y.ok("✓"), crate::tf!("всё в порядке", "all good", "tudo certo"));
        0
    } else {
        println!(" {} {}\n", y.warn("!"), crate::tf!("проблем: {}", "problems: {}", "problemas: {}", problems));
        1
    }
}

pub fn status(y: &Sty) {
    y.section(&crate::tf!("работает", "running", "em execução"));
    match shell_pids().into_iter().find(|p| *p != std::process::id()) {
        Some(pid) => line(y, &y.ok("✓"), "yami-shell", &crate::tf!("pid {}, {:.1} МБ", "pid {}, {:.1} MB", "pid {}, {:.1} MB", pid, pss_mb(pid))),
        None => line(y, &y.bad("✕"), "yami-shell", &crate::tf!("не запущен", "not running", "fora de execução")),
    }
    match pids("yami-files", "--portal").first() {
        Some(pid) => line(y, &y.ok("✓"), &crate::tf!("портал файлов", "file portal", "portal de arquivos"), &crate::tf!("pid {}, {:.1} МБ", "pid {}, {:.1} MB", "pid {}, {:.1} MB", pid, pss_mb(*pid))),
        None => line(y, &y.dim("·"), &crate::tf!("портал файлов", "file portal", "portal de arquivos"), &crate::tf!("запустится по запросу", "starts on demand", "inicia sob demanda")),
    }
    let lvl = crate::niri::fork_level();
    let nv = std::process::Command::new("niri").arg("--version").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if std::env::var_os("NIRI_SOCKET").is_some() {
        line(y, &y.ok("✓"), "niri", &if lvl > 0 { crate::tf!("{} (yami-niri, уровень {})", "{} (yami-niri, level {})", "{} (yami-niri, nível {})", nv, lvl) } else { nv });
    } else {
        line(y, &y.dim("·"), "niri", &crate::tf!("не в сеансе niri", "not in a niri session", "fora de uma sessão niri"));
    }
    if crate::niri::game_focused() {
        line(y, &y.acc("▪"), &crate::tf!("игра", "game", "jogo"), &crate::tf!("в фокусе — шелл не перезапускается", "has focus: the shell will not restart", "está em foco: o shell não será reiniciado"));
    }
    let u = crate::update::installed();
    let last = crate::update::last();
    let upd = match last {
        Some(i) if !i.err.is_empty() => y.warn(&i.err),
        Some(i) if i.available => y.acc(&crate::tf!("доступно {} (yami update)", "{} available (yami update)", "{} disponível (yami update)", i.latest)),
        Some(i) if i.reference.is_empty() => crate::tf!("выпусков ещё нет", "no releases yet", "ainda sem versões publicadas"),
        Some(_) => crate::tf!("свежее", "up to date", "atualizado"),
        None => crate::tf!("не проверялось", "never checked", "nunca verificado"),
    };
    line(y, &y.ok("✓"), &crate::tf!("версия", "version", "versão"), &format!("{}  ·  {upd}", if u.version.is_empty() { env!("CARGO_PKG_VERSION") } else { &u.version }));
}

/// снимки системы: yami сам откатывает только свои бинари (yami update rollback); всю систему
/// откатывают снимки btrfs (snapper, timeshift) или zfs — показать, есть ли они и как сделать
fn snapshots(y: &Sty) {
    let fs = std::process::Command::new("findmnt").args(["-no", "FSTYPE", "/"]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    if fs != "btrfs" && fs != "zfs" {
        return;
    }
    y.section(&crate::tf!("снимки системы", "system snapshots", "instantâneos do sistema"));
    let tool = ["snapper", "timeshift", "zfs"].into_iter().find(|t| which(t));
    let (mark, note) = match tool {
        Some("snapper") => (y.ok("✓"), crate::tf!("перед большим обновлением: sudo snapper create -d 'before yami update'", "before a big update: sudo snapper create -d 'before yami update'", "antes de uma atualização grande: sudo snapper create -d 'before yami update'")),
        Some("timeshift") => (y.ok("✓"), crate::tf!("перед большим обновлением: sudo timeshift --create --comments 'before yami update'", "before a big update: sudo timeshift --create --comments 'before yami update'", "antes de uma atualização grande: sudo timeshift --create --comments 'before yami update'")),
        Some(_) => (y.ok("✓"), crate::tf!("перед большим обновлением: sudo zfs snapshot <пул>/ROOT@yami", "before a big update: sudo zfs snapshot <pool>/ROOT@yami", "antes de uma atualização grande: sudo zfs snapshot <pool>/ROOT@yami")),
        None => (y.dim("·"), crate::tf!("корень на {}, но нет snapper или timeshift: систему откатить нечем", "the root is {} but there is no snapper or timeshift: nothing rolls the system back", "a raiz é {}, mas não há snapper nem timeshift: não é possível reverter o sistema", fs)),
    };
    line(y, &mark, &fs, &note);
}
