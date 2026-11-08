// Выбор обоев (Modal::Walls, #11): сетка миниатюр папки обоев (wallDir; пусто — ~/Pictures/Wallpapers,
// папка текущих обоев или ~/Pictures), первая клетка — пиксельные из палитры. Папку читает одна
// нить, миниатюры — другая, через кеш миниатюр (yami-px icons::thumb): сперва видимые
// строки, потом по две соседних, в памяти — не больше нескольких десятков. Щелчок ставит обои и
// возвращает в настройки, если выбор открыли оттуда; закрылся — миниатюры отпущены.

use crate::app::{App, Modal, Msg, SKind, State};
use crate::config;
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, R};
use crate::ui::{Act, Cur, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};

pub const SIZE: (i32, i32) = (780, 560);
const COLS: usize = 4;
const ROWS: usize = 4;
/// клетка 16:9 и зазор между клетками
const TW: i32 = 172;
const TH: i32 = 97;
const GAP: i32 = 12;
/// откуда начинается сетка
const GX: i32 = 25;
const GY: i32 = 76;
/// больше картинок из одной папки не показываем
const MAX: usize = 500;
/// строк про запас сверху и снизу видимых
const SPARE: usize = 2;

/// очередь нити миниатюр: первая видимая строка, что уже сделано, пора ли выйти
struct Q {
    top: usize,
    have: Vec<bool>,
    stop: bool,
}

struct Worker(Arc<(Mutex<Q>, Condvar)>);

impl Drop for Worker {
    fn drop(&mut self) {
        if let Ok(mut q) = self.0 .0.lock() {
            q.stop = true;
        }
        self.0 .1.notify_all();
    }
}

#[derive(Default)]
pub struct Walls {
    /// номер открытия: ответы прошлого открытия выбрасываются
    pub gen: u64,
    pub dir: PathBuf,
    /// клетки: 0 — пиксельные из палитры (state/yami/wallpaper.png), дальше картинки папки; пусто,
    /// пока нить читает папку. Тот же список у нити миниатюр
    pub tiles: Arc<[PathBuf]>,
    pub thumbs: HashMap<usize, Option<Rc<Img>>>,
    /// клетка под клавиатурой
    pub cur: usize,
    /// первая видимая строка
    pub top: usize,
    /// открыт из настроек: туда и вернуться
    pub back: bool,
    /// сколько картинок не поместилось в MAX
    pub more: usize,
    worker: Option<Worker>,
}

/// папка обоев: своя из настроек (wallDir), иначе Wallpapers в «Изображениях», папка текущих
/// обоев, сами «Изображения», дом. Трогает диск (сетевая папка может думать): только в нити
fn dir(wall_dir: &str, cur: Option<&Path>) -> PathBuf {
    let own = yami_px::paths::expand(wall_dir);
    if !wall_dir.trim().is_empty() && own.is_dir() {
        return own;
    }
    let pics = Some(yami_px::paths::pictures()).filter(|d| d.is_dir()).unwrap_or_else(|| config::home().join("Pictures"));
    if let Some(d) = ["Wallpapers", "wallpapers", "Wallpaper", "wallpaper"].iter().map(|n| pics.join(n)).find(|d| d.is_dir()) {
        return d;
    }
    if let Some(d) = cur.and_then(Path::parent).map(Path::to_path_buf).filter(|d| d.is_dir()) {
        return d;
    }
    if pics.is_dir() {
        pics
    } else {
        config::home()
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// картинки папки, которые шелл декодирует сам (без скрытых), по имени «естественно»; сколько
/// отброшено сверх MAX
fn list(dir: &Path) -> (Vec<PathBuf>, usize) {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| !file_name(p).starts_with('.') && crate::wall::decodable(p) && p.is_file()).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| yami_px::paths::natural(&file_name(a), &file_name(b)));
    let more = v.len().saturating_sub(MAX);
    v.truncate(MAX);
    (v, more)
}

/// клетки строк top.. (видимых) и по spare строк сверху и снизу
fn window(top: usize, n: usize, spare: usize) -> std::ops::Range<usize> {
    (top.saturating_sub(spare) * COLS).min(n)..((top + ROWS + spare) * COLS).min(n)
}

/// миниатюра под клетку: обрезка по центру до 16:9 и уменьшение ровно до размера окна клетки
fn make(p: &Path) -> Option<Img> {
    let mtime = std::fs::metadata(p).ok()?.mtime();
    let t = crate::icons::thumb(p, mtime, 192)?;
    let (w, h) = ((TW - 6) as u32, (TH - 6) as u32);
    // наибольший кусок с пропорциями клетки, как обои «заполнить»
    let (x, y, cw, ch) = crate::wall::cover_rect(t.width(), t.height(), w, h);
    let crop = image::imageops::crop_imm(&t, x, y, cw, ch).to_image();
    let r = image::imageops::resize(&crop, w, h, image::imageops::FilterType::Triangle);
    Some(crate::icons::from_rgba(&r))
}

/// нить миниатюр: берёт несделанную клетку (видимые строки, потом запасные), ждёт, когда окно
/// сдвинут; send — отдать готовую (false — получателя нет, выйти)
fn spawn(tiles: Arc<[PathBuf]>, q: Arc<(Mutex<Q>, Condvar)>, send: impl Fn(usize, Option<Img>) -> bool + Send + 'static) {
    let _ = std::thread::Builder::new().name("walls".into()).spawn(move || {
        let mut made = 0;
        loop {
            let i = {
                let (m, cv) = &*q;
                let Ok(mut g) = m.lock() else { return };
                loop {
                    if g.stop {
                        yami_px::trim_heap();
                        return;
                    }
                    let (top, n) = (g.top, tiles.len());
                    if let Some(i) = window(top, n, 0).chain(window(top, n, SPARE)).find(|&i| !g.have[i]) {
                        g.have[i] = true;
                        break i;
                    }
                    // делать нечего: память больших декодов — системе, пока ждём
                    if made > 0 {
                        made = 0;
                        yami_px::trim_heap();
                    }
                    let Ok(n) = cv.wait(g) else { return };
                    g = n;
                }
            };
            if !send(i, make(&tiles[i])) {
                return;
            }
            made += 1;
            if made % 8 == 0 {
                yami_px::trim_heap();
            }
        }
    });
}

fn tile_rect(i: usize, top: usize) -> Option<R> {
    let row = i / COLS;
    if row < top || row >= top + ROWS {
        return None;
    }
    let (c, r) = ((i % COLS) as i32, (row - top) as i32);
    Some(R::new(GX + c * (TW + GAP), GY + r * (TH + GAP), TW, TH))
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let wl = &st.walls;
    let loading = wl.tiles.is_empty();
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, false);
    ui.header(R::new(1, 1, fr.w - 2, 30), "image", "обои", "");
    ui.ghost(R::new(fr.right() - 28, 5, 24, 22), "close", Act::WallsBack);
    // папка — как есть (путь не переводится), кнопки справа
    let (b1, b2) = (126, 156);
    let bx = fr.right() - GX - b1 - b2 - 8;
    ui.cv.icon("folder", GX, 44, 2, t.text_faint);
    ui.cv.text_in(ui.f, PX, R::new(GX + 22, 38, bx - GX - 30, 26), &yami_px::paths::pretty(&wl.dir), t.text_dim);
    // сетка: под клетками — зона для колеса
    let grid = R::new(GX, GY, COLS as i32 * (TW + GAP) - GAP, ROWS as i32 * (TH + GAP) - GAP);
    ui.zone(grid, Act::WallsScroll, Cur::Default);
    let own = crate::wall::own(&st.set);
    let mut hovered = None;
    for i in window(wl.top, wl.tiles.len(), 0) {
        let Some(r) = tile_rect(i, wl.top) else { continue };
        let on = if i == 0 { own.is_none() } else { own.as_deref() == Some(wl.tiles[i].as_path()) };
        let hov = ui.hover(r);
        if hov {
            hovered = Some(i);
        }
        ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, if on { t.red } else if hov { t.text_dim } else { t.border }).shadow(if hov { 2 } else { 0 }));
        let inner = r.inset(3);
        match wl.thumbs.get(&i) {
            Some(Some(img)) => ui.cv.image(img, inner.x, inner.y, 1.0),
            // не вышло (битая или огромная): значок, щёлкнуть всё равно можно
            Some(None) => ui.cv.icon_in("image", inner, 2, t.text_faint),
            None => ui.cv.icon_in("image", inner, 2, t.border),
        }
        if i == 0 {
            // пиксельные: подпись на полосе внизу
            let band = R::new(inner.x, inner.bottom() - 18, inner.w, 18);
            ui.cv.fill(band, t.bg.alpha(0.78));
            ui.text_in(PX, R::new(band.x + 6, band.y, band.w - 8, band.h), "из палитры", t.text);
        }
        if on {
            ui.cv.fill(R::new(inner.right() - 17, inner.y + 2, 15, 15), t.red);
            ui.cv.icon("check", inner.right() - 13, inner.y + 6, 1, t.on_accent);
        }
        if wl.cur == i {
            ui.cv.outline(R::new(r.x - 3, r.y - 3, r.w + 6, r.h + 6), t.ember, 1);
        }
        ui.zone(r, Act::WallPick(i), Cur::Pointer);
    }
    if loading {
        ui.text_center(PX, R::new(grid.x, GY + TH + GAP, grid.w, 24), "читаю папку…", t.text_faint);
    } else if wl.tiles.len() == 1 {
        ui.text_center(PX, R::new(grid.x, GY + TH + GAP, grid.w, 24), "в папке нет картинок (png, jpg, webp)", t.text_faint);
        ui.text_center(PX, R::new(grid.x, GY + TH + GAP + 24, grid.w, 24), "выбери другую папку", t.text_faint);
    }
    // полоса прокрутки, если строк больше, чем видно
    let rows = wl.tiles.len().div_ceil(COLS);
    if rows > ROWS {
        let tr = R::new(grid.right() + 9, GY, 4, grid.h);
        ui.cv.fill(tr, t.border.fade(0.5));
        let th = (tr.h * ROWS as i32 / rows as i32).max(12);
        let ty = tr.y + (tr.h - th) * wl.top as i32 / (rows - ROWS) as i32;
        ui.cv.fill(R::new(tr.x, ty, 4, th), t.red);
    }
    // низ: имя картинки под курсором (или под клавиатурой), подсказка
    let fy = fr.bottom() - 34;
    ui.cv.fill(R::new(1, fy - 8, fr.w - 2, 1), t.border);
    let name = match hovered.unwrap_or(wl.cur) {
        _ if loading => String::new(),
        0 => crate::i18n::tr("пиксельные из палитры").to_string(),
        i => wl.tiles.get(i).map(|p| file_name(p)).unwrap_or_default(),
    };
    ui.cv.text_in(ui.f, PX, R::new(GX, fy, fr.w / 2, 24), &name, t.text);
    let hint = if wl.more > 0 {
        crate::tf!("показаны первые {}, ещё {}", "the first {} shown, {} more", MAX, wl.more)
    } else {
        crate::i18n::tr("стрелки, Enter — поставить, Esc — назад").to_string()
    };
    ui.cv.text_right(ui.f, PX, R::new(fr.w / 2, fy, fr.w / 2 - GX, 24), &hint, t.text_faint);
}

impl App {
    /// открыть выбор (или закрыть, если открыт): back — из настроек, вернуться туда
    pub fn walls_open(&mut self, back: bool) {
        if self.st.modal == Some(Modal::Walls) {
            self.walls_back();
            return;
        }
        self.walls_start(back);
    }

    /// открыть (уже открытый — заново, на новой папке): папку читает нить, клетки придут в
    /// walls_listed. Wayland-нить диск не ждёт: в ~/Pictures бывают тысячи фото, папка — сетевой
    fn walls_start(&mut self, back: bool) {
        if self.st.modal != Some(Modal::Walls) {
            self.toggle_modal(Modal::Walls);
            if self.st.modal != Some(Modal::Walls) {
                return;
            }
        }
        let gen = self.st.walls.gen + 1;
        self.st.walls = Walls { gen, back, ..Default::default() };
        let (wd, cur, tx) = (self.st.set.wall_dir.clone(), crate::wall::own(&self.st.set), self.tx.clone());
        let _ = std::thread::Builder::new().name("walls-list".into()).spawn(move || {
            let d = dir(&wd, cur.as_deref());
            let (files, more) = list(&d);
            let _ = tx.send(Msg::WallsListed(gen, d, files, more));
            // кеш миниатюр: убрать старое (раз в сутки), уже после списка
            crate::icons::thumbs_tidy();
        });
        self.dirty(&SKind::Modal);
    }

    /// папка прочитана: клетки, текущие обои под клавиатурой и видны, нить миниатюр — с их строк
    pub fn walls_listed(&mut self, gen: u64, dir: PathBuf, files: Vec<PathBuf>, more: usize) {
        if gen != self.st.walls.gen || self.st.modal != Some(Modal::Walls) {
            return;
        }
        // клетка 0 — пиксельные из палитры: пути нет, миниатюра рисуется здесь же
        let tiles: Arc<[PathBuf]> = std::iter::once(PathBuf::new()).chain(files).collect();
        let own = crate::wall::own(&self.st.set);
        let cur = own.and_then(|p| tiles.iter().skip(1).position(|t| *t == p).map(|i| i + 1)).unwrap_or(0);
        let top = (cur / COLS + 1).saturating_sub(ROWS);
        let mut have = vec![false; tiles.len()];
        have[0] = true;
        let q = Arc::new((Mutex::new(Q { top, have, stop: false }), Condvar::new()));
        let tx = self.tx.clone();
        spawn(tiles.clone(), q.clone(), move |i, img| tx.send(Msg::WallThumb(gen, i, img)).is_ok());
        let back = self.st.walls.back;
        let t = &self.st.t;
        let pal = crate::wall::generate(t.bg, t.red, t.border, t.light);
        let small = image::imageops::resize(&pal, (TW - 6) as u32, (TH - 6) as u32, image::imageops::FilterType::Nearest);
        let mut thumbs = HashMap::new();
        thumbs.insert(0, Some(Rc::new(crate::icons::from_rgba(&small))));
        self.st.walls = Walls { gen, dir, tiles, cur, top, back, more, thumbs, worker: Some(Worker(q)), ..Default::default() };
        self.dirty(&SKind::Modal);
    }

    /// закрыть; открыт из настроек — вернуться на вкладку «обои»
    pub fn walls_back(&mut self) {
        let back = self.st.walls.back;
        self.close_modal();
        if back {
            self.open_settings_tab("wall");
        }
    }

    /// отпустить миниатюры и нить (close_modal)
    pub fn walls_free(&mut self) {
        let gen = self.st.walls.gen;
        self.st.walls = Walls { gen, ..Default::default() };
        yami_px::trim_heap();
    }

    pub fn walls_thumb(&mut self, gen: u64, i: usize, img: Option<Img>) {
        let wl = &mut self.st.walls;
        if gen != wl.gen || self.st.modal != Some(Modal::Walls) {
            return;
        }
        if window(wl.top, wl.tiles.len(), SPARE).contains(&i) {
            wl.thumbs.insert(i, img.map(Rc::new));
            // запасные строки не видны: перерисовывать нечего
            if tile_rect(i, wl.top).is_some() {
                self.dirty(&SKind::Modal);
            }
        } else if let Some(w) = &wl.worker {
            // пока делалась, её прокрутили из виду: понадобится — сделается снова
            if let Ok(mut q) = w.0 .0.lock() {
                q.have[i] = false;
            }
        }
    }

    /// прокрутить так, чтобы клетка i была видна
    fn walls_show(&mut self, i: usize) {
        let wl = &mut self.st.walls;
        let row = i / COLS;
        if row < wl.top {
            wl.top = row;
        } else if row >= wl.top + ROWS {
            wl.top = row + 1 - ROWS;
        }
        self.walls_sync();
    }

    /// окно сдвинулось: лишние миниатюры — отпустить, нити — новое окно
    fn walls_sync(&mut self) {
        let wl = &mut self.st.walls;
        let win = window(wl.top, wl.tiles.len(), SPARE);
        let gone: Vec<usize> = wl.thumbs.keys().copied().filter(|i| !win.contains(i)).collect();
        for i in &gone {
            wl.thumbs.remove(i);
        }
        if let Some(w) = &wl.worker {
            if let Ok(mut q) = w.0 .0.lock() {
                q.top = wl.top;
                for i in gone {
                    q.have[i] = false;
                }
            }
            w.0 .1.notify_all();
        }
        self.dirty(&SKind::Modal);
    }

    pub fn walls_scroll(&mut self, d: i32) {
        let wl = &mut self.st.walls;
        let max = wl.tiles.len().div_ceil(COLS).saturating_sub(ROWS);
        let top = (wl.top as i32 + d).clamp(0, max as i32) as usize;
        if top != wl.top {
            wl.top = top;
            self.walls_sync();
        }
    }

    /// поставить обои отсюда (пусто — пиксельные)
    fn walls_set(&mut self, p: &str) {
        self.set_wall(p);
    }

    /// клетка выбрана: пиксельные (0) или картинка
    pub fn walls_pick(&mut self, i: usize) {
        let Some(p) = self.st.walls.tiles.get(i).cloned() else { return };
        self.walls_set(&if i == 0 { String::new() } else { p.to_string_lossy().to_string() });
        self.walls_back();
    }



    pub fn walls_key(&mut self, e: &KeyEvent) {
        let n = self.st.walls.tiles.len();
        let cur = self.st.walls.cur;
        let to = match e.keysym {
            Keysym::Escape => return self.walls_back(),
            Keysym::Return | Keysym::KP_Enter | Keysym::space => return self.walls_pick(cur),
            Keysym::Right | Keysym::Tab => cur + 1,
            Keysym::Left | Keysym::ISO_Left_Tab => cur.saturating_sub(1),
            Keysym::Down => cur + COLS,
            Keysym::Up => cur.saturating_sub(COLS),
            Keysym::Page_Down => cur + COLS * ROWS,
            Keysym::Page_Up => cur.saturating_sub(COLS * ROWS),
            Keysym::Home => 0,
            Keysym::End => n,
            _ => return,
        };
        self.st.walls.cur = to.min(n.saturating_sub(1));
        let c = self.st.walls.cur;
        self.walls_show(c);
    }
}

/// для `shot walls`: папка-образец с картинками в тонах палитры, без нити и диска
pub fn mock(st: &mut State) {
    let t = st.t;
    let names = ["dusk-field.png", "pixel-rain.jpg", "ash-city.png", "night-train.webp", "tokyo-2.jpg", "tokyo-10.jpg", "lantern.png", "snow-dither.png", "neon-alley.jpg", "moon-sea.png"];
    let dir = config::home().join("Pictures/Wallpapers");
    let tiles: Arc<[PathBuf]> = std::iter::once(PathBuf::from("/shot/wallpaper.png")).chain(names.iter().map(|n| dir.join(n))).collect();
    let (w, h) = (TW - 6, TH - 6);
    let tones = [t.bg, t.panel, t.surface_hi, t.dim, t.red, t.ember, t.text_dim];
    let mut thumbs = HashMap::new();
    for i in 0..tiles.len() {
        // полосы горизонта и «солнце»: у каждой своя высота и тон, чтобы клетки различались
        let mut px = vec![0u32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let band = (y * 5 / h + i as i32) as usize % tones.len();
                let (sx, sy) = (w * (2 + i as i32 % 5) / 8, h / 3 + i as i32 % 3 * 6);
                let sun = (x - sx).pow(2) + (y - sy).pow(2) < (10 + i as i32 % 4 * 3).pow(2);
                let c = if sun { t.ember } else { tones[band] };
                px[(y * w + x) as usize] = 0xff000000 | (c.0 & 0xffffff);
            }
        }
        thumbs.insert(i, if i == 7 { None } else { Some(Rc::new(Img { w, h, px })) });
    }
    st.walls = Walls { gen: 1, dir, tiles, thumbs, cur: 2, back: true, ..Default::default() };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("yami-walls-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn window_bounds() {
        assert_eq!(window(0, 100, SPARE), 0..24);
        assert_eq!(window(5, 100, SPARE), 12..44);
        assert_eq!(window(5, 30, SPARE), 12..30);
        assert_eq!(window(9, 3, SPARE), 3..3);
        assert_eq!(window(5, 100, 0), 20..36);
    }

    #[test]
    fn list_and_thumbs() {
        let d = tmp("list");
        // кеш миниатюр — во временной папке, не в настоящем ~/.cache. Другие тесты кеш не читают;
        // переменная возвращается в конце
        let was = std::env::var_os("XDG_CACHE_HOME");
        std::env::set_var("XDG_CACHE_HOME", d.join("cache"));
        image::RgbaImage::from_pixel(400, 300, image::Rgba([200, 40, 30, 255])).save(d.join("b10.png")).unwrap();
        image::RgbImage::from_pixel(1000, 200, image::Rgb([20, 40, 200])).save(d.join("b2.jpg")).unwrap();
        image::RgbaImage::from_pixel(90, 160, image::Rgba([0, 255, 0, 255])).save(d.join("A1.png")).unwrap();
        std::fs::write(d.join(".hidden.png"), b"x").unwrap();
        std::fs::write(d.join("notes.txt"), b"x").unwrap();
        std::fs::write(d.join("broken.png"), b"not a png").unwrap();
        let (v, more) = list(&d);
        let names: Vec<String> = v.iter().map(|p| file_name(p)).collect();
        assert_eq!(names, ["A1.png", "b2.jpg", "b10.png", "broken.png"]);
        assert_eq!(more, 0);
        // любая картинка — ровно в окно клетки, пропорции любые; битая — None
        for p in &v[..3] {
            let img = make(p).unwrap();
            assert_eq!((img.w, img.h), (TW - 6, TH - 6), "{}", p.display());
        }
        assert!(make(&v[3]).is_none());
        // второй раз — из кеша
        assert!(std::fs::read_dir(d.join("cache/yami/thumbs")).unwrap().count() >= 3);
        match was {
            Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn worker_follows_the_window() {
        let d = tmp("worker");
        let tiles: Arc<[PathBuf]> = (0..60).map(|i| d.join(format!("{i}.png"))).collect();
        let q = Arc::new((Mutex::new(Q { top: 0, have: vec![false; tiles.len()], stop: false }), Condvar::new()));
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(tiles, q.clone(), move |i, _| tx.send(i).is_ok());
        let w = Worker(q.clone());
        let got = |n: usize| -> Vec<usize> { (0..n).map(|_| rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()).collect() };
        // сначала видимые строки 0..4, потом запасные 4..6
        let a = got(24);
        let mut first = a[..16].to_vec();
        first.sort();
        assert_eq!(first, (0..16).collect::<Vec<_>>());
        let mut all = a;
        all.sort();
        assert_eq!(all, (0..24).collect::<Vec<_>>());
        assert!(rx.recv_timeout(std::time::Duration::from_millis(200)).is_err());
        // прокрутили к строке 8: окно 24..56, сделанное заново не делается
        q.0.lock().unwrap().top = 8;
        q.1.notify_all();
        let mut b = got(32);
        b.sort();
        assert_eq!(b, (24..56).collect::<Vec<_>>());
        // закрыли — нить выходит и роняет send
        drop(w);
        assert!(matches!(rx.recv_timeout(std::time::Duration::from_secs(5)), Err(std::sync::mpsc::RecvTimeoutError::Disconnected)));
        let _ = std::fs::remove_dir_all(&d);
    }
}
