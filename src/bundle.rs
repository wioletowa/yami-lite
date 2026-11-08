// Свои темы: ~/.config/yami-lite/themes/<имя>.json — только вид (палитра и тона, светлая или
// тёмная, эффекты, шрифт, бар, виджеты, вид дока) и картинка обоев внутри (base64). Тема ничего
// не может запустить: в ней нет ни команд, ни путей этой машины. `yami-lite theme save | list |
// apply | remove` и раздел «темы» во вкладке «цвет».
// i18n: inline pairs

use crate::cmd::Sty;
use crate::config::{config_dir, settings_path, write_atomic};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const FORMAT: &str = "yami-bundle";

/// ключи вида (camelCase, как в settings.json)
fn is_look(k: &str) -> bool {
    const EXACT: &[&str] = &[
        "accent", "palette", "tint", "light", "glass", "lift", "inner", "dim", "blurShell", "animations", "animSpeed", "animClose", "reveal", "revealGrain", "scanlines", "iconPixel", "iconTint",
        "cpuStyle", "tempStyle", "trayMode", "clockDate", "clockSeconds", "taskWidth", "widgets", "widgetContrast", "mediaViz", "mediaView",
    ];
    // barOutputs, dockPins, wallDir — имена экранов, программы и папки этой машины
    EXACT.contains(&k) || ((k.starts_with("bar") || k.starts_with("wall") || k.starts_with("font") || k.starts_with("dock")) && !matches!(k, "dockPins" | "wallpaper" | "wallDir" | "barOutputs"))
}

// ── base64 (RFC 4648; crate ради 40 строк не нужен) ───────────────────────────
const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64(d: &[u8]) -> String {
    let mut s = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

pub fn unb64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for ch in s.bytes().filter(|c| !c.is_ascii_whitespace()) {
        if ch == b'=' {
            break;
        }
        let v = B64.iter().position(|&x| x == ch)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn settings_value() -> Map<String, Value> {
    std::fs::read_to_string(settings_path()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// картинка обоев в файл (подготовленная шеллом копия не нужна: она делается заново)
fn wall_entry(set: &Map<String, Value>) -> Option<Value> {
    let p = set.get("wallpaper")?.as_str()?;
    if p.is_empty() {
        return None;
    }
    let path = Path::new(p);
    let data = std::fs::read(path).ok()?;
    // больше 32 МБ — не обои, чтобы пересылать их файлом
    if data.len() > 32 << 20 {
        return None;
    }
    Some(json!({ "name": path.file_name()?.to_string_lossy(), "data": b64(&data) }))
}

/// тема из текущих настроек: все ключи вида (и те, которых в файле ещё нет) и картинка обоев.
/// Через текст, не to_value: f32 там становится f64 (0.78 → 0.7799999713897705)
fn bundle() -> Value {
    let set = serde_json::to_string(&crate::config::Settings::load()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_else(settings_value);
    let s: Map<String, Value> = set.iter().filter(|(k, _)| is_look(k)).map(|(k, v)| (k.clone(), v.clone())).collect();
    let mut b = json!({ "format": FORMAT, "version": 1, "kind": "theme", "yami": env!("CARGO_PKG_VERSION"), "settings": s });
    if let Some(w) = wall_entry(&set) {
        b["wallpaper"] = w;
    }
    b
}

const MAX_FILE: u64 = 64 << 20;

/// файл yami: не больше 64 МБ, JSON нашего формата
fn load(file: &Path) -> Result<Value, String> {
    if std::fs::metadata(file).map(|m| m.len() > MAX_FILE).unwrap_or(false) {
        return Err(crate::tf!("слишком большой файл: {}", "file too large: {}", file.display()));
    }
    let b: Value = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or_else(|| crate::tf!("не читается: {}", "cannot read: {}", file.display()))?;
    if b["format"] != FORMAT {
        return Err(crate::tf!("это не тема yami", "not a yami theme"));
    }
    Ok(b)
}

pub fn themes_dir() -> PathBuf {
    config_dir().join("yami-lite/themes")
}

/// имя темы (оно же имя файла): буквы, цифры, пробел, «-», «_»; не больше 40 знаков
pub fn theme_name(s: &str) -> String {
    let n: String = s.chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).take(40).collect();
    n.trim().to_string()
}

fn theme_path(name: &str) -> Option<PathBuf> {
    let n = theme_name(name);
    (!n.is_empty()).then(|| themes_dir().join(format!("{n}.json")))
}

/// файл темы из библиотеки: карточка зовёт его по имени файла, а файл, положенный руками, может
/// зваться иначе, чем дал бы theme_name («Nord (dark).json»): сначала как есть, потом очищенное
fn theme_file(name: &str) -> Option<PathBuf> {
    let raw = (!name.is_empty() && !name.contains('/') && !name.starts_with('.')).then(|| themes_dir().join(format!("{name}.json")));
    raw.filter(|p| p.is_file()).or_else(|| theme_path(name).filter(|p| p.is_file()))
}

/// карточка темы для настроек: имя, цвет акцента, светлая ли, есть ли своя картинка
#[derive(Clone, Debug)]
pub struct Card {
    pub name: String,
    pub accent: u32,
    pub light: bool,
    pub wall: bool,
}

/// цвет акцента темы: свой (#rrggbb) или пресета палитры
fn accent_of(s: &Value) -> u32 {
    let own = s["accent"].as_str().and_then(yami_px::theme::parse_hex);
    own.or_else(|| yami_px::theme::PRESETS.iter().find(|p| Some(p.id) == s["palette"].as_str()).map(|p| p.accent)).unwrap_or(yami_px::theme::PRESETS[0].accent)
}

/// что карточке нужно из файла темы: картинку обоев (base64, мегабайты) разбор пропускает, не собирая
#[derive(serde::Deserialize)]
struct Head {
    #[serde(default)]
    format: Value,
    #[serde(default)]
    kind: Value,
    #[serde(default)]
    settings: Value,
    #[serde(default)]
    wallpaper: Option<serde::de::IgnoredAny>,
}

/// сохранённые темы в естественном порядке имён (не больше 48). Файлы с картинкой весят мегабайты:
/// шелл зовёт это из нити, файл читается потоком, без копии в памяти
pub fn cards() -> Vec<Card> {
    let Ok(rd) = std::fs::read_dir(themes_dir()) else { return vec![] };
    let mut v: Vec<Card> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let name = p.file_stem()?.to_str()?.to_string();
            let f = std::fs::File::open(&p).ok().filter(|f| f.metadata().is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE))?;
            let b: Head = serde_json::from_reader(std::io::BufReader::new(f)).ok()?;
            (b.format == FORMAT && b.kind == "theme").then_some(())?;
            let s = &b.settings;
            Some(Card { name, accent: accent_of(s), light: s["light"].as_bool().unwrap_or(false), wall: b.wallpaper.is_some() })
        })
        .collect();
    v.sort_by(|a, b| yami_px::paths::natural(&a.name.to_lowercase(), &b.name.to_lowercase()));
    v.truncate(48);
    v
}

/// та же тема в файле p: всё, кроме имени и версии yami
fn same(p: &Path, b: &Value) -> bool {
    let Ok(mut v) = load(p) else { return false };
    let mut b = b.clone();
    for x in [&mut v, &mut b] {
        x["name"] = Value::Null;
        x["yami"] = Value::Null;
    }
    v == b
}

/// записать тему под именем name. overwrite — заменить тему с тем же именем (явное `save ИМЯ`);
/// иначе свободное имя: name, name-2, name-3… (та же тема под этим именем — её имя)
fn put(name: &str, b: &mut Value, overwrite: bool) -> Result<String, String> {
    let base = theme_name(name);
    let base = if base.is_empty() { "theme".to_string() } else { base };
    let n = if overwrite {
        base
    } else {
        (1..200).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|n| theme_path(n).is_some_and(|p| !p.exists() || same(&p, b))).unwrap_or(base)
    };
    b["name"] = Value::String(n.clone());
    let text = serde_json::to_string_pretty(&*b).unwrap_or_default() + "\n";
    let p = theme_path(&n).ok_or_else(|| crate::tf!("нужно имя темы", "a theme needs a name"))?;
    std::fs::create_dir_all(themes_dir()).map_err(|e| e.to_string())?;
    write_atomic(&p, &text).map_err(|_| crate::tf!("не могу записать {}", "cannot write {}", p.display()))?;
    Ok(n)
}

/// текущий вид — темой в библиотеку (с картинкой обоев). Имя дано — тема с ним заменяется;
/// пустое — по палитре («ash-dark»), а занятое — ash-dark-2 и дальше
pub fn save(name: &str) -> Result<String, String> {
    let mut b = bundle();
    let s = &b["settings"];
    let auto = format!("{}-{}", if s["accent"].as_str().unwrap_or("").is_empty() { s["palette"].as_str().unwrap_or("theme") } else { "custom" }, if s["light"].as_bool() == Some(true) { "light" } else { "dark" });
    let given = !theme_name(name).is_empty();
    put(if given { name } else { &auto }, &mut b, given)
}

pub fn remove(name: &str) -> Result<(), String> {
    let p = theme_file(name).ok_or_else(|| crate::tf!("нет такой темы: {}", "no such theme: {}", name))?;
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

/// применить тему из библиотеки (шелл: молча, ответ — ошибкой)
pub fn apply(name: &str) -> Result<(), String> {
    let p = theme_file(name).ok_or_else(|| crate::tf!("нет такой темы: {}", "no such theme: {}", name))?;
    let b = load(&p)?;
    let mut set = settings_value();
    for (k, v) in b["settings"].as_object().into_iter().flatten() {
        if is_look(k) {
            set.insert(k.clone(), v.clone());
        }
    }
    // своя картинка — в папку обоев; темы без картинки — пиксельные обои из её палитры
    let wall = b.get("wallpaper").and_then(|w| Some((w["name"].as_str()?, w["data"].as_str().and_then(unb64)?))).and_then(|(n, d)| wall_file(n, &d));
    set.insert("wallpaper".into(), Value::String(wall.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()));
    // через Settings: чужие строки чистятся (sanitize), негодные значения — по умолчанию
    let raw = serde_json::to_string(&Value::Object(set)).unwrap_or_default();
    let mut merged = crate::config::Settings::lenient(&raw);
    merged.sanitize();
    write_atomic(&settings_path(), &merged.to_json()).map_err(|_| crate::tf!("не могу записать настройки", "cannot write the settings"))
}

/// картинка обоев из темы → ~/.local/share/yami-lite/wallpapers: имя — только имя файла (без пути и
/// скрытых), расширение — по содержимому (PNG, JPEG, WebP; другое не обои); чужую картинку с тем же
/// именем не затирает
fn wall_file(name: &str, data: &[u8]) -> Option<PathBuf> {
    let ext = if data.starts_with(b"\x89PNG") {
        "png"
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        "jpg"
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        "webp"
    } else {
        return None;
    };
    let stem: String = Path::new(name).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().chars().filter(|c| c.is_alphanumeric() || "._- ".contains(*c)).take(64).collect();
    let stem = stem.trim_start_matches('.').trim();
    let stem = if stem.is_empty() { "wallpaper" } else { stem };
    let dir = crate::config::data_dir().join("yami-lite/wallpapers");
    std::fs::create_dir_all(&dir).ok()?;
    let p = (0..100).map(|n| dir.join(if n == 0 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") })).find(|p| std::fs::read(p).map(|d| d == data).unwrap_or(true))?;
    if !p.exists() {
        std::fs::write(&p, data).ok()?;
    }
    Some(p)
}

/// итог команды: ✓ и что сделано (0) или ✕ и почему (1)
fn report(y: &Sty, r: Result<String, String>) -> i32 {
    match r {
        Ok(m) => {
            println!(" {} {m}", y.ok("✓"));
            0
        }
        Err(e) => {
            eprintln!(" {} {e}", y.bad("✕"));
            1
        }
    }
}

pub fn run(y: &Sty, cmd: &str, args: &[&str]) -> i32 {
    // тема по имени: имя может быть из нескольких слов
    let name = args.join(" ");
    match cmd {
        "list" => {
            let list = cards();
            y.section(&crate::tf!("темы", "themes"));
            println!("   {}", y.dim(&themes_dir().display().to_string()));
            if list.is_empty() {
                println!("   {}", y.dim(&crate::tf!("пусто: yami-lite theme save ИМЯ сохранит текущий вид", "empty: yami-lite theme save NAME keeps the current look")));
            }
            for c in &list {
                println!("   {} {}{}", y.bold(&c.name), y.dim(&format!("#{:06x} {}", c.accent, if c.light { "light" } else { "dark" })), if c.wall { y.dim(&crate::tf!(" + обои", " + wallpaper")) } else { String::new() });
            }
            println!();
            0
        }
        "save" => report(y, save(&name).map(|n| format!("{n} → {}", y.dim(&themes_dir().display().to_string())))),
        "apply" if !name.is_empty() => report(y, apply(&name).map(|_| crate::tf!("{} — шелл перечитает сам", "{}: the shell reloads by itself", name))),
        "remove" if !name.is_empty() => report(y, remove(&name).map(|_| crate::tf!("{} удалена", "{} removed", name))),
        _ => {
            y.section("yami-lite theme");
            y.row("theme save [NAME]", &crate::tf!("текущий вид — в свои темы", "keep the current look in your themes"));
            y.row("theme list | apply NAME | remove NAME", &crate::tf!("свои темы: список, применить, удалить", "your themes: list, apply, remove"));
            println!();
            2
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_round_trip() {
        for d in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", &[0, 255, 128, 7]] {
            assert_eq!(super::unb64(&super::b64(d)).unwrap(), d);
        }
        assert_eq!(super::b64(b"foobar"), "Zm9vYmFy");
    }
    #[test]
    fn keys() {
        assert!(super::is_look("barLeft") && super::is_look("light") && super::is_look("fontShell"));
        assert!(!super::is_look("launcherCmd") && !super::is_look("dockPins") && !super::is_look("wallDir") && !super::is_look("wallpaper"));
        assert_eq!(super::theme_name(" ash/../dark: 2 "), "ashdark 2");
    }
}
