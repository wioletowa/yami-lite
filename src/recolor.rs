// Перекраска чужих программ по палитре шелла, как у Noctalia: только выбранные (настройки → цвет →
// программы), только цвета — никаких своих тем, иконок и курсоров. Каждая программа получает
// свой файл цветов рядом с конфигом и одну строку подключения в нём; выключил — строка уходит,
// остальной конфиг не трогается.

use crate::config::config_dir;
use crate::gfx::C;
use crate::theme::Theme;
use std::path::{Path, PathBuf};

/// программы: ключ (themeApps), имя, что пишется
pub const APPS: &[(&str, &str, &str)] = &[
    ("gtk", "GTK 3 и 4", "@define-color в gtk.css, тёмная или светлая схема"),
    ("qt", "Qt (qt6ct, qt5ct)", "цветовая схема qt6ct и qt5ct"),
    ("kitty", "kitty", "include в kitty.conf"),
    ("foot", "foot", "include в foot.ini"),
    ("alacritty", "alacritty", "import в alacritty.toml"),
    ("fuzzel", "fuzzel", "include в fuzzel.ini"),
    ("niri", "niri", "рамки окон: include в конце config.kdl"),
];

/// метка наших строк и файлов
const MARK: &str = "yami-lite";

/// всё, что нужно для перекраски, — снимок темы (в нить уходит копия)
#[derive(Clone)]
pub struct Pal {
    bg: C,
    panel: C,
    surface: C,
    surface_hi: C,
    code_bg: C,
    border: C,
    dim: C,
    accent: C,
    deep: C,
    bright: C,
    selection: C,
    text: C,
    text_dim: C,
    text_faint: C,
    text_hi: C,
    on_accent: C,
    ok: C,
    warn: C,
    light: bool,
}

impl Pal {
    pub fn of(t: &Theme) -> Pal {
        Pal {
            bg: t.bg,
            panel: t.panel,
            surface: t.surface,
            surface_hi: t.surface_hi,
            code_bg: t.code_bg,
            border: t.border,
            dim: t.dim,
            accent: t.red,
            deep: t.blood,
            bright: t.ember,
            selection: t.selection,
            text: t.text,
            text_dim: t.text_dim,
            text_faint: t.text_faint,
            text_hi: t.text_hi,
            on_accent: t.on_accent,
            ok: t.ok,
            warn: t.warn,
            light: t.light,
        }
    }

    /// 16 цветов терминала: ANSI чуть в сторону акцента; на светлом фоне — темнее
    fn ansi(&self) -> [C; 16] {
        let t = |h: u32| {
            let c = C(0xff000000 | h).lerp(self.accent, 0.18);
            if self.light {
                C(0xff000000 | scale(c.0, 0.7))
            } else {
                c
            }
        };
        let red = |d: u32, l: u32| C(0xff000000 | if self.light { l } else { d });
        [
            if self.light { self.text_dim } else { self.panel },
            red(0xd0352d, 0xb02a22),
            t(0x78b86a),
            t(0xd9a03a),
            t(0x5a7fa8),
            t(0x9a5aa0),
            t(0x4f9a96),
            self.text_dim,
            self.text_faint,
            red(0xff5a4c, 0xd0352d),
            t(0xa2d692),
            t(0xf0c46a),
            t(0x86a6c8),
            t(0xc486c8),
            t(0x7cc2bc),
            self.text,
        ]
    }
}

fn scale(rgb: u32, k: f32) -> u32 {
    let ch = |s: u32| (((rgb >> s) & 255) as f32 * k).round().clamp(0.0, 255.0) as u32;
    ch(16) << 16 | ch(8) << 8 | ch(0)
}

fn h(c: C) -> String {
    c.hex_str()
}

/// без «#»
fn hs(c: C) -> String {
    c.hex_str().trim_start_matches('#').to_string()
}

fn write(p: &Path, text: &str) {
    let _ = crate::config::write_atomic(p, text);
}

/// дописать строку подключения в конец конфига (если её там нет); ссылку (stow, Home Manager)
/// пишем насквозь, а не подменяем файлом
fn attach(conf: &Path, line: &str, before: &str) {
    let t = std::fs::read_to_string(conf).unwrap_or_default();
    if t.lines().any(|l| l.trim() == line) {
        return;
    }
    let sep = if t.is_empty() || t.ends_with('\n') { "" } else { "\n" };
    let text = format!("{t}{sep}\n// {MARK}: palette (settings → color → apps)\n{before}{line}\n").replace("// ", comment(conf));
    put(conf, &text);
}

/// префикс комментария по формату файла
fn comment(conf: &Path) -> &'static str {
    match conf.extension().and_then(|e| e.to_str()) {
        Some("kdl") => "// ",
        Some("css") => "/* ",
        _ => "# ",
    }
}

/// убрать наши строки: метку, строку подключения и (для ini) наш заголовок секции после метки
fn detach(conf: &Path, line: &str) {
    let Ok(t) = std::fs::read_to_string(conf) else { return };
    let mut out: Vec<&str> = vec![];
    let mut after_mark = false;
    for l in t.lines() {
        let tl = l.trim();
        if tl.contains(&format!("{MARK}: palette")) {
            after_mark = true;
            continue;
        }
        if tl == line || (after_mark && tl.starts_with('[')) {
            continue;
        }
        after_mark = false;
        out.push(l);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let text = out.join("\n") + "\n";
    if text != t {
        put(conf, &text);
    }
}

fn put(conf: &Path, text: &str) {
    if conf.is_symlink() {
        let _ = std::fs::write(conf, text);
    } else {
        write(conf, text);
    }
}

fn cfg(rel: &str) -> PathBuf {
    config_dir().join(rel)
}

/// перекрасить выбранные программы и отцепить остальные. Медленные вызовы (gsettings, pkill) —
/// здесь же: зовётся из нити
pub fn apply(p: &Pal, apps: &[String]) {
    let on = |id: &str| apps.iter().any(|a| a == id);
    gtk(p, on("gtk"));
    qt(p, on("qt"));
    kitty(p, on("kitty"));
    foot(p, on("foot"));
    alacritty(p, on("alacritty"));
    fuzzel(p, on("fuzzel"));
    niri(p, on("niri"));
}

fn gtk(p: &Pal, on: bool) {
    let line = format!("@import url(\"{MARK}-colors.css\");");
    for v in ["gtk-3.0", "gtk-4.0"] {
        let dir = cfg(v);
        let main = dir.join("gtk.css");
        if !on {
            detach(&main, &line);
            continue;
        }
        let side = |a: C, b: C| a.lerp(b, 0.5);
        let d: [(&str, String); 27] = [
            ("accent_color", h(p.bright)),
            ("accent_bg_color", h(p.accent)),
            ("accent_fg_color", h(p.on_accent)),
            ("destructive_bg_color", h(p.deep)),
            ("success_color", h(p.ok)),
            ("warning_color", h(p.warn)),
            ("window_bg_color", h(p.panel)),
            ("window_fg_color", h(p.text)),
            ("view_bg_color", h(p.bg)),
            ("view_fg_color", h(p.text)),
            ("headerbar_bg_color", h(p.panel)),
            ("headerbar_fg_color", h(p.text)),
            ("headerbar_border_color", h(p.border)),
            ("headerbar_backdrop_color", h(p.bg)),
            ("sidebar_bg_color", h(side(p.bg, p.panel))),
            ("sidebar_fg_color", h(p.text)),
            ("sidebar_backdrop_color", h(p.bg)),
            ("card_bg_color", h(p.surface)),
            ("card_fg_color", h(p.text)),
            ("dialog_bg_color", h(p.panel)),
            ("dialog_fg_color", h(p.text)),
            ("popover_bg_color", h(p.panel)),
            ("popover_fg_color", h(p.text)),
            ("thumbnail_bg_color", h(p.surface)),
            ("thumbnail_fg_color", h(p.text)),
            ("borders", h(p.border)),
            ("theme_selected_bg_color", h(p.selection)),
        ];
        let mut css = format!("/* {MARK}: written by the shell from its palette; rewritten on every change */\n");
        for (k, v) in d {
            css += &format!("@define-color {k} {v};\n");
        }
        // старые темы GTK3 читают свои имена
        for (k, v) in [("theme_bg_color", p.panel), ("theme_fg_color", p.text), ("theme_base_color", p.bg), ("theme_text_color", p.text), ("theme_selected_fg_color", p.text_hi)] {
            css += &format!("@define-color {k} {};\n", h(v));
        }
        write(&dir.join(format!("{MARK}-colors.css")), &css);
        // @import — первой строкой: в CSS он после правил не действует
        let t = std::fs::read_to_string(&main).unwrap_or_default();
        if !t.lines().any(|l| l.trim() == line) {
            put(&main, &format!("/* {MARK}: palette (settings → color → apps) */\n{line}\n{t}"));
        }
    }
    if on && crate::proc::which("gsettings") {
        let scheme = if p.light { "prefer-light" } else { "prefer-dark" };
        let _ = std::process::Command::new("gsettings").args(["set", "org.gnome.desktop.interface", "color-scheme", scheme]).output();
    }
    if on {
        gtk_variant(p.light);
    }
}

/// тема GTK в пару к схеме: Electron (Discord, Claude) и старые программы смотрят не только на
/// color-scheme, но и на имя темы (adw-gtk3-dark). Меняется только суффикс -dark у своей темы, и
/// только если вторая из пары установлена. gtk-application-prefer-dark-theme — всегда 0: GTK
/// читает его один раз при запуске программы, и Chromium на смене темы спрашивал бы старое «тёмная»
/// поверх светлой схемы из портала — программа светлела и тут же темнела обратно
fn gtk_variant(light: bool) {
    let get = std::process::Command::new("gsettings").args(["get", "org.gnome.desktop.interface", "gtk-theme"]).output();
    let cur = get.ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string()).unwrap_or_default();
    if let Some(want) = variant_name(&cur, light).filter(|w| theme_installed(w)) {
        let _ = std::process::Command::new("gsettings").args(["set", "org.gnome.desktop.interface", "gtk-theme", &want]).output();
    }
    for v in ["gtk-3.0", "gtk-4.0"] {
        let conf = cfg(v).join("settings.ini");
        let Ok(t) = std::fs::read_to_string(&conf) else { continue };
        let mut kv = vec![("gtk-application-prefer-dark-theme", "0".to_string())];
        if let Some(want) = t.lines().find_map(|l| l.trim().strip_prefix("gtk-theme-name=")).and_then(|n| variant_name(n.trim(), light)).filter(|w| theme_installed(w)) {
            kv.push(("gtk-theme-name", want));
        }
        set_ini(&conf, "Settings", &kv);
    }
}

/// имя пары к теме: светлая — без -dark, тёмная — с ним; None — менять нечего
fn variant_name(cur: &str, light: bool) -> Option<String> {
    let base = cur.strip_suffix("-dark").unwrap_or(cur);
    let want = if light { base.to_string() } else { format!("{base}-dark") };
    (!base.is_empty() && want != cur).then_some(want)
}

fn theme_installed(name: &str) -> bool {
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    [home.join(".themes"), home.join(".local/share/themes"), std::path::PathBuf::from("/usr/share/themes")].iter().any(|d| d.join(name).is_dir())
}

fn qt(p: &Pal, on: bool) {
    // роли QPalette по порядку: WindowText, Button, Light, Midlight, Dark, Mid, Text, BrightText,
    // ButtonText, Base, Window, Shadow, Highlight, HighlightedText, Link, LinkVisited,
    // AlternateBase, NoRole, ToolTipBase, ToolTipText, PlaceholderText
    let black = C(0xff000000);
    let act = [p.text, p.surface, p.border, p.surface_hi, p.code_bg, p.panel, p.text, p.text_hi, p.text, p.bg, p.panel, black, p.selection, p.text_hi, p.bright, p.accent, p.bg.lerp(p.panel, 0.6), black, p.panel, p.text, p.text_faint];
    let mut dis = act;
    for i in [0, 6, 8, 13, 19] {
        dis[i] = p.text_faint;
    }
    dis[12] = p.surface_hi;
    let mut ina = act;
    ina[12] = p.dim;
    let j = |xs: &[C; 21]| xs.iter().map(|c| format!("#ff{}", hs(*c))).collect::<Vec<_>>().join(", ");
    let scheme = format!("[ColorScheme]\nactive_colors={}\ndisabled_colors={}\ninactive_colors={}\n", j(&act), j(&dis), j(&ina));
    for v in ["qt6ct", "qt5ct"] {
        let dir = cfg(v);
        let file = dir.join(format!("colors/{MARK}.conf"));
        let conf = dir.join(format!("{v}.conf"));
        if on {
            // qt5ct без qt5ct не нужен
            if v == "qt5ct" && !dir.is_dir() && !crate::proc::which("qt5ct") {
                continue;
            }
            write(&file, &scheme);
            set_ini(&conf, "Appearance", &[("color_scheme_path", file.display().to_string()), ("custom_palette", "true".into())]);
        } else if std::fs::read_to_string(&conf).is_ok_and(|t| t.contains(&format!("colors/{MARK}.conf"))) {
            set_ini(&conf, "Appearance", &[("custom_palette", "false".into())]);
        }
    }
}

/// ключи в секции ini: заменить или дописать; секции нет — добавить
fn set_ini(conf: &Path, sec: &str, kv: &[(&str, String)]) {
    let t = std::fs::read_to_string(conf).unwrap_or_default();
    let mut lines: Vec<String> = t.lines().map(String::from).collect();
    let head = format!("[{sec}]");
    let start = match lines.iter().position(|l| l.trim() == head) {
        Some(i) => i,
        None => {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(head);
            lines.len() - 1
        }
    };
    for (k, v) in kv {
        let end = lines.iter().skip(start + 1).position(|l| l.trim_start().starts_with('[')).map(|i| start + 1 + i).unwrap_or(lines.len());
        let item = format!("{k}={v}");
        match (start + 1..end).find(|&i| lines[i].split('=').next().map(str::trim) == Some(k)) {
            Some(i) => lines[i] = item,
            None => lines.insert(end, item),
        }
    }
    let text = lines.join("\n") + "\n";
    if text != t {
        put(conf, &text);
    }
}

fn kitty(p: &Pal, on: bool) {
    let dir = cfg("kitty");
    let line = format!("include {MARK}-colors.conf");
    if !on {
        detach(&dir.join("kitty.conf"), &line);
        return;
    }
    if !dir.is_dir() && !crate::proc::which("kitty") {
        return;
    }
    let a = p.ansi();
    let mut cols: Vec<(String, C)> = vec![
        ("foreground".into(), p.text),
        ("background".into(), p.bg),
        ("selection_foreground".into(), p.text_hi),
        ("selection_background".into(), p.selection),
        ("cursor".into(), p.bright),
        ("cursor_text_color".into(), p.bg),
        ("url_color".into(), p.bright),
        ("active_border_color".into(), p.accent),
        ("inactive_border_color".into(), p.border),
        ("bell_border_color".into(), p.bright),
        ("active_tab_foreground".into(), p.on_accent),
        ("active_tab_background".into(), p.accent),
        ("inactive_tab_foreground".into(), p.text_dim),
        ("inactive_tab_background".into(), p.panel),
        ("tab_bar_background".into(), p.bg),
    ];
    cols.extend(a.iter().enumerate().map(|(i, c)| (format!("color{i}"), *c)));
    let body: String = cols.iter().map(|(k, c)| format!("{k:<24}{}\n", h(*c))).collect();
    write(&dir.join(format!("{MARK}-colors.conf")), &format!("# {MARK}: written by the shell from its palette\n{body}"));
    attach(&dir.join("kitty.conf"), &line, "");
    let _ = std::process::Command::new("pkill").args(["-USR1", "-x", "kitty"]).output();
}

fn foot(p: &Pal, on: bool) {
    let dir = cfg("foot");
    let file = dir.join(format!("{MARK}-colors.ini"));
    let line = format!("include={}", file.display());
    if !on {
        detach(&dir.join("foot.ini"), &line);
        return;
    }
    if !dir.is_dir() && !crate::proc::which("foot") {
        return;
    }
    let a = p.ansi();
    // foot 1.22: [colors] стала [colors-dark] (старое имя читается с предупреждением)
    let sec = std::process::Command::new("foot")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            let v: Vec<u32> = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).take(2).filter_map(|x| x.parse().ok()).collect();
            (v.len() == 2).then(|| (v[0], v[1]) >= (1, 22))
        })
        .map_or("colors", |new| if new { "colors-dark" } else { "colors" });
    let mut l = vec![format!("# {MARK}: written by the shell from its palette"), format!("[{sec}]")];
    l.push(format!("foreground={}", hs(p.text)));
    l.push(format!("background={}", hs(p.bg)));
    l.push(format!("selection-foreground={}", hs(p.text_hi)));
    l.push(format!("selection-background={}", hs(p.selection)));
    l.push(format!("cursor={} {}", hs(p.bg), hs(p.bright)));
    l.push(format!("urls={}", hs(p.bright)));
    for i in 0..8 {
        l.push(format!("regular{i}={}", hs(a[i])));
    }
    for i in 0..8 {
        l.push(format!("bright{i}={}", hs(a[i + 8])));
    }
    write(&file, &(l.join("\n") + "\n"));
    attach(&dir.join("foot.ini"), &line, "[main]\n");
}

fn alacritty(p: &Pal, on: bool) {
    let dir = cfg("alacritty");
    let file = dir.join(format!("{MARK}-colors.toml"));
    let conf = dir.join("alacritty.toml");
    let line = format!("import = [\"{}\"]", file.display());
    if !on {
        detach(&conf, &line);
        return;
    }
    if !dir.is_dir() && !crate::proc::which("alacritty") {
        return;
    }
    let a = p.ansi();
    let names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    let mut t = format!("# {MARK}: written by the shell from its palette\n[colors.primary]\nforeground = \"{}\"\nbackground = \"{}\"\n", h(p.text), h(p.bg));
    t += &format!("[colors.cursor]\ntext = \"{}\"\ncursor = \"{}\"\n", h(p.bg), h(p.bright));
    t += &format!("[colors.selection]\ntext = \"{}\"\nbackground = \"{}\"\n", h(p.text_hi), h(p.selection));
    for (sec, off) in [("normal", 0), ("bright", 8)] {
        t += &format!("[colors.{sec}]\n");
        for (i, n) in names.iter().enumerate() {
            t += &format!("{n} = \"{}\"\n", h(a[i + off]));
        }
    }
    write(&file, &t);
    // своя секция [general] в конфиге — вторую дописать нельзя (TOML): тогда import — руками
    let cur = std::fs::read_to_string(&conf).unwrap_or_default();
    if !cur.lines().any(|l| l.trim() == "[general]") {
        attach(&conf, &line, "[general]\n");
    }
}

fn fuzzel(p: &Pal, on: bool) {
    let dir = cfg("fuzzel");
    let file = dir.join(format!("{MARK}-colors.ini"));
    let line = format!("include={}", file.display());
    if !on {
        detach(&dir.join("fuzzel.ini"), &line);
        return;
    }
    if !dir.is_dir() && !crate::proc::which("fuzzel") {
        return;
    }
    let ff = |c: C| format!("{}ff", hs(c));
    let t = format!(
        "# {MARK}: written by the shell from its palette\n[colors]\nbackground={}\ntext={}\nprompt={}\nplaceholder={}\ninput={}\nmatch={}\nselection={}\nselection-text={}\nselection-match={}\ncounter={}\nborder={}\n",
        format!("{}f2", hs(p.panel)),
        ff(p.text),
        ff(p.accent),
        ff(p.text_faint),
        ff(p.text),
        ff(p.bright),
        ff(p.selection),
        ff(p.text_hi),
        ff(p.bright),
        ff(p.text_dim),
        ff(p.accent)
    );
    write(&file, &t);
    attach(&dir.join("fuzzel.ini"), &line, "[main]\n");
}

fn niri(p: &Pal, on: bool) {
    let dir = cfg("niri");
    let line = format!("include \"{MARK}-colors.kdl\"");
    if !on {
        detach(&dir.join("config.kdl"), &line);
        return;
    }
    if !dir.join("config.kdl").exists() {
        return;
    }
    let (a, b, br) = (h(p.accent), h(p.border), h(p.bright));
    let t = format!(
        "// {MARK}: written by the shell from its palette; rewritten on every change\nlayout {{\n    border {{\n        active-color \"{a}\"\n        inactive-color \"{b}\"\n        urgent-color \"{br}\"\n    }}\n    focus-ring {{\n        active-color \"{a}\"\n        inactive-color \"{b}\"\n        urgent-color \"{br}\"\n    }}\n    tab-indicator {{\n        active-color \"{a}\"\n        inactive-color \"{b}\"\n        urgent-color \"{br}\"\n    }}\n    insert-hint {{\n        color \"{a}60\"\n    }}\n}}\noverview {{\n    backdrop-color \"{}\"\n}}\n",
        h(p.bg)
    );
    write(&dir.join(format!("{MARK}-colors.kdl")), &t);
    // в конце: так наши цвета перекрывают заданные выше
    attach(&dir.join("config.kdl"), &line, "");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_theme_pairs_by_dark_suffix() {
        assert_eq!(variant_name("adw-gtk3-dark", true).as_deref(), Some("adw-gtk3"));
        assert_eq!(variant_name("adw-gtk3", false).as_deref(), Some("adw-gtk3-dark"));
        assert_eq!(variant_name("adw-gtk3", true), None);
        assert_eq!(variant_name("", false), None);
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("yami-lite-recolor-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    #[test]
    fn attach_then_detach_leaves_the_file_as_it_was() {
        let f = tmp("a").join("foot.ini");
        let orig = "[main]\nfont=Terminus:size=12\n";
        std::fs::write(&f, orig).unwrap();
        attach(&f, "include=/x/colors.ini", "[main]\n");
        attach(&f, "include=/x/colors.ini", "[main]\n");
        let t = std::fs::read_to_string(&f).unwrap();
        assert_eq!(t.matches("include=/x/colors.ini").count(), 1);
        detach(&f, "include=/x/colors.ini");
        assert_eq!(std::fs::read_to_string(&f).unwrap(), orig);
    }

    #[test]
    fn ini_keys_replaced_in_their_section() {
        let f = tmp("b").join("qt6ct.conf");
        std::fs::write(&f, "[Appearance]\ncustom_palette=false\nstyle=Fusion\n\n[Fonts]\nfixed=x\n").unwrap();
        set_ini(&f, "Appearance", &[("custom_palette", "true".into()), ("color_scheme_path", "/p".into())]);
        let t = std::fs::read_to_string(&f).unwrap();
        assert!(t.contains("custom_palette=true") && !t.contains("custom_palette=false"));
        assert!(t.find("color_scheme_path=/p").unwrap() < t.find("[Fonts]").unwrap());
    }
}
