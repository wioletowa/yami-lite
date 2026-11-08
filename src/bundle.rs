// Настройки и темы одним файлом. `yami theme export ФАЙЛ` упаковывает только вид: палитру и
// тона, светлую или тёмную тему, эффекты, шрифты, бар, виджеты, вид дока, отступы, курсор и саму
// картинку обоев; `yami theme import ФАЙЛ` применяет его и больше ничего не трогает. Чужая тема
// ничего не может запустить: ни клавиш, ни команд, ни скриптов, ни источника обновлений.
// `yami export ФАЙЛ` — полный бэкап yami этой машины (все настройки, настройки файлов,
// niri/user.kdl, скрипты бара, обои); `yami import ФАЙЛ` показывает, что идёт с командами (свои
// клавиши, user.kdl, скрипты), и ставит это только с `--yes`. Перед любым импортом всё текущее
// копируется в ~/.local/state/yami/before-import-*.json (тот же формат, пять последних;
// `yami import КОПИЯ --yes` возвращает). Файл — JSON, двоичное в
// base64: читается глазами, сравнивается diff'ом, делится одним файлом.
// Темы под рукой (#8) — ~/.config/yami/themes/<имя>.json того же формата: `yami theme save ИМЯ`,
// `apply`, `remove`, `list`, раздел «темы» во вкладке «цвет»; `yami theme import` кладёт тему и
// туда, в том числе по https-ссылке (только тема, никогда не полный файл).
// i18n: inline pairs

use crate::cmd::Sty;
use crate::config::{config_dir, settings_path, state_dir, write_atomic};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

const FORMAT: &str = "yami-bundle";

/// ключи вида (camelCase, как в yami-shell.json)
fn is_look(k: &str) -> bool {
    const EXACT: &[&str] = &[
        "accent", "palette", "tint", "light", "glass", "lift", "inner", "dim", "blurShell", "blurTerm", "animations", "animSpeed", "animClose", "reveal", "revealGrain", "scanlines", "iconTheme", "iconPixel", "cursor", "cursorSize",
        "curtainStyle", "curtainCell", "cpuStyle", "tempStyle", "trayMode", "clockDate", "clockSeconds", "taskWidth", "widgets", "widgetContrast", "titlebars", "fetchFull", "winGap", "winLeft", "winRight", "winTop", "winBottom",
        "mediaViz", "mediaView",
    ];
    // barOutputs, dockPins, wallDir — имена экранов, программы и папки этой машины
    EXACT.contains(&k) || ((k.starts_with("bar") || k.starts_with("wall") || k.starts_with("font") || k.starts_with("dock")) && !matches!(k, "dockPins" | "wallpaper" | "wallDir" | "barOutputs"))
}

/// ключи, которые никуда не переносятся: откуда обновления и проверяются ли они
fn is_policy(k: &str) -> bool {
    k.starts_with("update")
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

/// содержимое файла: тема (только вид) или все настройки; обои — картинкой, если with_wall
fn bundle(theme: bool, with_wall: bool) -> Value {
    // все ключи, и те, которых в файле ещё нет (значения по умолчанию): тема должна быть полной.
    // Через текст, не to_value: f32 там становится f64 (0.78 → 0.7799999713897705), и импорт
    // своего же файла показывал бы «изменения»
    let set = serde_json::to_string(&crate::config::Settings::load()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_else(settings_value);
    let mut s = Map::new();
    for (k, v) in &set {
        if (theme && is_look(k)) || (!theme && !is_policy(k) && k != "wallpaper") {
            s.insert(k.clone(), v.clone());
        }
    }
    let mut b = json!({ "format": FORMAT, "version": 1, "kind": if theme { "theme" } else { "full" }, "yami": env!("CARGO_PKG_VERSION"), "settings": s });
    if let Some(w) = with_wall.then(|| wall_entry(&set)).flatten() {
        b["wallpaper"] = w;
    }
    if !theme {
        let mut files = Map::new();
        for rel in ["yami-files.json", "niri/user.kdl"] {
            if let Ok(t) = std::fs::read_to_string(config_dir().join(rel)) {
                files.insert(rel.into(), Value::String(t));
            }
        }
        for m in crate::plugins::available() {
            let name = &m[2..];
            if let Ok(t) = std::fs::read(crate::plugins::dir().join(name)) {
                files.insert(format!("yami/modules/{name}"), json!({ "exec": true, "data": b64(&t) }));
            }
        }
        b["files"] = Value::Object(files);
    }
    b
}

pub fn export(y: &Sty, file: &str, theme: bool) -> i32 {
    let b = bundle(theme, true);
    let text = serde_json::to_string_pretty(&b).unwrap_or_default() + "\n";
    if write_atomic(Path::new(file), &text).is_err() {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("не могу записать {}", "cannot write {}", "não foi possível gravar {}", file));
        return 1;
    }
    let what = if theme { crate::tf!("тема", "theme", "tema") } else { crate::tf!("все настройки", "every setting", "todas as configurações") };
    println!(" {} {} → {}", y.ok("✓"), what, y.bold(file));
    0
}

/// ключи настроек, что запускают команды: только с --yes
const RUNS: &[&str] = &["customBinds", "binds"];

/// какие файлы бывают в полном файле (ровно то, что пишет export) и запускает ли файл команды
fn file_kind(rel: &str) -> Option<bool> {
    match rel {
        "yami-files.json" => Some(false),
        "niri/user.kdl" => Some(true),
        _ => rel.strip_prefix("yami/modules/").filter(|n| crate::plugins::name_ok(n)).map(|_| true),
    }
}

/// что в полном файле может что-то запустить: свои клавиши, user.kdl, скрипты, терминал файлов
fn risky(b: &Value) -> Vec<String> {
    let mut v = vec![];
    if b["settings"]["customBinds"].as_array().is_some_and(|a| !a.is_empty()) {
        v.push(crate::tf!("свои сочетания с командами (customBinds)", "custom keybindings with commands (customBinds)", "atalhos personalizados com comandos (customBinds)"));
    }
    if b["settings"]["binds"].as_object().is_some_and(|a| !a.is_empty()) {
        v.push(crate::tf!("переназначенные клавиши (binds)", "reassigned keys (binds)", "atalhos redefinidos (binds)"));
    }
    if let Some(f) = b["files"].as_object() {
        for k in f.keys() {
            if file_kind(k) == Some(true) {
                v.push(k.clone());
            }
        }
        if f.get("yami-files.json").and_then(|t| t.as_str()).is_some_and(|t| t.contains("\"terminal\"")) {
            v.push(crate::tf!("терминал файлового менеджера", "the file manager's terminal", "terminal do gerenciador de arquivos"));
        }
    }
    v
}

/// yami-files.json из файла; без --yes терминал остаётся свой: это команда, которую файлы запускают
fn files_json(incoming: &str, yes: bool) -> Option<String> {
    let mut v: Map<String, Value> = serde_json::from_str::<Value>(incoming).ok()?.as_object()?.clone();
    if !yes {
        let cur = std::fs::read_to_string(config_dir().join("yami-files.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
        match cur.as_ref().and_then(|c| c.get("terminal")) {
            Some(t) => v.insert("terminal".into(), t.clone()),
            None => v.remove("terminal"),
        };
    }
    Some(serde_json::to_string_pretty(&Value::Object(v)).unwrap_or_default() + "\n")
}

/// больше — не файл настроек и не тема (обои внутри — до 32 МБ, в base64 это ~43 МБ)
const MAX_FILE: u64 = 64 << 20;

/// файл yami: не больше 64 МБ, JSON нашего формата
fn load(file: &Path) -> Result<Value, String> {
    if std::fs::metadata(file).map(|m| m.len() > MAX_FILE).unwrap_or(false) {
        return Err(crate::tf!("слишком большой файл: {}", "file too large: {}", "arquivo grande demais: {}", file.display()));
    }
    let b: Value = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or_else(|| crate::tf!("не читается: {}", "cannot read: {}", "não foi possível ler: {}", file.display()))?;
    if b["format"] != FORMAT {
        return Err(crate::tf!("это не файл yami (export)", "not a yami file (export)", "não é um arquivo yami (export)"));
    }
    Ok(b)
}

/// что файл поменяет: настройки после него, изменённые ключи, пропущенное с командами
struct Plan {
    theme: bool,
    set: Map<String, Value>,
    changed: Vec<String>,
    warn: Vec<String>,
}

fn plan(b: &Value, theme_only: bool, yes: bool) -> Plan {
    let theme = theme_only || b["kind"] == "theme";
    let incoming = b["settings"].as_object().cloned().unwrap_or_default();
    let mut set = settings_value();
    let mut changed = vec![];
    for (k, v) in &incoming {
        // тема — только вид; полный — всё, кроме политики обновлений
        if is_policy(k) || k == "wallpaper" || (theme && !is_look(k)) || (!yes && RUNS.contains(&k.as_str())) {
            continue;
        }
        if set.get(k) != Some(v) {
            changed.push(k.clone());
            set.insert(k.clone(), v.clone());
        }
    }
    let warn = if theme { vec![] } else { risky(b) };
    Plan { theme, set, changed, warn }
}

/// записать: копия текущего, обои, файлы (полный файл), настройки. Возвращает копию «как было»;
/// y — куда сказать о пропущенных чужих файлах (из шелла — молча)
fn commit(b: &Value, p: Plan, yes: bool, y: Option<&Sty>) -> Result<PathBuf, String> {
    let Plan { theme, mut set, .. } = p;
    // копия того, что было, тем же форматом: `yami import КОПИЯ --yes` возвращает всё назад
    let backup = backup().ok_or_else(|| crate::tf!("не могу сохранить копию текущих настроек — ничего не меняю", "cannot save a copy of the current settings: nothing changed", "não foi possível copiar as configurações atuais: nada foi alterado"))?;
    if let Some(w) = b.get("wallpaper").filter(|w| w.is_object()) {
        if let (Some(name), Some(data)) = (w["name"].as_str(), w["data"].as_str().and_then(unb64)) {
            if let Some(p) = wall_file(name, &data) {
                set.insert("wallpaper".into(), Value::String(p.to_string_lossy().to_string()));
            }
        }
    } else if theme {
        // тема без своей картинки — пиксельные обои из её палитры
        set.insert("wallpaper".into(), Value::String(String::new()));
    } else if let Some(p) = b["restore"]["wallpaper"].as_str() {
        // копия перед импортом: обои были файлом на этой машине — путь назад, если файл на месте
        if p.is_empty() || Path::new(p).is_file() {
            set.insert("wallpaper".into(), Value::String(p.to_string()));
        }
    }
    if !theme {
        if let Some(files) = b["files"].as_object() {
            for (rel, v) in files {
                // только файлы, что пишет export: путь вроде autostart/… или fish/config.fish
                // запустил бы что угодно при следующем входе
                let Some(runs) = file_kind(rel) else {
                    if let Some(y) = y {
                        println!("   {} {}", y.warn(&crate::tf!("пропущен чужой файл:", "skipped an unknown file:", "arquivo desconhecido ignorado:")), rel);
                    }
                    continue;
                };
                if (runs && !yes) || !safe_rel(rel) {
                    continue;
                }
                let dst = config_dir().join(rel);
                if let Some(d) = dst.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                match v {
                    Value::String(t) if rel == "yami-files.json" => {
                        if let Some(t) = files_json(t, yes) {
                            let _ = write_atomic(&dst, &t);
                        }
                    }
                    Value::String(t) => {
                        let _ = write_atomic(&dst, t);
                    }
                    Value::Object(o) => {
                        if let Some(data) = o.get("data").and_then(|d| d.as_str()).and_then(unb64) {
                            if std::fs::write(&dst, data).is_ok() && o.get("exec").and_then(|e| e.as_bool()) == Some(true) {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // через Settings: чужие строки чистятся (sanitize), негодные значения — по умолчанию, файл —
    // в том же виде, что пишет шелл
    let raw = serde_json::to_string(&Value::Object(set)).unwrap_or_default();
    let mut merged = crate::config::Settings::lenient(&raw);
    merged.sanitize();
    write_atomic(&settings_path(), &merged.to_json()).map_err(|_| crate::tf!("не могу записать настройки", "cannot write the settings", "não foi possível salvar as configurações"))?;
    Ok(backup)
}

pub fn import(y: &Sty, file: &str, theme_only: bool, yes: bool, dry: bool) -> i32 {
    let b = match load(Path::new(file)) {
        Ok(b) => b,
        Err(e) => {
            eprintln!(" {} {e}", y.bad("✕"));
            return 2;
        }
    };
    import_value(y, &b, theme_only, yes, dry)
}

fn import_value(y: &Sty, b: &Value, theme_only: bool, yes: bool, dry: bool) -> i32 {
    let p = plan(b, theme_only, yes);
    y.section(&if p.theme { crate::tf!("тема", "theme", "tema") } else { crate::tf!("все настройки", "every setting", "todas as configurações") });
    println!("   {} {}", y.dim(&crate::tf!("меняется:", "changes:", "alterações:")), if p.changed.is_empty() { "—".to_string() } else { p.changed.join(" ") });
    if let Some(w) = b.get("wallpaper").filter(|w| w.is_object()) {
        println!("   {} {}", y.dim(&crate::tf!("обои:", "wallpaper:", "papel de parede:")), w["name"].as_str().unwrap_or(""));
    }
    if !p.warn.is_empty() {
        let note = if yes { crate::tf!("ставится (--yes):", "installed (--yes):", "instalado (--yes):") } else { crate::tf!("пропущено, запускает команды (добавь --yes, если файл твой):", "skipped, it runs commands (add --yes if the file is yours):", "ignorado, executa comandos (adicione --yes se o arquivo for seu):") };
        println!("   {} {}", y.warn(&note), p.warn.join(", "));
    }
    if dry {
        println!("\n   {}\n", y.dim(&crate::tf!("--dry-run: ничего не изменено", "--dry-run: nothing changed", "--dry-run: nada foi alterado")));
        return 0;
    }
    match commit(b, p, yes, Some(y)) {
        Ok(backup) => {
            println!("\n {} {}", y.ok("✓"), crate::tf!("готово — шелл перечитает сам", "done; the shell reloads by itself", "concluído; o shell recarrega automaticamente"));
            println!("   {}\n", y.dim(&crate::tf!("вернуть как было: yami import {} --yes", "to undo: yami import {} --yes", "para desfazer: yami import {} --yes", backup.display())));
            0
        }
        Err(e) => {
            eprintln!(" {} {e}", y.bad("✕"));
            1
        }
    }
}

// ── темы под рукой: ~/.config/yami/themes ───────────────────────────────────

pub fn themes_dir() -> PathBuf {
    config_dir().join("yami/themes")
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
    let p = theme_path(&n).ok_or_else(|| crate::tf!("нужно имя темы", "a theme needs a name", "o tema precisa de um nome"))?;
    std::fs::create_dir_all(themes_dir()).map_err(|e| e.to_string())?;
    write_atomic(&p, &text).map_err(|_| crate::tf!("не могу записать {}", "cannot write {}", "não foi possível gravar {}", p.display()))?;
    Ok(n)
}

/// текущий вид — темой в библиотеку (с картинкой обоев). Имя дано — тема с ним заменяется;
/// пустое — по палитре («ash-dark»), а занятое — ash-dark-2 и дальше
pub fn save(name: &str) -> Result<String, String> {
    let mut b = bundle(true, true);
    let s = &b["settings"];
    let auto = format!("{}-{}", if s["accent"].as_str().unwrap_or("").is_empty() { s["palette"].as_str().unwrap_or("theme") } else { "custom" }, if s["light"].as_bool() == Some(true) { "light" } else { "dark" });
    let given = !theme_name(name).is_empty();
    put(if given { name } else { &auto }, &mut b, given)
}

pub fn remove(name: &str) -> Result<(), String> {
    let p = theme_file(name).ok_or_else(|| crate::tf!("нет такой темы: {}", "no such theme: {}", "tema inexistente: {}", name))?;
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

/// применить тему из библиотеки (шелл: молча, ответ — ошибкой)
pub fn apply(name: &str) -> Result<(), String> {
    let p = theme_file(name).ok_or_else(|| crate::tf!("нет такой темы: {}", "no such theme: {}", "tema inexistente: {}", name))?;
    let b = load(&p)?;
    commit(&b, plan(&b, true, false), false, None).map(|_| ())
}

/// тема по ссылке: только https (и после переадресаций), не больше 64 МБ, 30 с; во временную
/// папку 0700 — и сразу разбор: ссылка даёт только тему, полный файл с командами — никогда
fn fetch(url: &str) -> Result<Value, String> {
    if !url.starts_with("https://") {
        return Err(crate::tf!("только https://", "https:// only", "somente https://"));
    }
    let dir = yami_px::paths::private_tmp("theme").map_err(|e| e.to_string())?;
    let out = dir.join("theme.json");
    let ok = crate::proc::curl(30, MAX_FILE)
        .args(["-fL", "--max-redirs", "3", "-o"])
        .arg(&out)
        .arg("--")
        .arg(url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let b = if ok { load(&out) } else { Err(crate::tf!("не скачалось: {}", "download failed: {}", "falha no download: {}", url)) };
    let _ = std::fs::remove_dir_all(&dir);
    let b = b?;
    if b["kind"] != "theme" {
        return Err(crate::tf!("по ссылке — только темы (theme export), не полный файл", "a link may only bring a theme (theme export), not a full file", "um link só pode conter um tema (theme export), não um arquivo completo"));
    }
    Ok(b)
}

/// `yami theme import ФАЙЛ|https://…`: в библиотеку и применить
fn theme_import(y: &Sty, src: &str, dry: bool) -> i32 {
    let got = if src.starts_with("https://") || src.starts_with("http://") { fetch(src) } else { load(Path::new(src)) };
    let mut b = match got {
        Ok(b) => b,
        Err(e) => {
            eprintln!(" {} {e}", y.bad("✕"));
            return 2;
        }
    };
    // имя: из файла темы, иначе из имени файла или конца ссылки
    let stem = src.rsplit('/').next().unwrap_or("").split(['?', '#']).next().unwrap_or("").trim_end_matches(".json").trim_end_matches(".yami").to_string();
    let name = b["name"].as_str().map(theme_name).filter(|n| !n.is_empty()).unwrap_or(stem);
    let code = import_value(y, &b, true, false, dry);
    if code == 0 && !dry {
        // полный файл (yami export) через theme import — только вид, в библиотеку как тема
        b["kind"] = Value::String("theme".into());
        if let Some(s) = b["settings"].as_object_mut() {
            s.retain(|k, _| is_look(k));
        }
        if let Some(o) = b.as_object_mut() {
            o.retain(|k, _| matches!(k.as_str(), "format" | "version" | "kind" | "yami" | "settings" | "wallpaper" | "name"));
        }
        match put(&name, &mut b, false) {
            Ok(n) => println!("   {} {}\n", y.dim(&crate::tf!("в темах:", "saved as:", "salvo como:")), n),
            Err(e) => eprintln!(" {} {e}", y.warn("!")),
        }
    }
    code
}

/// картинка обоев из файла → ~/.local/share/yami/wallpapers: имя — только имя файла (без пути и
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
    let dir = crate::config::data_dir().join("yami/wallpapers");
    std::fs::create_dir_all(&dir).ok()?;
    let p = (0..100).map(|n| dir.join(if n == 0 { format!("{stem}.{ext}") } else { format!("{stem}-{n}.{ext}") })).find(|p| std::fs::read(p).map(|d| d == data).unwrap_or(true))?;
    if !p.exists() {
        std::fs::write(&p, data).ok()?;
    }
    Some(p)
}

/// номер копии из имени before-import-<секунды>[-<n>].json: по нему, а не по времени файла
/// (копии за одну секунду получают одно и то же время изменения)
fn backup_key(p: &Path) -> Option<(u64, u32)> {
    let n = p.file_name()?.to_str()?.strip_prefix("before-import-")?.strip_suffix(".json")?;
    let (a, b) = n.split_once('-').unwrap_or((n, "0"));
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// копия всех настроек перед импортом: ~/.local/state/yami/before-import-<время>.json, пять последних
fn backup() -> Option<PathBuf> {
    let dir = state_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let mut b = bundle(false, false);
    b["restore"] = json!({ "wallpaper": settings_value().get("wallpaper").and_then(|w| w.as_str()).unwrap_or("") });
    let stamp = crate::clock::unix_now();
    // два импорта за секунду не затирают копию первого: номер — следующий за самым большим в эту
    // секунду, так копии по имени идут ровно в том порядке, в каком их сняли
    let n = std::fs::read_dir(&dir).ok()?.flatten().filter_map(|e| backup_key(&e.path())).filter(|k| k.0 == stamp).map(|k| k.1 + 1).max().unwrap_or(0);
    let p = dir.join(if n == 0 { format!("before-import-{stamp}.json") } else { format!("before-import-{stamp}-{n}.json") });
    write_atomic(&p, &(serde_json::to_string_pretty(&b).ok()? + "\n")).ok()?;
    let mut old: Vec<(PathBuf, (u64, u32))> = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path()).filter_map(|p| backup_key(&p).map(|k| (p, k))).collect();
    old.sort_by_key(|o| o.1);
    // только что снятая копия — последняя по имени и никогда не удаляется
    for o in old.iter().take(old.len().saturating_sub(5)).filter(|o| o.0 != p) {
        let _ = std::fs::remove_file(&o.0);
    }
    Some(p)
}

/// только простые относительные пути внутри ~/.config: без «..» и без абсолютных
fn safe_rel(rel: &str) -> bool {
    let p = PathBuf::from(rel);
    !rel.is_empty() && p.is_relative() && p.components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// `yami theme …`, `yami export …`, `yami import …`
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
    let flags = |f: &str| args.contains(&f);
    let file = args.iter().find(|a| !a.starts_with("--")).copied();
    // тема по имени: имя может быть из нескольких слов
    let name = args.iter().filter(|a| !a.starts_with("--")).copied().collect::<Vec<_>>().join(" ");
    match cmd {
        "theme-list" => {
            let list = cards();
            y.section(&crate::tf!("темы", "themes", "temas"));
            println!("   {}", y.dim(&themes_dir().display().to_string()));
            if list.is_empty() {
                println!("   {}", y.dim(&crate::tf!("пусто: yami theme save ИМЯ сохранит текущий вид", "empty: yami theme save NAME keeps the current look", "vazio: yami theme save NOME salva a aparência atual")));
            }
            for c in &list {
                println!("   {} {}{}", y.bold(&c.name), y.dim(&format!("#{:06x} {}", c.accent, if c.light { "light" } else { "dark" })), if c.wall { y.dim(&crate::tf!(" + обои", " + wallpaper", " + papel de parede")) } else { String::new() });
            }
            println!();
            return 0;
        }
        "theme-save" => return report(y, save(&name).map(|n| format!("{n} → {}", y.dim(&themes_dir().display().to_string())))),
        "theme-apply" if !name.is_empty() => return report(y, apply(&name).map(|_| crate::tf!("{} — шелл перечитает сам", "{}: the shell reloads by itself", "{}: o shell recarrega automaticamente", name))),
        "theme-remove" if !name.is_empty() => return report(y, remove(&name).map(|_| crate::tf!("{} удалена", "{} removed", "{} removidos", name))),
        _ => {}
    }
    // опечатка во флаге («--dryrun») не должна молча превратиться в настоящий импорт
    let known: &[&str] = match cmd {
        "import" => &["--yes", "--dry-run"],
        "theme-import" => &["--dry-run"],
        _ => &[],
    };
    if let Some(bad) = args.iter().find(|a| a.starts_with("--") && !known.contains(a)) {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("неизвестный флаг: {}", "unknown flag: {}", "opção desconhecida: {}", bad));
        return 2;
    }
    if args.iter().filter(|a| !a.starts_with("--")).count() > 1 {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("нужен один файл", "one file, please", "apenas um arquivo, por favor"));
        return 2;
    }
    match (cmd, file) {
        ("theme-export", Some(f)) => export(y, f, true),
        ("export", Some(f)) => export(y, f, false),
        ("theme-import", Some(f)) => theme_import(y, f, flags("--dry-run")),
        ("import", Some(f)) => import(y, f, false, flags("--yes"), flags("--dry-run")),
        _ => {
            y.section("yami theme / export / import");
            y.row("theme export FILE", &crate::tf!("вид в один файл: палитра, эффекты, шрифты, бар, виджеты, обои", "the look in one file: palette, effects, fonts, bar, widgets, wallpaper", "o visual num arquivo: paleta, efeitos, fontes, barra, widgets e papel de parede"));
            y.row("theme import FILE|URL [--dry-run]", &crate::tf!("применить чужую тему (файл или https-ссылка): только вид, без команд; остаётся в темах", "apply someone's theme (a file or an https link): the look only, no commands; kept in your themes", "aplicar um tema de outra pessoa (arquivo ou link https): só a aparência, sem comandos; salvo nos seus temas"));
            y.row("theme save [NAME]", &crate::tf!("текущий вид — в свои темы (~/.config/yami/themes)", "keep the current look in your themes (~/.config/yami/themes)", "salvar a aparência atual nos seus temas (~/.config/yami/themes)"));
            y.row("theme list | apply NAME | remove NAME", &crate::tf!("свои темы: список, применить, удалить", "your themes: list, apply, remove", "seus temas: list, apply, remove"));
            y.row("export FILE", &crate::tf!("все настройки, user.kdl, скрипты бара, обои", "every setting, user.kdl, bar scripts, wallpaper", "todas as configurações, user.kdl, scripts da barra e papel de parede"));
            y.row("import FILE [--yes] [--dry-run]", &crate::tf!("вернуть всё; команды и скрипты — только с --yes", "restore it all; commands and scripts only with --yes", "restaurar tudo; comandos e scripts somente com --yes"));
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
        assert_eq!(super::b64(b"fo"), "Zm8=");
    }
    #[test]
    fn keys() {
        assert!(super::is_look("barLeft") && super::is_look("light") && super::is_look("fontShell"));
        assert!(!super::is_look("customBinds") && !super::is_look("updateRepo") && !super::is_look("dockPins"));
        // папка выбора обоев — путь этой машины, в тему не идёт; вид трея и температуры — идёт
        assert!(!super::is_look("wallDir") && !super::is_look("wallpaper") && super::is_look("trayMode") && super::is_look("tempStyle"));
        // экраны с баром — этой машины; эквалайзер — вид
        assert!(!super::is_look("barOutputs") && super::is_look("mediaViz"));
        assert_eq!(super::theme_name(" ash/../dark: 2 "), "ashdark 2");
        assert!(super::is_policy("updateRepo"));
        assert!(super::safe_rel("niri/user.kdl") && !super::safe_rel("../x") && !super::safe_rel("/etc/x"));
        assert_eq!(super::file_kind("yami-files.json"), Some(false));
        assert_eq!(super::file_kind("yami/modules/w.sh"), Some(true));
        assert_eq!(super::file_kind("autostart/x.desktop"), None);
        assert_eq!(super::file_kind("yami/modules/../../fish/config.fish"), None);
        assert_eq!(super::backup_key(std::path::Path::new("/s/before-import-1793885973-12.json")), Some((1793885973, 12)));
        assert_eq!(super::backup_key(std::path::Path::new("/s/before-import-1793885973.json")), Some((1793885973, 0)));
        assert_eq!(super::backup_key(std::path::Path::new("/s/other.json")), None);
    }
}
