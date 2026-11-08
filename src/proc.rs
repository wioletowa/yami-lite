// Внешние программы: запуск отвязанными (setsid), долгие дети нитей (PDEATHSIG), curl к чужим
// сайтам, подбор детей, доставшихся от образа до перезапуска.

use crate::app::App;

/// curl к чужому сайту (погода, тексты песен, тема по ссылке): только https, и после
/// переадресаций; потолок времени и размера; имя yami в User-Agent. Флаги -f, -G, -L, -o — у вызова
pub fn curl(secs: u32, max: u64) -> std::process::Command {
    let mut c = std::process::Command::new("curl");
    let ua = concat!("yami-lite/", env!("CARGO_PKG_VERSION"));
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



/// дети, доставшиеся от образа до перезапуска: их Child остались в старых нитях, ждать их некому.
/// Список берётся до первого запуска чего-либо (все дети на этот момент — чужие) и
/// подбирается в tick, иначе каждый перезапуск оставлял зомби
static INHERITED: std::sync::Mutex<Vec<i32>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn note_inherited() {
    // `msg restart` — exec в том же pid с меткой в окружении: детям шелла она ни к чему
    std::env::remove_var("YAMI_RESTARTED");
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



pub(crate) fn reap_inherited() {
    let Ok(mut v) = INHERITED.lock() else { return };
    // 0 — ещё жив; pid — подобран; -1 — уже не наш
    v.retain(|&p| unsafe { libc::waitpid(p, std::ptr::null_mut(), libc::WNOHANG) } == 0);
}




impl App {
    /// папка (или trash:///) — в файловом менеджере по умолчанию (xdg-open)
    pub fn open_dir(what: String) {
        App::spawn(&["xdg-open".into(), what]);
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

/// программа есть в PATH
pub fn which(c: &str) -> bool {
    yami_px::paths::find_exe(c).is_some()
}

/// терминал для текстовых программ (nmtui, btop): первый найденный
pub fn term() -> String {
    ["kitty", "foot", "alacritty", "wezterm", "ghostty", "konsole"].into_iter().find(|t| which(t)).unwrap_or("xterm").into()
}
