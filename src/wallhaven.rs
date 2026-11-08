// Wallhaven (wallhaven.cc): поиск обоев всплывашкой из модуля бара, как плагин Noctalia — теги,
// категории, чистота (NSFW — только с ключом API), сортировка, соотношение сторон, разрешение,
// страницы. Щелчок по миниатюре скачивает картинку в папку обоев и ставит её. Сеть — curl в нитях
// (только https); миниатюры — в кеше ~/.cache/yami-lite/wallhaven. Ключ — настройки → обои.

use crate::app::{App, Msg, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, R};
use crate::ui::{Act, Cur, Field, Tone, Ui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

pub const SIZE: (i32, i32) = (760, 700);
const API: &str = "https://wallhaven.cc/api/v1/search";
/// сетка 4×5: своя страница — 20 картинок; у API страница — 24, своя собирается из одной-двух его
const COLS: i32 = 4;
const ROWS: i32 = 5;
const PER: usize = (COLS * ROWS) as usize;
const API_PER: usize = 24;

pub const SORTS: &[(&str, &str)] = &[("date_added", "дата добавления"), ("relevance", "по запросу"), ("random", "случайно"), ("views", "просмотры"), ("favorites", "в избранном"), ("toplist", "лучшие"), ("hot", "популярное")];
pub const RATIOS: &[(&str, &str)] = &[("", "любое"), ("landscape", "широкие"), ("portrait", "вертикальные"), ("16x9", "16:9"), ("16x10", "16:10"), ("21x9", "21:9"), ("32x9", "32:9"), ("9x16", "9:16"), ("1x1", "1:1"), ("3x2", "3:2"), ("4x3", "4:3")];

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub thumb: String,
    pub full: String,
    pub res: String,
    /// sfw | sketchy | nsfw: рамка у миниатюры
    pub purity: String,
}

/// состояние окна: фильтры, страница, найденное, миниатюры
pub struct Wh {
    pub query: Field,
    pub width: Field,
    pub height: Field,
    /// общее, аниме, люди
    pub cats: [bool; 3],
    /// SFW, сомнительно, NSFW
    pub purity: [bool; 3],
    pub sort: usize,
    pub desc: bool,
    pub ratio: usize,
    pub exact: bool,
    pub page: u32,
    pub last: u32,
    pub items: Vec<Item>,
    pub thumbs: HashMap<String, Option<Rc<Img>>>,
    pub loading: bool,
    pub err: String,
    /// качаем и ставим эту
    pub applying: Option<String>,
    /// номер запроса: ответы прошлых — мимо
    pub gen: u64,
}

impl Default for Wh {
    fn default() -> Wh {
        Wh { query: Field::default(), width: Field::default(), height: Field::default(), cats: [true, true, false], purity: [true, false, false], sort: 0, desc: true, ratio: 1, exact: false, page: 1, last: 1, items: vec![], thumbs: HashMap::new(), loading: false, err: String::new(), applying: None, gen: 0 }
    }
}

fn bits(b: &[bool; 3]) -> String {
    b.iter().map(|x| if *x { '1' } else { '0' }).collect()
}

fn thumbs_dir() -> PathBuf {
    yami_px::paths::cache_dir().join("yami-lite/wallhaven")
}

/// папка, куда ложатся скачанные обои: своя из настроек, иначе папка обоев, иначе ~/Pictures/Wallpapers
fn download_dir(st: &State) -> PathBuf {
    for d in [&st.set.wallhaven_dir, &st.set.wall_dir] {
        if !d.trim().is_empty() {
            return yami_px::paths::expand(d);
        }
    }
    yami_px::paths::pictures().join("Wallpapers")
}

/// запрос страницы: (найденное, последняя страница) или ошибка
fn search(params: &[(&'static str, String)], key: &str) -> Result<(Vec<Item>, u32), String> {
    let mut c = crate::proc::curl_get(20, 4 << 20, API, params);
    c.args(["-f", "-H", "Accept: application/json"]);
    if !key.is_empty() {
        c.arg("-H").arg(format!("X-API-Key: {key}"));
    }
    let out = c.output().map_err(|_| crate::i18n::tr("нужен curl").to_string())?;
    if !out.status.success() {
        return Err(crate::i18n::tr("Wallhaven не ответил (сеть или неверный ключ)").to_string());
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| crate::i18n::tr("непонятный ответ Wallhaven").to_string())?;
    let items = v["data"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let s = |k: &str| x[k].as_str().map(String::from);
                    // только картинки с самого wallhaven.cc
                    let full = s("path").filter(|u| u.starts_with("https://w.wallhaven.cc/"))?;
                    let thumb = x["thumbs"]["small"].as_str().filter(|u| u.starts_with("https://th.wallhaven.cc/"))?.to_string();
                    let id = s("id").filter(|i| i.chars().all(|c| c.is_ascii_alphanumeric()))?;
                    Some(Item { id, thumb, full, res: s("resolution").unwrap_or_default(), purity: s("purity").unwrap_or_default() })
                })
                .collect()
        })
        .unwrap_or_default();
    // всего найдено — из него число своих страниц по 20
    Ok((items, v["meta"]["total"].as_u64().unwrap_or(0) as u32))
}

/// миниатюра: из кеша или скачать, потом вырезать под клетку
fn thumb(it: &Item, w: u32, h: u32) -> Option<Img> {
    let dir = thumbs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let p = dir.join(format!("{}.jpg", it.id));
    if !p.is_file() {
        let tmp = p.with_extension("part");
        let ok = crate::proc::curl(20, 2 << 20).arg("-f").arg("-o").arg(&tmp).arg(&it.thumb).status().is_ok_and(|s| s.success());
        if !ok || std::fs::rename(&tmp, &p).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return None;
        }
    }
    let src = image::open(&p).ok()?.to_rgba8();
    let (x, y, cw, ch) = crate::wall::cover_rect(src.width(), src.height(), w, h);
    let crop = image::imageops::crop_imm(&src, x, y, cw, ch).to_image();
    Some(crate::icons::from_rgba(&image::imageops::resize(&crop, w, h, image::imageops::FilterType::Triangle)))
}

/// клетка i сетки при первом видимом ряде top; не на виду — None
fn cell(area: R, i: i32, top: i32) -> Option<R> {
    let (gap, cw) = (10, (area.w - (COLS - 1) * 10) / COLS);
    let ch = (area.h - (ROWS - 1) * gap) / ROWS;
    let row = i / COLS - top;
    (0..ROWS).contains(&row).then(|| R::new(area.x + (i % COLS) * (cw + gap), area.y + row * (ch + gap), cw, ch))
}

/// окно миниатюры (клетка без рамки) — под него режутся картинки
fn thumb_size() -> (u32, u32) {
    let g = grid(R::new(0, 0, SIZE.0, SIZE.1));
    let r = cell(g, 0, 0).map(|r| r.inset(3)).unwrap_or(R::new(0, 0, 170, 96));
    (r.w as u32, r.h as u32)
}

fn grid(b: R) -> R {
    R::new(b.x, b.y + 106, b.w, b.h - 106 - 36)
}

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let wh = &st.wh;
    let s = &st.set;
    // строка поиска
    let focused = st.field_focus == Some("wh_query");
    ui.field(R::new(b.x, b.y, b.w - 96, 28), &wh.query, "теги: природа, ночной город…", focused, false, Act::SField("wh_query"));
    ui.button(R::new(b.right() - 88, b.y + 1, 88, 26), "найти", "search", Tone::Primary, false, !wh.loading, Act::WhSearch);
    // фильтры: категории и чистота — фишками
    let mut x = b.x;
    let y = b.y + 38;
    let chip = |ui: &mut Ui, x: &mut i32, label: &str, on: bool, enabled: bool, act: Act| {
        let w = ui.w(PX, label) + 20;
        ui.button(R::new(*x, y, w, 24), label, "", Tone::Plain, on, enabled, act);
        *x += w + 6;
    };
    ui.text_in(PX, R::new(x, y, 80, 24), "категории", t.text_dim);
    x += ui.w(PX, crate::i18n::tr("категории")) + 10;
    for (i, n) in ["общее", "аниме", "люди"].iter().enumerate() {
        chip(ui, &mut x, n, wh.cats[i], true, Act::WhCat(i));
    }
    x += 12;
    ui.text_in(PX, R::new(x, y, 80, 24), "чистота", t.text_dim);
    x += ui.w(PX, crate::i18n::tr("чистота")) + 10;
    let key = !s.wallhaven_key.trim().is_empty();
    for (i, n) in ["SFW", "сомнительно", "NSFW"].iter().enumerate() {
        chip(ui, &mut x, n, wh.purity[i] && (i < 2 || key), i < 2 || key, Act::WhPurity(i));
    }
    // сортировка, соотношение, разрешение
    let y2 = y + 32;
    let third = (b.w - 20) / 3;
    crate::settings::opt_btn(ui, R::new(b.x, y2, third - 30, 26), "сортировка", crate::widgets::name_of(SORTS, SORTS[wh.sort].0), Act::Drop("wh_sort"));
    ui.button(R::new(b.x + third - 26, y2, 26, 26), "", if wh.desc { "down" } else { "up" }, Tone::Plain, false, true, Act::WhOrder);
    crate::settings::opt_btn(ui, R::new(b.x + third + 10, y2, third, 26), "стороны", RATIOS[wh.ratio].1, Act::Drop("wh_ratio"));
    let rx = b.x + 2 * (third + 10);
    let rw = b.right() - rx;
    ui.button(R::new(rx, y2, 72, 26), if wh.exact { "ровно" } else { "от" }, "", Tone::Ghost, false, true, Act::WhExact);
    let fw = (rw - 72 - 6 - 14 - 6) / 2;
    ui.field(R::new(rx + 78, y2, fw, 26), &wh.width, "ширина", st.field_focus == Some("wh_w"), false, Act::SField("wh_w"));
    ui.text_center(PX, R::new(rx + 78 + fw, y2, 14, 26), "×", t.text_faint);
    ui.field(R::new(rx + 78 + fw + 14 + 6, y2, fw, 26), &wh.height, "высота", st.field_focus == Some("wh_h"), false, Act::SField("wh_h"));
    // сетка миниатюр
    let g = grid(b);
    if wh.items.is_empty() {
        let msg = if wh.loading {
            crate::i18n::tr("ищу…").to_string()
        } else if !wh.err.is_empty() {
            wh.err.clone()
        } else {
            crate::i18n::tr("ничего не нашлось").to_string()
        };
        ui.text_center(PX, R::new(g.x, g.y + g.h / 2 - 10, g.w, 20), &msg, if wh.err.is_empty() { t.text_faint } else { t.ember });
    }
    for (i, it) in wh.items.iter().enumerate() {
        let Some(r) = cell(g, i as i32, 0) else { continue };
        let hov = ui.hover(r);
        let busy = wh.applying.as_deref() == Some(it.id.as_str());
        // рамка по чистоте: сомнительные — жёлтая, NSFW — красная
        let edge = match it.purity.as_str() {
            "nsfw" => crate::privacy::RED,
            "sketchy" => t.warn,
            _ if hov => t.red,
            _ => t.border,
        };
        ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, if busy { t.ember } else { edge }).shadow(if hov { 2 } else { 0 }));
        if it.purity != "sfw" && !it.purity.is_empty() {
            ui.cv.outline(r.inset(1), edge, 1);
        }
        let inner = r.inset(3);
        match wh.thumbs.get(&it.id) {
            Some(Some(img)) => ui.cv.image(img, inner.x, inner.y, 1.0),
            Some(None) => ui.cv.icon_in("image", inner, 2, t.text_faint),
            None => ui.cv.icon_in("image", inner, 2, t.border),
        }
        if hov || busy {
            let band = R::new(inner.x, inner.bottom() - 18, inner.w, 18);
            ui.cv.fill(band, t.bg.alpha(0.8));
            let label = if busy { crate::i18n::tr("ставлю…").to_string() } else { it.res.clone() };
            ui.cv.text_in(ui.f, PX, R::new(band.x + 6, band.y, band.w - 8, band.h), &label, t.text);
        }
        ui.zone(r, Act::WhPick(it.id.clone()), Cur::Pointer);
    }
    // страницы
    let fy = b.bottom() - 28;
    ui.button(R::new(b.x, fy, 90, 26), "назад", "left", Tone::Plain, false, !wh.loading && wh.page > 1, Act::WhPage(-1));
    ui.button(R::new(b.right() - 90, fy, 90, 26), "дальше", "right", Tone::Plain, false, !wh.loading && wh.page < wh.last, Act::WhPage(1));
    ui.text_center(PX, R::new(b.x + 100, fy, b.w - 200, 26), &crate::tf!("страница {} из {}", "page {} of {}", wh.page, wh.last), t.text_dim);
}

impl App {
    /// открыть: первый раз — сразу искать (последние добавленные)
    pub fn wh_opened(&mut self) {
        if self.st.wh.items.is_empty() && !self.st.wh.loading {
            self.wh_search(1);
        }
    }

    pub fn wh_search(&mut self, page: u32) {
        let wh = &mut self.st.wh;
        wh.page = page.max(1);
        wh.loading = true;
        wh.err.clear();
        wh.gen += 1;
        let key = self.st.set.wallhaven_key.trim().to_string();
        let mut purity = wh.purity;
        purity[2] &= !key.is_empty();
        if !purity.iter().any(|x| *x) {
            purity[0] = true;
        }
        let mut cats = wh.cats;
        if !cats.iter().any(|x| *x) {
            cats = [true; 3];
        }
        let sort = SORTS[wh.sort].0;
        let q = wh.query.text.trim().to_string();
        // без запроса «по запросу» отдаёт пустую первую страницу (как замечено в плагине Noctalia)
        let skip = (sort == "relevance" && q.is_empty()) as usize;
        let mut p: Vec<(&'static str, String)> = vec![("categories", bits(&cats)), ("purity", bits(&purity)), ("sorting", sort.into()), ("order", if wh.desc { "desc" } else { "asc" }.into())];
        if !q.is_empty() {
            p.push(("q", q));
        }
        if !RATIOS[wh.ratio].0.is_empty() {
            p.push(("ratios", RATIOS[wh.ratio].0.into()));
        }
        let (w, h) = (wh.width.text.trim().parse::<u32>().ok(), wh.height.text.trim().parse::<u32>().ok());
        if let (Some(w), Some(h)) = (w.filter(|x| *x > 0), h.filter(|x| *x > 0)) {
            p.push((if wh.exact { "resolutions" } else { "atleast" }, format!("{w}x{h}")));
        }
        // своя страница — картинки [from, from + 20) общего списка: одна-две страницы API
        let from = (wh.page as usize - 1) * PER;
        let (a1, a2) = (from / API_PER + 1, (from + PER - 1) / API_PER + 1);
        let (gen, tx) = (wh.gen, self.tx.clone());
        std::thread::spawn(move || {
            let mut all: Vec<Item> = vec![];
            let mut total = 0;
            let mut err = None;
            for a in a1..=a2 {
                let mut q = p.clone();
                q.push(("page", (a + skip).to_string()));
                match search(&q, &key) {
                    Ok((items, t)) => {
                        all.extend(items);
                        total = t;
                    }
                    Err(e) => {
                        err = Some(e);
                        break;
                    }
                }
            }
            let off = from - (a1 - 1) * API_PER;
            let page: Vec<Item> = all.into_iter().skip(off).take(PER).collect();
            let total = (total as usize).saturating_sub(skip * API_PER);
            let last = total.div_ceil(PER).max(1) as u32;
            let r = match err {
                Some(e) if page.is_empty() => Err(e),
                _ => Ok((page, last)),
            };
            let items = r.as_ref().map(|x| x.0.clone()).unwrap_or_default();
            let _ = tx.send(Msg::WhResults(gen, r));
            // миниатюры по одной, ровно под окно клетки сетки
            let (tw, th) = thumb_size();
            for it in items {
                let img = thumb(&it, tw, th);
                if tx.send(Msg::WhThumb(gen, it.id.clone(), img)).is_err() {
                    return;
                }
            }
        });
        self.dirty(&SKind::Popup);
    }

    pub fn wh_results(&mut self, gen: u64, r: Result<(Vec<Item>, u32), String>) {
        let wh = &mut self.st.wh;
        if gen != wh.gen {
            return;
        }
        wh.loading = false;
        wh.thumbs.clear();
        match r {
            Ok((items, last)) => {
                wh.last = last;
                wh.items = items;
            }
            Err(e) => {
                wh.items.clear();
                wh.err = e;
            }
        }
        self.dirty(&SKind::Popup);
    }

    pub fn wh_thumb(&mut self, gen: u64, id: String, img: Option<Img>) {
        if gen == self.st.wh.gen {
            self.st.wh.thumbs.insert(id, img.map(Rc::new));
            self.dirty(&SKind::Popup);
        }
    }

    /// щелчок по миниатюре: скачать в папку обоев и поставить
    pub fn wh_pick(&mut self, id: &str) {
        let Some(it) = self.st.wh.items.iter().find(|i| i.id == id).cloned() else { return };
        if self.st.wh.applying.is_some() {
            return;
        }
        self.st.wh.applying = Some(it.id.clone());
        let dir = download_dir(&self.st);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ext = it.full.rsplit('.').next().filter(|e| matches!(*e, "jpg" | "jpeg" | "png" | "webp")).unwrap_or("jpg");
            let _ = std::fs::create_dir_all(&dir);
            let p = dir.join(format!("wallhaven-{}.{ext}", it.id));
            let r = if p.is_file() {
                Ok(p)
            } else {
                let tmp = p.with_extension("part");
                let ok = crate::proc::curl(180, 96 << 20).arg("-f").arg("-o").arg(&tmp).arg(&it.full).status().is_ok_and(|s| s.success());
                if ok && std::fs::rename(&tmp, &p).is_ok() {
                    Ok(p)
                } else {
                    let _ = std::fs::remove_file(&tmp);
                    Err(crate::i18n::tr("не скачалось").to_string())
                }
            };
            let _ = tx.send(Msg::WhApplied(r));
        });
        self.dirty(&SKind::Popup);
    }

    pub fn wh_applied(&mut self, r: Result<PathBuf, String>) {
        self.st.wh.applying = None;
        match r {
            Ok(p) => self.set_wall(&p.to_string_lossy()),
            Err(e) => self.st.wh.err = e,
        }
        self.dirty(&SKind::Popup);
    }

    /// клики всплывашки Wallhaven; true — обработано
    pub fn wh_click(&mut self, a: &Act) -> bool {
        let wh = &mut self.st.wh;
        match a {
            Act::WhSearch => {}
            Act::WhCat(i) => wh.cats[*i] = !wh.cats[*i],
            Act::WhPurity(i) => wh.purity[*i] = !wh.purity[*i],
            Act::WhOrder => wh.desc = !wh.desc,
            Act::WhExact => wh.exact = !wh.exact,
            Act::WhPage(d) => {
                let p = (wh.page as i32 + d).clamp(1, wh.last as i32) as u32;
                self.wh_search(p);
                return true;
            }
            Act::WhPick(id) => {
                let id = id.clone();
                self.wh_pick(&id);
                return true;
            }
            _ => return false,
        }
        self.st.field_focus = None;
        self.wh_search(1);
        true
    }


    /// ввод в полях поиска: Enter — искать
    pub fn wh_key(&mut self, e: &smithay_client_toolkit::seat::keyboard::KeyEvent) -> bool {
        use smithay_client_toolkit::seat::keyboard::Keysym;
        let Some(f) = self.st.field_focus.filter(|f| f.starts_with("wh_")) else { return false };
        match e.keysym {
            Keysym::Escape => self.st.field_focus = None,
            Keysym::Return | Keysym::KP_Enter => {
                self.st.field_focus = None;
                self.wh_search(1);
            }
            _ => {
                let mods = self.mods;
                let field = match f {
                    "wh_w" => &mut self.st.wh.width,
                    "wh_h" => &mut self.st.wh.height,
                    _ => &mut self.st.wh.query,
                };
                App::edit_field(field, e, &mods);
            }
        }
        self.dirty(&SKind::Popup);
        true
    }
}
