// Буфер обмена (Modal::Clip, модуль clipboard): история копий карточками — текст, код с
// подсветкой и номерами строк, картинки, файлы, цвета, символы. Справа во всю высоту (под ловцом,
// без затемнения) или окном по центру (clipPos). Печать сразу идёт в нечёткий поиск, Enter —
// копировать и закрыть, Ctrl+S — закрепить, Delete — удалить. Приват: копии не записываются,
// превью — мозаикой, пока на карточку не навели.
//
// Буфер слушает своя нить (clipwl.rs), история — clipstore.rs. Миниатюры и сведения о файлах
// готовят нити: рисование только читает готовое.

use crate::app::{App, Modal, Msg, SKind, State};
use crate::clipstore::{self, Item, Kind, Store, Tok};
use crate::clipwl::{self, Ev, Handle, Shared};
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, Img, C, R};
use crate::ui::{Act, Cur, Field, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// окно по центру и колонка справа
const CENTER: (i32, i32) = (560, 640);
const SIDE_W: i32 = 400;
/// выбор опустел: столько ждать, прежде чем вернуть последнюю запись (программы часто очищают
/// буфер и тут же ставят новый)
const RESTORE: Duration = Duration::from_millis(300);
/// строка текста и кода в карточке
const LH: i32 = 18;
const CODE_LH: i32 = 16;
const TEXT_LINES: usize = 3;
const CODE_LINES: usize = 5;
const IMG_H: i32 = 104;
/// где начинается список
const LIST_Y: i32 = 80;
const PAD: i32 = 12;

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub name: String,
    pub dir: bool,
    pub size: Option<u64>,
    /// имена значка типа по порядку (fileops::Mime::icons)
    pub icons: Vec<String>,
}

#[derive(Default)]
pub struct Clip {
    pub store: Store,
    wl: Option<Handle>,
    shared: Shared,
    pub query: Field,
    /// записи под запрос (id), карточка под клавиатурой и первая видимая
    pub view: Vec<u64>,
    pub cur: usize,
    pub top: usize,
    /// сколько карточек влезло и высота списка — с прошлого кадра (клавиатура держит cur видимой)
    pub shown: Cell<usize>,
    pub list_h: Cell<i32>,
    pub width: Cell<i32>,
    pub thumbs: HashMap<String, Option<Rc<Img>>>,
    pub files: HashMap<u64, Vec<FileInfo>>,
    pending: HashSet<String>,
    /// «очистить» нажато раз: второе нажатие до этого времени — очистить
    pub clear_armed: Option<Instant>,
}

/// сколько файлов в записи
fn file_count(it: &Item) -> usize {
    it.text.as_deref().map(|t| t.lines().filter(|l| l.trim().starts_with("file://")).count()).unwrap_or(0).max(1)
}

/// высота карточки: рамка, строка заголовка и тело по виду записи
pub fn card_h(it: &Item) -> i32 {
    let body = match it.kind {
        Kind::Text => TEXT_LINES as i32 * LH,
        Kind::Code => {
            let n = it.text.as_deref().map(|t| t.trim_end().lines().count()).unwrap_or(1).clamp(1, CODE_LINES);
            n as i32 * CODE_LH
        }
        Kind::Image => IMG_H,
        Kind::Files => {
            let n = file_count(it);
            if n == 1 {
                34
            } else {
                n.min(3) as i32 * 22 + if n > 3 { 18 } else { 0 }
            }
        }
        Kind::Color => 32,
        Kind::Char => 52,
    };
    8 + 20 + 6 + body + 10
}

fn kind_name(k: Kind) -> (&'static str, &'static str) {
    match k {
        Kind::Text => ("text", "текст"),
        Kind::Code => ("term", "код"),
        Kind::Color => ("drop", "цвет"),
        Kind::Char => ("pen", "символ"),
        Kind::Image => ("image", "картинка"),
        Kind::Files => ("folder", "файлы"),
    }
}

/// мозаика поверх r (приват): средний цвет клеток cell×cell
fn mosaic(ui: &mut Ui, r: R, cell: i32) {
    let cv = &mut ui.cv;
    let r = R::new(r.x + cv.ox, r.y + cv.oy, r.w, r.h).inter(&R::new(0, 0, cv.w, cv.h));
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    let mut y = r.y;
    while y < r.bottom() {
        let mut x = r.x;
        let ch = cell.min(r.bottom() - y);
        while x < r.right() {
            let cw = cell.min(r.right() - x);
            let (mut s, n) = ([0u32; 4], (cw * ch) as u32);
            for yy in y..y + ch {
                for xx in x..x + cw {
                    let p = cv.buf[(yy * cv.w + xx) as usize];
                    for (k, v) in s.iter_mut().enumerate() {
                        *v += p >> (k * 8) & 0xff;
                    }
                }
            }
            let avg = s.iter().enumerate().fold(0u32, |a, (k, v)| a | (v / n) << (k * 8));
            for yy in y..y + ch {
                for xx in x..x + cw {
                    cv.buf[(yy * cv.w + xx) as usize] = avg;
                }
            }
            x += cell;
        }
        y += cell;
    }
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let c = &st.clip;
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, false);
    let private = st.set.clip_private;
    let n = c.store.items.len();
    let sub = if private { crate::i18n::tr("приват").to_string() } else { n.to_string() };
    ui.header(R::new(1, 1, fr.w - 2, 30), "copy", "буфер", &sub);
    // шапка справа налево: закрыть, настройки, приват, очистить
    let mut x = fr.right() - 28;
    ui.ghost(R::new(x, 5, 24, 22), "close", Act::CloseModal);
    x -= 26;
    ui.ghost(R::new(x, 5, 24, 22), "gear", Act::ClipSettings);
    x -= 26;
    ui.button(R::new(x, 5, 24, 22), "", "mosaic", Tone::Ghost, private, true, Act::ClipPrivate);
    let armed = c.clear_armed.is_some_and(|a| Instant::now() < a);
    if armed {
        let bw = ui.w(PX, "ещё раз") + 36;
        x -= bw + 4;
        ui.button(R::new(x, 5, bw, 22), "ещё раз", "trash", Tone::Danger, false, true, Act::ClipClear);
    } else {
        x -= 26;
        ui.ghost(R::new(x, 5, 24, 22), "trash", Act::ClipClear);
    }
    ui.field(R::new(PAD, 40, fr.w - 2 * PAD, 28), &c.query, "поиск — просто печатай", true, false, Act::ClipField);

    let list = R::new(PAD, LIST_Y, fr.w - 2 * PAD - 8, fr.bottom() - LIST_Y - PAD);
    c.list_h.set(list.h);
    c.width.set(list.w);
    ui.zone(list, Act::ClipList, Cur::Default);
    if c.view.is_empty() {
        let msg = if c.wl.is_none() && n == 0 {
            "буфер не подключён: композитор без data-control"
        } else if c.query.text.trim().is_empty() {
            if private { "приват: копии не записываются" } else { "история пуста — скопируй что-нибудь" }
        } else {
            "ничего не нашлось"
        };
        ui.text_center(PX, R::new(list.x, list.y + 30, list.w, 24), msg, t.text_faint);
    }
    ui.cv.push_clip(list);
    let mut y = list.y;
    let mut shown = 0;
    for (i, id) in c.view.iter().enumerate().skip(c.top) {
        let Some(it) = c.store.get(*id) else { continue };
        let ch = card_h(it);
        if y + ch > list.bottom() && shown > 0 {
            break;
        }
        card(ui, st, it, R::new(list.x, y, list.w, ch), i == c.cur, private);
        y += ch + 8;
        shown += 1;
    }
    ui.cv.pop_clip();
    c.shown.set(shown.max(1));
    // полоса прокрутки: по записям
    if c.view.len() > shown {
        let tr = R::new(list.right() + 4, list.y, 4, list.h);
        ui.cv.fill(tr, t.border.fade(0.5));
        let th = (tr.h * shown as i32 / c.view.len() as i32).max(14);
        let max = c.view.len().saturating_sub(shown).max(1);
        let ty = tr.y + (tr.h - th) * c.top.min(max) as i32 / max as i32;
        ui.cv.fill(R::new(tr.x, ty, 4, th), t.red);
    }
}

fn card(ui: &mut Ui, st: &State, it: &Item, r: R, sel: bool, private: bool) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if sel { t.selection.alpha(0.55) } else { t.surface.alpha(0.6) }, if sel { t.ember } else if hov { t.text_dim } else { t.border }).shadow(if sel || hov { 2 } else { 0 }));
    ui.zone(r, Act::ClipPick(it.id), Cur::Pointer);
    // заголовок: вид, время; справа — закрепить и удалить (видны при наведении, выбранной и у закреплённых)
    let (icon, name) = kind_name(it.kind);
    let hy = r.y + 8;
    ui.cv.icon(icon, r.x + 10, hy + 3, 2, if sel { t.red } else { t.text_dim });
    let nw = ui.text_in(PX, R::new(r.x + 30, hy, 120, 20), name, t.text_dim);
    let mut info = clipstore::ago(it.used);
    if it.secret {
        info = format!("{info} · {}", crate::i18n::tr("секрет"));
    }
    ui.cv.text_in(ui.f, PX, R::new(r.x + 38 + nw, hy, r.w / 2, 20), &info, t.text_faint);
    let bx = r.right() - 30;
    if (hov || sel) && !it.pinned {
        ui.ghost(R::new(bx, hy - 1, 24, 22), "trash", Act::ClipDel(it.id));
    }
    if hov || sel || it.pinned {
        let pr = R::new(if it.pinned { bx } else { bx - 26 }, hy - 1, 24, 22);
        if it.pinned {
            ui.button(pr, "", "pin", Tone::Ghost, true, true, Act::ClipPin(it.id));
        } else {
            ui.ghost(pr, "pin", Act::ClipPin(it.id));
        }
    }
    let body = R::new(r.x + 10, hy + 26, r.w - 20, r.h - 8 - 26 - 10);
    ui.cv.push_clip(body);
    match it.kind {
        Kind::Text => {
            let s: String = it.text.as_deref().unwrap_or("").trim().chars().take(600).collect::<String>().replace('\t', "    ");
            // своё как есть, без перевода интерфейса
            ui.cv.text_wrap(ui.f, PX, body, &s, t.text, TEXT_LINES);
        }
        Kind::Code => code(ui, it, body),
        Kind::Image => match it.blob.as_ref().and_then(|b| st.clip.thumbs.get(b)) {
            Some(Some(img)) => ui.cv.image(img, body.x, body.y + (body.h - img.h) / 2, 1.0),
            Some(None) => {
                ui.cv.icon_in("image", R::new(body.x, body.y, 60, body.h), 3, t.text_faint);
                ui.text_in(PX, R::new(body.x + 70, body.y, body.w - 70, body.h), "не открылась", t.text_faint);
            }
            None => ui.cv.icon_in("image", R::new(body.x, body.y, 60, body.h), 3, t.border),
        },
        Kind::Files => files(ui, st, it, body),
        Kind::Color => {
            let s = it.text.as_deref().unwrap_or("").trim();
            let rgb = clipstore::color_rgb(s).unwrap_or(0);
            let sw = R::new(body.x, body.y + 2, 56, body.h - 4);
            let c = C(0xff000000 | rgb);
            ui.cv.pxbox(sw, &BoxStyle::new(c, t.border).bevel(true));
            ui.cv.text_in(ui.f, PX, R::new(sw.right() + 12, body.y, body.w - 70, body.h), s, t.text);
            // второе написание: rgb() к hex и hex ко всему остальному
            let alt = if s.starts_with('#') { format!("rgb({}, {}, {})", rgb >> 16, rgb >> 8 & 0xff, rgb & 0xff) } else { format!("#{rgb:06x}") };
            ui.cv.text_right(ui.f, PX, R::new(body.x, body.y, body.w, body.h), &alt, t.text_faint);
        }
        Kind::Char => {
            let s = it.text.as_deref().unwrap_or("").trim();
            ui.cv.text_in(ui.f, font::px(48), R::new(body.x, body.y - 4, body.w / 2, body.h + 4), s, t.text_hi);
            let cps: Vec<String> = s.chars().take(4).map(|c| format!("U+{:04X}", c as u32)).collect();
            ui.cv.text_right(ui.f, PX, R::new(body.x, body.y, body.w, body.h), &cps.join(" "), t.text_faint);
        }
    }
    ui.cv.pop_clip();
    // приват и секреты: мозаика, пока не навели
    if (private || it.secret) && !hov {
        mosaic(ui, body, 6);
    }
}

/// код: номера строк и подсветка; отступы — шагом пробела, остальное пропорционально (у Tiny5
/// сетка по самой широкой букве слишком редкая)
fn code(ui: &mut Ui, it: &Item, body: R) {
    let t = ui.t;
    let text = it.text.as_deref().unwrap_or("");
    let lang = clipstore::language(text).unwrap_or("code");
    let cw = ui.f.advance(PX, '0').max(4);
    let lines: Vec<&str> = text.trim_end().lines().take(CODE_LINES).collect();
    // общий отступ убирается: превью не начинается с пустоты
    let indent = lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    let gutter = cw * 2 + 10;
    let total = text.trim_end().lines().count();
    for (n, l) in lines.iter().enumerate() {
        let y = body.y + n as i32 * CODE_LH;
        ui.cv.text_right(ui.f, PX, R::new(body.x, y, gutter - 8, CODE_LH), &(n + 1).to_string(), t.text_faint);
        let l = l.get(indent.min(l.len() - l.trim_start().len())..).unwrap_or(l).replace('\t', "    ");
        let mut x = body.x + gutter;
        for (tok, s) in clipstore::highlight(&l, lang) {
            let col = match tok {
                Tok::Key => t.red,
                Tok::Str => t.ok,
                Tok::Num => t.warn,
                Tok::Comment => t.text_faint,
                Tok::Punct => t.text_dim,
                Tok::Plain => t.text,
            };
            if x > body.right() {
                break;
            }
            // пробелы — ровным шагом: отступы не плывут
            let lead = s.chars().take_while(|c| *c == ' ').count();
            x += lead as i32 * cw;
            x += ui.cv.text(ui.f, PX, x, y, &s[lead..], col);
        }
    }
    if total > CODE_LINES {
        ui.cv.text_right(ui.f, PX, R::new(body.x, body.bottom() - CODE_LH, body.w, CODE_LH), &crate::tf!("ещё {}", "{} more", total - CODE_LINES), t.text_faint);
    }
}

fn files(ui: &mut Ui, st: &State, it: &Item, body: R) {
    let t = ui.t;
    let Some(list) = st.clip.files.get(&it.id) else {
        ui.text_in(PX, body, "…", t.text_faint);
        return;
    };
    let icon = |ui: &mut Ui, f: &FileInfo, x: i32, y: i32, size: i32| {
        if let Some(img) = f.icons.iter().find_map(|n| ui.ic.get(n, size)) {
            ui.cv.image(&img, x, y, 1.0);
        } else {
            ui.cv.icon_in(if f.dir { "folder" } else { "text" }, R::new(x, y, size, size), 2, t.text_dim);
        }
    };
    let meta = |f: &FileInfo| match (f.dir, f.size) {
        (true, _) => crate::i18n::tr("папка").to_string(),
        (false, Some(s)) => crate::fileops::human(s),
        (false, None) => crate::i18n::tr("нет на диске").to_string(),
    };
    if list.len() == 1 {
        let f = &list[0];
        icon(ui, f, body.x, body.y + 1, 32);
        ui.cv.text_in(ui.f, PX, R::new(body.x + 42, body.y, body.w - 42, 18), &f.name, t.text);
        ui.text_in(PX, R::new(body.x + 42, body.y + 17, body.w - 42, 18), &meta(f), t.text_faint);
        return;
    }
    for (n, f) in list.iter().take(3).enumerate() {
        let y = body.y + n as i32 * 22;
        icon(ui, f, body.x, y + 1, 20);
        ui.cv.text_in(ui.f, PX, R::new(body.x + 28, y, body.w - 120, 22), &f.name, t.text);
        ui.text_right(PX, R::new(body.x, y, body.w, 22), &meta(f), t.text_faint);
    }
    if list.len() > 3 {
        ui.cv.text_in(ui.f, PX, R::new(body.x + 28, body.y + 66, body.w, 18), &crate::tf!("и ещё {}", "and {} more", list.len() - 3), t.text_faint);
    }
}

/// вкладка настроек «буфер»
pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let mut y = crate::settings::title(ui, b.x, b.y, b.w, "буфер обмена");
    let pos = if s.clip_pos == "center" { "окном по центру" } else { "справа во всю высоту" };
    crate::settings::opt_btn(ui, R::new(b.x, y, half, 26), "где", pos, Act::Drop("clip_pos"));
    crate::settings::num_btn(ui, R::new(b.x + half + 20, y, half, 26), "хранить записей", s.num("clip_limit"), "clip_limit");
    y += 36;
    ui.toggle(R::new(b.x, y, half, 40), "приват", "не записывать, превью мозаикой", s.clip_private, Act::SBool("clip_private"));
    let c = &st.clip;
    let armed = c.clear_armed.is_some_and(|a| Instant::now() < a);
    let label = if armed { "нажми ещё раз" } else { "очистить историю" };
    ui.button(R::new(b.x + half + 20, y + 6, half, 26), label, "trash", if armed { Tone::Danger } else { Tone::Plain }, false, !c.store.items.is_empty(), Act::ClipClear);
    y += 52;
    let pinned = c.store.items.iter().filter(|i| i.pinned).count();
    let line = crate::tf!("записей: {}, закреплено: {} — закреплённые не уходят по лимиту и при очистке", "items: {}, pinned: {} — pinned ones survive the limit and clearing", c.store.items.len(), pinned);
    ui.cv.text_wrap(ui.f, PX, R::new(b.x, y, b.w, 40), &line, t.text_dim, 2);
    y += 48;
    y = crate::settings::title(ui, b.x, y, b.w, "клавиша");
    ui.text_in(PX, R::new(b.x, y, b.w, 20), "в config.kdl niri, в binds:", t.text_dim);
    y += 26;
    let r = R::new(b.x, y, b.w, 28);
    ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
    ui.cv.text_in(ui.f, PX, R::new(r.x + 10, r.y, r.w - 20, r.h), "Mod+V { spawn \"yami-lite\" \"msg\" \"clip\"; }", t.text);
    y += 40;
    if c.wl.is_none() {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "буфер не подключён: композитор без data-control", t.warn);
    }
}

impl App {
    /// поднять модуль: история с диска, нить буфера
    pub fn clip_init(&mut self) {
        self.st.clip.store = Store::load();
        let lim = self.st.set.num("clip_limit") as usize;
        self.st.clip.store.trim(lim);
        self.st.clip.shared.private.store(self.st.set.clip_private, std::sync::atomic::Ordering::SeqCst);
        self.st.clip.wl = clipwl::spawn(self.tx.clone(), self.st.clip.shared.clone(), clipstore::blobs_dir());
    }

    pub fn clip_event(&mut self, e: Ev) {
        match e {
            Ev::Captured(p) => {
                // приват включили, пока читалось
                if self.st.set.clip_private {
                    return;
                }
                let c = &mut self.st.clip;
                c.store.add(p);
                c.store.trim(self.st.set.num("clip_limit") as usize);
                c.store.save();
                if self.st.modal == Some(Modal::Clip) {
                    self.clip_refilter(false);
                }
            }
            Ev::Cleared => {
                let _ = self.handle.insert_source(calloop::timer::Timer::from_duration(RESTORE), |_, _, app: &mut App| {
                    app.clip_restore();
                    calloop::timer::TimeoutAction::Drop
                });
            }
            Ev::Died => {
                eprintln!("clipboard: {}", crate::tf!("связь с буфером потеряна", "lost the clipboard connection"));
                self.st.clip.wl = None;
            }
        }
    }

    /// выбор всё ещё пуст: вернуть последнюю запись (секреты — никогда: менеджер паролей чистит
    /// буфер нарочно). Нить ещё раз проверит пустоту перед записью
    fn clip_restore(&mut self) {
        let c = &self.st.clip;
        if self.st.set.clip_private || !c.shared.is_empty() {
            return;
        }
        let Some(it) = c.store.items.first().filter(|i| !i.secret) else { return };
        if let (Some(p), Some(wl)) = (c.store.payload(it), c.wl.as_ref()) {
            wl.restore(p);
        }
    }

    pub fn clip_toggle(&mut self) {
        if self.st.modal == Some(Modal::Clip) {
            return self.close_modal();
        }
        self.close_all();
        if self.st.locked {
            return;
        }
        let out = self.focus_out();
        let (ow, oh) = self.output_size(&out);
        let c = &mut self.st.clip;
        c.query.clear();
        c.cur = 0;
        c.top = 0;
        c.clear_armed = None;
        self.st.modal = Some(Modal::Clip);
        if self.st.set.clip_pos == "center" {
            let (w, h) = (CENTER.0.min(ow - 40), CENTER.1.min(oh - 80));
            self.mk(SKind::Dim, &out, Layer::Overlay, "yami-dim", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
            self.mk(SKind::Modal, &out, Layer::Overlay, "yami-clip", Anchor::empty(), (w, h + 12), (0, 0, 0, 0), -1, KeyboardInteractivity::Exclusive);
        } else {
            // колонка от бара до низа; поверхность на подъём (12) выше видимого верха
            let top = (self.bar_bottom(&out) + 6 - 12).max(0);
            let bottom = self.bar_reserve_bottom(&out) + 6;
            self.st.modal_anchored = true;
            self.mk_catcher_on(&out, Layer::Overlay);
            let id = self.mk(SKind::Modal, &out, Layer::Overlay, "yami-clip", Anchor::TOP | Anchor::BOTTOM | Anchor::RIGHT, (SIDE_W, 0), (top, 6, bottom, 0), -1, KeyboardInteractivity::Exclusive);
            if let Some(s) = self.surfs.get_mut(&id) {
                s.margin = (ow - SIDE_W - 6, top);
                s.applied = s.margin;
            }
        }
        self.clip_refilter(true);
    }

    /// закрылось: миниатюры и сведения о файлах — отпустить
    pub fn clip_closed(&mut self) {
        let c = &mut self.st.clip;
        c.thumbs.clear();
        c.files.clear();
        c.pending.clear();
        c.query.clear();
        yami_px::trim_heap();
    }

    /// пересобрать список под запрос; reset — к началу, иначе карточка под клавиатурой остаётся той же
    fn clip_refilter(&mut self, reset: bool) {
        let c = &mut self.st.clip;
        let was = c.view.get(c.cur).copied();
        c.view = clipstore::filter(&c.store.items, &c.query.text).iter().map(|i| i.id).collect();
        if reset {
            c.cur = 0;
            c.top = 0;
        } else {
            c.cur = was.and_then(|w| c.view.iter().position(|x| *x == w)).unwrap_or(0);
            c.cur = c.cur.min(c.view.len().saturating_sub(1));
            c.top = c.top.min(c.cur);
        }
        self.clip_show();
    }

    /// cur — в видимой части; миниатюры и файлы видимых — заказать
    fn clip_show(&mut self) {
        let c = &mut self.st.clip;
        if c.cur < c.top {
            c.top = c.cur;
        } else {
            // снизу: сколько карточек над cur влезает вместе с ней
            let lh = c.list_h.get().max(200);
            let mut h = 0;
            let mut first = c.cur;
            for i in (c.top..=c.cur).rev() {
                let Some(it) = c.view.get(i).and_then(|id| c.store.get(*id)) else { break };
                h += card_h(it) + 8;
                if h - 8 > lh && i < c.cur {
                    break;
                }
                first = i;
            }
            c.top = c.top.max(first);
        }
        self.clip_want();
        self.dirty_panels();
    }

    /// миниатюры картинок и сведения о файлах для видимых (и пары следующих) — из нитей
    fn clip_want(&mut self) {
        let c = &mut self.st.clip;
        // до первого кадра shown ещё не знает, сколько влезет: с запасом
        let range = c.top..(c.top + c.shown.get().max(8) + 4).min(c.view.len());
        let w = (c.width.get().max(200) - 20).max(100);
        let mut imgs: Vec<(String, PathBuf)> = vec![];
        let mut fls: Vec<(u64, String)> = vec![];
        for id in &c.view[range] {
            let Some(it) = c.store.get(*id) else { continue };
            match (it.kind, &it.blob) {
                (Kind::Image, Some(b)) if !c.thumbs.contains_key(b) && !c.pending.contains(b) => {
                    c.pending.insert(b.clone());
                    imgs.push((b.clone(), c.store.blob_path(b)));
                }
                (Kind::Files, _) if !c.files.contains_key(id) && !c.pending.contains(&id.to_string()) => {
                    c.pending.insert(id.to_string());
                    fls.push((*id, it.text.clone().unwrap_or_default()));
                }
                _ => {}
            }
        }
        if !imgs.is_empty() {
            let tx = self.tx.clone();
            let _ = std::thread::Builder::new().name("clip-thumbs".into()).spawn(move || {
                for (b, p) in imgs {
                    let img = std::fs::read(&p).ok().and_then(|d| yami_px::icons::decode(&d, 256 << 20)).map(|i| yami_px::icons::fit_in(i.to_rgba8(), w, IMG_H));
                    if tx.send(Msg::ClipThumb(b, img)).is_err() {
                        return;
                    }
                }
                yami_px::trim_heap();
            });
        }
        if !fls.is_empty() {
            let tx = self.tx.clone();
            let _ = std::thread::Builder::new().name("clip-files".into()).spawn(move || {
                static MIME: std::sync::OnceLock<crate::fileops::Mime> = std::sync::OnceLock::new();
                let mime = MIME.get_or_init(crate::fileops::Mime::load);
                for (id, text) in fls {
                    let v = crate::fileops::parse_uri_list(&text)
                        .into_iter()
                        .map(|p| {
                            let md = std::fs::metadata(&p).ok();
                            let dir = md.as_ref().is_some_and(|m| m.is_dir());
                            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string());
                            FileInfo { icons: mime.icons(&mime.of(&name, dir)), size: md.filter(|m| m.is_file()).map(|m| m.len()), dir, name }
                        })
                        .collect();
                    if tx.send(Msg::ClipFiles(id, v)).is_err() {
                        return;
                    }
                }
            });
        }
    }

    pub fn clip_thumb(&mut self, blob: String, img: Option<Img>) {
        let c = &mut self.st.clip;
        c.pending.remove(&blob);
        if self.st.modal == Some(Modal::Clip) {
            c.thumbs.insert(blob, img.map(Rc::new));
            self.dirty_panels();
        }
    }

    pub fn clip_files(&mut self, id: u64, v: Vec<FileInfo>) {
        let c = &mut self.st.clip;
        c.pending.remove(&id.to_string());
        if self.st.modal == Some(Modal::Clip) {
            c.files.insert(id, v);
            self.dirty_panels();
        }
    }

    /// копировать запись в буфер и закрыть
    pub fn clip_pick(&mut self, id: u64) {
        let c = &mut self.st.clip;
        let Some(p) = c.store.get(id).and_then(|it| c.store.payload(it)) else { return };
        if let Some(wl) = &c.wl {
            wl.set(p);
        }
        c.store.touch(id);
        c.store.save();
        self.close_modal();
    }

    pub fn clip_pin(&mut self, id: u64) {
        let c = &mut self.st.clip;
        let on = c.store.get(id).is_some_and(|i| !i.pinned);
        c.store.set_pinned(id, on);
        c.store.save();
        self.dirty_panels();
    }

    /// удалить; закреплённую — сначала открепить
    pub fn clip_del(&mut self, id: u64) {
        let c = &mut self.st.clip;
        if c.store.get(id).is_none_or(|i| i.pinned) {
            return;
        }
        c.store.delete(id);
        c.store.save();
        self.clip_refilter(false);
    }

    /// первое нажатие — взвести, второе за 3 с — убрать всё незакреплённое
    pub fn clip_clear(&mut self) {
        let c = &mut self.st.clip;
        if c.clear_armed.is_some_and(|a| Instant::now() < a) {
            c.clear_armed = None;
            c.store.clear(false);
            c.store.save();
            if self.st.modal == Some(Modal::Clip) {
                self.clip_refilter(true);
            }
        } else {
            c.clear_armed = Some(Instant::now() + Duration::from_secs(3));
            // погаснуть самой, если второго нажатия не будет
            let _ = self.handle.insert_source(calloop::timer::Timer::from_duration(Duration::from_millis(3050)), |_, _, app: &mut App| {
                app.dirty_panels();
                calloop::timer::TimeoutAction::Drop
            });
        }
        self.dirty_panels();
    }

    /// приват: вкл, выкл или переключить (None)
    pub fn clip_private(&mut self, on: Option<bool>) {
        let v = on.unwrap_or(!self.st.set.clip_private);
        self.st.set.clip_private = v;
        self.clip_sync_private();
        self.settings_changed();
    }

    /// настройки сменили приват: нить буфера узнаёт сразу
    pub fn clip_sync_private(&mut self) {
        self.st.clip.shared.private.store(self.st.set.clip_private, std::sync::atomic::Ordering::SeqCst);
        self.dirty_panels();
    }

    pub fn clip_scroll(&mut self, d: i32) {
        let c = &mut self.st.clip;
        let max = c.view.len().saturating_sub(c.shown.get());
        c.top = (c.top as i32 + d).clamp(0, max as i32) as usize;
        c.cur = c.cur.clamp(c.top, (c.top + c.shown.get()).saturating_sub(1).min(c.view.len().saturating_sub(1)));
        self.clip_want();
        self.dirty_panels();
    }

    pub fn clip_key(&mut self, e: &KeyEvent) {
        let n = self.st.clip.view.len();
        let cur = self.st.clip.cur;
        let id = self.st.clip.view.get(cur).copied();
        let page = self.st.clip.shown.get().max(1);
        let to = match e.keysym {
            Keysym::Escape => {
                // первый Esc чистит запрос, второй закрывает
                if self.st.clip.query.text.is_empty() {
                    return self.close_modal();
                }
                self.st.clip.query.clear();
                return self.clip_refilter(true);
            }
            Keysym::Return | Keysym::KP_Enter => return id.map_or((), |i| self.clip_pick(i)),
            Keysym::Delete => return id.map_or((), |i| self.clip_del(i)),
            Keysym::Down | Keysym::Tab => cur + 1,
            Keysym::Up | Keysym::ISO_Left_Tab => cur.saturating_sub(1),
            Keysym::Page_Down => cur + page,
            Keysym::Page_Up => cur.saturating_sub(page),
            Keysym::Home if self.st.clip.query.text.is_empty() => 0,
            Keysym::End if self.st.clip.query.text.is_empty() => n,
            _ => {
                if self.mods.ctrl && crate::keys::latin_letter(e) == Some('S') {
                    return id.map_or((), |i| self.clip_pin(i));
                }
                let before = self.st.clip.query.text.clone();
                let mods = self.mods;
                if App::edit_field(&mut self.st.clip.query, e, &mods) && self.st.clip.query.text != before {
                    self.clip_refilter(true);
                }
                return self.dirty_panels();
            }
        };
        self.st.clip.cur = to.min(n.saturating_sub(1));
        self.clip_show();
    }
}
