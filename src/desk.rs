// Рабочий стол: файлы и папки из ~/Desktop (XDG_DESKTOP_DIR) значками на обоях — порт плагина
// Noctalia-desktop (wioletowa/Noctalia-desktop) на раст. Одна поверхность на слое обоев, над ними и
// под виджетами; ловит мышь целиком (рамка и меню по пустому месту), клавиатуру — по щелчку.
// Сетка: клетка по размеру значка (ширина значок+48, высота значок+22+строки подписи), отступ 16 от
// краёв между барами. Места — в ~/.local/state/yami-lite/desktop.json по папкам; сначала стоят
// сохранённые, остальные — в свободные клетки по столбцам (мимо виджетов) и сразу запоминаются.
// Нажатие выделяет (Ctrl — добавить/убрать, Shift — прямоугольник от опорного), отпускание по
// одному из нескольких — только его; двойной щелчок открывает. Тянут значок — перетаскивание
// Wayland (dnd.rs): группа под курсором, клетки, куда встанут, подсвечены; рамка — по пустому месту.

use crate::app::{App, Msg, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, C, R};
use crate::menu::{MAct, MItem, MenuState};
use crate::ui::{Act, Cur, Field, Tone, Ui};
use calloop::channel::Sender;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// второй щелчок по тому же значку за это время — двойной
const DOUBLE: Duration = Duration::from_millis(400);
/// отступ сетки от краёв (между барами)
const MARGIN: i32 = 16;
/// строка подписи
const LINE: i32 = 18;
/// «открыть в»: столько программ
const OPEN_WITH_LIMIT: usize = 10;
/// выделение проявляется быстро, гаснет медленнее (как у плагина: 70 и 220 мс)
const FADE_IN: f32 = 0.07;
const FADE_OUT: f32 = 0.22;
/// значок, когда своего у типа в теме нет
pub const FALLBACK: [&str; 5] = ["text-x-generic", "application-x-generic", "unknown", "empty", "gtk-file"];

#[derive(Clone, Debug, Default)]
pub struct Item {
    /// имя в папке: ключ мест и выделения
    pub name: String,
    pub path: PathBuf,
    pub dir: bool,
    pub link: bool,
    pub mime: String,
    /// подпись: имя, у ярлыка .desktop — имя программы
    pub label: String,
    /// имена значка в теме по порядку (у ярлыка первым — его Icon)
    pub icons: Vec<String>,
    /// ярлык .desktop: что запускать и его действия (имя, команда)
    pub exec: Option<String>,
    pub actions: Vec<(String, String)>,
    pub mtime: i64,
    pub size: u64,
}

impl Item {
    pub fn launcher(&self) -> bool {
        self.exec.is_some()
    }
    fn image(&self) -> bool {
        self.mime.starts_with("image/") && self.size <= if self.mime == "image/svg+xml" { 2 << 20 } else { 64 << 20 }
    }
    fn video(&self) -> bool {
        self.mime.starts_with("video/")
    }
    fn sort_key(&self, mode: &str) -> (bool, String, i64, String) {
        let name = self.label.to_lowercase();
        match mode {
            "type" => (!self.dir, self.mime.clone(), 0, name),
            "size" => (!self.dir, String::new(), -(self.size as i64), name),
            "modified" => (false, String::new(), -self.mtime, name),
            _ => (!self.dir, String::new(), 0, name),
        }
    }
}

/// тянут свои значки (names) или чужие файлы над столом (names пуст): за какой взялись, куда бросят
#[derive(Clone, Debug, Default)]
pub struct Drag {
    pub names: Vec<String>,
    pub anchor: String,
    pub drop_cell: Option<(i32, i32)>,
    pub drop_tile: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Grid {
    pub x0: i32,
    pub y0: i32,
    pub cw: i32,
    pub ch: i32,
    pub cols: i32,
    pub rows: i32,
}

impl Grid {
    /// сетка в прямоугольнике area (монитор между барами) для значка size и lines строк подписи
    pub fn new(area: R, size: i32, lines: i32) -> Grid {
        let cw = (size + 48).max(100);
        let ch = size + 22 + lines.max(1) * LINE;
        let cols = ((area.w - 2 * MARGIN) / cw).max(1);
        let rows = ((area.h - 2 * MARGIN) / ch).max(1);
        Grid { x0: area.x + MARGIN, y0: area.y + MARGIN, cw, ch, cols, rows }
    }

    pub fn origin(&self, c: (i32, i32)) -> (i32, i32) {
        (self.x0 + c.0 * self.cw, self.y0 + c.1 * self.ch)
    }

    pub fn rect(&self, c: (i32, i32)) -> R {
        let (x, y) = self.origin(c);
        R::new(x, y, self.cw, self.ch)
    }

    /// клетка под точкой (за краями — крайняя)
    pub fn at(&self, x: i32, y: i32) -> (i32, i32) {
        (((x - self.x0).div_euclid(self.cw)).clamp(0, self.cols - 1), ((y - self.y0).div_euclid(self.ch)).clamp(0, self.rows - 1))
    }

    /// клетки по столбцам: сверху вниз, слева направо
    fn order(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        (0..self.cols).flat_map(move |c| (0..self.rows).map(move |r| (c, r)))
    }

    fn inside(&self, c: (i32, i32)) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.cols && c.1 < self.rows
    }
}

/// клетка под виджетом — новым значкам не годится
fn blocked_cell(g: &Grid, c: (i32, i32), blocked: &[R]) -> bool {
    let rr = g.rect(c);
    blocked.iter().any(|b| {
        let i = b.inter(&rr);
        i.w > 0 && i.h > 0
    })
}

/// ближайшая свободная клетка к cell (по расстоянию, при равенстве — раньше по столбцам)
pub fn nearest_free(g: &Grid, cell: (i32, i32), occupied: &[(i32, i32)], blocked: &[R]) -> (i32, i32) {
    let mut best: Option<((i32, i32), i32)> = None;
    for c in g.order() {
        if occupied.contains(&c) || blocked_cell(g, c, blocked) {
            continue;
        }
        let d = (c.0 - cell.0).pow(2) + (c.1 - cell.1).pow(2);
        if best.is_none_or(|b| d < b.1) {
            best = Some((c, d));
        }
    }
    best.map(|b| b.0).unwrap_or((cell.0.clamp(0, g.cols - 1), cell.1.clamp(0, g.rows - 1)))
}

/// места всех значков: сохранённые (cells) — первыми, остальные по имени — в свободные клетки по
/// столбцам; новые места дописываются в cells
pub fn layout(items: &[Item], cells: &mut HashMap<String, (i32, i32)>, g: &Grid, blocked: &[R]) -> HashMap<String, (i32, i32)> {
    let mut placement = HashMap::new();
    let mut occupied: Vec<(i32, i32)> = vec![];
    let mut names: Vec<&Item> = items.iter().collect();
    names.sort_by(|a, b| a.name.cmp(&b.name));
    for it in &names {
        if let Some(&c) = cells.get(&it.name) {
            if g.inside(c) && !occupied.contains(&c) {
                placement.insert(it.name.clone(), c);
                occupied.push(c);
            }
        }
    }
    let mut pending: Vec<&Item> = names.into_iter().filter(|i| !placement.contains_key(&i.name)).collect();
    pending.sort_by(|a, b| a.sort_key("name").cmp(&b.sort_key("name")));
    let last = (g.cols - 1, g.rows - 1);
    for it in pending {
        let free = g.order().find(|c| !occupied.contains(c) && !blocked_cell(g, *c, blocked));
        let c = free.unwrap_or(last);
        occupied.push(c);
        placement.insert(it.name.clone(), c);
        // запомнить: следующие новые не перетасуют этот
        if free.is_some() {
            cells.insert(it.name.clone(), c);
        }
    }
    placement
}

/// куда встанет группа, если anchor бросить в drop: сдвиг тот же, занятое — ближайшая свободная
pub fn plan_move(placement: &HashMap<String, (i32, i32)>, names: &[String], anchor: &str, drop: (i32, i32), g: &Grid) -> Vec<(String, (i32, i32))> {
    let start = placement.get(anchor).copied().unwrap_or(drop);
    let (dc, dr) = (drop.0 - start.0, drop.1 - start.1);
    let mut occupied: Vec<(i32, i32)> = placement.iter().filter(|(n, _)| !names.contains(n)).map(|(_, c)| *c).collect();
    let mut rest: Vec<&String> = names.iter().filter(|n| n.as_str() != anchor && placement.contains_key(*n)).collect();
    rest.sort_by_key(|n| {
        let c = placement[*n];
        (c.0 - start.0).abs() + (c.1 - start.1).abs()
    });
    let anchor_s = anchor.to_string();
    let mut plan = vec![];
    for n in std::iter::once(&anchor_s).chain(rest) {
        let Some(&(c, r)) = placement.get(n) else { continue };
        let mut t = ((c + dc).clamp(0, g.cols - 1), (r + dr).clamp(0, g.rows - 1));
        if occupied.contains(&t) {
            t = nearest_free(g, t, &occupied, &[]);
        }
        occupied.push(t);
        plan.push((n.clone(), t));
    }
    plan
}

// ── места на диске: {папка: {имя: [столбец, строка]}} ───────────────────────
type Places = HashMap<String, HashMap<String, (i32, i32)>>;

fn places_path() -> PathBuf {
    yami_px::paths::state_dir().join("desktop.json")
}

fn load_places() -> Places {
    let t = std::fs::read_to_string(places_path()).unwrap_or_default();
    serde_json::from_str(&t).unwrap_or_default()
}

fn save_places(p: &Places) {
    if let Ok(t) = serde_json::to_string(p) {
        let _ = yami_px::paths::write_atomic(&places_path(), &t);
    }
}

pub fn desktop_dir() -> PathBuf {
    yami_px::paths::xdg_dir("DESKTOP").unwrap_or_else(|| yami_px::paths::home().join("Desktop"))
}

#[derive(Default)]
pub struct Desk {
    pub dir: PathBuf,
    pub items: Vec<Item>,
    pub loaded: bool,
    /// сохранённые места всех папок; этой — places[dir]
    places: Places,
    /// где стоит каждый значок сейчас
    pub placement: HashMap<String, (i32, i32)>,
    pub sel: Vec<String>,
    /// опорный: от него Shift выделяет прямоугольник, от него ходят стрелки
    pub anchor: Option<String>,
    /// рамка: угол нажатия и текущий; выделенное до неё (Ctrl/Shift — добавить к нему)
    pub band: Option<(i32, i32, i32, i32)>,
    band_base: Vec<String>,
    /// рамка: когда появилась; отпущенная — гаснет (где была и когда отпустили)
    band_at: Option<Instant>,
    band_fade: Option<((i32, i32, i32, i32), Instant)>,
    /// плашки выделения: имя → (выделен ли, с какой яркости и когда сменилось)
    fades: std::cell::RefCell<HashMap<String, (bool, f32, Instant)>>,
    pub drag: Option<Drag>,
    /// нажали на один из выделенных: отпустили на нём же — только он
    press: Option<String>,
    /// прошлое нажатие: чьё, когда и где — второе рядом и вскоре — двойной щелчок
    last_click: Option<(String, Instant, i32, i32)>,
    /// двойной щелчок открывает при отпускании, а не при нажатии: потянули — это перетаскивание
    open_on_release: bool,
    pub rename: Option<String>,
    pub field: Field,
    rename_next: Option<String>,
    /// вырезанные (бледные) — пути
    pub cut: Vec<PathBuf>,
    /// миниатюры: ключ (путь, время, размер, размер значка) → картинка (None — не вышло)
    pub thumbs: HashMap<String, Option<Rc<Img>>>,
    thumbs_asked: std::collections::HashSet<String>,
    pub grid: Grid,
    /// прямоугольники виджетов: новые значки туда не встают
    pub blocked: Vec<R>,
    /// окно «свойства»: что показывать и посчитанный размер
    pub info: Vec<Item>,
    pub info_size: String,
}

impl Desk {
    fn cells(&mut self) -> &mut HashMap<String, (i32, i32)> {
        let k = self.dir.display().to_string();
        self.places.entry(k).or_default()
    }

    pub fn item(&self, name: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.name == name)
    }

    pub fn selected(&self) -> Vec<Item> {
        self.items.iter().filter(|i| self.sel.contains(&i.name)).cloned().collect()
    }
}

/// ярлык .desktop: имя, значок, команда и действия
fn read_launcher(it: &mut Item) {
    let Ok(t) = std::fs::read_to_string(&it.path) else { return };
    let ini = yami_px::icons::parse_ini(&t);
    let get = |m: &HashMap<String, String>, k: &str| if crate::i18n::en() { None } else { m.get(&format!("{k}[ru]")) }.or_else(|| m.get(k)).cloned();
    let Some((_, e)) = ini.iter().find(|(s, _)| s == "Desktop Entry") else { return };
    if let Some(n) = get(e, "Name") {
        it.label = n;
    }
    if let Some(i) = e.get("Icon") {
        it.icons.insert(0, i.clone());
    }
    it.exec = e.get("Exec").cloned();
    let acts: Vec<String> = e.get("Actions").map(|s| s.split(';').filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default();
    for a in acts {
        if let Some((_, s)) = ini.iter().find(|(n, _)| *n == format!("Desktop Action {a}")) {
            if let (Some(n), Some(x)) = (get(s, "Name"), s.get("Exec")) {
                it.actions.push((n, x.clone()));
            }
        }
    }
}

/// список папки: скрытые (с точки и «~» в конце) — по настройке
fn list(dir: &Path, hidden: bool, mime: &crate::fileops::Mime) -> Vec<Item> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !hidden && (name.starts_with('.') || name.ends_with('~')) {
            continue;
        }
        let path = e.path();
        let link = e.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        let Ok(md) = std::fs::metadata(&path).or_else(|_| std::fs::symlink_metadata(&path)) else { continue };
        let dir = md.is_dir();
        let m = mime.of(&name, dir);
        let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        let mut it = Item { label: name.clone(), icons: mime.icons(&m), name, path, dir, link, mime: m, mtime, size: md.len(), ..Default::default() };
        if it.name.ends_with(".desktop") && !dir {
            read_launcher(&mut it);
        }
        out.push(it);
    }
    out.sort_by(|a, b| a.sort_key("name").cmp(&b.sort_key("name")));
    out
}

/// какая нить смотрит папку: прежние уходят при первом же событии
static GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// события inotify из буфера: (маска, cookie, имя)
fn events(buf: &[u8]) -> Vec<(u32, u32, String)> {
    let mut out = vec![];
    let mut i = 0;
    let head = std::mem::size_of::<libc::inotify_event>();
    while i + head <= buf.len() {
        // SAFETY: в буфере не меньше head байт с i; read_unaligned не требует выравнивания
        let ev: libc::inotify_event = unsafe { std::ptr::read_unaligned(buf[i..].as_ptr() as *const libc::inotify_event) };
        let len = ev.len as usize;
        let name = buf.get(i + head..i + head + len).map(|b| String::from_utf8_lossy(b).trim_end_matches('\0').to_string()).unwrap_or_default();
        out.push((ev.mask, ev.cookie, name));
        i += head + len;
    }
    out
}

/// нить: список сразу и после каждой перемены в папке (пачки — раз в 150 мс); переименования
/// (пары MOVED_FROM/MOVED_TO с одним cookie) — отдельно: место значка уходит за новым именем
pub fn spawn(dir: PathBuf, hidden: bool, tx: Sender<Msg>) {
    use std::sync::atomic::Ordering;
    let me = GEN.fetch_add(1, Ordering::SeqCst) + 1;
    std::thread::Builder::new()
        .name("desk".into())
        .spawn(move || {
            let _ = std::fs::create_dir_all(&dir);
            let mime = crate::fileops::Mime::load();
            let alive = || GEN.load(Ordering::SeqCst) == me;
            let send = |tx: &Sender<Msg>| alive() && tx.send(Msg::Desk(dir.clone(), list(&dir, hidden, &mime))).is_ok();
            if !send(&tx) {
                return;
            }
            // SAFETY: inotify — обычный fd этой нити, читается и закрывается только здесь
            let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if fd < 0 {
                return;
            }
            let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()) else { return };
            let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_CLOSE_WRITE | libc::IN_ATTRIB | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF;
            if unsafe { libc::inotify_add_watch(fd, c.as_ptr(), mask) } < 0 {
                unsafe { libc::close(fd) };
                return;
            }
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                if n <= 0 {
                    break;
                }
                let mut evs = events(&buf[..n as usize]);
                std::thread::sleep(Duration::from_millis(150));
                let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
                while unsafe { libc::poll(&mut pfd, 1, 0) } > 0 {
                    let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                    if n <= 0 {
                        break;
                    }
                    evs.extend(events(&buf[..n as usize]));
                }
                let from: Vec<&(u32, u32, String)> = evs.iter().filter(|e| e.0 & libc::IN_MOVED_FROM != 0).collect();
                let pairs: Vec<(String, String)> = evs.iter().filter(|e| e.0 & libc::IN_MOVED_TO != 0).filter_map(|t| from.iter().find(|f| f.1 == t.1).map(|f| (f.2.clone(), t.2.clone()))).collect();
                if !pairs.is_empty() && (!alive() || tx.send(Msg::DeskRenamed(dir.clone(), pairs)).is_err()) {
                    break;
                }
                if !send(&tx) {
                    break;
                }
            }
            unsafe { libc::close(fd) };
        })
        .ok();
}

/// размер значка по настройке
pub fn icon_size(st: &State) -> i32 {
    match st.set.desk_size {
        48 | 56 | 64 | 96 => st.set.desk_size,
        _ => 56,
    }
}

fn lines(st: &State) -> i32 {
    st.set.desk_lines.clamp(1, 3)
}

fn thumb_key(it: &Item, size: i32) -> String {
    format!("{}\t{}\t{}\t{}", it.path.display(), it.mtime, it.size, size)
}

/// подпись: строки по ширине; одна выделенная — целиком, остальные — в lines строк
fn label_lines(ui: &mut Ui, it: &Item, w: i32, expanded: bool, lines: i32) -> Vec<String> {
    wrap_label(&it.label, w, if expanded { 8 } else { lines as usize }, &mut |s| ui.w(PX, s))
}

/// перенос имени файла: рвать после пробела, точки, «-» и «_» (а не посреди слова: «st5icker.» +
/// «webp», а не «st5icker.web» + «p»); слово длиннее строки — по буквам; не влезло — «…» в конце
pub fn wrap_label(s: &str, max: i32, lines: usize, width: &mut dyn FnMut(&str) -> i32) -> Vec<String> {
    let mut tokens: Vec<String> = vec![];
    let mut cur = String::new();
    for ch in s.chars() {
        cur.push(ch);
        if matches!(ch, ' ' | '.' | '-' | '_') {
            tokens.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    let mut out: Vec<String> = vec![];
    let mut line = String::new();
    for tok in tokens {
        let joined = format!("{line}{tok}");
        if width(joined.trim_end()) <= max {
            line = joined;
            continue;
        }
        if !line.is_empty() {
            out.push(std::mem::take(&mut line).trim_end().to_string());
        }
        if width(tok.trim_end()) <= max {
            line = tok;
            continue;
        }
        // кусок длиннее строки — по буквам
        for ch in tok.chars() {
            let next = format!("{line}{ch}");
            if width(&next) > max && !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            line.push(ch);
        }
    }
    if !line.trim().is_empty() {
        out.push(line.trim_end().to_string());
    }
    if out.len() > lines {
        out.truncate(lines);
        let mut last = out.pop().unwrap_or_default().trim_end_matches([' ', '.', '-', '_']).to_string();
        while !last.is_empty() && width(&format!("{last}…")) > max {
            last.pop();
        }
        out.push(format!("{last}…"));
    }
    out
}

/// прямоугольник значка в клетке: высота клетки, у раскрытой подписи — по ней
fn tile_rect(g: &Grid, cell: (i32, i32), size: i32, nlines: i32, expanded: bool) -> R {
    let (x, y) = g.origin(cell);
    let natural = 6 + size + 4 + nlines * LINE + 6;
    let h = if expanded { natural.max(g.ch - 4) } else { g.ch - 4 };
    R::new(x + 4, y + 2, g.cw - 8, h)
}

/// подпись на обоях: белая с тенью — читается на любой картинке
fn shadowed(ui: &mut Ui, r: R, s: &str, c: C) {
    let sh = C::hex(0x000000);
    ui.text_center(PX, R::new(r.x + 1, r.y + 1, r.w, r.h), s, sh.alpha(0.9));
    ui.text_center(PX, R::new(r.x, r.y + 1, r.w, r.h), s, sh.alpha(0.5));
    ui.text_center(PX, r, s, c);
}

/// картинка значка: миниатюра или значок типа из темы
pub fn tile_img(ui: &mut Ui, st: &State, it: &Item, size: i32) -> (Option<Rc<Img>>, bool) {
    let thumb = if st.set.desk_thumbs { st.desk.thumbs.get(&thumb_key(it, size)).cloned().flatten() } else { None };
    let is_thumb = thumb.is_some();
    (thumb.or_else(|| it.icons.iter().map(String::as_str).chain(FALLBACK).find_map(|n| ui.ic.get(n, size))), is_thumb)
}

/// один значок: плашка (наведение, выделение, цель броска), картинка, подпись
#[allow(clippy::too_many_arguments)]
pub fn draw_tile(ui: &mut Ui, st: &State, it: &Item, r: R, sel: f32, expanded: bool, target: bool, hover: bool) {
    let t = ui.t;
    let size = icon_size(st);
    if target {
        ui.cv.pxbox(r, &BoxStyle::new(t.red.alpha(0.45), t.red));
    } else if sel > 0.01 {
        // плашка выделения — с яркостью проявления
        if hover && sel < 0.99 {
            ui.cv.fill(r, C::hex(0xffffff).alpha(0.10 * (1.0 - sel)));
        }
        ui.cv.pxbox(r, &BoxStyle::new(t.red.alpha(0.32 * sel), t.red.alpha(0.75 * sel)));
    } else if hover {
        ui.cv.fill(r, C::hex(0xffffff).alpha(0.10));
    }
    let (ix, iy) = (r.x + (r.w - size) / 2, r.y + 6);
    let (img, thumb) = tile_img(ui, st, it, size);
    if let Some(img) = img {
        let ir = R::new(ix + (size - img.w) / 2, iy + (size - img.h) / 2, img.w, img.h);
        ui.cv.fill(R::new(ir.x + 2, ir.bottom(), ir.w - 4, 1), C::hex(0x000000).alpha(0.25));
        ui.cv.image_scaled(&img, ir, 1.0);
        if thumb {
            ui.cv.outline(ir, C::hex(0x000000).alpha(0.35), 1);
        }
        if it.link {
            ui.cv.icon("right", ir.right() - 9, ir.bottom() - 9, 1, t.text_hi);
        }
    }
    let ty = iy + size + 4;
    if st.desk.rename.as_deref() == Some(&it.name) {
        ui.field(R::new(r.x - 2, ty - 4, r.w + 4, 28), &st.desk.field, "", true, false, Act::Block);
        return;
    }
    let ls = label_lines(ui, it, r.w - 8, expanded, lines(st));
    for (k, l) in ls.iter().enumerate() {
        shadowed(ui, R::new(r.x + 4, ty + k as i32 * LINE, r.w - 8, LINE), l, C::hex(0xffffff));
    }
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let d = &st.desk;
    // фон — весь стол: щелчок мимо значков, рамка, меню по пустому месту
    ui.zone(R::new(0, 0, w, h), Act::DeskBg, Cur::Default);
    if !d.loaded {
        return;
    }
    let t = ui.t;
    let g = d.grid;
    let size = icon_size(st);
    let one = d.sel.len() == 1;
    // куда встанут тянущиеся (или чужие файлы): клетки акцентом
    let mut drop_cells: Vec<(i32, i32)> = vec![];
    if let Some(dr) = &d.drag {
        if let Some(c) = dr.drop_cell {
            if dr.names.is_empty() {
                drop_cells.push(c);
            } else {
                drop_cells = plan_move(&d.placement, &dr.names, &dr.anchor, c, &g).into_iter().map(|p| p.1).collect();
            }
        }
    }
    for c in &drop_cells {
        let r = g.rect(*c);
        ui.cv.pxbox(R::new(r.x + 4, r.y + 2, r.w - 8, r.h - 4), &BoxStyle::new(t.red.alpha(0.18), t.red.alpha(0.7)));
    }
    let base = if st.editing { 0.35 } else { 1.0 };
    let now = Instant::now();
    let mut fades = d.fades.borrow_mut();
    let mut moving = false;
    // яркость плашки: от прежней к цели за FADE_IN (появление) или FADE_OUT (исчезание)
    let mut fade = |name: &str, on: bool| -> f32 {
        let e = fades.entry(name.to_string()).or_insert((on, if on { 1.0 } else { 0.0 }, now - Duration::from_secs(1)));
        let cur = |e: &(bool, f32, Instant)| {
            let dur = if e.0 { FADE_IN } else { FADE_OUT };
            let k = (now.duration_since(e.2).as_secs_f32() / dur).min(1.0);
            let k = 1.0 - (1.0 - k) * (1.0 - k);
            e.1 + ((if e.0 { 1.0 } else { 0.0 }) - e.1) * k
        };
        if e.0 != on {
            *e = (on, cur(e), now);
        }
        let a = cur(e);
        if a > 0.001 && a < 0.999 {
            moving = true;
        }
        a
    };
    // выделенные — поверх: раскрытая подпись не прячется под соседом
    let mut order: Vec<&Item> = d.items.iter().collect();
    order.sort_by_key(|i| d.sel.contains(&i.name));
    let mut hit: Vec<(R, String)> = vec![];
    for it in order {
        let Some(&cell) = d.placement.get(&it.name) else { continue };
        let sel = d.sel.contains(&it.name);
        let expanded = sel && one;
        let nl = if expanded { label_lines(ui, it, g.cw - 16, true, lines(st)).len() as i32 } else { lines(st) };
        let r = tile_rect(&g, cell, size, nl, expanded);
        let cut = d.cut.contains(&it.path) || d.drag.as_ref().is_some_and(|dr| dr.names.contains(&it.name));
        let target = d.drag.as_ref().and_then(|dr| dr.drop_tile.as_deref()) == Some(&it.name);
        let hov = ui.hover(r) && d.drag.is_none() && d.band.is_none() && !st.editing;
        ui.cv.opacity = if cut { base * 0.5 } else { base };
        let k = fade(&it.name, sel);
        draw_tile(ui, st, it, r, k, expanded, target, hov);
        hit.push((r, it.name.clone()));
    }
    ui.cv.opacity = 1.0;
    if !st.editing {
        for (r, n) in hit {
            ui.zone(r, Act::DeskItem(n), Cur::Default);
        }
    }
    fades.retain(|n, _| d.items.iter().any(|i| &i.name == n));
    drop(fades);
    // рамка: проявляется, отпущенная — гаснет
    let band = match (d.band, d.band_fade) {
        (Some(b), _) => Some((b, d.band_at.map_or(1.0, |t| (now.duration_since(t).as_secs_f32() / FADE_IN).min(1.0)))),
        (None, Some((b, t))) => Some((b, 1.0 - (now.duration_since(t).as_secs_f32() / FADE_OUT).min(1.0))),
        _ => None,
    };
    if let Some(((x0, y0, x1, y1), a)) = band {
        if a < 1.0 {
            moving = true;
        }
        let a = a * a;
        let r = R::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
        ui.cv.fill(r, t.red.alpha(0.18 * a));
        ui.cv.outline(r, t.red.alpha(0.85 * a), 1);
    }
    // пока что-то проявляется или гаснет — следующий кадр
    if moving {
        ui.anim = true;
    }
}

impl App {
    /// поверхность стола: есть, если включён (на мониторе с фокусом); пересчёт сетки
    pub fn sync_desk(&mut self) {
        let have = self.find(&SKind::Desk);
        if !self.st.set.desk || !crate::features::DESKTOP {
            for id in have {
                self.destroy(id);
            }
            GEN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.st.desk = Desk::default();
            return;
        }
        let out = self.focus_out();
        if out.is_empty() {
            return;
        }
        if have.is_empty() {
            use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
            self.mk(SKind::Desk, &out, Layer::Background, "yami-desk", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::OnDemand);
            self.desk_watch();
        }
        self.desk_relayout();
        // размер значка сменился или миниатюры включили: ключ у миниатюры с размером — новые
        self.desk_thumbs();
    }

    /// смотреть папку заново (F5, «обновить», скрытые файлы)
    pub fn desk_watch(&mut self) {
        let dir = desktop_dir();
        self.st.desk.dir = dir.clone();
        if self.st.desk.places.is_empty() {
            self.st.desk.places = load_places();
        }
        spawn(dir, self.st.set.desk_hidden, self.tx.clone());
    }

    /// сетка под монитор, бары, размер; места — заново по ней
    pub fn desk_relayout(&mut self) {
        let Some(out) = self.surfs.values().find(|s| s.kind == SKind::Desk).map(|s| s.output.clone()) else { return };
        let (ow, oh) = self.output_size(&out);
        let top = self.bar_bottom(&out);
        let bottom = oh - self.bar_reserve_bottom(&out);
        self.st.desk.grid = Grid::new(R::new(0, top, ow, bottom - top), icon_size(&self.st), lines(&self.st));
        self.st.desk.blocked = self.surfs.values().filter(|s| matches!(s.kind, SKind::Widget(_)) && !s.closing && s.output == out).map(|s| R::new(s.margin.0, s.margin.1, s.w, s.h)).collect();
        let d = &mut self.st.desk;
        let (items, g, blocked) = (d.items.clone(), d.grid, d.blocked.clone());
        let before = d.cells().clone();
        let placement = layout(&items, d.cells(), &g, &blocked);
        d.placement = placement;
        if *d.cells() != before {
            save_places(&d.places);
        }
        self.dirty(&SKind::Desk);
    }

    pub fn desk_got(&mut self, dir: PathBuf, items: Vec<Item>) {
        if dir != self.st.desk.dir {
            return;
        }
        let d = &mut self.st.desk;
        d.sel.retain(|n| items.iter().any(|i| &i.name == n));
        if d.rename.as_ref().is_some_and(|n| !items.iter().any(|i| &i.name == n)) {
            d.rename = None;
        }
        // удалённые — из мест (пустой список бывает при ошибке чтения — тогда не трогать)
        if !items.is_empty() {
            d.cells().retain(|n, _| items.iter().any(|i| &i.name == n));
        }
        d.items = items;
        d.loaded = true;
        if let Some(n) = d.rename_next.take() {
            if d.items.iter().any(|i| i.name == n) {
                d.sel = vec![n.clone()];
                d.anchor = Some(n.clone());
                self.desk_rename_start(&n);
            } else {
                self.st.desk.rename_next = Some(n);
            }
        }
        self.desk_relayout();
        self.desk_thumbs();
    }

    /// переименовали снаружи: место — за новым именем
    pub fn desk_renamed(&mut self, dir: PathBuf, pairs: Vec<(String, String)>) {
        if dir != self.st.desk.dir {
            return;
        }
        let d = &mut self.st.desk;
        for (old, new) in pairs {
            if let Some(c) = d.cells().remove(&old) {
                d.cells().insert(new, c);
            }
        }
        save_places(&d.places);
    }

    /// миниатюры картинок и видео (ffmpegthumbnailer), которых ещё нет: одна нить по очереди
    fn desk_thumbs(&mut self) {
        if !self.st.set.desk_thumbs {
            return;
        }
        let size = icon_size(&self.st);
        let ffmpeg = crate::proc::which("ffmpegthumbnailer");
        let want: Vec<(String, Item)> = self.st.desk.items.iter().filter(|i| !i.dir && !i.launcher() && (i.image() || (i.video() && ffmpeg))).map(|i| (thumb_key(i, size), i.clone())).filter(|(k, _)| !self.st.desk.thumbs_asked.contains(k)).collect();
        if want.is_empty() {
            return;
        }
        for (k, _) in &want {
            self.st.desk.thumbs_asked.insert(k.clone());
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            for (k, it) in want {
                let src = if it.video() { video_thumb(&it) } else { Some(it.path.clone()) };
                let img = src.and_then(|p| {
                    if it.mime == "image/svg+xml" {
                        yami_px::paths::read_capped(&p, 2 << 20).and_then(|d| yami_px::icons::svg(&d, size))
                    } else {
                        yami_px::icons::thumb(&p, it.mtime, 256).map(|t| yami_px::icons::fit_in(t, size, size))
                    }
                });
                if tx.send(Msg::DeskThumb(k, img)).is_err() {
                    return;
                }
            }
        });
    }

    pub fn desk_thumb(&mut self, k: String, img: Option<Img>) {
        self.st.desk.thumbs.insert(k, img.map(Rc::new));
        self.dirty(&SKind::Desk);
    }

    // ── мышь ────────────────────────────────────────────────────────────────
    /// нажатие (а не отпускание): выделение, двойной щелчок, меню
    pub fn desk_press(&mut self, a: &Act, button: u32, x: i32, y: i32) {
        if self.st.editing {
            return;
        }
        self.desk_rename_apply();
        let (ctrl, shift) = (self.mods.ctrl, self.mods.shift);
        let name = match a {
            Act::DeskItem(n) => Some(n.clone()),
            _ => None,
        };
        self.st.desk.press = None;
        if button == crate::click::RMB {
            let d = &mut self.st.desk;
            match &name {
                Some(n) if !d.sel.contains(n) => {
                    d.sel = vec![n.clone()];
                    d.anchor = Some(n.clone());
                }
                None if !ctrl => d.sel.clear(),
                _ => {}
            }
            self.dirty(&SKind::Desk);
            self.desk_menu(name.is_some(), x, y);
            return;
        }
        if button != crate::click::LMB {
            return;
        }
        let Some(n) = name else {
            if !ctrl && !shift {
                self.st.desk.sel.clear();
            }
            self.dirty(&SKind::Desk);
            return;
        };
        let d = &mut self.st.desk;
        let double = d.last_click.as_ref().is_some_and(|(m, t, lx, ly)| *m == n && t.elapsed() < DOUBLE && (x - lx).abs() <= 6 && (y - ly).abs() <= 6);
        d.last_click = if double { None } else { Some((n.clone(), Instant::now(), x, y)) };
        d.open_on_release = false;
        if ctrl {
            match d.sel.iter().position(|m| *m == n) {
                Some(i) => {
                    d.sel.remove(i);
                }
                None => d.sel.push(n.clone()),
            }
            d.anchor = Some(n);
        } else if shift {
            self.desk_select_range(&n);
        } else {
            if !d.sel.contains(&n) {
                d.sel = vec![n.clone()];
            }
            d.anchor = Some(n.clone());
            self.st.desk.open_on_release = double && !self.st.set.desk_single;
            self.st.desk.press = Some(n);
        }
        self.dirty(&SKind::Desk);
    }

    /// отпустили без перетаскивания: одиночный щелчок открывает (если так настроено), щелчок по
    /// одному из нескольких выделенных — оставляет только его
    pub fn desk_release(&mut self, a: &Act, button: u32) {
        let Some(n) = self.st.desk.press.take() else { return };
        let open = std::mem::take(&mut self.st.desk.open_on_release);
        if button != crate::click::LMB || *a != Act::DeskItem(n.clone()) {
            return;
        }
        if open {
            let sel = self.st.desk.sel.clone();
            self.desk_open(&sel);
        } else if self.st.set.desk_single {
            self.st.desk.sel = vec![n.clone()];
            self.desk_open(&[n]);
        } else if self.st.desk.sel.len() > 1 {
            self.st.desk.sel = vec![n];
        }
        self.dirty(&SKind::Desk);
    }

    /// Shift: прямоугольник клеток от опорного до этого
    fn desk_select_range(&mut self, n: &str) {
        let d = &mut self.st.desk;
        let (Some(a), Some(&b)) = (d.anchor.as_ref().and_then(|a| d.placement.get(a)).copied(), d.placement.get(n)) else {
            d.sel = vec![n.to_string()];
            d.anchor = Some(n.to_string());
            return;
        };
        let (lc, hc, lr, hr) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
        d.sel = d.placement.iter().filter(|(_, c)| c.0 >= lc && c.0 <= hc && c.1 >= lr && c.1 <= hr).map(|(n, _)| n.clone()).collect();
    }

    /// тянут: значок — перетаскиванием Wayland (наружу тоже), пустое место — рамкой
    pub fn desk_drag(&mut self, a: &Act, press: (f64, f64), x: f64, y: f64) {
        if self.st.editing {
            return;
        }
        let (px, py) = (press.0 as i32, press.1 as i32);
        let (x, y) = (x as i32, y as i32);
        match a {
            Act::DeskItem(name) => {
                if self.st.desk.drag.is_some() {
                    // без протокола (его нет): куда бросят — под курсором
                    self.desk_drag_over(x, y);
                    return;
                }
                if !self.st.desk.sel.contains(name) {
                    self.st.desk.sel = vec![name.clone()];
                    self.st.desk.anchor = Some(name.clone());
                }
                // потянули: не щелчок — ни открытия, ни двойного со следующим нажатием
                self.st.desk.press = None;
                self.st.desk.open_on_release = false;
                self.st.desk.last_click = None;
                let names: Vec<String> = self.st.desk.items.iter().filter(|i| self.st.desk.sel.contains(&i.name)).map(|i| i.name.clone()).collect();
                self.st.desk.drag = Some(Drag { names, anchor: name.clone(), drop_cell: None, drop_tile: None });
                self.desk_dnd_start(press);
            }
            Act::DeskBg => {
                if self.st.desk.band.is_none() {
                    if (x - px).abs() < 3 && (y - py).abs() < 3 {
                        return;
                    }
                    let add = self.mods.ctrl || self.mods.shift;
                    self.st.desk.band_base = if add { self.st.desk.sel.clone() } else { vec![] };
                    self.st.desk.band_at = Some(Instant::now());
                    self.st.desk.band_fade = None;
                }
                self.st.desk.band = Some((px, py, x, y));
                let band = R::new(px.min(x), py.min(y), (x - px).abs(), (y - py).abs());
                let (size, nl) = (icon_size(&self.st), lines(&self.st));
                let d = &mut self.st.desk;
                let mut sel = d.band_base.clone();
                for (n, c) in &d.placement {
                    let i = tile_rect(&d.grid, *c, size, nl, false).inter(&band);
                    if i.w > 0 && i.h > 0 && !sel.contains(n) {
                        sel.push(n.clone());
                    }
                }
                d.sel = sel;
            }
            _ => return,
        }
        self.dirty(&SKind::Desk);
    }

    /// курсор с тянущимся над столом: значок-цель (папка, ярлык) или клетка
    pub fn desk_drag_over(&mut self, x: i32, y: i32) {
        let (tile, cell) = self.desk_drop_kind(x, y);
        if let Some(dr) = self.st.desk.drag.as_mut() {
            dr.drop_tile = tile;
            dr.drop_cell = cell;
        }
        self.dirty(&SKind::Desk);
    }

    /// бросить можно на папку или ярлык (не из тянущихся); иначе — в клетку
    fn desk_drop_kind(&self, x: i32, y: i32) -> (Option<String>, Option<(i32, i32)>) {
        let d = &self.st.desk;
        let (size, nl) = (icon_size(&self.st), lines(&self.st));
        let dragging: &[String] = d.drag.as_ref().map(|dr| dr.names.as_slice()).unwrap_or(&[]);
        for it in &d.items {
            if dragging.contains(&it.name) || !(it.dir || it.launcher()) {
                continue;
            }
            let Some(&c) = d.placement.get(&it.name) else { continue };
            if tile_rect(&d.grid, c, size, nl, false).contains(x as f64, y as f64) {
                return (Some(it.name.clone()), None);
            }
        }
        (None, Some(d.grid.at(x, y)))
    }

    /// бросили свои значки на стол (или отпустили без протокола)
    pub fn desk_drag_end(&mut self) {
        // отпущенная рамка гаснет
        if let Some(b) = self.st.desk.band.take() {
            self.st.desk.band_fade = Some((b, Instant::now()));
        }
        let Some(dr) = self.st.desk.drag.take() else {
            self.dirty(&SKind::Desk);
            return;
        };
        if let Some(t) = dr.drop_tile.clone() {
            let d = &self.st.desk;
            let paths: Vec<PathBuf> = dr.names.iter().filter_map(|n| d.item(n).map(|i| i.path.clone())).collect();
            match d.item(&t).cloned() {
                Some(it) if it.launcher() => self.desk_launch_with(&it, &paths),
                Some(it) => self.desk_transfer(paths, it.path.clone(), true, None),
                None => {}
            }
        } else if let Some(c) = dr.drop_cell {
            self.desk_move_items(&dr.names, &dr.anchor, c);
        }
        self.dirty(&SKind::Desk);
    }

    /// сдвинуть группу: anchor — в клетку drop, остальные — тем же сдвигом, занятое — рядом
    fn desk_move_items(&mut self, names: &[String], anchor: &str, drop: (i32, i32)) {
        let d = &mut self.st.desk;
        let plan = plan_move(&d.placement, names, anchor, drop, &d.grid);
        if plan.iter().all(|(n, c)| d.placement.get(n) == Some(c)) {
            return;
        }
        for (n, c) in plan {
            d.cells().insert(n, c);
        }
        save_places(&d.places);
        self.desk_relayout();
    }

    /// новые (вставленные, брошенные) — в свободные клетки поближе к cell, по порядку
    fn desk_place_near(&mut self, names: &[String], cell: (i32, i32)) {
        let d = &mut self.st.desk;
        let mut occupied: Vec<(i32, i32)> = d.placement.iter().filter(|(n, _)| !names.contains(n)).map(|(_, c)| *c).collect();
        let (g, blocked) = (d.grid, d.blocked.clone());
        for n in names {
            let c = nearest_free(&g, cell, &occupied, &blocked);
            occupied.push(c);
            d.cells().insert(n.clone(), c);
        }
        save_places(&d.places);
        self.desk_relayout();
    }

    // ── действия ────────────────────────────────────────────────────────────
    /// открыть: ярлык — его программой, остальное — программой из mimeapps.list (как GIO)
    pub fn desk_open(&mut self, names: &[String]) {
        for n in names {
            let Some(it) = self.st.desk.item(n).cloned() else { continue };
            if let Some(e) = &it.exec {
                App::spawn(&yami_px::icons::exec_argv(e));
                continue;
            }
            self.desk_open_path(&it.path, &it.mime);
        }
    }

    /// файл или папку — первой программой для типа (как GIO); нет ни одной — xdg-open
    fn desk_open_path(&mut self, path: &Path, mime: &str) {
        match self.desk_apps(mime).into_iter().next() {
            Some(a) => desk_run(&a, std::slice::from_ref(&path.to_path_buf())),
            None => App::spawn(&["xdg-open".into(), path.display().to_string()]),
        }
    }

    /// программы для типа: по умолчанию и добавленные (mimeapps.list), потом все, что его берут
    fn desk_apps(&self, mime: &str) -> Vec<yami_px::icons::App> {
        let mut out: Vec<yami_px::icons::App> = vec![];
        for id in crate::fileops::default_apps(mime) {
            if let Some(a) = self.st.apps.iter().find(|a| format!("{}.desktop", a.id) == id || a.id == id) {
                if !out.iter().any(|o| o.id == a.id) {
                    out.push(a.clone());
                }
            }
        }
        for a in self.st.apps.iter().filter(|a| a.mime.iter().any(|m| m == mime)) {
            if !out.iter().any(|o| o.id == a.id) {
                out.push(a.clone());
            }
        }
        out
    }

    /// бросили файлы на ярлык: открыть ими его программу
    pub fn desk_launch_with(&mut self, it: &Item, paths: &[PathBuf]) {
        if let Some(e) = &it.exec {
            for argv in yami_px::icons::exec_with(e, paths) {
                App::spawn(&argv);
            }
        }
    }

    /// копия или перенос в папку (в нити); в папку стола — новым клеткам поближе к place_at
    pub fn desk_transfer(&mut self, src: Vec<PathBuf>, dest: PathBuf, mv: bool, place_at: Option<(i32, i32)>) {
        if src.is_empty() {
            return;
        }
        let at = if dest == self.st.desk.dir { place_at } else { None };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut placed = vec![];
            let mut err = vec![];
            for s in &src {
                let r = if mv { crate::fileops::move_into(s, &dest) } else { crate::fileops::copy_into(s, &dest) };
                match r {
                    Ok(d) => placed.extend(d.file_name().map(|n| n.to_string_lossy().to_string())),
                    Err(e) => err.push(format!("{}: {e}", s.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())),
                }
            }
            let _ = tx.send(Msg::DeskPlaced(placed, at, err.into_iter().take(6).collect::<Vec<_>>().join("\n")));
        });
    }

    pub fn desk_placed(&mut self, names: Vec<String>, at: Option<(i32, i32)>, err: String) {
        if let Some(c) = at {
            self.desk_place_near(&names, c);
        }
        self.desk_done(err);
    }

    fn desk_trash(&mut self, paths: Vec<PathBuf>) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let err = crate::fileops::trash(&paths).err().unwrap_or_default();
            let _ = tx.send(Msg::DeskDone(err));
        });
    }

    pub fn desk_done(&mut self, err: String) {
        if !err.is_empty() {
            self.toast_note(crate::i18n::tr("не вышло"), &err);
        }
    }

    /// новая папка или файл (пустой или из шаблона) — сразу на переименование
    fn desk_new(&mut self, folder: bool, template: Option<PathBuf>, cell: Option<(i32, i32)>) {
        let dir = self.st.desk.dir.clone();
        let name = match &template {
            Some(t) => t.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            None => crate::i18n::tr(if folder { "Новая папка" } else { "Новый документ" }).to_string(),
        };
        let p = crate::fileops::unique(&dir, &name);
        let r = match (folder, &template) {
            (true, _) => std::fs::create_dir(&p),
            (false, Some(t)) => std::fs::copy(t, &p).map(|_| ()),
            (false, None) => std::fs::write(&p, ""),
        };
        let Some(n) = p.file_name().map(|n| n.to_string_lossy().to_string()) else { return };
        match r {
            Ok(()) => {
                if let Some(c) = cell {
                    self.st.desk.cells().insert(n.clone(), c);
                    save_places(&self.st.desk.places);
                }
                self.st.desk.rename_next = Some(n);
            }
            Err(e) => self.toast_note(crate::i18n::tr("не вышло"), &e.to_string()),
        }
    }

    fn desk_rename_start(&mut self, name: &str) {
        let d = &mut self.st.desk;
        d.field.set(name);
        d.field.all = true;
        d.rename = Some(name.to_string());
        self.st.field_focus = Some("desk_name");
        self.dirty(&SKind::Desk);
    }

    /// применить имя (Enter или щелчок мимо); пустое и занятое — как было
    fn desk_rename_apply(&mut self) {
        let Some(old) = self.st.desk.rename.take() else { return };
        self.st.field_focus = None;
        let new = self.st.desk.field.text.trim().to_string();
        self.dirty(&SKind::Desk);
        if new.is_empty() || new == old || new.contains('/') || new == "." || new == ".." {
            return;
        }
        let d = &mut self.st.desk;
        let (src, dst) = (d.dir.join(&old), d.dir.join(&new));
        if std::fs::symlink_metadata(&dst).is_ok() {
            let m = crate::tf!("«{}» уже есть", "“{}” already exists", new);
            self.toast_note(crate::i18n::tr("не вышло переименовать"), &m);
            return;
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => {
                if let Some(c) = d.placement.get(&old).copied() {
                    d.cells().remove(&old);
                    d.cells().insert(new.clone(), c);
                    save_places(&d.places);
                }
                d.sel = vec![new.clone()];
                d.anchor = Some(new);
            }
            Err(e) => self.toast_note(crate::i18n::tr("не вышло переименовать"), &e.to_string()),
        }
    }

    /// упорядочить: места заново плотно по столбцам — по имени, типу, размеру или дате
    fn desk_arrange(&mut self, mode: &str) {
        let d = &mut self.st.desk;
        let mut items = d.items.clone();
        items.sort_by(|a, b| a.sort_key(mode).cmp(&b.sort_key(mode)));
        let g = d.grid;
        *d.cells() = items.iter().zip(g.order()).map(|(i, c)| (i.name.clone(), c)).collect();
        save_places(&d.places);
        self.desk_relayout();
    }

    /// терминал в папке
    fn desk_terminal(&mut self, dir: &Path) {
        let term = crate::proc::term();
        let _ = std::process::Command::new("setsid").arg("-f").arg(&term).current_dir(dir).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }

    fn desk_menu(&mut self, on_item: bool, x: i32, y: i32) {
        let out = self.surfs.values().find(|s| s.kind == SKind::Desk).map(|s| s.output.clone()).unwrap_or_default();
        let op = |label: &str, icon: &str, k: &'static str| MItem::new(label, icon, MAct::Desk(k, String::new()));
        let has_files = self.clip_has_files();
        let cell = self.st.desk.grid.at(x, y);
        let cs = format!("{},{}", cell.0, cell.1);
        let (title, items) = if on_item {
            let sel = self.st.desk.selected();
            let single = (sel.len() == 1).then(|| sel[0].clone());
            let app = single.as_ref().is_some_and(|i| i.launcher());
            let mut v = vec![op(if app { "запустить" } else { "открыть" }, "play", "open")];
            if let (true, Some(s)) = (app, &single) {
                for (n, e) in &s.actions {
                    v.push(MItem::new(n, "play", MAct::Desk("action", e.clone())));
                }
            } else if !sel.is_empty() && sel.iter().all(|i| i.mime == sel[0].mime) {
                let with: Vec<MItem> = self.desk_apps(&sel[0].mime).iter().take(OPEN_WITH_LIMIT).map(|a| MItem::new(&a.name, "play", MAct::Desk("with", a.exec.clone()))).collect();
                if !with.is_empty() {
                    v.push(MItem::sub("открыть в", "menu", with));
                }
            }
            v.push(MItem::sep());
            v.push(op("вырезать", "close", "cut"));
            v.push(op("копировать", "copy", "copy"));
            if single.as_ref().is_some_and(|i| i.dir) {
                let mut p = op("вставить", "copy", "paste_into");
                p.enabled = has_files;
                v.push(p);
            }
            v.push(MItem::sep());
            if single.is_some() {
                v.push(op("переименовать", "pen", "rename"));
            }
            v.push(op("в корзину", "trash", "trash").danger());
            v.push(MItem::sep());
            v.push(op("копировать путь", "copy", "copy_path"));
            if single.as_ref().is_some_and(|i| i.dir) {
                v.push(op("открыть терминал здесь", "term", "terminal_item"));
            }
            v.push(op("свойства", "about", "info"));
            let title = match &single {
                Some(i) => i.label.clone(),
                None => crate::tf!("выбрано: {}", "selected: {}", sel.len()),
            };
            (title, v)
        } else {
            // шаблоны документов — из XDG_TEMPLATES_DIR (xdg_dir не отдаёт саму домашнюю папку)
            let mut docs = vec![MItem::new("пустой файл", "pen", MAct::Desk("new_file", cs.clone()))];
            if let Some(td) = yami_px::paths::xdg_dir("TEMPLATES").filter(|d| d.is_dir()) {
                let mut ts: Vec<PathBuf> = std::fs::read_dir(&td).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_file() && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect()).unwrap_or_default();
                ts.sort();
                if !ts.is_empty() {
                    docs.push(MItem::sep());
                }
                for t in ts {
                    let stem = t.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    docs.push(MItem::new(&stem, "pen", MAct::Desk("new_tpl", format!("{cs}\t{}", t.display()))));
                }
            }
            let sort = [("name", "по имени"), ("type", "по типу"), ("size", "по размеру"), ("modified", "по дате изменения")].iter().map(|(m, l)| MItem::new(l, "menu", MAct::Desk("arrange", m.to_string()))).collect();
            let mut paste = MItem::new("вставить", "copy", MAct::Desk("paste", cs.clone()));
            paste.enabled = has_files;
            let v = vec![
                MItem::new("новая папка", "folder", MAct::Desk("new_folder", cs.clone())),
                MItem::sub("новый документ", "pen", docs),
                MItem::sep(),
                paste,
                MItem::sep(),
                op("выделить всё", "check", "all"),
                MItem::sub("упорядочить", "grid", sort),
                op("обновить", "reboot", "refresh"),
                MItem::sep(),
                op("открыть терминал здесь", "term", "terminal"),
                op("открыть в файловом менеджере", "folder", "folder"),
                MItem::sep(),
                op("обои", "image", "wallpaper"),
                op("настройки стола", "gear", "settings"),
            ];
            (crate::i18n::tr("рабочий стол").to_string(), v)
        };
        self.open_menu(MenuState::new(&out, x, y, &title, items));
    }

    pub fn desk_op(&mut self, op: &str, arg: String) {
        let sel = self.st.desk.sel.clone();
        let items = self.st.desk.selected();
        let paths: Vec<PathBuf> = items.iter().map(|i| i.path.clone()).collect();
        let cell = arg.split('\t').next().and_then(|s| s.split_once(',')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
        match op {
            "open" => self.desk_open(&sel),
            "action" => App::spawn(&yami_px::icons::exec_argv(&arg)),
            "with" => {
                for argv in yami_px::icons::exec_with(&arg, &paths) {
                    App::spawn(&argv);
                }
            }
            "cut" => self.clip_set_files(paths, true),
            "copy" => self.clip_set_files(paths, false),
            "paste" => {
                let dir = self.st.desk.dir.clone();
                self.clip_paste(dir, cell);
            }
            "paste_into" => {
                if let Some(p) = paths.first() {
                    self.clip_paste(p.clone(), None);
                }
            }
            "rename" => {
                if let Some(n) = sel.first() {
                    self.desk_rename_start(&n.clone());
                }
            }
            "trash" => self.desk_trash(paths),
            "copy_path" => self.clip_set_text(paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")),
            "terminal_item" => {
                if let Some(p) = paths.first() {
                    self.desk_terminal(&p.clone());
                }
            }
            "terminal" => {
                let d = self.st.desk.dir.clone();
                self.desk_terminal(&d);
            }
            "info" => {
                if !items.is_empty() {
                    self.st.desk.info = items.clone();
                    self.st.desk.info_size.clear();
                    self.desk_info_count(items);
                    self.desk_info_open();
                }
            }
            "new_folder" => self.desk_new(true, None, cell),
            "new_file" => self.desk_new(false, None, cell),
            "new_tpl" => {
                let t = arg.split_once('\t').map(|(_, p)| PathBuf::from(p));
                self.desk_new(false, t, cell);
            }
            "all" => self.st.desk.sel = self.st.desk.items.iter().map(|i| i.name.clone()).collect(),
            "arrange" => self.desk_arrange(&arg),
            "refresh" => self.desk_watch(),
            // как папку со стола: программой для inode/directory (xdg-open мог взять ranger без
            // терминала — и ничего не открывалось)
            "folder" => {
                let dir = self.st.desk.dir.clone();
                self.desk_open_path(&dir, "inode/directory");
            }
            "wallpaper" => self.walls_open(false, false),
            "settings" => self.open_settings_tab("desk"),
            _ => {}
        }
        self.dirty(&SKind::Desk);
    }

    /// размер выбранного для «свойств» (папки — обходом) в нити
    fn desk_info_count(&mut self, items: Vec<Item>) {
        fn walk(p: &Path, n: &mut u64, size: &mut u64) {
            let Ok(rd) = std::fs::read_dir(p) else { return };
            for e in rd.flatten() {
                *n += 1;
                let Ok(md) = e.path().symlink_metadata() else { continue };
                if md.is_dir() {
                    walk(&e.path(), n, size);
                } else {
                    *size += md.len();
                }
            }
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (mut n, mut size) = (0u64, 0u64);
            for it in &items {
                if it.dir {
                    walk(&it.path, &mut n, &mut size);
                    if items.len() > 1 {
                        n += 1;
                    }
                } else {
                    n += 1;
                    size += it.size;
                }
            }
            let _ = tx.send(Msg::DeskInfo(crate::tf!("{} объектов, {}", "{} items, {}", n, crate::fileops::human(size))));
        });
    }

    pub fn desk_info(&mut self, s: String) {
        self.st.desk.info_size = s;
        self.dirty(&SKind::Info);
    }

    /// «свойства» — обычное окно (как настройки): niri ставит его среди окон, а не поверх всего
    fn desk_info_open(&mut self) {
        for id in self.find(&SKind::Info) {
            self.destroy(id);
        }
        self.mk_window(SKind::Info, "yami-lite-properties", "свойства", INFO_SIZE);
    }

    pub fn desk_info_close(&mut self) {
        for id in self.find(&SKind::Info) {
            self.destroy(id);
        }
        self.st.desk.info.clear();
    }

    // ── клавиатура ──────────────────────────────────────────────────────────
    pub fn desk_key(&mut self, e: &KeyEvent) {
        if self.st.desk.rename.is_some() {
            match e.keysym {
                Keysym::Return | Keysym::KP_Enter => self.desk_rename_apply(),
                Keysym::Escape => {
                    self.st.desk.rename = None;
                    self.st.field_focus = None;
                }
                _ => {
                    let mods = self.mods;
                    App::edit_field(&mut self.st.desk.field, e, &mods);
                }
            }
            self.dirty(&SKind::Desk);
            return;
        }
        let (ctrl, shift) = (self.mods.ctrl, self.mods.shift);
        let letter = crate::keys::latin_letter(e);
        let any = !self.st.desk.sel.is_empty();
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter if any => self.desk_op("open", String::new()),
            Keysym::Delete if any => self.desk_op("trash", String::new()),
            Keysym::F2 if self.st.desk.sel.len() == 1 => self.desk_op("rename", String::new()),
            Keysym::F5 => self.desk_watch(),
            Keysym::Escape => self.st.desk.sel.clear(),
            Keysym::Menu => self.desk_menu_key(any),
            Keysym::F10 if shift => self.desk_menu_key(any),
            Keysym::Left | Keysym::Right | Keysym::Up | Keysym::Down => {
                let (dc, dr) = match e.keysym {
                    Keysym::Left => (-1, 0),
                    Keysym::Right => (1, 0),
                    Keysym::Up => (0, -1),
                    _ => (0, 1),
                };
                self.desk_step(dc, dr, shift);
            }
            _ if ctrl && shift && letter == Some('N') => self.desk_new(true, None, None),
            _ if ctrl && letter == Some('A') => self.desk_op("all", String::new()),
            _ if ctrl && letter == Some('C') && any => self.desk_op("copy", String::new()),
            _ if ctrl && letter == Some('X') && any => self.desk_op("cut", String::new()),
            _ if ctrl && letter == Some('V') => {
                let dir = self.st.desk.dir.clone();
                self.clip_paste(dir, None);
            }
            _ => {}
        }
        self.dirty(&SKind::Desk);
    }

    /// клавиша Menu (Shift+F10): меню у опорного значка
    fn desk_menu_key(&mut self, any: bool) {
        let d = &self.st.desk;
        let at = d.anchor.as_ref().and_then(|a| d.placement.get(a)).map(|c| d.grid.rect(*c)).unwrap_or(R::new(d.grid.x0, d.grid.y0, 0, 0));
        self.desk_menu(any, at.x + at.w / 2, at.y + at.h / 2);
    }

    /// стрелка: ближайший значок в ту сторону (вдоль — ближе, поперёк — вдвое дороже)
    fn desk_step(&mut self, dc: i32, dr: i32, extend: bool) {
        let d = &mut self.st.desk;
        let Some(start) = d.anchor.as_ref().and_then(|a| d.placement.get(a)).copied() else {
            if let Some((n, _)) = d.placement.iter().min_by_key(|(_, c)| **c) {
                d.sel = vec![n.clone()];
                d.anchor = Some(n.clone());
            }
            return;
        };
        let best = d
            .placement
            .iter()
            .filter_map(|(n, c)| {
                let (vx, vy) = (c.0 - start.0, c.1 - start.1);
                let along = vx * dc + vy * dr;
                (along > 0).then(|| (n.clone(), along + ((vx * dr).abs() + (vy * dc).abs()) * 2))
            })
            .min_by_key(|x| x.1)
            .map(|x| x.0);
        if let Some(n) = best {
            if extend {
                if !d.sel.contains(&n) {
                    d.sel.push(n.clone());
                }
            } else {
                d.sel = vec![n.clone()];
            }
            d.anchor = Some(n);
        }
    }
}

/// запустить программу с файлами; программе для терминала (Terminal=true) — терминал
fn desk_run(a: &yami_px::icons::App, paths: &[PathBuf]) {
    for mut argv in yami_px::icons::exec_with(&a.exec, paths) {
        if a.terminal {
            argv.splice(0..0, [crate::proc::term(), "-e".into()]);
        }
        App::spawn(&argv);
    }
}

/// кадр видео ffmpegthumbnailer'ом в свой кеш (ключ — путь и время)
fn video_thumb(it: &Item) -> Option<PathBuf> {
    let h = yami_px::fnv(yami_px::FNV0, format!("{}|{}", it.path.display(), it.mtime).as_bytes());
    let out = yami_px::paths::cache_dir().join("yami-lite/thumbs").join(format!("v{h:016x}.png"));
    if out.exists() {
        return Some(out);
    }
    let _ = std::fs::create_dir_all(out.parent()?);
    let st = std::process::Command::new("ffmpegthumbnailer").arg("-i").arg(&it.path).arg("-o").arg(&out).args(["-s", "256", "-f"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    (st.is_ok() && out.exists()).then_some(out)
}

// ── вкладка настроек ────────────────────────────────────────────────────────
pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = crate::settings::title(ui, b.x, b.y, b.w, "значки файлов на обоях");
    ui.toggle(R::new(lx, y, half, 40), "рабочий стол", "файлы и папки из папки стола", s.desk, Act::SBool("desk"));
    ui.toggle(R::new(rx, y, half, 40), "скрытые файлы", "имена с точки", s.desk_hidden, Act::SBool("desk_hidden"));
    y += 50;
    ui.toggle(R::new(lx, y, half, 40), "миниатюры", "картинки и видео (ffmpegthumbnailer)", s.desk_thumbs, Act::SBool("desk_thumbs"));
    ui.toggle(R::new(rx, y, half, 40), "открывать одним щелчком", "иначе — двойным", s.desk_single, Act::SBool("desk_single"));
    y += 50;
    crate::settings::opt_btn(ui, R::new(lx, y, half, 26), "размер значков", &format!("{} px", icon_size(st)), Act::SCycle("desk_size"));
    crate::settings::opt_btn(ui, R::new(rx, y, half, 26), "строк в подписи", &lines(st).to_string(), Act::SCycle("desk_lines"));
    y += 34;
    ui.button(R::new(lx, y, half, 26), "открыть папку", "folder", Tone::Plain, false, true, Act::SCycle("desk_folder"));
    ui.text_in(PX, R::new(rx, y, half, 26), &yami_px::paths::pretty(&desktop_dir()), ui.t.text_faint);
}

// ── свойства ────────────────────────────────────────────────────────────────
pub const INFO_SIZE: (i32, i32) = (460, 260);

pub fn draw_info(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    // окно, как настройки: на всю поверхность, без тени; за заголовок двигает niri
    let fr = R::new(0, 0, w, h);
    if t.glass_bg().a() < 255 {
        ui.cv.blur_rect(fr, 0);
    }
    ui.cv.pxbox(fr, &BoxStyle::new(t.glass_bg(), t.border));
    ui.zone(fr, Act::Block, Cur::Default);
    ui.header(R::new(1, 1, fr.w - 2, 30), "about", "свойства", "");
    ui.zone(R::new(1, 1, fr.w - 34, 30), Act::MDrag, Cur::Default);
    ui.ghost(R::new(fr.right() - 28, 5, 24, 22), "close", Act::InfoClose);
    let d = &st.desk;
    let counting = if d.info_size.is_empty() { crate::i18n::tr("считаю…").to_string() } else { d.info_size.clone() };
    let mut rows: Vec<(&str, String)> = vec![];
    let icon = if let [it] = d.info.as_slice() {
        rows.push(("имя", it.label.clone()));
        rows.push(("тип", if it.launcher() { "application/x-desktop".into() } else { it.mime.clone() }));
        if it.link {
            rows.push(("ведёт в", std::fs::read_link(&it.path).map(|p| p.display().to_string()).unwrap_or_default()));
        }
        rows.push(("где", yami_px::paths::pretty(it.path.parent().unwrap_or(Path::new("/")))));
        let tm = yami_px::time::local_tm(it.mtime);
        rows.push(("изменён", format!("{}-{:02}-{:02} {:02}:{:02}", tm.year, tm.mon, tm.day, tm.hour, tm.min)));
        rows.push(if it.dir { ("внутри", counting) } else { ("размер", format!("{} ({} Б)", crate::fileops::human(it.size), it.size)) });
        it.icons.iter().map(String::as_str).chain(FALLBACK).find_map(|n| ui.ic.get(n, 48))
    } else {
        rows.push(("имя", crate::tf!("выбрано: {}", "selected: {}", d.info.len())));
        rows.push(("где", yami_px::paths::pretty(&d.dir)));
        rows.push(("размер", counting));
        ui.ic.get("folder", 48)
    };
    if let Some(img) = icon {
        ui.cv.image_scaled(&img, R::new(16, 44, img.w, img.h), 1.0);
    }
    for (i, (k, v)) in rows.iter().enumerate() {
        let y = 44 + i as i32 * 24;
        ui.text_in(PX, R::new(76, y, 80, 20), k, t.text_faint);
        ui.cv.text_in(ui.f, PX, R::new(160, y, fr.w - 176, 20), v, t.text);
    }
    ui.button(R::new(fr.right() - 134, fr.bottom() - 38, 120, 26), "закрыть", "close", Tone::Plain, false, true, Act::InfoClose);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(n: &str) -> Item {
        Item { name: n.into(), label: n.into(), ..Default::default() }
    }

    #[test]
    fn grid_from_icon_size() {
        // 56 px: клетка 104×(56+22+2·18) = 104×114; отступ 16 от области между барами
        let g = Grid::new(R::new(0, 40, 1920, 1040), 56, 2);
        assert_eq!((g.x0, g.y0, g.cw, g.ch), (16, 56, 104, 114));
        assert_eq!(g.cols, (1920 - 32) / 104);
        assert_eq!(g.at(20, 60), (0, 0));
        assert_eq!(g.at(5000, 5000), (g.cols - 1, g.rows - 1));
    }

    #[test]
    fn saved_first_new_in_free_cells_around_widgets() {
        let g = Grid::new(R::new(0, 0, 1920, 1080), 56, 2);
        let mut cells = HashMap::new();
        cells.insert("b".to_string(), (0, 0));
        let widget = g.rect((0, 1));
        let p = layout(&[it("a"), it("b"), it("c")], &mut cells, &g, &[widget]);
        assert_eq!(p["b"], (0, 0));
        assert_eq!(p["a"], (0, 2));
        assert_eq!(p["c"], (0, 3));
        assert_eq!(cells["a"], (0, 2));
    }

    #[test]
    fn group_moves_by_the_same_offset_and_skips_taken_cells() {
        let g = Grid::new(R::new(0, 0, 1920, 1080), 56, 2);
        let mut p = HashMap::new();
        p.insert("a".to_string(), (0, 0));
        p.insert("b".to_string(), (0, 1));
        p.insert("x".to_string(), (3, 1));
        let plan = plan_move(&p, &["a".into(), "b".into()], "a", (3, 0), &g);
        assert_eq!(plan[0], ("a".to_string(), (3, 0)));
        assert_ne!(plan[1].1, (3, 1));
        assert_eq!(nearest_free(&g, (3, 1), &[(3, 1), (3, 0)], &[]), (2, 1));
    }

    #[test]
    fn labels_break_at_dots_and_get_an_ellipsis() {
        // ширина — по 8 точек на букву
        let mut w = |s: &str| s.chars().count() as i32 * 8;
        assert_eq!(wrap_label("st5icker.webp", 96, 2, &mut w), ["st5icker.", "webp"]);
        assert_eq!(wrap_label("Папка на рабочем столе", 72, 2, &mut w), ["Папка на", "рабочем…"]);
        assert_eq!(wrap_label("короткое", 96, 2, &mut w), ["короткое"]);
        assert_eq!(wrap_label("оченьдлинноеслово", 64, 3, &mut w), ["оченьдли", "нноеслов", "о"]);
    }

    #[test]
    fn inotify_events_parse() {
        let mut b = vec![];
        for (mask, cookie, name) in [(libc::IN_MOVED_FROM, 7u32, "old.txt"), (libc::IN_MOVED_TO, 7, "new.txt")] {
            b.extend(1i32.to_ne_bytes());
            b.extend(mask.to_ne_bytes());
            b.extend(cookie.to_ne_bytes());
            b.extend(16u32.to_ne_bytes());
            let mut n = name.as_bytes().to_vec();
            n.resize(16, 0);
            b.extend(n);
        }
        assert_eq!(events(&b), vec![(libc::IN_MOVED_FROM, 7, "old.txt".into()), (libc::IN_MOVED_TO, 7, "new.txt".into())]);
    }
}
