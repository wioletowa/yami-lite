// Тема курсора на всю систему (настройки → вид): niri (свой курсор и XCURSOR_* для всего, что он
// запускает, — X-программы через xwayland-satellite тоже), GTK (gsettings и settings.ini), X11 по
// умолчанию (~/.icons/default), Flatpak (переменные и доступ к папкам тем). Применяется в нити:
// gsettings и flatpak — внешние процессы.

use crate::config::config_dir;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// что стоит сейчас — читается с диска один раз, дальше обновляется своими правками
static CURRENT: Mutex<Option<(String, i32)>> = Mutex::new(None);
/// ещё не разосланная тема (последняя побеждает) и замок на саму рассылку: правки файлов не
/// пересекаются, даже если стрелки размера жмут быстрее, чем пишутся конфиги
static PENDING: Mutex<Option<(String, i32)>> = Mutex::new(None);
static APPLY: Mutex<()> = Mutex::new(());

/// папки, где ищутся темы курсора
fn dirs() -> Vec<PathBuf> {
    let home = yami_px::paths::home();
    let mut v = vec![home.join(".icons"), yami_px::paths::data_dir().join("icons")];
    v.extend(yami_px::paths::data_dirs().into_iter().map(|d| d.join("icons")));
    v
}

/// установленные темы курсора (папки с cursors/), по имени
pub fn themes() -> Vec<String> {
    let mut v: Vec<String> = dirs()
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().join("cursors").is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != "default")
        .collect();
    v.sort_by_key(|n| n.to_lowercase());
    v.dedup();
    v
}

/// тема и размер, что стоят сейчас (см. read_current); зовётся и из отрисовки — без диска
pub fn current() -> (String, i32) {
    let mut c = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    c.get_or_insert_with(read_current).clone()
}

/// тема и размер с диска: из niri, иначе из ~/.icons/default
fn read_current() -> (String, i32) {
    let (mut theme, mut size) = (String::new(), 24);
    if let Ok(t) = std::fs::read_to_string(niri_conf()) {
        if let Some((a, b)) = block(&t) {
            for l in &t.lines().collect::<Vec<_>>()[a + 1..b] {
                let l = l.trim();
                if let Some(v) = l.strip_prefix("xcursor-theme") {
                    theme = v.trim().trim_matches('"').to_string();
                } else if let Some(v) = l.strip_prefix("xcursor-size") {
                    size = v.trim().parse().unwrap_or(size);
                }
            }
        }
    }
    if theme.is_empty() {
        let idx = yami_px::paths::home().join(".icons/default/index.theme");
        theme = std::fs::read_to_string(idx).ok().and_then(|t| t.lines().find_map(|l| l.trim().strip_prefix("Inherits=").map(|v| v.trim().to_string()))).unwrap_or_default();
    }
    (theme, size)
}

fn niri_conf() -> PathBuf {
    config_dir().join("niri/config.kdl")
}

/// строки начала и конца блока `cursor { … }` на верхнем уровне конфига niri (`cursor{` и
/// комментарий в конце строки — тоже он)
fn block(t: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = t.lines().collect();
    let code = |l: &str| l.split("//").next().unwrap_or("").trim_end().to_string();
    let top = |l: &str| !l.starts_with(char::is_whitespace);
    let a = lines.iter().position(|l| top(l) && code(l).strip_prefix("cursor").is_some_and(|r| r.trim_start() == "{"))?;
    let b = lines.iter().skip(a + 1).position(|l| top(l) && code(l) == "}")? + a + 1;
    Some((a, b))
}

/// поставить тему и размер в блок cursor конфига niri (нет блока — дописать в конец). niri сам
/// перечитывает конфиг. Перед первой правкой — копия рядом (config.kdl.yami-bak)
fn niri(theme: &str, size: i32) {
    let conf = niri_conf();
    let Ok(t) = std::fs::read_to_string(&conf) else { return };
    let q = theme.replace('\\', "\\\\").replace('"', "\\\"");
    let (tl, sl) = (format!("    xcursor-theme \"{q}\""), format!("    xcursor-size {size}"));
    let mut lines: Vec<String> = t.lines().map(String::from).collect();
    match block(&t) {
        Some((a, b)) => {
            let mut b = b;
            for (key, line) in [("xcursor-theme", &tl), ("xcursor-size", &sl)] {
                match (a + 1..b).find(|&i| lines[i].trim_start().starts_with(key)) {
                    Some(i) => lines[i] = line.clone(),
                    None => {
                        lines.insert(a + 1, line.clone());
                        b += 1;
                    }
                }
            }
        }
        None => {
            lines.push(String::new());
            lines.extend(["cursor {".to_string(), tl, sl, "}".to_string()]);
        }
    }
    let text = lines.join("\n") + "\n";
    if text == t {
        return;
    }
    let bak = conf.with_extension("kdl.yami-bak");
    if !bak.exists() {
        let _ = std::fs::copy(&conf, &bak);
    }
    put(&conf, &text);
}

/// записать, не разрывая ссылку (конфиг из dotfiles часто ссылка)
fn put(p: &Path, text: &str) {
    if p.is_symlink() {
        let _ = std::fs::write(p, text);
    } else {
        let _ = crate::config::write_atomic(p, text);
    }
}

/// разослать тему в нити (gsettings и flatpak — внешние процессы). Нити идут по одной, и каждая
/// берёт самую свежую тему: быстрые щелчки не пишут конфиги наперегонки
pub fn apply_bg(theme: String, size: i32) {
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some((theme, size));
    std::thread::spawn(|| {
        let _run = APPLY.lock().unwrap_or_else(|e| e.into_inner());
        let Some((theme, size)) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
        apply(&theme, size);
    });
}

/// тема везде
fn apply(theme: &str, size: i32) {
    if theme.is_empty() {
        return;
    }
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some((theme.to_string(), size));
    niri(theme, size);
    if crate::proc::which("gsettings") {
        for (k, v) in [("cursor-theme", theme.to_string()), ("cursor-size", size.to_string())] {
            let _ = std::process::Command::new("gsettings").args(["set", "org.gnome.desktop.interface", k, &v]).output();
        }
    }
    for v in ["gtk-3.0", "gtk-4.0"] {
        let conf = config_dir().join(v).join("settings.ini");
        let _ = std::fs::create_dir_all(conf.parent().unwrap_or(Path::new(".")));
        crate::recolor::set_ini(&conf, "Settings", &[("gtk-cursor-theme-name", theme.to_string()), ("gtk-cursor-theme-size", size.to_string())]);
    }
    // X11 и всё, что ищет тему «default»
    let def = yami_px::paths::home().join(".icons/default");
    if std::fs::create_dir_all(&def).is_ok() {
        put(&def.join("index.theme"), &format!("[Icon Theme]\nName=Default\nComment=Default Cursor Theme\nInherits={theme}\n"));
    }
    // Flatpak: в песочнице свои /usr — темы хоста видны в /run/host, свои — по доступу к ~/.icons
    if crate::proc::which("flatpak") {
        let home = yami_px::paths::home();
        let path = format!("/run/host/user-share/icons:/run/host/share/icons:{}", home.join(".icons").display());
        let _ = std::process::Command::new("flatpak")
            .args(["override", "--user", &format!("--env=XCURSOR_THEME={theme}"), &format!("--env=XCURSOR_SIZE={size}"), &format!("--env=XCURSOR_PATH={path}"), "--filesystem=~/.icons:ro", "--filesystem=xdg-data/icons:ro"])
            .output();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn niri_block() {
        let t = "input {\n}\n\ncursor {\n    xcursor-theme \"A\"\n    xcursor-size 12\n}\n\nlayout {\n    cursor {\n    }\n}\n";
        assert_eq!(super::block(t), Some((3, 6)));
        assert_eq!(super::block("layout {\n}\n"), None);
        assert_eq!(super::block("cursor{ // тема\n\txcursor-size 12\n}\n"), Some((0, 2)));
    }
}
