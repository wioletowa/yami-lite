// Внешние программы: запуск отвязанными (setsid), долгие дети нитей (PDEATHSIG), curl к чужим
// сайтам, окно выбора yami-files, tools/theme.py с пониженным приоритетом, подбор детей,
// доставшихся от образа до перезапуска.

use crate::app::App;
use crate::config;

/// curl к чужому сайту (погода, тексты песен, тема по ссылке): только https, и после
/// переадресаций; потолок времени и размера; имя yami в User-Agent. Флаги -f, -G, -L, -o — у вызова
pub fn curl(secs: u32, max: u64) -> std::process::Command {
    let mut c = std::process::Command::new("curl");
    let ua = concat!("yami/", env!("CARGO_PKG_VERSION"), " (https://github.com/Trendorin/yami)");
    c.args(["-q", "-s", "--proto", "=https", "--proto-redir", "=https", "-A", ua, "--max-time", &secs.to_string(), "--max-filesize", &max.to_string()]);
    c
}

/// GET url?k=v&… тем же curl: значения — через --data-urlencode (пробелы, кириллица, «&»)
pub fn curl_get(secs: u32, max: u64, url: &str, q: &[(&str, String)]) -> std::process::Command {
    let mut c = curl(secs, max);
    c.args(["-G", url]);
    for (k, v) in q {
        c.arg("--data-urlencode").arg(format!("{k}={v}"));
    }
    c
}

/// долгий процесс нити шелла (playerctl -F, pactl subscribe, parec) уходит вместе с ней: при
/// падении шелла и при перезапуске (exec в том же pid уничтожает нити, их дети получают сигнал).
/// Без этого каждый `yami restart` и каждое обновление оставляли по живой копии каждого из них
pub fn owned(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: между fork и exec только prctl
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        })
    }
}

/// исполняемый файл bin в PATH или в ~/.local/bin (туда ставит установщик): yami-files, yami-view и
/// yami-play ставятся по выбору, шелл без них открывает чужими программами
pub fn have(bin: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let local = yami_px::paths::home().join(".local/bin");
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).chain(std::iter::once(local)).any(|d| d.join(bin).metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// окно выбора yami-files (`--pick MODE`, папка, свои ключи): первая строка ответа или None
/// (отмена). Ждёт, пока окно открыто, — только из нити; окно закрывается вместе с нитью (owned)
pub fn files_pick(mode: &str, folder: &std::path::Path, args: &[&str]) -> Option<String> {
    let mut c = std::process::Command::new("yami-files");
    c.args(["--pick", mode]).args(args).arg("--folder").arg(folder);
    owned(&mut c).output().ok().filter(|o| o.status.success()).and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(str::to_string))
}

/// дети, доставшиеся от образа до перезапуска: их Child остались в старых нитях, ждать их некому.
/// Список берётся до первого запуска чего-либо (все дети на этот момент — чужие) и
/// подбирается в tick, иначе каждый перезапуск оставлял зомби
static INHERITED: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn note_inherited() {
    // `msg restart` и обновление — exec в том же pid с меткой в окружении (её снимаем: детям шелла
    // она ни к чему); при входе в систему меток нет
    if std::env::var_os("YAMI_RESTARTED").is_some() {
        std::env::remove_var("YAMI_RESTARTED");
        RESTARTED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // дети всех нитей: /proc/self/task/*/children (на старте нить одна); нет файла (ядро без
    // CONFIG_PROC_CHILDREN) — обход всего /proc
    let mut kids: Vec<i32> = vec![];
    let mut known = false;
    for t in std::fs::read_dir("/proc/self/task").into_iter().flatten().flatten() {
        if let Ok(s) = std::fs::read_to_string(t.path().join("children")) {
            known = true;
            kids.extend(s.split_whitespace().filter_map(|p| p.parse::<i32>().ok()));
        }
    }
    if !known {
        let me = std::process::id().to_string();
        let Ok(rd) = std::fs::read_dir("/proc") else { return };
        kids = rd
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
            .filter(|p| {
                // «pid (comm) S ppid …»: comm может содержать пробелы и скобки — режем по последней «)»
                let Ok(s) = std::fs::read_to_string(format!("/proc/{p}/stat")) else { return false };
                s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)) == Some(me.as_str())
            })
            .collect();
    }
    if let Ok(mut v) = INHERITED.lock() {
        *v = kids;
    }
}

static RESTARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// шелл запущен заново из себя же (`msg restart`, обновление), а не при входе в систему
pub(crate) fn restarted() -> bool {
    RESTARTED.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn reap_inherited() {
    let Ok(mut v) = INHERITED.lock() else { return };
    // 0 — ещё жив; pid — подобран; -1 — уже не наш
    v.retain(|&p| unsafe { libc::waitpid(p, std::ptr::null_mut(), libc::WNOHANG) } == 0);
}

/// tools/theme.py, тихо и с пониженным приоритетом: на слабой машине перекраска (обои, курсор,
/// темы программ) идёт секунды и не должна отнимать процессор у стола
pub fn theme_py() -> std::process::Command {
    let mut c = std::process::Command::new("python3");
    c.arg("-I").arg(theme_script()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    gentle(&mut c);
    c
}

/// фоновая работа (theme.py, распознавание текста): nice 10 у процесса, шелл свой не меняет
pub fn gentle(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: между fork и exec только setpriority
    unsafe {
        cmd.pre_exec(|| {
            libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            Ok(())
        })
    }
}

pub fn theme_script() -> std::path::PathBuf {
    // tools/theme.py вшит в шелл и пишется рядом с состоянием
    let p = config::state_dir().join("theme.py");
    let src = include_str!("../../tools/theme.py");
    if std::fs::read_to_string(&p).map(|t| t != src).unwrap_or(true) {
        let _ = std::fs::write(&p, src);
    }
    p
}

impl App {
    /// папка (или trash:///) — в yami-files, а без него — в файловом менеджере по умолчанию (xdg-open)
    pub fn open_dir(what: String) {
        let app = if have("yami-files") { "yami-files" } else { "xdg-open" };
        App::spawn(&[app.into(), what]);
    }

    // ── запуск программ ─────────────────────────────────────────────────────
    pub fn spawn(argv: &[String]) {
        if argv.is_empty() {
            return;
        }
        // setsid -f: процесс отвязывается от шелла и не становится зомби
        let _ = std::process::Command::new("setsid")
            .arg("-f")
            .args(argv)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    pub fn sh(cmd: &str) {
        Self::spawn(&["sh".to_string(), "-c".to_string(), cmd.to_string()]);
    }
}
