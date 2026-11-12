// История буфера (модуль clipboard): один JSON-файл ~/.local/share/yami-lite/clip/history.json,
// картинки и очень длинный текст — файлами в clip/blobs по отпечатку. Без базы: записей сотня-другая,
// файл переписывается целиком (атомарно) после каждой перемены. Здесь же — что за текст
// (цвет, символ, код, просто текст), язык кода для подсветки и нечёткий поиск.
//
// Первый запуск: история старого Vortex (~/.local/share/vortex/history.db) переносится через sqlite3,
// если он есть, — один раз.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// сколько текста держать в записи; длиннее — целиком в файле, в записи начало
const INLINE: usize = 64 * 1024;
/// начало длинного текста: превью и поиск
pub const PREVIEW: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Code,
    Color,
    Char,
    Image,
    Files,
}

impl Kind {
    fn parse(s: &str) -> Kind {
        match s {
            "code" => Kind::Code,
            "color" => Kind::Color,
            "char" => Kind::Char,
            "image" => Kind::Image,
            "files" => Kind::Files,
            _ => Kind::Text,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub kind: Kind,
    pub mime: String,
    /// текст (длинный — только начало, весь — в blobs/<hash>.txt)
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub long: bool,
    /// картинка: имя файла в blobs
    #[serde(default)]
    pub blob: Option<String>,
    pub hash: String,
    pub size: u64,
    #[serde(default)]
    pub pinned: bool,
    /// когда копировали последний раз, мс
    pub used: i64,
    /// от менеджера паролей: в буфер сам не возвращается
    #[serde(default)]
    pub secret: bool,
}

/// что отдавать по каждому типу, когда выбор у шелла
pub type Payload = Vec<(String, Arc<Vec<u8>>)>;

/// копия, разобранная вне главной нити (картинка уже в blobs)
#[derive(Debug)]
pub struct Prepared {
    kind: Kind,
    mime: String,
    text: Option<String>,
    long: bool,
    blob: Option<String>,
    hash: String,
    size: u64,
    secret: bool,
    /// момент копирования: медленное чтение не обгоняет следующее копирование
    at: i64,
}

pub fn dir() -> PathBuf {
    crate::config::data_dir().join("yami-lite/clip")
}

pub fn blobs_dir() -> PathBuf {
    dir().join("blobs")
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// отпечаток содержимого: два FNV-1a с разными началами (128 бит; не криптография — только
/// чтобы одинаковое было одной записью)
fn digest(tag: &str, data: &[u8]) -> String {
    let a = yami_px::fnv(yami_px::fnv(yami_px::FNV0, tag.as_bytes()), data);
    let b = yami_px::fnv(yami_px::fnv(0x84222325_cbf29ce4, data), tag.as_bytes());
    format!("{a:016x}{b:016x}")
}

fn write_blob(p: &Path, data: &[u8]) -> std::io::Result<()> {
    if p.exists() {
        return Ok(());
    }
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, p)
}

/// разобрать копию, посчитать отпечаток, картинку (и длинный текст) — в blobs. Из любой нити.
/// Пустой текст — None
pub fn prepare(blobs: &Path, mime: &str, data: &[u8], secret: bool, at: i64) -> Option<Prepared> {
    let (kind, mime, text) = if mime.starts_with("image/") {
        (Kind::Image, mime.to_owned(), None)
    } else {
        let t = String::from_utf8_lossy(data).into_owned();
        if t.trim().is_empty() {
            return None;
        }
        if mime == "text/uri-list" {
            let uris: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
            if !uris.is_empty() && uris.iter().all(|u| u.starts_with("file://")) {
                (Kind::Files, mime.to_owned(), Some(t))
            } else {
                // ссылки браузеры тоже кладут списком: это текст, не файлы
                let links = uris.join("\n");
                (text_kind(&links), "text/plain;charset=utf-8".to_owned(), Some(links))
            }
        } else {
            (text_kind(&t), mime.to_owned(), Some(t))
        }
    };
    // одинаковый текст — одна запись, каким бы его ни сочли
    let tag = match kind {
        Kind::Image => "image",
        Kind::Files => "files",
        _ => "text",
    };
    let content = text.as_deref().map(str::as_bytes).unwrap_or(data);
    let hash = digest(tag, content);
    let size = content.len() as u64;
    let _ = std::fs::create_dir_all(blobs);
    let blob = if kind == Kind::Image {
        write_blob(&blobs.join(&hash), data).ok()?;
        Some(hash.clone())
    } else {
        None
    };
    let (text, long) = match text {
        Some(t) if t.len() > INLINE => {
            write_blob(&blobs.join(format!("{hash}.txt")), t.as_bytes()).ok()?;
            (Some(t.chars().take(PREVIEW).collect()), true)
        }
        t => (t, false),
    };
    Some(Prepared { kind, mime, text, long, blob, hash, size, secret, at })
}

#[derive(Default)]
pub struct Store {
    /// сначала свежие
    pub items: Vec<Item>,
    next: u64,
    path: PathBuf,
    blobs: PathBuf,
}

impl Store {
    pub fn load() -> Store {
        let d = dir();
        let path = d.join("history.json");
        let blobs = blobs_dir();
        let _ = std::fs::create_dir_all(&blobs);
        let mut items: Vec<Item> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let fresh = !path.exists();
        if fresh {
            items = import_vortex(&blobs);
        }
        items.sort_by(|a, b| b.used.cmp(&a.used));
        let next = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let s = Store { items, next, path, blobs };
        if fresh {
            s.save();
        }
        s
    }

    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string(&self.items) {
            let _ = crate::config::write_atomic(&self.path, &t);
        }
    }

    pub fn blob_path(&self, name: &str) -> PathBuf {
        self.blobs.join(name)
    }

    /// записать копию или поднять такую же наверх
    pub fn add(&mut self, p: Prepared) {
        if let Some(i) = self.items.iter().position(|x| x.hash == p.hash) {
            let mut it = self.items.remove(i);
            it.used = it.used.max(p.at);
            it.secret |= p.secret;
            self.items.insert(0, it);
            return;
        }
        let it = Item { id: self.next, kind: p.kind, mime: p.mime, text: p.text, long: p.long, blob: p.blob, hash: p.hash, size: p.size, pinned: false, used: p.at, secret: p.secret };
        self.next += 1;
        // по времени копирования: медленно прочитанная картинка встаёт под более позднюю копию
        let at = self.items.iter().position(|x| x.used <= it.used).unwrap_or(self.items.len());
        self.items.insert(at, it);
    }

    pub fn get(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// наверх
    pub fn touch(&mut self, id: u64) {
        if let Some(i) = self.items.iter().position(|x| x.id == id) {
            let mut it = self.items.remove(i);
            it.used = now();
            self.items.insert(0, it);
        }
    }

    /// весь текст записи (длинный — из файла)
    pub fn full_text(&self, it: &Item) -> String {
        if it.long {
            if let Ok(t) = std::fs::read_to_string(self.blobs.join(format!("{}.txt", it.hash))) {
                return t;
            }
        }
        it.text.clone().unwrap_or_default()
    }

    /// что отдавать по типам, когда запись копируют обратно
    pub fn payload(&self, it: &Item) -> Option<Payload> {
        let text = |mimes: &[&str]| -> Payload {
            let d = Arc::new(self.full_text(it).into_bytes());
            mimes.iter().map(|m| (m.to_string(), d.clone())).collect()
        };
        Some(match it.kind {
            Kind::Image => {
                let data = std::fs::read(self.blobs.join(it.blob.as_ref()?)).ok()?;
                let mut out = vec![];
                // многие программы берут только PNG: рядом с исходным форматом
                if it.mime != "image/png" {
                    if let Some(png) = to_png(&data) {
                        out.push(("image/png".to_owned(), Arc::new(png)));
                    }
                }
                out.insert(0, (it.mime.clone(), Arc::new(data)));
                out
            }
            Kind::Files => text(&["text/uri-list", "text/plain;charset=utf-8", "text/plain"]),
            _ => text(&["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"]),
        })
    }

    pub fn set_pinned(&mut self, id: u64, on: bool) {
        if let Some(it) = self.items.iter_mut().find(|i| i.id == id) {
            it.pinned = on;
        }
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|i| i.id != id);
        self.tidy();
    }

    /// оставить limit последних незакреплённых
    pub fn trim(&mut self, limit: usize) {
        let mut n = 0;
        let before = self.items.len();
        self.items.retain(|i| {
            if i.pinned {
                return true;
            }
            n += 1;
            n <= limit
        });
        // папку blobs обходить, только если что-то ушло
        if self.items.len() != before {
            self.tidy();
        }
    }

    /// убрать незакреплённые (all — всё)
    pub fn clear(&mut self, all: bool) {
        self.items.retain(|i| !all && i.pinned);
        self.tidy();
    }

    /// файлы в blobs, на которые больше нет записей
    fn tidy(&self) {
        let Ok(rd) = std::fs::read_dir(&self.blobs) else { return };
        let keep: std::collections::HashSet<String> = self.items.iter().flat_map(|i| [i.blob.clone(), i.long.then(|| format!("{}.txt", i.hash))]).flatten().collect();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // prepare пишет файл раньше записи: свежие не трогать
            let fresh = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().map_or(true, |a| a.as_secs() < 60));
            if !fresh && !name.ends_with(".tmp") && !keep.contains(&name) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn to_png(data: &[u8]) -> Option<Vec<u8>> {
    let img = yami_px::icons::decode(data, 512 << 20)?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// история старого Vortex: строки из SQLite через sqlite3 -json, картинки — копией в blobs
fn import_vortex(blobs: &Path) -> Vec<Item> {
    let old = crate::config::data_dir().join("vortex");
    let db = old.join("history.db");
    if !db.exists() || yami_px::paths::find_exe("sqlite3").is_none() {
        return vec![];
    }
    let q = "SELECT kind, mime, text, blob, hash, size, pinned, used, secret FROM items ORDER BY used DESC";
    let Ok(out) = std::process::Command::new("sqlite3").args(["-readonly", "-json"]).arg(&db).arg(q).stderr(std::process::Stdio::null()).output() else { return vec![] };
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let mut items = vec![];
    for (n, r) in rows.iter().enumerate() {
        let s = |k: &str| r.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let i = |k: &str| r.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
        let kind = Kind::parse(&s("kind").unwrap_or_default());
        let mut text = s("text");
        let blob = s("blob");
        let Some(hash) = s("hash") else { continue };
        if let Some(b) = &blob {
            let to = blobs.join(b);
            if !to.exists() && std::fs::hard_link(old.join("blobs").join(b), &to).is_err() && std::fs::copy(old.join("blobs").join(b), &to).is_err() {
                continue;
            }
        }
        let mut long = false;
        if let Some(t) = text.as_ref().filter(|t| t.len() > INLINE) {
            if write_blob(&blobs.join(format!("{hash}.txt")), t.as_bytes()).is_ok() {
                text = Some(t.chars().take(PREVIEW).collect());
                long = true;
            }
        }
        items.push(Item { id: n as u64 + 1, kind, mime: s("mime").unwrap_or_default(), text, long, blob, hash, size: i("size").max(0) as u64, pinned: i("pinned") != 0, used: i("used"), secret: i("secret") != 0 });
    }
    items
}

// ── что за текст ───────────────────────────────────────────────────────────

pub fn text_kind(text: &str) -> Kind {
    let t = text.trim();
    if color_rgb(t).is_some() {
        Kind::Color
    } else if one_grapheme(t) {
        Kind::Char
    } else if language(text).is_some() {
        Kind::Code
    } else {
        Kind::Text
    }
}

/// один видимый знак: буква с диакритикой, эмодзи с оттенком кожи или склейкой (ZWJ), флаг
fn one_grapheme(t: &str) -> bool {
    let cs: Vec<char> = t.chars().collect();
    let Some(&first) = cs.first() else { return false };
    let ri = |c: char| ('\u{1F1E6}'..='\u{1F1FF}').contains(&c);
    if ri(first) {
        return cs.len() == 2 && ri(cs[1]) || cs.len() == 1;
    }
    let extend = |c: char| {
        matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0x0483..=0x0489 | 0x0591..=0x05BD | 0x0E31 | 0x0E34..=0x0E3A)
    };
    let mut i = 1;
    while i < cs.len() {
        let c = cs[i];
        if c == '\u{200D}' {
            // после склейки — следующий знак той же графемы
            i += 2;
            continue;
        }
        if !extend(c) {
            return false;
        }
        i += 1;
    }
    true
}

/// цвет: #rgb, #rgba, #rrggbb, #rrggbbaa, rgb()/rgba(), hsl()/hsla() → 0xRRGGBB
pub fn color_rgb(t: &str) -> Option<u32> {
    let t = t.trim();
    if let Some(h) = t.strip_prefix('#') {
        if !matches!(h.len(), 3 | 4 | 6 | 8) || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let d = |i: usize| u32::from_str_radix(&h[i..i + 1], 16).unwrap_or(0) * 17;
        return Some(match h.len() {
            3 | 4 => d(0) << 16 | d(1) << 8 | d(2),
            _ => u32::from_str_radix(&h[..6], 16).ok()?,
        });
    }
    let low = t.to_ascii_lowercase();
    let (f, rest) = low.split_once('(')?;
    let body = rest.strip_suffix(')')?;
    let parts: Vec<&str> = body.split(|c| c == ',' || c == '/' || c == ' ').map(str::trim).filter(|s| !s.is_empty()).collect();
    if !(parts.len() == 3 || parts.len() == 4) {
        return None;
    }
    let num = |s: &str, full: f32| -> Option<f32> {
        match s.strip_suffix('%') {
            Some(p) => p.parse::<f32>().ok().map(|v| v / 100.0 * full),
            None => s.trim_end_matches("deg").parse::<f32>().ok(),
        }
    };
    if parts.len() == 4 {
        num(parts[3], 1.0)?;
    }
    let (r, g, b) = match f {
        "rgb" | "rgba" => (num(parts[0], 255.0)?, num(parts[1], 255.0)?, num(parts[2], 255.0)?),
        "hsl" | "hsla" => {
            let (h, s, l) = (num(parts[0], 360.0)?.rem_euclid(360.0) / 360.0, num(parts[1], 1.0)?, num(parts[2], 1.0)?);
            let s = if parts[1].ends_with('%') { s } else { s / 100.0 };
            let l = if parts[2].ends_with('%') { l } else { l / 100.0 };
            let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
            let p = 2.0 * l - q;
            let ch = |mut t: f32| {
                t = t.rem_euclid(1.0);
                255.0
                    * if t < 1.0 / 6.0 {
                        p + (q - p) * 6.0 * t
                    } else if t < 0.5 {
                        q
                    } else if t < 2.0 / 3.0 {
                        p + (q - p) * (2.0 / 3.0 - t) * 6.0
                    } else {
                        p
                    }
            };
            (ch(h + 1.0 / 3.0), ch(h), ch(h - 1.0 / 3.0))
        }
        _ => return None,
    };
    let c = |v: f32| v.round().clamp(0.0, 255.0) as u32;
    Some(c(r) << 16 | c(g) << 8 | c(b))
}

/// язык кода (для подсветки); проза и одна строка — None, код неизвестного языка — "code"
pub fn language(text: &str) -> Option<&'static str> {
    let t = text.trim();
    let lines: Vec<&str> = t.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let first = lines[0].trim_start();
    if first.starts_with("#!") {
        return Some(match first {
            f if f.contains("python") => "python",
            f if f.contains("node") => "js",
            _ => "sh",
        });
    }
    if (t.starts_with('{') && t.ends_with('}') || t.starts_with('[') && t.ends_with(']')) && lines.len() > 1 && t.contains('"') && t.contains(':') {
        return Some("json");
    }
    if first.starts_with("<?xml") || first.to_ascii_lowercase().starts_with("<!doctype html") || first.starts_with("<html") {
        return Some("html");
    }
    if lines.len() < 2 || !looks_like_code(&lines) {
        return None;
    }
    let has = |n: &[&str]| n.iter().filter(|x| t.contains(**x)).count();
    let scores = [
        ("rust", has(&["fn ", "let mut ", "impl ", "pub fn", "::", "-> ", "&self", "use std", "println!", "match "])),
        ("python", has(&["def ", "import ", "self.", "elif ", "print(", "__init__", " in range(", "None", "):\n"])),
        ("js", has(&["function ", "const ", "=> ", "console.log", "let ", "export ", "require(", "===", "document."])),
        ("js", has(&["interface ", ": string", ": number", "export type", "readonly "]) * 2),
        ("c", has(&["#include", "int main", "printf(", "->", "malloc(", "NULL", "void ", "std::", "nullptr", "namespace "])),
        ("go", has(&["package ", "func ", ":= ", "fmt.", "go func", "chan "])),
        ("java", has(&["public class", "public static void", "System.out", "private ", "new ", "@Override"])),
        ("sh", has(&["echo ", "fi\n", "then\n", "$(", "${", "| grep", "sudo ", "export "])),
        ("css", has(&["{\n", "px;", "color:", "margin:", "padding:", "display:"])),
        ("sql", has(&["SELECT ", "FROM ", "WHERE ", "INSERT INTO", "CREATE TABLE", "JOIN "])),
        ("lua", has(&["local ", "function ", "end\n", "then\n", "nil", "~="])),
        ("nix", has(&["{ pkgs", "pkgs.", "mkIf", "inherit ", "let\n", " in\n", "= {\n"])),
        ("toml", has(&["[package]", "[dependencies]", " = \"", "\n["])),
        ("yaml", has(&[":\n  ", "- name:", "---\n"])),
        ("html", has(&["</div>", "<div", "</a>", "class=\"", "</p>"])),
    ];
    scores.iter().filter(|(_, s)| *s >= 2).max_by_key(|(_, s)| *s).map(|(id, _)| *id).or(Some("code"))
}

const BLOCK_WORDS: &[&str] = &["def ", "class ", "if ", "elif ", "else", "for ", "while ", "try", "except", "with "];

fn is_list_item(s: &str) -> bool {
    s.starts_with("- ") || s.starts_with("* ") || s.starts_with("• ") || {
        let d = s.chars().take_while(char::is_ascii_digit).count();
        d > 0 && s[d..].starts_with(". ")
    }
}

/// признаки кода в большинстве языков: скобки, точки с запятой, отступы, присваивания
fn looks_like_code(lines: &[&str]) -> bool {
    let hits = lines
        .iter()
        .filter(|l| {
            let s = l.trim_end();
            let lead = l.len() - l.trim_start().len();
            s.ends_with(';')
                || s.ends_with('{')
                || s.ends_with('}')
                || s.ends_with(':') && (lead > 0 || s.contains('(') || BLOCK_WORDS.iter().any(|w| s.starts_with(w)))
                || s.ends_with(')') && s.contains('(')
                || s.trim_start().starts_with("//")
                || s.trim_start().starts_with("# ") && lead > 0
                // проза редко с отступом, кроме вложенных списков
                || lead >= 2 && !is_list_item(s.trim_start())
                || s.contains(" = ") && !s.contains(". ")
        })
        .count();
    hits * 2 >= lines.len()
}

// ── подсветка ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Plain,
    Key,
    Str,
    Num,
    Comment,
    Punct,
}

fn keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" => &["fn", "let", "mut", "pub", "impl", "struct", "enum", "trait", "use", "mod", "match", "if", "else", "for", "while", "loop", "return", "self", "Self", "const", "static", "where", "as", "in", "ref", "move", "async", "await", "dyn", "true", "false", "crate", "super", "type", "unsafe", "break", "continue", "Some", "None", "Ok", "Err"],
        "python" => &["def", "class", "import", "from", "as", "if", "elif", "else", "for", "while", "return", "in", "not", "and", "or", "is", "None", "True", "False", "with", "try", "except", "finally", "raise", "lambda", "yield", "pass", "break", "continue", "self", "async", "await", "global"],
        "js" => &["function", "const", "let", "var", "if", "else", "for", "while", "return", "new", "this", "class", "extends", "import", "export", "from", "default", "async", "await", "true", "false", "null", "undefined", "typeof", "interface", "type", "readonly", "of", "in", "try", "catch", "throw"],
        "c" => &["int", "char", "void", "float", "double", "long", "short", "unsigned", "struct", "union", "enum", "typedef", "if", "else", "for", "while", "do", "return", "static", "const", "sizeof", "switch", "case", "break", "continue", "include", "define", "NULL", "auto", "class", "namespace", "template", "public", "private", "nullptr", "std", "bool", "true", "false"],
        "go" => &["package", "import", "func", "var", "const", "type", "struct", "interface", "if", "else", "for", "range", "return", "go", "chan", "defer", "map", "nil", "true", "false", "switch", "case"],
        "java" => &["public", "private", "protected", "class", "static", "void", "int", "new", "return", "if", "else", "for", "while", "import", "package", "extends", "implements", "this", "null", "true", "false", "final", "boolean", "String"],
        "sh" => &["if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac", "function", "echo", "export", "local", "return", "set", "sudo", "cd"],
        "lua" => &["local", "function", "end", "if", "then", "else", "elseif", "for", "in", "do", "while", "return", "nil", "true", "false", "and", "or", "not", "repeat", "until"],
        "sql" => &["SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "JOIN", "LEFT", "ON", "AND", "OR", "NOT", "NULL", "ORDER", "BY", "GROUP", "LIMIT", "AS", "select", "from", "where", "insert", "into", "values", "update", "set", "delete", "create", "table", "join", "on", "and", "or", "not", "null", "order", "by", "limit", "as"],
        "nix" => &["let", "in", "with", "inherit", "rec", "if", "then", "else", "import", "true", "false", "null", "assert"],
        "toml" | "yaml" | "json" => &["true", "false", "null"],
        _ => &["if", "else", "for", "while", "return", "function", "fn", "def", "class", "let", "const", "var", "true", "false", "null", "None", "import"],
    }
}

/// куски одной строки с их видом (строки и блочные комментарии через строку не тянутся — для
/// превью хватает)
pub fn highlight(line: &str, lang: &str) -> Vec<(Tok, String)> {
    let kw = keywords(lang);
    let hash_comment = matches!(lang, "python" | "sh" | "toml" | "yaml" | "nix");
    let dash_comment = matches!(lang, "lua" | "sql");
    let cs: Vec<char> = line.chars().collect();
    let mut out: Vec<(Tok, String)> = vec![];
    let mut push = |t: Tok, s: String| match out.last_mut() {
        Some(l) if l.0 == t => l.1.push_str(&s),
        _ => out.push((t, s)),
    };
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let rest = || cs[i..].iter().collect::<String>();
        if (c == '/' && cs.get(i + 1) == Some(&'/')) || (hash_comment && c == '#') || (dash_comment && c == '-' && cs.get(i + 1) == Some(&'-')) {
            push(Tok::Comment, rest());
            break;
        }
        if c == '/' && cs.get(i + 1) == Some(&'*') {
            let s: String = rest();
            let end = s.find("*/").map(|e| e + 2).unwrap_or(s.len());
            let part: String = s[..end].to_string();
            i += part.chars().count();
            push(Tok::Comment, part);
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            // апостроф в Rust — чаще время жизни: строкой, только если закрывается рядом
            let mut j = i + 1;
            while j < cs.len() && cs[j] != c {
                if cs[j] == '\\' {
                    j += 1;
                }
                j += 1;
            }
            if c == '\'' && lang == "rust" && j > i + 3 {
                push(Tok::Punct, c.to_string());
                i += 1;
                continue;
            }
            let j = (j + 1).min(cs.len());
            push(Tok::Str, cs[i..j].iter().collect());
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < cs.len() && (cs[j].is_ascii_alphanumeric() || cs[j] == '.' || cs[j] == '_') {
                j += 1;
            }
            push(Tok::Num, cs[i..j].iter().collect());
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < cs.len() && (cs[j].is_alphanumeric() || cs[j] == '_') {
                j += 1;
            }
            let w: String = cs[i..j].iter().collect();
            // макрос Rust (println!) — как ключевое слово
            let mac = lang == "rust" && cs.get(j) == Some(&'!');
            push(if kw.contains(&w.as_str()) || mac { Tok::Key } else { Tok::Plain }, w);
            i = j;
            continue;
        }
        push(if c.is_ascii_punctuation() { Tok::Punct } else { Tok::Plain }, c.to_string());
        i += 1;
    }
    out
}

// ── поиск ──────────────────────────────────────────────────────────────────

/// текст, по которому ищется запись: начало содержимого; у картинки — «картинка image png»
pub fn haystack(it: &Item) -> String {
    match (&it.text, it.kind) {
        (Some(t), _) => t.chars().take(PREVIEW).collect(),
        (None, Kind::Image) => format!("картинка image {}", it.mime),
        _ => String::new(),
    }
}

/// нечёткое совпадение: все буквы запроса по порядку (регистр не важен); очки — за подряд идущие,
/// за начало слова и за целую подстроку. None — не подходит
pub fn score(hay: &str, q: &str) -> Option<i32> {
    let q: Vec<char> = q.trim().to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let low = hay.to_lowercase();
    let h: Vec<char> = low.chars().collect();
    let qs: String = q.iter().collect();
    let mut best = None;
    // подстрока целиком — сильнее всего, и раньше — лучше
    if let Some(p) = low.find(&qs) {
        best = Some(1000 - (low[..p].chars().count() as i32).min(500));
    }
    let (mut qi, mut s, mut run, mut prev) = (0, 0, 0, None::<usize>);
    for (i, &c) in h.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            run = if prev == Some(i.wrapping_sub(1)) { run + 1 } else { 0 };
            let start = i == 0 || !h[i - 1].is_alphanumeric();
            s += 10 + run * 8 + if start { 12 } else { 0 };
            prev = Some(i);
            qi += 1;
        }
    }
    if qi < q.len() {
        return best;
    }
    // разброс: далеко раскиданные буквы — хуже
    s -= (prev.unwrap_or(0) as i32) / 8;
    Some(best.map_or(s, |b: i32| b.max(s)))
}

/// записи под запрос: лучшие сначала, равные — в порядке истории
pub fn filter<'a>(items: &'a [Item], q: &str) -> Vec<&'a Item> {
    if q.trim().is_empty() {
        return items.iter().collect();
    }
    let mut v: Vec<(i32, usize, &Item)> = items.iter().enumerate().filter_map(|(i, it)| score(&haystack(it), q).map(|s| (s, i, it))).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|x| x.2).collect()
}

/// «только что», «5 мин», «3 ч», «2 дн»
pub fn ago(ms: i64) -> String {
    let s = ((now() - ms) / 1000).max(0);
    if s < 60 {
        crate::i18n::tr("только что").to_string()
    } else if s < 3600 {
        crate::tf!("{} мин", "{} min", s / 60)
    } else if s < 86400 {
        crate::tf!("{} ч", "{} h", s / 3600)
    } else {
        crate::tf!("{} дн", "{} d", s / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        for c in ["#fff", "#ff8800", "#ff880080", "rgb(10, 20, 30)", "rgba(10,20,30,0.5)", "hsl(120, 50%, 50%)", "#f80c"] {
            assert!(color_rgb(c).is_some(), "{c}");
        }
        for c in ["#ggg", "rgb(", "red", "#12345", "rgb(1,2)"] {
            assert!(color_rgb(c).is_none(), "{c}");
        }
        assert_eq!(color_rgb("#f80"), Some(0xff8800));
        assert_eq!(color_rgb("hsl(0, 100%, 50%)"), Some(0xff0000));
        assert_eq!(color_rgb("rgb(255 0 0 / 50%)"), Some(0xff0000));
    }

    #[test]
    fn chars() {
        assert_eq!(text_kind("🤰"), Kind::Char);
        assert_eq!(text_kind(" A\n"), Kind::Char);
        assert_eq!(text_kind("👨‍👩‍👧"), Kind::Char);
        assert_eq!(text_kind("👍🏽"), Kind::Char);
        assert_eq!(text_kind("🇷🇺"), Kind::Char);
        assert_eq!(text_kind("é"), Kind::Char);
        assert_eq!(text_kind("ab"), Kind::Text);
    }

    #[test]
    fn code() {
        assert_eq!(language("fn main() {\n    println!(\"hi\");\n}\n"), Some("rust"));
        assert_eq!(language("def f(x):\n    return x + 1\n\nprint(f(2))"), Some("python"));
        assert_eq!(language("#!/bin/bash\necho hi"), Some("sh"));
        assert_eq!(language("{\n  \"a\": 1\n}"), Some("json"));
        assert_eq!(language("#include <stdio.h>\nint main(void) {\n    printf(\"x\");\n    return 0;\n}"), Some("c"));
    }

    #[test]
    fn prose_is_not_code() {
        assert_eq!(language("Hello there.\nThis is a normal message, nothing more."), None);
        assert_eq!(language("cargo build --release"), None);
        assert_eq!(language("Список дел:\n- купить хлеб\n  - белый\n  - чёрный\n- позвонить"), None);
        assert_eq!(language("Первый абзац текста.\n\nВторой абзац (с пояснением).\nИ ещё строка."), None);
    }

    #[test]
    fn tokens() {
        let t = highlight("let x = \"hi\"; // note", "rust");
        assert_eq!(t[0], (Tok::Key, "let".into()));
        assert!(t.iter().any(|x| x == &(Tok::Str, "\"hi\"".into())));
        assert_eq!(t.last().unwrap(), &(Tok::Comment, "// note".into()));
        assert!(highlight("fn f<'a>(x: &'a str)", "rust").iter().all(|x| x.0 != Tok::Str));
    }

    #[test]
    fn fuzzy_order() {
        let mk = |id, t: &str| Item { id, kind: Kind::Text, mime: "text/plain".into(), text: Some(t.into()), long: false, blob: None, hash: id.to_string(), size: 0, pinned: false, used: 0, secret: false };
        let items = vec![mk(1, "hello world"), mk(2, "git commit -m fix"), mk(3, "good comment"), mk(4, "Привет, Мир")];
        assert_eq!(filter(&items, "gtcmt").first().map(|i| i.id), Some(2));
        assert!(filter(&items, "zzz").is_empty());
        assert_eq!(filter(&items, "").len(), 4);
        assert_eq!(filter(&items, "мир").iter().map(|i| i.id).collect::<Vec<_>>(), vec![4]);
    }

    #[test]
    fn store_dedup_trim_pin() {
        let d = std::env::temp_dir().join(format!("yami-clip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let blobs = d.join("blobs");
        std::fs::create_dir_all(&blobs).unwrap();
        let mut s = Store { items: vec![], next: 1, path: d.join("history.json"), blobs: blobs.clone() };
        for (i, t) in ["a1", "b2", "c3", "a1"].iter().enumerate() {
            s.add(prepare(&blobs, "text/plain", t.as_bytes(), false, 10 + i as i64).unwrap());
        }
        assert_eq!(s.items.iter().map(|i| i.text.clone().unwrap()).collect::<Vec<_>>(), ["a1", "c3", "b2"]);
        let b = s.items[2].id;
        s.set_pinned(b, true);
        s.trim(1);
        assert_eq!(s.items.len(), 2);
        assert!(s.get(b).is_some());
        s.clear(false);
        assert_eq!(s.items.len(), 1);
        let big = "x".repeat(INLINE + 10);
        s.add(prepare(&blobs, "text/plain", big.as_bytes(), false, 99).unwrap());
        assert!(s.items[0].long);
        assert_eq!(s.full_text(&s.items[0]).len(), big.len());
        let _ = std::fs::remove_dir_all(&d);
    }
}
