// `yami report`: всё, что нужно для issue, одним архивом. Версии, дистрибутив, видеокарта, что
// говорят `yami doctor` и `yami status`, журнал шелла и niri за эту загрузку, конфиги шелла и niri.
// Секреты не попадают: строки, похожие на пароли и токены, вычищаются, домашний путь, имя
// пользователя и хоста заменяются. Ничего никуда не отправляется: человек сам смотрит архив и
// прикладывает его.
// i18n: inline pairs

use crate::cmd::Sty;
use std::path::{Path, PathBuf};
use std::process::Command;

/// вывод команды текстом или пометка, почему его нет
fn out(cmd: &str, args: &[&str]) -> String {
    match Command::new(cmd).args(args).env("NO_COLOR", "1").stdin(std::process::Stdio::null()).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).to_string();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.trim().is_empty() {
                s.push_str("\n[stderr]\n");
                s.push_str(&e);
            }
            s
        }
        Err(e) => format!("({cmd}: {e})\n"),
    }
}

/// как выглядит секрет: в конфигах — любой из этих ключей, в журнале — только явные
/// («auth» есть во всех сообщениях PAM и polkit, а их как раз полезно читать)
const CONFIG_KEYS: &[&str] = &["password", "passwd", "token", "secret", "api_key", "apikey", "api-key", "auth", "cookie", "psk"];
const LOG_KEYS: &[&str] = &["password", "passwd", "token", "secret", "api_key", "apikey"];

/// спрятать то, что выдаёт человека или может быть секретом. `log` — журнал (там имя хоста —
/// отдельное слово после времени; в os-release это имя дистрибутива, его не трогаем)
pub fn scrub(text: &str, home: &str, user: &str, host: &str, log: bool) -> String {
    let keys = if log { LOG_KEYS } else { CONFIG_KEYS };
    let mut res = String::with_capacity(text.len());
    for line in text.lines() {
        let low = line.to_lowercase();
        let mut l = if !keys.iter().any(|k| low.contains(k)) {
            line.to_string()
        } else if log {
            // время и процесс оставить, сообщение — убрать
            match line.find("]: ") {
                Some(i) => format!("{} <hidden>", &line[..i + 2]),
                None => "<hidden line>".to_string(),
            }
        } else {
            // ключ оставить (видно, что это было), значение — убрать
            match line.find(['=', ':']) {
                Some(i) => format!("{} <hidden>", &line[..=i]),
                None => "<hidden line>".to_string(),
            }
        };
        if log && host.len() > 1 {
            l = l.replacen(&format!(" {host} "), " <host> ", 1);
        }
        res.push_str(&l);
        res.push('\n');
    }
    let mut res = if home.len() > 1 { res.replace(home, "~") } else { res };
    if user.len() > 2 {
        res = word_replace(&res, user, "<user>");
    }
    res
}

/// заменить `w` только целым словом (не внутри «annotation» у пользователя «ann»)
fn word_replace(s: &str, w: &str, with: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(w) {
        let before = rest[..i].chars().last();
        let after = rest[i + w.len()..].chars().next();
        out.push_str(&rest[..i]);
        out.push_str(if word(before) || word(after) { w } else { with });
        rest = &rest[i + w.len()..];
    }
    out.push_str(rest);
    out
}

pub fn run(y: &Sty, args: &[&str]) -> i32 {
    let home = crate::config::home();
    let stamp = out("date", &["+%Y%m%d-%H%M%S"]).trim().to_string();
    let target: PathBuf = match args.iter().position(|a| *a == "-o") {
        Some(i) => match args.get(i + 1) {
            Some(p) => PathBuf::from(p),
            None => {
                eprintln!(" {} yami report [-o FILE.tar.gz]", y.bad("✕"));
                return 2;
            }
        },
        None => std::env::current_dir().unwrap_or_else(|_| home.clone()).join(format!("yami-report-{stamp}.tar.gz")),
    };
    // свой каталог 0700: предсказуемое имя в /tmp дало бы чужим подложить ссылки
    let Ok(dir) = yami_px::paths::private_tmp("yami-report") else {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("не могу создать временный каталог", "cannot create a temporary folder", "não foi possível criar uma pasta temporária"));
        return 1;
    };
    let base = dir.join("yami-report");
    if std::fs::create_dir_all(base.join("config")).is_err() {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("не могу создать {}", "cannot create {}", "não foi possível criar {}", dir.display()));
        let _ = std::fs::remove_dir_all(&dir);
        return 1;
    }
    let h = home.to_string_lossy().to_string();
    let user = std::env::var("USER").unwrap_or_default();
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default().trim().to_string();
    let put = |name: &str, text: &str| {
        let _ = std::fs::write(base.join(name), scrub(text, &h, &user, &host, name.starts_with("journal")));
    };
    let me = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "yami-shell".into());

    let mut sys = String::new();
    sys += &format!("yami-shell {}\n", env!("CARGO_PKG_VERSION"));
    sys += &out("niri", &["--version"]);
    sys += &out("uname", &["-srvm"]);
    sys += &std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    sys += "\n[gpu]\n";
    sys += &out("sh", &["-c", "lspci -k 2>/dev/null | grep -A3 -Ei 'vga|3d|display' || echo 'no lspci'"]);
    sys += &out("sh", &["-c", "cat /proc/driver/nvidia/version 2>/dev/null || true"]);
    sys += "\n[session]\n";
    for k in ["XDG_SESSION_TYPE", "XDG_CURRENT_DESKTOP", "WAYLAND_DISPLAY", "LANG", "LC_ALL"] {
        sys += &format!("{k}={}\n", std::env::var(k).unwrap_or_default());
    }
    put("system.txt", &sys);
    put("doctor.txt", &out(&me, &["doctor"]));
    put("status.txt", &out(&me, &["status"]));
    put("update.txt", &out(&me, &["update", "status"]));
    // только эта загрузка: шелл — по имени процесса, niri — по юниту
    put("journal-shell.txt", &out("journalctl", &["--user", "-b", "--no-pager", "-o", "short-iso", "-n", "3000", "_COMM=yami-shell"]));
    put("journal-niri.txt", &out("journalctl", &["--user", "-b", "--no-pager", "-o", "short-iso", "-n", "3000", "-u", "niri"]));
    let cfg = crate::config::config_dir();
    let mut files: Vec<PathBuf> = vec![cfg.join("yami-shell.json"), cfg.join("yami-files.json")];
    if let Ok(rd) = std::fs::read_dir(cfg.join("niri")) {
        files.extend(rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "kdl")));
    }
    for f in files {
        if let (Some(name), Ok(t)) = (f.file_name(), std::fs::read_to_string(&f)) {
            let t = private_keys(&t);
            let _ = std::fs::write(base.join("config").join(name), scrub(&t, &h, &user, &host, false));
        }
    }
    let ok = Command::new("tar").arg("-czf").arg(&target).arg("-C").arg(&dir).arg("yami-report").status().map(|s| s.success()).unwrap_or(false);
    let _ = std::fs::remove_dir_all(&dir);
    if !ok {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("tar не смог записать {}", "tar could not write {}", "tar não conseguiu gravar {}", target.display()));
        return 1;
    }
    println!(" {} {}", y.ok("✓"), y.bold(&target.display().to_string()));
    println!("   {}", y.dim(&crate::tf!("версии, doctor, журнал шелла и niri за эту загрузку, конфиги; пароли, токены, домашний путь, имя и хост скрыты", "versions, doctor, this boot's shell and niri journal, configs; passwords, tokens, home path, user and host are hidden", "versões, diagnóstico, registros do shell e niri desta inicialização, configurações; senhas, tokens, pasta pessoal, usuário e host são ocultados")));
    println!("   {}", y.dim(&crate::tf!("загляни внутрь перед отправкой: tar -tzf {}", "look inside before you send it: tar -tzf {}", "confira o conteúdo antes de enviar: tar -tzf {}", display(&target))));
    0
}

/// ключи JSON-настроек, что говорят о человеке, а не о поломке: где он живёт (ночной свет по
/// координатам), его серверы, имена устройств, команды на клавишах (в них бывают токены)
const PRIVATE: &[&str] = &["nightGeo", "servers", "audioNames", "customBinds", "dockPins", "updateRepo"];

fn private_keys(text: &str) -> String {
    let Ok(serde_json::Value::Object(mut m)) = serde_json::from_str::<serde_json::Value>(text) else { return text.to_string() };
    for k in PRIVATE {
        if let Some(v) = m.get_mut(*k) {
            let empty = v.as_str().is_some_and(str::is_empty) || v.as_array().is_some_and(Vec::is_empty) || v.as_object().is_some_and(serde_json::Map::is_empty);
            if !empty {
                *v = serde_json::Value::String("<hidden>".into());
            }
        }
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(m)).unwrap_or_default()
}

fn display(p: &Path) -> String {
    p.display().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn hides_secrets_and_names() {
        let t = "wallpaper=/home/ann/Pictures/a.png\npassword = hunter2\nspawn \"kitty\"\n";
        let s = super::scrub(t, "/home/ann", "ann", "annpc", false);
        assert!(!s.contains("hunter2"));
        assert!(!s.contains("/home/ann"));
        assert!(s.contains("~/Pictures"));
        assert!(s.contains("spawn \"kitty\""));
        let j = "2026-11-04T14:29 annpc yami-shell[1]: polkit auth ok\n2026-11-04T14:30 annpc nm[2]: psk token=abc\n";
        let s = super::scrub(j, "/home/ann", "ann", "annpc", true);
        assert!(s.contains("<host> yami-shell[1]: polkit auth ok"));
        assert!(!s.contains("abc"));
        let j = super::private_keys(r#"{"nightGeo":"55.7, 37.6","servers":["smb://nas"],"lang":"ru"}"#);
        assert!(!j.contains("55.7") && !j.contains("nas") && j.contains("\"ru\""));
        let p = super::scrub("pam: logname=ann user=ann annotation\n", "/home/ann", "ann", "annpc", true);
        assert_eq!(p, "pam: logname=<user> user=<user> annotation\n");
    }
}
