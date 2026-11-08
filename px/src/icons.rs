// Иконки из темы (freedesktop: index.theme, Inherits, каталоги по размерам) и программы из
// .desktop-файлов. Иконка рисуется один раз нужного размера и живёт в кэше.

use crate::gfx::Img;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

struct ThemeDir {
    path: PathBuf,
    size: i32,
    scalable: bool,
    /// какая это тема в цепочке Inherits (0 — выбранная): значок берётся из первой, где он есть
    theme: u16,
}

pub struct Icons {
    dirs: Vec<ThemeDir>,
    found: HashMap<(String, i32), Option<PathBuf>>,
    imgs: HashMap<(String, i32), Option<std::rc::Rc<Img>>>,
    /// тона палитры: обводка и 4 тона от тёмного к светлому (обводка пиксельных значков и тона
    /// при tint, запасной значок)
    ramp: [u32; 5],
    /// значки программ пиксельными (iconPixel)
    pixel: bool,
    /// пиксельные — в тонах палитры, а не в своих цветах (iconTint)
    tint: bool,
    /// пиксельные копии чужих картинок (значки трея) по отпечатку картинки и вида
    pixeled: HashMap<u64, std::rc::Rc<Img>>,
}

pub fn icon_roots() -> Vec<PathBuf> {
    let h = crate::paths::home();
    let mut v = vec![crate::paths::data_dir().join("icons"), h.join(".icons")];
    v.extend(crate::paths::data_dirs().into_iter().map(|d| d.join("icons")));
    v.push("/var/lib/flatpak/exports/share/icons".into());
    v
}

pub fn parse_ini(text: &str) -> Vec<(String, HashMap<String, String>)> {
    let mut out: Vec<(String, HashMap<String, String>)> = vec![];
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') && l.ends_with(']') {
            out.push((l[1..l.len() - 1].to_string(), HashMap::new()));
        } else if let Some((k, v)) = l.split_once('=') {
            if let Some(last) = out.last_mut() {
                last.1.entry(k.trim().to_string()).or_insert_with(|| v.trim().to_string());
            }
        }
    }
    out
}

/// набор значков системы, когда в настройках «авто»: тот, что выбран для GTK (settings.ini), иначе
/// у KDE (kdeglobals), иначе Adwaita. gsettings не спрашиваем: это процесс на каждом старте
pub fn system_theme() -> String {
    let cfg = crate::paths::config_dir();
    let mut got = None;
    for v in ["gtk-4.0", "gtk-3.0"] {
        let Ok(t) = std::fs::read_to_string(cfg.join(v).join("settings.ini")) else { continue };
        got = t.lines().find_map(|l| l.trim().strip_prefix("gtk-icon-theme-name").map(|r| r.trim_start().trim_start_matches('=').trim().trim_matches('"').to_string())).filter(|s| !s.is_empty());
        if got.is_some() {
            break;
        }
    }
    if got.is_none() {
        if let Ok(t) = std::fs::read_to_string(cfg.join("kdeglobals")) {
            got = parse_ini(&t).into_iter().find(|(n, _)| n == "Icons").and_then(|(_, m)| m.get("Theme").cloned()).filter(|s| !s.is_empty());
        }
    }
    let roots = icon_roots();
    let have = |t: &str| roots.iter().any(|r| r.join(t).join("index.theme").exists());
    got.filter(|t| have(t)).unwrap_or_else(|| if have("Adwaita") { "Adwaita".into() } else { "hicolor".into() })
}

impl Icons {
    pub fn new(theme: &str) -> Icons {
        let mut ic = Icons { dirs: vec![], found: HashMap::new(), imgs: HashMap::new(), ramp: [0xff000000, 0xff404040, 0xff808080, 0xffb0b0b0, 0xffe0e0e0], pixel: false, tint: false, pixeled: HashMap::new() };
        let roots = icon_roots();
        let mut order: Vec<String> = vec![];
        let mut queue: Vec<String> = vec![if theme.is_empty() { system_theme() } else { theme.to_string() }];
        while let Some(t) = queue.pop() {
            if order.contains(&t) || t.is_empty() || t.contains('/') {
                continue;
            }
            order.push(t.clone());
            let n = order.len() as u16 - 1;
            // index.theme — из первого корня, где он есть; папки по нему ищутся во всех корнях
            // (свои значки в ~/.local/share/icons/hicolor живут без своего index.theme)
            let Some(txt) = roots.iter().find_map(|r| std::fs::read_to_string(r.join(&t).join("index.theme")).ok()) else {
                if queue.is_empty() && !order.iter().any(|x| x == "hicolor") {
                    queue.push("hicolor".into());
                }
                continue;
            };
            let ini = parse_ini(&txt);
            let head = ini.iter().find(|(n, _)| n == "Icon Theme");
            let mut inherits: Vec<String> = head.and_then(|(_, m)| m.get("Inherits")).map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
            inherits.reverse();
            queue.extend(inherits);
            let dirs: Vec<String> = head.and_then(|(_, m)| m.get("Directories")).map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
            for d in dirs {
                let Some((_, m)) = ini.iter().find(|(n, _)| *n == d) else { continue };
                let size = m.get("Size").and_then(|s| s.parse().ok()).unwrap_or(48);
                let scale: i32 = m.get("Scale").and_then(|s| s.parse().ok()).unwrap_or(1);
                if scale != 1 {
                    continue;
                }
                let ctx = m.get("Context").map(|s| s.as_str()).unwrap_or("");
                if !matches!(ctx, "Applications" | "Apps" | "Status" | "Devices" | "Places" | "Actions" | "Panel" | "Categories" | "") {
                    continue;
                }
                let scalable = m.get("Type").map(|t| t == "Scalable").unwrap_or(false);
                for r in &roots {
                    let path = r.join(&t).join(&d);
                    if path.is_dir() {
                        ic.dirs.push(ThemeDir { path, size, scalable, theme: n });
                    }
                }
            }
            if queue.is_empty() && !order.iter().any(|x| x == "hicolor") {
                queue.push("hicolor".into());
            }
        }
        ic.dirs.push(ThemeDir { path: "/usr/share/pixmaps".into(), size: 48, scalable: false, theme: u16::MAX });
        ic
    }

    /// файл значка под размер want: первая тема цепочки, где он есть, а в ней — ровно этого
    /// размера, векторный (мелким — после ручных 16–24 px) или ближайший крупнее; мельче
    /// нужного — в последнюю очередь (растянутый вышел бы мыльным или кубиками)
    pub fn find(&mut self, name: &str, want: i32) -> Option<PathBuf> {
        if name.is_empty() {
            return None;
        }
        let key = (name.to_string(), want);
        if let Some(p) = self.found.get(&key) {
            return p.clone();
        }
        let r = if name.starts_with('/') {
            Path::new(name).exists().then(|| PathBuf::from(name))
        } else {
            let mut best: Option<(i32, u16, PathBuf)> = None;
            for d in &self.dirs {
                if let Some((_, th, _)) = &best {
                    if *th != d.theme {
                        break;
                    }
                }
                let Some(p) = ["svg", "png"].iter().map(|e| d.path.join(format!("{name}.{e}"))).find(|p| p.exists()) else { continue };
                let score = if d.scalable {
                    if want > 24 { 0 } else { 3 }
                } else if d.size == want {
                    0
                } else if d.size > want {
                    1 + (d.size - want) / 16
                } else {
                    100 + want - d.size
                };
                if best.as_ref().map(|(s, _, _)| score < *s).unwrap_or(true) {
                    best = Some((score, d.theme, p));
                }
            }
            best.map(|b| b.2)
        };
        self.found.insert(key, r.clone());
        r
    }

    /// иконка по имени (или пути) нужного размера; нет — None
    pub fn get(&mut self, name: &str, size: i32) -> Option<std::rc::Rc<Img>> {
        let key = (name.to_string(), size);
        if let Some(i) = self.imgs.get(&key) {
            return i.clone();
        }
        let img = self.find(name, size).and_then(|p| load_file(&p, size)).map(std::rc::Rc::new);
        self.imgs.insert(key, img.clone());
        img
    }

    /// значок из файла (IconThemePath у программ трея): кеш по пути и времени изменения, чтобы
    /// не читать и не декодировать его на каждой перерисовке бара
    pub fn file(&mut self, p: &Path, size: i32) -> Option<std::rc::Rc<Img>> {
        let at = std::fs::metadata(p).ok()?.modified().ok()?;
        let key = (format!("\u{1}{}\u{1}{at:?}", p.display()), size);
        if let Some(i) = self.imgs.get(&key) {
            return i.clone();
        }
        let img = load_file(p, size).map(std::rc::Rc::new);
        self.imgs.insert(key, img.clone());
        img
    }

    /// вид значков программ: тона палитры (ramp), пиксельные ли (iconPixel), в тонах палитры ли (iconTint)
    pub fn set_style(&mut self, ramp: [u32; 5], pixel: bool, tint: bool) {
        if (self.ramp, self.pixel, self.tint) != (ramp, pixel, tint) {
            (self.ramp, self.pixel, self.tint) = (ramp, pixel, tint);
            self.imgs.retain(|k, _| !k.0.starts_with('\0'));
            self.pixeled.clear();
        }
    }

    /// готовая картинка (значок трея) пиксельной, как значки программ; копии помнятся по
    /// отпечатку пикселей и вида, чтобы бар не пересчитывал их каждую секунду
    pub fn pixeled(&mut self, img: &Img, size: i32) -> std::rc::Rc<Img> {
        let mut h = crate::fnv(crate::FNV0, &[size as u32, self.tint as u32].map(u32::to_le_bytes).concat());
        for p in self.ramp.iter().chain(img.px.iter()) {
            h = crate::fnv(h, &p.to_le_bytes());
        }
        if let Some(i) = self.pixeled.get(&h) {
            return i.clone();
        }
        // значки трея меняются (мигают, показывают счёт): старые копии не копить
        if self.pixeled.len() > 64 {
            self.pixeled.clear();
        }
        let out = std::rc::Rc::new(pixelate(img, size, &self.ramp, self.tint));
        self.pixeled.insert(h, out.clone());
        out
    }

    /// значок программы: как есть или пиксельный (чистая сетка, см. pixelate); нет ни в теме, ни
    /// запасного — простое окошко в тонах палитры, чтобы в доке не было дыры
    pub fn app(&mut self, name: &str, size: i32) -> Option<std::rc::Rc<Img>> {
        if !self.pixel {
            return self.app_raw(name, size).or_else(|| self.generic(size));
        }
        let key = (format!("\0{name}"), size);
        if let Some(i) = self.imgs.get(&key) {
            return i.clone();
        }
        // исходник крупный и в кэш не идёт: 128² на программу — лишняя память
        let (g, _) = grid(size);
        let src = (g * 4).clamp(64, 128);
        let img = self.app_path(name, src).and_then(|p| load_file(&p, src)).map(|r| std::rc::Rc::new(pixelate(&r, size, &self.ramp, self.tint))).or_else(|| self.generic(size));
        self.imgs.insert(key, img.clone());
        img
    }

    /// файл значка программы с запасными вариантами: имя, в нижнем регистре, «программа»
    fn app_path(&mut self, name: &str, size: i32) -> Option<PathBuf> {
        let low = name.to_lowercase();
        self.find(name, size).or_else(|| if low != name { self.find(&low, size) } else { None }).or_else(|| self.find("application-x-executable", size))
    }

    fn app_raw(&mut self, name: &str, size: i32) -> Option<std::rc::Rc<Img>> {
        let key = (name.to_string(), size);
        if let Some(i) = self.imgs.get(&key) {
            return i.clone();
        }
        let img = self.app_path(name, size).and_then(|p| load_file(&p, size)).map(std::rc::Rc::new);
        self.imgs.insert(key, img.clone());
        img
    }

    /// запасной значок: окошко с полосой заголовка, клетками и в тонах палитры
    fn generic(&mut self, size: i32) -> Option<std::rc::Rc<Img>> {
        const ART: [&str; 12] = ["oooooooooooo", "o3333333232o", "o3333333333o", "oooooooooooo", "o1111111111o", "o1222222221o", "o1211111121o", "o1211111121o", "o1222222221o", "o1111111111o", "o1111111111o", "oooooooooooo"];
        let key = ("\0\0generic".to_string(), size);
        if let Some(i) = self.imgs.get(&key) {
            return i.clone();
        }
        let cell = (size / 14).max(1);
        let off = (size - 12 * cell) / 2;
        let mut px = vec![0u32; (size * size).max(0) as usize];
        for (y, row) in ART.iter().enumerate() {
            for (x, ch) in row.bytes().enumerate() {
                let c = match ch {
                    b'o' => self.ramp[0],
                    b'1' => self.ramp[1],
                    b'2' => self.ramp[2],
                    _ => self.ramp[3],
                } | 0xff000000;
                for dy in 0..cell {
                    for dx in 0..cell {
                        let (px_, py) = (off + x as i32 * cell + dx, off + y as i32 * cell + dy);
                        if px_ >= 0 && py >= 0 && px_ < size && py < size {
                            px[(py * size + px_) as usize] = c;
                        }
                    }
                }
            }
        }
        let img = (size > 0).then(|| std::rc::Rc::new(Img { w: size, h: size, px }));
        self.imgs.insert(key, img.clone());
        img
    }

    pub fn drop_cache(&mut self) {
        self.imgs.clear();
        self.found.clear();
    }
}

/// сетка пиксельного значка размера size: (клеток по стороне, px в клетке). Клетка растёт
/// ступенями, чтобы значок в любом месте читался как рисунок 16–30 клеток, а не как мыло
pub fn grid(size: i32) -> (i32, i32) {
    let cell = match size {
        ..=29 => 1,
        30..=59 => 2,
        60..=89 => 3,
        _ => 4,
    };
    ((size / cell).max(1), cell)
}

/// картинка → чистый пиксельный значок size×size. Сетка из grid(size) усреднением по площади
/// (не выборкой: края не рвутся), жёсткий силуэт без одиночных выступов и дырок, свои цвета
/// значка, сведённые медианным сечением к 5–10, или 4 тона палитры (tint) — без дизеринга,
/// тёмная обводка снаружи (ramp[0]); клетки по целому множителю, по центру
pub fn pixelate(src: &Img, size: i32, ramp: &[u32; 5], tint: bool) -> Img {
    let (g, cell) = grid(size);
    let (w, h) = (src.w.max(1), src.h.max(1));
    let gu = g as usize;
    // 1. усреднение по площади (предумноженные каналы)
    let mut cells = vec![[0u32; 4]; gu * gu];
    for ty in 0..g {
        let (y0, y1) = (ty * h / g, ((ty + 1) * h / g).max(ty * h / g + 1).min(h));
        for tx in 0..g {
            let (x0, x1) = (tx * w / g, ((tx + 1) * w / g).max(tx * w / g + 1).min(w));
            let mut s = [0u32; 4];
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = src.px.get((y * src.w + x) as usize).copied().unwrap_or(0);
                    s[0] += p >> 24;
                    s[1] += (p >> 16) & 255;
                    s[2] += (p >> 8) & 255;
                    s[3] += p & 255;
                    n += 1;
                }
            }
            cells[ty as usize * gu + tx as usize] = if n > 0 { s.map(|v| v / n) } else { [0; 4] };
        }
    }
    // 2. силуэт: половина покрытия; одиночные выступы срезать, одиночные дырки залить
    let raw: Vec<bool> = cells.iter().map(|c| c[0] >= 128).collect();
    let at = |m: &[bool], x: i32, y: i32| x >= 0 && y >= 0 && x < g && y < g && m[(y * g + x) as usize];
    let n4 = |m: &[bool], x: i32, y: i32| at(m, x - 1, y) as u8 + at(m, x + 1, y) as u8 + at(m, x, y - 1) as u8 + at(m, x, y + 1) as u8;
    let mut solid = raw.clone();
    for y in 0..g {
        for x in 0..g {
            let k = n4(&raw, x, y);
            let i = (y * g + x) as usize;
            if raw[i] && k <= 1 {
                solid[i] = false;
            } else if !raw[i] && k >= 3 {
                solid[i] = true;
            }
        }
    }
    // 3. цвета: свои (без предумножения), медианное сечение до k
    let mut cols: Vec<[u8; 3]> = Vec::new();
    let mut at_col: Vec<usize> = vec![usize::MAX; gu * gu];
    for (i, c) in cells.iter().enumerate() {
        if solid[i] {
            let a = c[0].max(1);
            let un = |v: u32| (v * 255 / a).min(255) as u8;
            at_col[i] = cols.len();
            cols.push([un(c[1]), un(c[2]), un(c[3])]);
        }
    }
    let k = (g / 3).clamp(5, 10) as usize;
    let (pal, which) = median_cut(&cols, k);
    let lum = |c: &[u8; 3]| 299 * c[0] as u32 + 587 * c[1] as u32 + 114 * c[2] as u32;
    let tone: Vec<u32> = if tint {
        // тона по яркости: самый тёмный цвет значка — ramp[1], самый светлый — ramp[4]
        let mut idx: Vec<usize> = (0..pal.len()).collect();
        idx.sort_by_key(|&i| lum(&pal[i]));
        let n = idx.len().max(2) - 1;
        let mut t = vec![0u32; pal.len()];
        for (r, &i) in idx.iter().enumerate() {
            t[i] = ramp[1 + (r * 3 + n / 2) / n] | 0xff000000;
        }
        t
    } else {
        pal.iter().map(|c| 0xff000000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect()
    };
    // 4. клетки по cell px, обводка снаружи силуэта
    let edge = ramp[0] | 0xff000000;
    let off = (size - g * cell) / 2;
    let mut px = vec![0u32; (size * size).max(0) as usize];
    for y in 0..g {
        for x in 0..g {
            let i = (y * g + x) as usize;
            let c = if solid[i] {
                which.get(at_col[i]).and_then(|&j| tone.get(j)).copied().unwrap_or(edge)
            } else if at(&solid, x - 1, y) || at(&solid, x + 1, y) || at(&solid, x, y - 1) || at(&solid, x, y + 1) {
                edge
            } else {
                continue;
            };
            for dy in 0..cell {
                let row = (off + y * cell + dy) * size + off + x * cell;
                for dx in 0..cell {
                    if let Some(p) = px.get_mut((row + dx) as usize) {
                        *p = c;
                    }
                }
            }
        }
    }
    Img { w: size, h: size, px }
}

/// медианное сечение: до k цветов (среднее каждой коробки) и номер коробки у каждого цвета
fn median_cut(cols: &[[u8; 3]], k: usize) -> (Vec<[u8; 3]>, Vec<usize>) {
    if cols.is_empty() {
        return (vec![], vec![]);
    }
    let range = |b: &[usize]| {
        (0..3)
            .map(|ch| {
                let (lo, hi) = b.iter().fold((255u8, 0u8), |(lo, hi), &i| (lo.min(cols[i][ch]), hi.max(cols[i][ch])));
                (hi.saturating_sub(lo), ch)
            })
            .max()
            .unwrap_or((0, 0))
    };
    let mut boxes: Vec<Vec<usize>> = vec![(0..cols.len()).collect()];
    while boxes.len() < k {
        // делится коробка с самым широким разбросом: редкие, но яркие цвета получают свою
        let Some((bi, (r, ch))) = boxes.iter().enumerate().filter(|(_, b)| b.len() > 1).map(|(i, b)| (i, range(b))).max_by_key(|(_, (r, _))| *r) else { break };
        if r < 8 {
            break;
        }
        let mut b = boxes.swap_remove(bi);
        b.sort_by_key(|&i| cols[i][ch]);
        let rest = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(rest);
    }
    let mut which = vec![0usize; cols.len()];
    for (bi, b) in boxes.iter().enumerate() {
        for &i in b {
            which[i] = bi;
        }
    }
    // средние коробок, потом три прохода k-средних: каждый цвет — к ближайшему, а не к своей
    // коробке (иначе редкий блик смешивался с соседями в бурую кашу)
    let mut pal = vec![[0u8; 3]; boxes.len()];
    let dist = |a: &[u8; 3], b: &[u8; 3]| {
        let d = |c: usize, w: i32| w * (a[c] as i32 - b[c] as i32).pow(2);
        d(0, 3) + d(1, 4) + d(2, 2)
    };
    for pass in 0..4 {
        let mut sum = vec![[0u32; 4]; pal.len()];
        for (i, c) in cols.iter().enumerate() {
            let s = &mut sum[which[i]];
            for ch in 0..3 {
                s[ch] += c[ch] as u32;
            }
            s[3] += 1;
        }
        for (j, s) in sum.iter().enumerate() {
            if s[3] > 0 {
                pal[j] = [s[0] / s[3], s[1] / s[3], s[2] / s[3]].map(|v| v as u8);
            }
        }
        if pass == 3 {
            break;
        }
        for (i, c) in cols.iter().enumerate() {
            which[i] = (0..pal.len()).min_by_key(|&j| dist(c, &pal[j])).unwrap_or(0);
        }
    }
    (pal, which)
}

/// потолки для картинок со стороны (значки уведомлений, обложки, значки трея): файл и память декодера
pub const MAX_FILE: u64 = 16 << 20;
pub const MAX_DECODE: u64 = 64 << 20;

/// картинка из байтов с потолком памяти и размеров: «бомба» (100000×100000 в маленьком png) не
/// раздует процесс, декодер просто откажется
pub fn decode(data: &[u8], max_alloc: u64) -> Option<image::DynamicImage> {
    let mut rd = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format().ok()?;
    let mut lim = image::Limits::default();
    lim.max_alloc = Some(max_alloc);
    lim.max_image_width = Some(8192);
    lim.max_image_height = Some(8192);
    rd.limits(lim);
    rd.decode().ok()
}

/// кеш миниатюр: не открытые 120 дней уходят, и всего не больше 512 МБ (раз в сутки, из нити)
pub fn thumbs_tidy() {
    crate::paths::tidy(&crate::paths::cache_dir().join("yami/thumbs"), 120, 512 << 20);
}

/// где лежит миниатюра (~/.cache/yami/thumbs): ключ — путь, время изменения и размер
pub fn thumb_path(path: &Path, mtime: i64, size: i32) -> PathBuf {
    let h = crate::fnv(crate::FNV0, format!("{}|{}|{}", path.display(), mtime, size).as_bytes());
    crate::paths::cache_dir().join("yami/thumbs").join(format!("{h:016x}.png"))
}

/// миниатюра картинки, не больше size по большей стороне. Кеш общий у yami-files и выбора обоев:
/// второй раз картинка не декодируется. Гигантские (больше 40 Мпикс) не декодируются вовсе
pub fn thumb(path: &Path, mtime: i64, size: i32) -> Option<image::RgbaImage> {
    let cp = thumb_path(path, mtime, size);
    if let Ok(img) = image::open(&cp) {
        // нужная миниатюра молодеет (раз в неделю, не на каждое чтение): уборка кеша
        // (paths::tidy) убирает по времени изменения только то, что давно не открывали
        if let Ok(m) = std::fs::metadata(&cp).and_then(|m| m.modified()) {
            if m.elapsed().map(|d| d.as_secs() > 7 * 86_400).unwrap_or(false) {
                let _ = std::fs::File::options().write(true).open(&cp).and_then(|f| f.set_modified(std::time::SystemTime::now()));
            }
        }
        return Some(img.into_rgba8());
    }
    let rd = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let (w, h) = rd.into_dimensions().ok()?;
    if (w as u64) * (h as u64) > 40_000_000 {
        return None;
    }
    let mut rd = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let mut lim = image::Limits::default();
    lim.max_alloc = Some(256 << 20);
    rd.limits(lim);
    let t = rd.decode().ok()?.thumbnail(size as u32, size as u32).to_rgba8();
    let _ = std::fs::create_dir_all(cp.parent()?);
    let _ = t.save(&cp);
    Some(t)
}

pub fn load_file(p: &Path, size: i32) -> Option<Img> {
    let data = crate::paths::read_capped(p, MAX_FILE)?;
    if p.extension().map(|e| e == "svg" || e == "svgz").unwrap_or(false) {
        return svg(&data, size);
    }
    Some(fit(decode(&data, MAX_DECODE)?.to_rgba8(), size))
}

pub fn svg(data: &[u8], size: i32) -> Option<Img> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let s = tree.size();
    let k = size as f32 / s.width().max(s.height());
    let mut pm = tiny_skia::Pixmap::new(size as u32, size as u32)?;
    let dx = (size as f32 - s.width() * k) / 2.0;
    let dy = (size as f32 - s.height() * k) / 2.0;
    resvg::render(&tree, tiny_skia::Transform::from_row(k, 0.0, 0.0, k, dx, dy), &mut pm.as_mut());
    let px = pm.data().chunks_exact(4).map(|c| (c[3] as u32) << 24 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect();
    Some(Img { w: size, h: size, px })
}

/// вписать в квадрат size без сглаживания (пиксели остаются пикселями)
/// вписать в mw×mh с сохранением пропорций, без полей (превью скриншота)
pub fn fit_in(img: image::RgbaImage, mw: i32, mh: i32) -> Img {
    let (w, h) = (img.width().max(1) as f32, img.height().max(1) as f32);
    let k = (mw as f32 / w).min(mh as f32 / h).min(1.0);
    let (nw, nh) = (((w * k).round() as u32).max(1), ((h * k).round() as u32).max(1));
    let r = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    let px = r
        .pixels()
        .map(|p| {
            let [cr, cg, cb, ca] = p.0;
            let a = ca as u32;
            let m = |v: u8| (v as u32 * a + 127) / 255;
            a << 24 | m(cr) << 16 | m(cg) << 8 | m(cb)
        })
        .collect();
    Img { w: nw as i32, h: nh as i32, px }
}

pub fn fit(img: image::RgbaImage, size: i32) -> Img {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let k = size as f32 / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * k).round() as i32).max(1), ((h as f32 * k).round() as i32).max(1));
    let filter = if k < 1.0 { image::imageops::FilterType::Triangle } else { image::imageops::FilterType::Nearest };
    let r = image::imageops::resize(&img, nw as u32, nh as u32, filter);
    let mut px = vec![0u32; (size * size) as usize];
    let (ox, oy) = ((size - nw) / 2, (size - nh) / 2);
    for (x, y, p) in r.enumerate_pixels() {
        let [cr, cg, cb, ca] = p.0;
        let a = ca as u32;
        let m = |v: u8| (v as u32 * a + 127) / 255;
        px[((y as i32 + oy) * size + x as i32 + ox) as usize] = a << 24 | m(cr) << 16 | m(cg) << 8 | m(cb);
    }
    Img { w: size, h: size, px }
}

/// картинка как есть (обои, обложки), в предумноженный ARGB
pub fn from_rgba(img: &image::RgbaImage) -> Img {
    let px = img
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            let a32 = a as u32;
            let m = |v: u8| (v as u32 * a32 + 127) / 255;
            a32 << 24 | m(r) << 16 | m(g) << 8 | m(b)
        })
        .collect();
    Img { w: img.width() as i32, h: img.height() as i32, px }
}

// ── программы ───────────────────────────────────────────────────────────────
#[derive(Clone, Debug, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    pub generic: String,
    pub comment: String,
    pub keywords: String,
    pub icon: String,
    pub exec: String,
    pub terminal: bool,
    pub wm_class: String,
    /// MimeType из .desktop: что программа умеет открывать
    pub mime: Vec<String>,
    /// в Exec есть %f/%F/%u/%U — принимает файлы
    pub takes_files: bool,
}

fn app_dirs() -> Vec<PathBuf> {
    let mut v = vec![crate::paths::data_dir().join("applications")];
    v.extend(crate::paths::data_dirs().into_iter().map(|d| d.join("applications")));
    v.push("/var/lib/flatpak/exports/share/applications".into());
    v.push(crate::paths::data_dir().join("flatpak/exports/share/applications"));
    v
}

fn localized(m: &HashMap<String, String>, key: &str) -> String {
    let lang = std::env::var("LANG").unwrap_or_default();
    let short: String = lang.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    for k in [format!("{key}[{}]", lang.split('.').next().unwrap_or("")), format!("{key}[{short}]"), key.to_string()] {
        if let Some(v) = m.get(&k) {
            return v.clone();
        }
    }
    String::new()
}

pub fn load_apps() -> Vec<App> {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut out = vec![];
    for d in app_dirs() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        // подкаталоги (kde4/…) — на один уровень
        let subs: Vec<PathBuf> = files.iter().filter(|p| p.is_dir()).cloned().collect();
        for s in subs {
            if let Ok(rd) = std::fs::read_dir(&s) {
                files.extend(rd.flatten().map(|e| e.path()));
            }
        }
        for p in files {
            if p.extension().map(|e| e != "desktop").unwrap_or(true) {
                continue;
            }
            let id = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if seen.contains_key(&id) {
                continue;
            }
            seen.insert(id.clone(), ());
            let Ok(txt) = std::fs::read_to_string(&p) else { continue };
            let ini = parse_ini(&txt);
            let Some((_, m)) = ini.iter().find(|(n, _)| n == "Desktop Entry") else { continue };
            if m.get("Type").map(|t| t != "Application").unwrap_or(false) {
                continue;
            }
            let yes = |k: &str| m.get(k).map(|v| v == "true").unwrap_or(false);
            if yes("NoDisplay") || yes("Hidden") {
                continue;
            }
            if let Some(only) = m.get("OnlyShowIn") {
                if !only.split(';').any(|x| x == "niri" || x.is_empty()) {
                    continue;
                }
            }
            out.push(App {
                id,
                name: localized(m, "Name"),
                generic: localized(m, "GenericName"),
                comment: localized(m, "Comment"),
                keywords: localized(m, "Keywords").replace(';', " "),
                icon: m.get("Icon").cloned().unwrap_or_default(),
                exec: m.get("Exec").cloned().unwrap_or_default(),
                terminal: yes("Terminal"),
                wm_class: m.get("StartupWMClass").cloned().unwrap_or_default(),
                mime: m.get("MimeType").map(|v| v.split(';').filter(|x| !x.is_empty()).map(|x| x.to_string()).collect()).unwrap_or_default(),
                takes_files: m.get("Exec").map(|e| ["%f", "%F", "%u", "%U"].iter().any(|c| e.contains(c))).unwrap_or(false),
            });
        }
    }
    // ключ — один раз на программу, а не две строки на каждое сравнение
    out.sort_by_cached_key(|a| a.name.to_lowercase());
    out
}

/// иконка окна по app_id: .desktop с тем же именем, StartupWMClass, последний кусок id
pub fn icon_for(apps: &[App], app_id: &str) -> String {
    if app_id.is_empty() {
        return String::new();
    }
    if let Some(n) = app_id.strip_prefix("steam_app_") {
        return format!("steam_icon_{n}");
    }
    let low = app_id.to_lowercase();
    let tail = low.rsplit('.').next().unwrap_or(&low).to_string();
    let hit = apps
        .iter()
        .find(|a| a.id.to_lowercase() == low)
        .or_else(|| apps.iter().find(|a| a.wm_class.to_lowercase() == low))
        .or_else(|| apps.iter().find(|a| a.id.to_lowercase().rsplit('.').next() == Some(&tail)))
        .or_else(|| apps.iter().find(|a| a.name.to_lowercase() == low));
    match hit {
        Some(a) if !a.icon.is_empty() => a.icon.clone(),
        _ => app_id.to_string(),
    }
}

/// Exec из .desktop → слова: кавычки и \\-экранирование внутри них (спецификация Desktop Entry).
/// Без оболочки: программа запускается напрямую, имена файлов не разбираются как команды.
fn exec_words(exec: &str) -> Vec<String> {
    let mut v = vec![];
    let mut cur = String::new();
    let mut q = false;
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => q = !q,
            '\\' if q => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ' ' if !q => {
                if !cur.is_empty() {
                    v.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        v.push(cur);
    }
    v
}

/// команда из Exec без %-кодов
pub fn exec_argv(exec: &str) -> Vec<String> {
    exec_words(exec).into_iter().filter(|a| !(a.len() == 2 && a.starts_with('%'))).map(|a| a.replace("%%", "%")).collect()
}

/// команда из Exec с файлами: %f/%u — по одному (вернёт несколько команд), %F/%U — все разом;
/// кодов нет — файлы в конец
pub fn exec_with(exec: &str, files: &[PathBuf]) -> Vec<Vec<String>> {
    let raw = exec_words(exec);
    let uri = |p: &PathBuf| file_uri(p);
    let path = |p: &PathBuf| p.to_string_lossy().to_string();
    let multi = raw.iter().any(|a| a == "%F" || a == "%U");
    let single = raw.iter().any(|a| a == "%f" || a == "%u");
    let build = |set: &[PathBuf]| -> Vec<String> {
        let mut out = vec![];
        for a in &raw {
            match a.as_str() {
                "%F" => out.extend(set.iter().map(path)),
                "%U" => out.extend(set.iter().map(uri)),
                "%f" => out.extend(set.first().map(path)),
                "%u" => out.extend(set.first().map(uri)),
                x if x.len() == 2 && x.starts_with('%') => {}
                x => out.push(x.replace("%%", "%")),
            }
        }
        out
    };
    if multi || files.is_empty() {
        vec![build(files)]
    } else if single {
        files.iter().map(|f| build(std::slice::from_ref(f))).collect()
    } else {
        let mut v = build(&[]);
        v.extend(files.iter().map(path));
        vec![v]
    }
}

/// file:// с процентным кодированием
pub fn file_uri(p: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut s = String::from("file://");
    // байты пути как есть: имя не в UTF-8 не превращается в «�»
    for &b in p.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// обратно из file:// (другие схемы — None)
pub fn uri_path(u: &str) -> Option<PathBuf> {
    let rest = u.trim().strip_prefix("file://")?;
    let rest = if rest.starts_with('/') { rest } else { rest.split_once('/').map(|(_, r)| r).unwrap_or("") };
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len() + 1);
    if !rest.starts_with('/') {
        out.push(b'/');
    }
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(out)))
}
