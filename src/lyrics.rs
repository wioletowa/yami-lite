// Тексты песен для всплывашки музыки (#13). Откуда, по порядку: сам плеер (MPRIS xesam:asText),
// файл .lrc или .txt рядом с треком (xesam:url), кеш, LRCLIB (lrclib.net) — только если разрешено
// (службы → «тексты песен из сети»). Ответ сети кешируется в ~/.cache/yami/lyrics, «не нашлось» —
// тоже, на неделю. В памяти — только текст текущего трека. LRC с метками времени — построчно,
// звучащая строка подсвечена; без меток — простым текстом.

use crate::app::Msg;
use crate::media::MediaState;
use calloop::channel::Sender;
use std::path::PathBuf;
use std::process::Command;

/// текст длиннее — не текст песни (и не держим его в памяти)
const MAX_TEXT: u64 = 256 << 10;
const MAX_LINES: usize = 2000;

pub struct Line {
    /// секунда трека; у простого текста — -1
    pub at: f64,
    pub text: String,
}

#[derive(Default)]
pub struct Lyrics {
    /// трек, к которому текст: исполнитель \t название
    pub key: String,
    pub lines: Vec<Line>,
    /// у строк есть время (LRC)
    pub synced: bool,
    /// поиск закончен; lines пуст — текста нет
    pub done: bool,
    /// строка, с которой всплывашка нарисована последний раз (tick перерисует на следующей)
    pub shown: Option<usize>,
}

pub fn key(m: &MediaState) -> String {
    format!("{}\t{}", m.artist.trim(), m.title.trim())
}

impl Lyrics {
    /// строка, что звучит на позиции pos (с); у простого текста — нет
    pub fn now(&self, pos: f64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        self.lines.iter().rposition(|l| l.at <= pos + 0.15)
    }
    /// через сколько секунд сменится строка (у простого текста — никогда)
    pub fn next_in(&self, pos: f64) -> Option<f64> {
        if !self.synced {
            return None;
        }
        self.lines.iter().map(|l| l.at - 0.15 - pos).find(|d| *d > 0.0)
    }
}

/// разбор: LRC ([мм:сс.xx] строка; меток у строки бывает несколько, теги [ar:…] пропускаются,
/// [offset:±мс] сдвигает всё) или простой текст
pub fn parse(text: &str) -> (Vec<Line>, bool) {
    let mut out: Vec<Line> = vec![];
    let mut offset = 0.0;
    let mut synced = false;
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = vec![];
        let mut meta = false;
        while let Some(r) = rest.strip_prefix('[') {
            let Some(end) = r.find(']') else { break };
            let tag = &r[..end];
            match stamp(tag) {
                Some(t) => times.push(t),
                None if times.is_empty() && tag.contains(':') => {
                    if let Some(v) = tag.strip_prefix("offset:") {
                        offset = v.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
                    }
                    meta = true;
                    break;
                }
                None => break,
            }
            rest = r[end + 1..].trim_start();
        }
        if meta {
            continue;
        }
        if !times.is_empty() {
            synced = true;
            for t in times {
                out.push(Line { at: t, text: tidy(rest) });
            }
        } else {
            out.push(Line { at: -1.0, text: tidy(raw) });
        }
        if out.len() >= MAX_LINES {
            break;
        }
    }
    if synced {
        // LRC: строки без меток (заголовки) не нужны; положительный offset — текст раньше
        out.retain(|l| l.at >= 0.0);
        for l in out.iter_mut() {
            l.at = (l.at - offset).max(0.0);
        }
        out.sort_by(|a, b| a.at.total_cmp(&b.at));
    }
    // пустые строки (в LRC — проигрыши): подряд — одна, по краям не нужны
    out.dedup_by(|b, a| a.text.is_empty() && b.text.is_empty());
    if out.first().is_some_and(|l| l.text.is_empty()) {
        out.remove(0);
    }
    if out.last().is_some_and(|l| l.text.is_empty()) {
        out.pop();
    }
    (out, synced)
}

/// строка как есть, только без управляющих символов (табуляция — пробел): шрифту их не нарисовать
fn tidy(s: &str) -> String {
    s.trim().chars().filter_map(|c| match c {
        '\t' => Some(' '),
        c if c.is_control() => None,
        c => Some(c),
    }).collect()
}

/// «мм:сс», «мм:сс.xx», «мм:сс:xx» → секунды
fn stamp(tag: &str) -> Option<f64> {
    let (m, s) = tag.split_once(':')?;
    if m.is_empty() || !m.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let s = match s.split_once(':') {
        Some((a, b)) => format!("{a}.{b}"),
        None => s.to_string(),
    };
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    Some(m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?)
}

/// найти текст в своей нити → Msg::Lyrics; net — можно спросить lrclib.net
pub fn fetch(m: &MediaState, net: bool, tx: Sender<Msg>) {
    let key = key(m);
    let (player, artist, title, len) = (m.player.clone(), m.artist.clone(), m.title.clone(), m.length);
    std::thread::spawn(move || {
        let text = from_player(&player, "xesam:asText").or_else(|| from_file(&player)).or_else(|| match cached(&key) {
            Some(hit) => hit,
            // промах lrclib — в кеш на неделю; нет сети или lrclib не ответил — не запоминаем
            None if net => from_net(&artist, &title, len).inspect(|t| store(&key, t.as_deref())).ok().flatten(),
            None => None,
        });
        let (lines, synced) = text.map(|t| parse(&t)).unwrap_or_default();
        let _ = tx.send(Msg::Lyrics(Lyrics { key, lines, synced, done: true, shown: None }));
    });
}

/// поле метаданных плеера, как есть (текст бывает многострочным — отдельным вызовом, не в -F)
fn from_player(player: &str, field: &str) -> Option<String> {
    let mut c = Command::new("playerctl");
    if !player.is_empty() {
        c.args(["-p", player]);
    }
    let o = c.args(["metadata", field]).stderr(std::process::Stdio::null()).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (o.status.success() && !s.is_empty() && s.len() as u64 <= MAX_TEXT).then_some(s)
}

/// «трек.lrc» или «трек.txt» рядом с файлом, который играет
fn from_file(player: &str) -> Option<String> {
    let url = from_player(player, "xesam:url")?;
    let p = yami_px::icons::uri_path(&url)?;
    ["lrc", "txt"].iter().find_map(|e| yami_px::paths::read_capped(&p.with_extension(e), MAX_TEXT)).map(|b| String::from_utf8_lossy(&b).into_owned()).filter(|s| !s.trim().is_empty())
}

fn cache_dir() -> PathBuf {
    yami_px::paths::cache_dir().join("yami-lite/lyrics")
}

fn cache_file(key: &str) -> PathBuf {
    // FNV-1a: имя файла из исполнителя и названия, без них самих в пути
    let h = yami_px::fnv(yami_px::FNV0, key.as_bytes());
    cache_dir().join(format!("{h:016x}.lrc"))
}

/// Some(Some) — текст есть, Some(None) — недавно искали и не нашли, None — не искали
fn cached(key: &str) -> Option<Option<String>> {
    let p = cache_file(key);
    let m = std::fs::metadata(&p).ok()?;
    if m.len() == 0 {
        let fresh = m.modified().ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() < 7 * 86400);
        return fresh.then_some(None);
    }
    let b = yami_px::paths::read_capped(&p, MAX_TEXT)?;
    Some(Some(String::from_utf8_lossy(&b).into_owned()))
}

fn store(key: &str, text: Option<&str>) {
    static TIDY: std::sync::Once = std::sync::Once::new();
    let d = cache_dir();
    TIDY.call_once(|| yami_px::paths::tidy(&d, 365, 8 << 20));
    let _ = yami_px::paths::write_atomic(&cache_file(key), text.unwrap_or(""));
}

/// исполнитель и название без приписок видео и каналов: «(Official Video)», «[HD]», « - Topic»;
/// у видео без исполнителя он часто в названии: «Исполнитель - Песня»
fn clean(artist: &str, title: &str) -> (String, String) {
    let strip = |s: &str| {
        let mut o = String::new();
        let mut depth = 0;
        for ch in s.chars() {
            match ch {
                '(' | '[' => depth += 1,
                ')' | ']' => depth = (depth - 1).max(0),
                _ if depth == 0 => o.push(ch),
                _ => {}
            }
        }
        o.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let mut a = artist.trim().trim_end_matches(" - Topic").to_string();
    let mut t = strip(title);
    if a.is_empty() {
        if let Some((x, y)) = t.split_once(" - ") {
            (a, t) = (x.trim().to_string(), y.trim().to_string());
        }
    }
    (a, t)
}

/// LRCLIB: сначала точное совпадение (исполнитель, название, длительность), потом поиск;
/// текст с метками времени — лучше простого
/// Ok(None) — lrclib ответил, что текста нет; Err — ответа нет (сеть, время вышло, ошибка сервера)
fn from_net(artist: &str, title: &str, len: f64) -> Result<Option<String>, ()> {
    // без -f: 404 («не нашлось») отличаем от сбоя по коду ответа в последней строке
    let get = |url: &str, q: &[(&str, String)]| -> Result<Option<serde_json::Value>, ()> {
        let o = crate::proc::curl_get(8, 2_000_000, url, q).args(["-w", "\n%{http_code}"]).stderr(std::process::Stdio::null()).output().map_err(|_| ())?;
        let n = o.stdout.iter().rposition(|b| *b == b'\n').ok_or(())?;
        match (o.status.success(), &o.stdout[n + 1..]) {
            (true, b"200") => Ok(serde_json::from_slice(&o.stdout[..n]).ok()),
            (true, b"404") => Ok(None),
            _ => Err(()),
        }
    };
    let text = |v: &serde_json::Value, k: &str| v[k].as_str().filter(|s| !s.trim().is_empty() && s.len() as u64 <= MAX_TEXT).map(String::from);
    let (a, t) = clean(artist, title);
    if t.is_empty() {
        return Ok(None);
    }
    let mut q = vec![("artist_name", a.clone()), ("track_name", t.clone())];
    if len > 0.0 {
        q.push(("duration", format!("{}", len.round() as i64)));
    }
    if let Some(v) = get("https://lrclib.net/api/get", &q)? {
        if let Some(s) = text(&v, "syncedLyrics").or_else(|| text(&v, "plainLyrics")) {
            return Ok(Some(s));
        }
    }
    let Some(v) = get("https://lrclib.net/api/search", &[("q", format!("{a} {t}").trim().to_string())])? else { return Ok(None) };
    let Some(list) = v.as_array() else { return Ok(None) };
    // поиск возвращает и чужие песни: берём только с тем же названием (и исполнителем, если он
    // известен и записан так же) и похожей длительностью
    let (nt, na) = (norm(&t), norm(&a));
    let fits = |x: &&serde_json::Value| {
        let (xt, xa) = (norm(x["trackName"].as_str().unwrap_or("")), norm(x["artistName"].as_str().unwrap_or("")));
        let dur = x["duration"].as_f64().unwrap_or(0.0);
        let title = !xt.is_empty() && (xt.contains(&nt) || nt.contains(&xt));
        let artist = na.is_empty() || (!xa.is_empty() && (xa.contains(&na) || na.contains(&xa)));
        title && (artist || xt == nt) && (len <= 0.0 || dur <= 0.0 || (dur - len).abs() < 15.0)
    };
    let good: Vec<&serde_json::Value> = list.iter().filter(fits).collect();
    Ok(good.iter().find_map(|x| text(x, "syncedLyrics")).or_else(|| good.iter().find_map(|x| text(x, "plainLyrics"))))
}

/// для сравнения названий: только буквы и цифры, строчные
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_and_plain() {
        let (l, s) = parse("[ar:Someone]\n[offset:+500]\n[00:01.00]one\n[00:03.50][00:10.00]two\n[01:02:30]three\n");
        assert!(s);
        let v: Vec<(f64, &str)> = l.iter().map(|x| (x.at, x.text.as_str())).collect();
        assert_eq!(v, vec![(0.5, "one"), (3.0, "two"), (9.5, "two"), (61.8, "three")]);
        let ly = Lyrics { key: String::new(), lines: l, synced: true, done: true, shown: None };
        assert_eq!(ly.now(0.0), None);
        assert_eq!(ly.now(3.2), Some(1));
        assert!((ly.next_in(3.2).unwrap() - 6.15).abs() < 1e-9);
        let (l, s) = parse("\n\nfirst\n\n\nsecond\n\n");
        assert!(!s);
        assert_eq!(l.iter().map(|x| x.text.as_str()).collect::<Vec<_>>(), vec!["first", "", "second"]);
        // скобки в простом тексте — не метки
        let (l, s) = parse("[Chorus]\nla la");
        assert!(!s);
        assert_eq!(l.len(), 2);
        // проигрыши LRC: подряд — один, по краям — прочь; табуляция — пробел
        let (l, _) = parse("[00:01.00]\n[00:02.00]a\tb\n[00:03.00]\n[00:04.00] \n[00:05.00]c\n[00:06.00]\n");
        assert_eq!(l.iter().map(|x| (x.at, x.text.as_str())).collect::<Vec<_>>(), vec![(2.0, "a b"), (3.0, ""), (5.0, "c")]);
    }

    /// живой запрос к lrclib.net: `cargo test -p yami-shell lyrics -- --ignored`
    #[test]
    #[ignore]
    fn lrclib_live() {
        let t = from_net("Kino", "Gruppa krovi (Remastered)", 282.0).expect("lrclib answers").expect("lrclib");
        assert!(t.lines().count() > 10);
        let t = from_net("", "Kino - Gruppa krovi [HD]", 0.0).expect("lrclib answers").expect("lrclib search");
        assert!(!parse(&t).0.is_empty());
        // нет такой песни — ответ «нет», а не сбой: такой промах кешируется
        assert_eq!(from_net("zzqx no such band", "zzqx no such song 9137", 77.0), Ok(None));
    }

    #[test]
    fn cleans_video_titles() {
        assert_eq!(clean("", "Artist - Song (Official Video) [HD]"), ("Artist".into(), "Song".into()));
        assert_eq!(clean("Band - Topic", "Song"), ("Band".into(), "Song".into()));
        assert_eq!(clean("Band", "Song (Remastered 2011)"), ("Band".into(), "Song".into()));
    }
}
