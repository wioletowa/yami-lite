// Виджет «заметки»: файл markdown (по умолчанию ~/Documents/notes.md, свой путь — w.text) красиво:
// заголовки, списки и нумерация, дела [ ] / [x] (щелчок по квадратику ставит и снимает галочку
// прямо в файле), цитаты, разделители, **жирный** (акцентом), `код`, ~~зачёркнутое~~. Колесо
// листает. Двойной щелчок — правка как есть: курсор, Enter (список продолжается), стрелки, Home/End,
// Backspace/Delete; Esc, щелчок мимо или уход фокуса — снова красивый вид. Пишется сразу в файл;
// правки снаружи (редактор, синхронизация) подхватываются, пока заметку не правят здесь.

use crate::app::{App, SKind, State};
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, C, R};
use crate::ui::{Act, Cur, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const LINE: i32 = 20;
const TEMPLATE: &str = "# заметки\n\n- [ ] первое дело\n- [x] сделанное\n\n**жирный**, `код`, ~~зачёркнуто~~\n";

#[derive(Default)]
pub struct Note {
    pub path: PathBuf,
    pub text: String,
    mtime: Option<std::time::SystemTime>,
    pub scroll: i32,
    /// высота содержимого с прошлого кадра (докуда листать)
    pub height: std::cell::Cell<i32>,
    pub editing: bool,
    /// курсор: байт в text
    pub cur: usize,
    last_click: Option<Instant>,
    /// правка: где на экране буквы видимой части (x, y, байт) — щелчок ставит курсор
    pub glyphs: std::cell::RefCell<Vec<(i32, i32, usize)>>,
}

pub fn size(size: &str) -> (i32, i32) {
    match size {
        "s" => (240, 200),
        "l" => (420, 420),
        _ => (320, 300),
    }
}

pub fn file_of(w: &crate::config::Widget) -> PathBuf {
    if !w.text.trim().is_empty() {
        return yami_px::paths::expand(w.text.trim());
    }
    yami_px::paths::xdg_dir("DOCUMENTS").unwrap_or_else(|| yami_px::paths::home().join("Documents")).join("notes.md")
}

// ── разбор строк ────────────────────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Blank,
    Rule,
    Head(u8, String),
    Todo(bool, String),
    Bullet(String),
    Num(String, String),
    Quote(String),
    Text(String),
}

/// строка → блок и отступ (по два пробела — уровень)
pub fn block(line: &str) -> (usize, Block) {
    let indent = (line.len() - line.trim_start().len()) / 2;
    let s = line.trim();
    if s.is_empty() {
        return (0, Block::Blank);
    }
    if s.chars().all(|c| c == '-' || c == '*' || c == '_') && s.len() >= 3 {
        return (0, Block::Rule);
    }
    if let Some(r) = s.strip_prefix("### ") {
        return (0, Block::Head(3, r.into()));
    }
    if let Some(r) = s.strip_prefix("## ") {
        return (0, Block::Head(2, r.into()));
    }
    if let Some(r) = s.strip_prefix("# ") {
        return (0, Block::Head(1, r.into()));
    }
    for p in ["- ", "* ", "+ "] {
        if let Some(r) = s.strip_prefix(p) {
            for (mark, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
                if let Some(t) = r.strip_prefix(mark) {
                    return (indent, Block::Todo(done, t.into()));
                }
            }
            if r == "[ ]" || r == "[x]" || r == "[X]" {
                return (indent, Block::Todo(r != "[ ]", String::new()));
            }
            return (indent, Block::Bullet(r.into()));
        }
    }
    if let Some(r) = s.strip_prefix("> ") {
        return (0, Block::Quote(r.into()));
    }
    if let Some((n, r)) = s.split_once(". ") {
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return (indent, Block::Num(format!("{n}."), r.into()));
        }
    }
    (indent, Block::Text(s.into()))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Span {
    Plain,
    Bold,
    Code,
    Strike,
}

/// выделение в строке: **жирный**, `код`, ~~зачёркнутое~~ (незакрытое — как есть)
pub fn spans(s: &str) -> Vec<(String, Span)> {
    let mut out: Vec<(String, Span)> = vec![];
    let mut rest = s;
    let mut plain = String::new();
    while !rest.is_empty() {
        let found = [("**", Span::Bold), ("`", Span::Code), ("~~", Span::Strike)].into_iter().find_map(|(m, k)| {
            let body = rest.strip_prefix(m)?;
            let end = body.find(m)?;
            (end > 0).then(|| (k, &body[..end], &body[end + m.len()..]))
        });
        match found {
            Some((k, inner, after)) => {
                if !plain.is_empty() {
                    out.push((std::mem::take(&mut plain), Span::Plain));
                }
                out.push((inner.to_string(), k));
                rest = after;
            }
            None => {
                let ch = rest.chars().next().unwrap_or(' ');
                plain.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
    }
    if !plain.is_empty() {
        out.push((plain, Span::Plain));
    }
    out
}

/// отрисовать строку с выделением, перенося по словам в ширину w; высота — сколько строк вышло
#[allow(clippy::too_many_arguments)]
fn rich(ui: &mut Ui, x0: i32, y0: i32, w: i32, text: &str, base: C, accent: C, strike_all: bool, draw: bool) -> i32 {
    let t = ui.t;
    let space = ui.w(PX, " ");
    let (mut x, mut y) = (x0, y0);
    for (seg, kind) in spans(text) {
        // слова с пробелами после — кусками, чтобы переносить по словам
        let mut words: Vec<&str> = vec![];
        let mut start = 0;
        for (i, c) in seg.char_indices() {
            if c == ' ' {
                words.push(&seg[start..=i]);
                start = i + 1;
            }
        }
        if start < seg.len() {
            words.push(&seg[start..]);
        }
        for word in words {
            let ww = ui.w(PX, word.trim_end());
            if x > x0 && x + ww > x0 + w {
                x = x0;
                y += LINE;
            }
            if draw {
                let c = match kind {
                    Span::Bold => accent,
                    Span::Code => t.text_hi,
                    _ => base,
                };
                if kind == Span::Code {
                    ui.cv.fill(R::new(x - 1, y + 2, ww + 2, LINE - 4), t.code_bg);
                }
                ui.cv.text_in(ui.f, PX, R::new(x, y, ww + 2, LINE), word.trim_end(), c);
                if kind == Span::Strike || strike_all {
                    ui.cv.fill(R::new(x, y + LINE / 2, ww, 1), c);
                }
            }
            x += ww + if word.ends_with(' ') { space } else { 0 };
        }
    }
    y + LINE - y0
}

pub fn draw(ui: &mut Ui, st: &State, id: u32, r: R, c: C) {
    let t = ui.t;
    let Some(n) = st.notes.get(&id) else { return };
    ui.cv.push_clip(r);
    if n.editing {
        draw_edit(ui, n, r, c);
    } else {
        let mut y = r.y - n.scroll;
        for (i, line) in n.text.lines().enumerate() {
            let (indent, b) = block(line);
            let x = r.x + indent as i32 * 14;
            let w = r.right() - x;
            let visible = y + LINE * 4 > r.y && y < r.bottom();
            let dh = match &b {
                Block::Blank => 8,
                Block::Rule => {
                    if visible {
                        ui.cv.fill(R::new(r.x, y + 8, r.w, 1), t.border);
                    }
                    18
                }
                Block::Head(1, s) => {
                    let f = font::px(24);
                    if visible {
                        ui.text_in(f, R::new(x, y, w, 30), s, t.text_hi);
                    }
                    32
                }
                Block::Head(_, s) => {
                    let col = if matches!(b, Block::Head(2, _)) { t.text_hi } else { c };
                    if visible {
                        ui.cv.fill(R::new(x, y + 6, 3, LINE - 10), c);
                    }
                    rich(ui, x + 8, y, w - 8, s, col, c, false, visible) + 2
                }
                Block::Todo(done, s) => {
                    let bx = R::new(x, y + 4, 12, 12);
                    if visible {
                        let hov = ui.hover(bx);
                        ui.cv.pxbox(bx, &BoxStyle::new(if *done { c } else { t.code_bg }, if hov || *done { c } else { t.border }));
                        if *done {
                            for (dx, dy) in [(2, 6), (3, 7), (4, 8), (5, 7), (6, 6), (7, 5), (8, 4), (9, 3)] {
                                ui.cv.fill(R::new(bx.x + dx, bx.y + dy - 1, 1, 2), t.on_accent);
                            }
                        }
                        ui.zone(R::new(bx.x - 3, bx.y - 3, 18, 18), Act::NotesCheck(id, i), Cur::Pointer);
                    }
                    rich(ui, x + 20, y, w - 20, s, if *done { t.text_faint } else { t.text }, c, *done, visible)
                }
                Block::Bullet(s) => {
                    if visible {
                        ui.cv.fill(R::new(x + 3, y + 8, 4, 4), c);
                    }
                    rich(ui, x + 14, y, w - 14, s, t.text, c, false, visible)
                }
                Block::Num(num, s) => {
                    let nw = ui.w(PX, num) + 6;
                    if visible {
                        ui.text_in(PX, R::new(x, y, nw, LINE), num, c);
                    }
                    rich(ui, x + nw, y, w - nw, s, t.text, c, false, visible)
                }
                Block::Quote(s) => {
                    let h = rich(ui, x + 12, y, w - 12, s, t.text_dim, c, false, false);
                    if visible {
                        ui.cv.fill(R::new(x + 2, y + 2, 2, h - 4), c.fade(0.7));
                        rich(ui, x + 12, y, w - 12, s, t.text_dim, c, false, true);
                    }
                    h
                }
                Block::Text(s) => rich(ui, x, y, w, s, t.text, c, false, visible),
            };
            y += dh;
        }
        n.height.set(y + n.scroll - r.y);
        // полоса прокрутки
        let total = n.height.get();
        if total > r.h {
            let th = (r.h * r.h / total).max(16);
            let ty = r.y + (r.h - th) * n.scroll / (total - r.h).max(1);
            ui.cv.fill(R::new(r.right() - 2, ty, 2, th), c.fade(0.6));
        }
    }
    ui.cv.pop_clip();
    // тело: двойной щелчок — правка, колесо — листать (галочки — поверх)
    ui.zones.insert(0, crate::ui::Zone { r: R::new(r.x + ui.cv.ox, r.y + ui.cv.oy, r.w, r.h), act: Act::NotesBody(id), cur: if n.editing { Cur::Text } else { Cur::Default } });
}

/// правка: текст как есть, перенос по буквам, курсор; видимая часть — вокруг курсора
fn draw_edit(ui: &mut Ui, n: &Note, r: R, c: C) {
    let t = ui.t;
    ui.cv.fill(r, t.code_bg.fade(0.6));
    // раскладка: позиция каждой буквы
    let mut pos: Vec<(i32, i32, usize)> = vec![];
    let (mut x, mut y) = (r.x + 4, 0);
    let mut cur_xy = (x, 0);
    for (i, ch) in n.text.char_indices() {
        if i == n.cur {
            cur_xy = (x, y);
        }
        if ch == '\n' {
            pos.push((x, y, i));
            x = r.x + 4;
            y += LINE;
            continue;
        }
        let a = ui.f.advance(PX, ch);
        if x + a > r.right() - 4 {
            x = r.x + 4;
            y += LINE;
        }
        pos.push((x, y, i));
        x += a;
    }
    if n.cur >= n.text.len() {
        cur_xy = (x, y);
    }
    // сдвиг: курсор всегда виден
    let off = (cur_xy.1 - r.h + LINE * 2).max(0);
    let mut glyphs = n.glyphs.borrow_mut();
    glyphs.clear();
    for &(gx, gy, i) in &pos {
        let sy = r.y + gy - off;
        if sy + LINE < r.y || sy > r.bottom() {
            continue;
        }
        glyphs.push((gx, sy, i));
        let ch = n.text[i..].chars().next().unwrap_or(' ');
        if ch != '\n' {
            let mut b = [0u8; 4];
            ui.cv.text_in(ui.f, PX, R::new(gx, sy, 20, LINE), ch.encode_utf8(&mut b), t.text);
        }
    }
    // конец текста — тоже место для курсора
    glyphs.push((x, r.y + y - off, n.text.len()));
    let (cx, cy) = (cur_xy.0, r.y + cur_xy.1 - off);
    ui.cv.fill(R::new(cx, cy + 3, 2, LINE - 6), c);
    ui.cv.outline(r, c, 1);
}

impl App {
    /// заметки виджетов: файл прочитан, свежий — перечитан (снаружи правили), пока не правят здесь
    pub fn notes_sync(&mut self) {
        let ws: Vec<(u32, PathBuf)> = self.st.set.widgets.iter().filter(|w| w.on && w.kind == "notes").map(|w| (w.id, file_of(w))).collect();
        self.st.notes.retain(|id, _| ws.iter().any(|w| w.0 == *id));
        for (id, path) in ws {
            let n = self.st.notes.entry(id).or_default();
            if n.path != path {
                *n = Note { path: path.clone(), ..Default::default() };
            }
            if n.editing {
                continue;
            }
            if !path.exists() {
                if let Some(d) = path.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let _ = std::fs::write(&path, crate::i18n::tr(TEMPLATE));
            }
            let m = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if m != n.mtime {
                n.mtime = m;
                n.text = std::fs::read_to_string(&path).unwrap_or_default();
                n.cur = n.cur.min(n.text.len());
                self.dirty(&SKind::Widget(id));
            }
        }
    }

    fn notes_save(&mut self, id: u32) {
        let Some(n) = self.st.notes.get_mut(&id) else { return };
        let _ = yami_px::paths::write_atomic(&n.path, &n.text);
        n.mtime = std::fs::metadata(&n.path).and_then(|m| m.modified()).ok();
    }

    /// квадратик дела: [ ] ↔ [x] в той строке файла
    pub fn notes_check(&mut self, id: u32, line: usize) {
        let Some(n) = self.st.notes.get_mut(&id) else { return };
        let mut lines: Vec<String> = n.text.lines().map(String::from).collect();
        let Some(l) = lines.get_mut(line) else { return };
        let new = if let Some(i) = l.find("[ ]") {
            format!("{}[x]{}", &l[..i], &l[i + 3..])
        } else if let Some(i) = l.find("[x]").or_else(|| l.find("[X]")) {
            format!("{}[ ]{}", &l[..i], &l[i + 3..])
        } else {
            return;
        };
        *l = new;
        let trail = n.text.ends_with('\n');
        n.text = lines.join("\n") + if trail { "\n" } else { "" };
        self.notes_save(id);
        self.dirty(&SKind::Widget(id));
    }

    /// щелчок по телу: двойной — правка; в правке — курсор к ближайшей букве
    pub fn notes_click(&mut self, id: u32, x: i32, y: i32) {
        let Some(n) = self.st.notes.get_mut(&id) else { return };
        if n.editing {
            let hit = n.glyphs.borrow().iter().filter(|(_, gy, _)| y >= *gy && y < gy + LINE).min_by_key(|(gx, _, _)| (gx - x).abs()).map(|g| g.2);
            if let Some(i) = hit {
                n.cur = i;
            }
            self.dirty(&SKind::Widget(id));
            return;
        }
        let double = n.last_click.is_some_and(|t| t.elapsed() < Duration::from_millis(400));
        n.last_click = Some(Instant::now());
        if double {
            n.editing = true;
            n.cur = n.text.len();
            n.last_click = None;
            self.st.field_focus = Some("notes");
            self.dirty(&SKind::Widget(id));
        }
    }

    pub fn notes_scroll(&mut self, id: u32, up: bool, h: i32) {
        let Some(n) = self.st.notes.get_mut(&id) else { return };
        if n.editing {
            return;
        }
        let max = (n.height.get() - h).max(0);
        n.scroll = (n.scroll + if up { -LINE * 2 } else { LINE * 2 }).clamp(0, max);
        self.dirty(&SKind::Widget(id));
    }

    /// выйти из правки (Esc, щелчок мимо, фокус ушёл) — записано уже по ходу
    pub fn notes_leave(&mut self) {
        let ids: Vec<u32> = self.st.notes.iter().filter(|(_, n)| n.editing).map(|(id, _)| *id).collect();
        for id in ids {
            if let Some(n) = self.st.notes.get_mut(&id) {
                n.editing = false;
            }
            self.notes_save(id);
            self.dirty(&SKind::Widget(id));
        }
        if self.st.field_focus == Some("notes") {
            self.st.field_focus = None;
        }
    }

    /// клавиши в правке
    pub fn notes_key(&mut self, id: u32, e: &KeyEvent) -> bool {
        let ctrl = self.mods.ctrl;
        let Some(n) = self.st.notes.get_mut(&id).filter(|n| n.editing) else { return false };
        let s = &mut n.text;
        let prev = |s: &str, i: usize| s[..i].char_indices().next_back().map_or(0, |(p, _)| p);
        let next = |s: &str, i: usize| s[i..].chars().next().map_or(i, |c| i + c.len_utf8());
        let line_start = |s: &str, i: usize| s[..i].rfind('\n').map_or(0, |p| p + 1);
        let line_end = |s: &str, i: usize| s[i..].find('\n').map_or(s.len(), |p| i + p);
        match e.keysym {
            Keysym::Escape => {
                self.notes_leave();
                return true;
            }
            Keysym::Return | Keysym::KP_Enter => {
                // список продолжается: «- [ ] », «- », «1. » — с тем же отступом
                let ls = line_start(s, n.cur);
                let line = &s[ls..n.cur];
                let ind: String = line.chars().take_while(|c| *c == ' ').collect();
                let body = line.trim_start();
                let cont = match block(line).1 {
                    Block::Todo(..) => body.split_once(']').map(|(a, _)| format!("{a}] ").replace("[x]", "[ ]").replace("[X]", "[ ]")),
                    Block::Bullet(_) => Some(body[..2].to_string()),
                    Block::Num(num, _) => num.trim_end_matches('.').parse::<u32>().ok().map(|k| format!("{}. ", k + 1)),
                    _ => None,
                };
                let ins = format!("\n{ind}{}", cont.unwrap_or_default());
                s.insert_str(n.cur, &ins);
                n.cur += ins.len();
            }
            Keysym::BackSpace if n.cur > 0 => {
                let p = prev(s, n.cur);
                s.replace_range(p..n.cur, "");
                n.cur = p;
            }
            Keysym::Delete if n.cur < s.len() => {
                let q = next(s, n.cur);
                s.replace_range(n.cur..q, "");
            }
            Keysym::Left => n.cur = prev(s, n.cur),
            Keysym::Right => n.cur = next(s, n.cur),
            Keysym::Home => n.cur = line_start(s, n.cur),
            Keysym::End => n.cur = line_end(s, n.cur),
            Keysym::Up | Keysym::Down => {
                // та же колонка (в буквах) в соседней строке
                let ls = line_start(s, n.cur);
                let col = s[ls..n.cur].chars().count();
                let target = if e.keysym == Keysym::Up {
                    if ls == 0 {
                        return true;
                    }
                    line_start(s, ls - 1)
                } else {
                    let le = line_end(s, n.cur);
                    if le >= s.len() {
                        return true;
                    }
                    le + 1
                };
                let te = line_end(s, target);
                n.cur = s[target..te].char_indices().nth(col).map_or(te, |(i, _)| target + i);
            }
            _ if ctrl => return true,
            _ => match &e.utf8 {
                Some(t) if !t.is_empty() && !t.chars().any(|c| c.is_control()) => {
                    s.insert_str(n.cur, t);
                    n.cur += t.len();
                }
                _ => return true,
            },
        }
        self.notes_save(id);
        self.dirty(&SKind::Widget(id));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_blocks() {
        assert_eq!(block("# Заголовок"), (0, Block::Head(1, "Заголовок".into())));
        assert_eq!(block("  - [x] сделано"), (1, Block::Todo(true, "сделано".into())));
        assert_eq!(block("- [ ] дело"), (0, Block::Todo(false, "дело".into())));
        assert_eq!(block("* пункт"), (0, Block::Bullet("пункт".into())));
        assert_eq!(block("12. двенадцать"), (0, Block::Num("12.".into(), "двенадцать".into())));
        assert_eq!(block("> цитата"), (0, Block::Quote("цитата".into())));
        assert_eq!(block("---"), (0, Block::Rule));
        assert_eq!(block("   "), (0, Block::Blank));
    }

    #[test]
    fn inline_spans() {
        assert_eq!(spans("a **b** `c` ~~d~~"), vec![("a ".into(), Span::Plain), ("b".into(), Span::Bold), (" ".into(), Span::Plain), ("c".into(), Span::Code), (" ".into(), Span::Plain), ("d".into(), Span::Strike)]);
        // незакрытое — как есть
        assert_eq!(spans("**нет"), vec![("**нет".into(), Span::Plain)]);
    }
}
