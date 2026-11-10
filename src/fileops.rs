// Файлы для рабочего стола: тип по расширению (база shared-mime-info, без процессов), значок типа,
// копия и перенос с именем «имя (2)» при совпадении, корзина (gio trash), список text/uri-list.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// база типов: расширение → mime (/usr/share/mime/globs2) и mime → общий значок (generic-icons)
#[derive(Default)]
pub struct Mime {
    ext: HashMap<String, String>,
    generic: HashMap<String, String>,
}

impl Mime {
    pub fn load() -> Mime {
        let mut m = Mime::default();
        for d in yami_px::paths::data_dirs() {
            let dir = d.join("mime");
            if let Ok(t) = std::fs::read_to_string(dir.join("globs2")) {
                m.add_globs(&t);
            }
            if let Ok(t) = std::fs::read_to_string(dir.join("generic-icons")) {
                for l in t.lines() {
                    if let Some((k, v)) = l.split_once(':') {
                        m.generic.entry(k.to_string()).or_insert_with(|| v.to_string());
                    }
                }
            }
        }
        m
    }

    /// строки «вес:mime:шаблон»; берутся простые «*.ext», у одного расширения — с большим весом
    /// (строки идут по убыванию веса: первое и остаётся)
    fn add_globs(&mut self, t: &str) {
        for l in t.lines() {
            let mut f = l.splitn(4, ':');
            let (Some(_), Some(mime), Some(glob)) = (f.next(), f.next(), f.next()) else { continue };
            let Some(ext) = glob.strip_prefix("*.") else { continue };
            if ext.contains(['*', '?', '[']) {
                continue;
            }
            self.ext.entry(ext.to_lowercase()).or_insert_with(|| mime.to_string());
        }
    }

    /// тип файла: папка, по самому длинному известному расширению («.tar.gz» раньше «.gz»)
    pub fn of(&self, name: &str, dir: bool) -> String {
        if dir {
            return "inode/directory".into();
        }
        let low = name.to_lowercase();
        let mut i = 0;
        while let Some(p) = low[i..].find('.') {
            let ext = &low[i + p + 1..];
            if let Some(m) = self.ext.get(ext) {
                return m.clone();
            }
            i += p + 1;
        }
        "application/octet-stream".into()
    }

    /// имена значка типа по порядку: свой («image-png»), общий из базы, общий по группе, запасной
    pub fn icons(&self, mime: &str) -> Vec<String> {
        if mime == "inode/directory" {
            return vec!["folder".into()];
        }
        let mut v = vec![mime.replace('/', "-")];
        if let Some(g) = self.generic.get(mime) {
            v.push(g.clone());
        }
        if let Some((major, _)) = mime.split_once('/') {
            v.push(format!("{major}-x-generic"));
        }
        v.push("text-x-generic".into());
        v.push("unknown".into());
        v
    }
}

/// программы для типа из mimeapps.list (~/.config, затем системные): сначала «по умолчанию»,
/// потом «добавленные» — как GIO; xdg-open второй раздел не читает и открывает чем попало
pub fn default_apps(mime: &str) -> Vec<String> {
    let mut files = vec![yami_px::paths::config_dir().join("mimeapps.list"), yami_px::paths::data_dir().join("applications/mimeapps.list")];
    files.extend(yami_px::paths::data_dirs().into_iter().map(|d| d.join("applications/mimeapps.list")));
    let texts: Vec<String> = files.iter().filter_map(|f| std::fs::read_to_string(f).ok()).collect();
    mimeapps_pick(&texts, mime)
}

fn mimeapps_pick(texts: &[String], mime: &str) -> Vec<String> {
    let mut out = vec![];
    for section in ["[Default Applications]", "[Added Associations]"] {
        for t in texts {
            let mut on = false;
            for l in t.lines().map(str::trim) {
                if l.starts_with('[') {
                    on = l == section;
                    continue;
                }
                let Some((k, v)) = l.split_once('=') else { continue };
                if on && k.trim() == mime {
                    out.extend(v.split(';').map(str::trim).filter(|x| !x.is_empty() && !out.iter().any(|o: &String| o == x)).map(String::from).collect::<Vec<_>>());
                }
            }
        }
    }
    out
}

/// свободное имя в папке: «имя», дальше «имя (2).ext», «имя (3).ext»…
pub fn unique(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if std::fs::symlink_metadata(&p).is_err() {
        return p;
    }
    // расширение — после последней точки, но не у скрытого «.config»
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..).map(|k| dir.join(format!("{stem} ({k}){ext}"))).find(|p| std::fs::symlink_metadata(p).is_err()).unwrap_or(p)
}

fn copy_rec(src: &Path, dst: &Path) -> std::io::Result<()> {
    let md = std::fs::symlink_metadata(src)?;
    if md.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)
    } else if md.is_dir() {
        std::fs::create_dir(dst)?;
        for e in std::fs::read_dir(src)? {
            let e = e?;
            copy_rec(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(src, dst).map(|_| ())
    }
}

/// скопировать файл или папку в dir; имя занято — «(2)»
pub fn copy_into(src: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    let name = src.file_name().ok_or(std::io::ErrorKind::InvalidInput)?.to_string_lossy().to_string();
    // папку в саму себя не копировать
    if dir.starts_with(src) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let dst = unique(dir, &name);
    copy_rec(src, &dst)?;
    Ok(dst)
}

/// перенести в dir: на том же диске — переименованием, на другом — копией и удалением
pub fn move_into(src: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    let name = src.file_name().ok_or(std::io::ErrorKind::InvalidInput)?.to_string_lossy().to_string();
    if src.parent() == Some(dir) {
        return Ok(src.to_path_buf());
    }
    if dir.starts_with(src) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let dst = unique(dir, &name);
    match std::fs::rename(src, &dst) {
        Ok(()) => Ok(dst),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_rec(src, &dst)?;
            if std::fs::symlink_metadata(src)?.is_dir() {
                std::fs::remove_dir_all(src)?;
            } else {
                std::fs::remove_file(src)?;
            }
            Ok(dst)
        }
        Err(e) => Err(e),
    }
}

/// в корзину (gio trash: по спецификации freedesktop, с восстановлением из файлового менеджера)
pub fn trash(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let st = std::process::Command::new("gio").arg("trash").arg("--").args(paths).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    match st {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(crate::tf!("не вышло убрать в корзину", "could not move to trash")),
        Err(_) => Err(crate::tf!("нет gio (glib2)", "no gio (glib2)")),
    }
}

/// text/uri-list: строки file://, комментарии «#» и пустые — мимо
pub fn parse_uri_list(s: &str) -> Vec<PathBuf> {
    s.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).filter_map(yami_px::icons::uri_path).collect()
}

/// список путей в text/uri-list
pub fn uri_list(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| yami_px::icons::file_uri(p) + "\r\n").collect()
}

/// размер для людей: 1.5 КБ, 3.2 МБ
pub fn human(b: u64) -> String {
    let b = b as f64;
    if b < 1024.0 {
        crate::tf!("{} Б", "{} B", b as u64)
    } else if b < 1024.0 * 1024.0 {
        crate::tf!("{:.1} КБ", "{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        crate::tf!("{:.1} МБ", "{:.1} MB", b / 1048576.0)
    } else {
        crate::tf!("{:.1} ГБ", "{:.1} GB", b / 1073741824.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_by_longest_extension() {
        let mut m = Mime::default();
        m.add_globs("50:application/x-compressed-tar:*.tar.gz\n50:application/gzip:*.gz\n50:image/png:*.png\n50:text/x-c:*.[ch]\n");
        assert_eq!(m.of("a.tar.gz", false), "application/x-compressed-tar");
        assert_eq!(m.of("b.GZ", false), "application/gzip");
        assert_eq!(m.of("Pasted image.png", false), "image/png");
        assert_eq!(m.of("x.c", false), "application/octet-stream");
        assert_eq!(m.of("Папка", true), "inode/directory");
        assert_eq!(m.icons("image/png")[..2], ["image-png".to_string(), "image-x-generic".to_string()]);
    }

    #[test]
    fn copies_get_numbered_names() {
        let d = std::env::temp_dir().join(format!("yami-fileops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        std::fs::create_dir(d.join(".config")).unwrap();
        assert_eq!(unique(&d, "notes.txt"), d.join("notes (2).txt"));
        assert_eq!(unique(&d, ".config"), d.join(".config (2)"));
        assert_eq!(unique(&d, "new.txt"), d.join("new.txt"));
        let sub = d.join("sub");
        std::fs::create_dir(&sub).unwrap();
        assert_eq!(copy_into(&d.join("notes.txt"), &sub).unwrap(), sub.join("notes.txt"));
        assert_eq!(move_into(&sub.join("notes.txt"), &d).unwrap(), d.join("notes (2).txt"));
        // папку в саму себя — нельзя
        assert!(copy_into(&d, &sub).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn default_app_falls_back_to_added_associations() {
        let user = "[Added Associations]\nimage/png=org.gnome.Loupe.desktop;zen.desktop;\n[Default Applications]\ntext/plain=gedit.desktop\n".to_string();
        assert_eq!(mimeapps_pick(&[user.clone()], "image/png"), ["org.gnome.Loupe.desktop", "zen.desktop"]);
        let sys = "[Default Applications]\nimage/png=eog.desktop\n".to_string();
        // «по умолчанию» из любого файла — раньше добавленных
        assert_eq!(mimeapps_pick(&[user, sys], "image/png")[0], "eog.desktop");
    }

    #[test]
    fn uri_list_roundtrip() {
        let ps = vec![PathBuf::from("/home/u/Desktop/Новая папка"), PathBuf::from("/tmp/a b.txt")];
        let s = uri_list(&ps);
        assert!(s.starts_with("file:///home/u/Desktop/%D0"));
        assert_eq!(parse_uri_list(&format!("# comment\r\n{s}\r\n")), ps);
    }
}
