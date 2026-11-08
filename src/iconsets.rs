// Наборы значков и курсоров: какие установлены (~/.local/share/icons, ~/.icons, системные), превью
// для настроек → вид (значки — шесть программ набора в выбранном виде, курсор — его стрелка из
// файла Xcursor), свой набор из архива или папки в ~/.local/share/icons; yami icons / yami cursor.
// Превью строит нить (Msg::Sets), пока открыта вкладка «вид»; настройки закрылись — память
// отдаётся (sets_free). Применяет выбор tools/theme.py: --icons (GTK, Qt), --cursor (niri, GTK, X11).

use crate::app::*;
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, R};
use crate::ui::{Act, Cur, Tone, Ui};
use std::path::{Path, PathBuf};

/// установленный набор: значки (index.theme с Directories) и/или курсоры (папка cursors)
pub struct Found {
    pub name: String,
    pub title: String,
    pub dir: PathBuf,
    pub icons: bool,
    pub cursors: bool,
}

/// карточка выбора: имя для настроек, подпись, превью
pub struct Card {
    pub name: String,
    pub title: String,
    pub imgs: Vec<Img>,
}

#[derive(Default)]
pub struct Sets {
    pub icons: Vec<Card>,
    pub cursors: Vec<Card>,
}

/// имя набора, которое можно отдать в settings.ini, qt6ct и KDL: без кавычек, разделителей и
/// служебных символов (config.rs sanitize чистит так же), не скрытое и не наше сгенерированное
fn good_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && !n.starts_with('.')
        && !n.chars().any(|c| c.is_control() || "/\"\\'`$;,{}<>|&#=".contains(c))
        && !matches!(n, "default" | "hicolor" | "locolor" | "yami-cursor")
        && !generated(n)
}

/// пиксельный курсор, который theme.py рисует в палитре: yami-<акцент>[-l]
fn generated(n: &str) -> bool {
    let Some(rest) = n.strip_prefix("yami-") else { return false };
    let hex = rest.strip_suffix("-l").unwrap_or(rest);
    hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn installed() -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for root in crate::icons::icon_roots() {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        let mut here: Vec<Found> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let dir = e.path();
                if !good_name(&name) || !dir.is_dir() {
                    return None;
                }
                let txt = yami_px::paths::read_capped(&dir.join("index.theme"), 1 << 20).map(|d| String::from_utf8_lossy(&d).into_owned()).unwrap_or_default();
                let ini = crate::icons::parse_ini(&txt);
                let head = ini.iter().find(|(n, _)| n == "Icon Theme").map(|(_, m)| m);
                // Hidden прячет набор из выбора; Adwaita — основа GTK и прячется тоже, но выбрать её нужно
                let hidden = name != "Adwaita" && head.and_then(|m| m.get("Hidden")).map(|v| v == "true").unwrap_or(false);
                let icons = !hidden && head.and_then(|m| m.get("Directories")).map(|d| !d.trim().is_empty()).unwrap_or(false);
                let cursors = dir.join("cursors").is_dir();
                if !icons && !cursors {
                    return None;
                }
                let title = head.and_then(|m| m.get("Name")).map(|s| s.chars().take(40).collect()).filter(|s: &String| !s.is_empty()).unwrap_or_else(|| name.clone());
                Some(Found { name, title, dir, icons, cursors })
            })
            .collect();
        here.sort_by(|a, b| yami_px::paths::natural(&a.name.to_lowercase(), &b.name.to_lowercase()));
        for f in here {
            if let Some(old) = out.iter_mut().find(|o| o.name == f.name) {
                // тот же набор в другом корне: курсоры могут лежать отдельно от значков
                old.icons |= f.icons;
                old.cursors |= f.cursors;
            } else {
                out.push(f);
            }
        }
    }
    out.truncate(64);
    out
}

/// программы для превью набора: первое имя, что в наборе есть
const SAMPLES: &[&[&str]] = &[
    &["system-file-manager", "org.gnome.Nautilus", "org.kde.dolphin", "folder"],
    &["utilities-terminal", "org.gnome.Terminal", "org.kde.konsole", "kitty"],
    &["web-browser", "internet-web-browser", "firefox", "chromium"],
    &["accessories-text-editor", "org.gnome.TextEditor", "org.kde.kate", "text-editor"],
    &["multimedia-audio-player", "audio-player", "org.gnome.Rhythmbox3", "audio-x-generic"],
    &["preferences-system", "preferences-desktop", "systemsettings", "org.gnome.Settings"],
];

/// превью всех наборов (в нити): значки — в виде из настроек, курсоры — стрелка ≤ 32 px
pub fn build(ramp: [u32; 5], pixel: bool, tint: bool) -> Sets {
    let all = installed();
    let preview = |theme: &str| {
        let mut ic = crate::icons::Icons::new(theme);
        ic.set_style(ramp, pixel, tint);
        SAMPLES.iter().filter_map(|alts| alts.iter().find(|a| ic.find(a, 24).is_some()).and_then(|a| ic.app(a, 24)).map(|i| (*i).clone())).collect::<Vec<Img>>()
    };
    let sys = crate::icons::system_theme();
    let mut icons = vec![Card { name: String::new(), title: crate::tf!("как в системе: {}", "as the system: {}", "como no sistema: {}", sys), imgs: preview("") }];
    icons.extend(all.iter().filter(|f| f.icons).map(|f| Card { name: f.name.clone(), title: f.title.clone(), imgs: preview(&f.name) }));
    let mut cursors = vec![Card { name: "yami".into(), title: crate::tf!("пиксельный yami, в цвет палитры", "yami pixel, in the palette", "pixel do yami, na paleta"), imgs: vec![] }];
    cursors.extend(all.iter().filter(|f| f.cursors).map(|f| Card { name: f.name.clone(), title: f.title.clone(), imgs: cursor_img(&f.dir, 32).into_iter().collect() }));
    cursors.push(Card { name: "system".into(), title: crate::tf!("не трогать", "leave alone", "não mexer"), imgs: vec![] });
    Sets { icons, cursors }
}

/// стрелка набора курсоров: left_ptr (или default/arrow) ближайшего к want размера
pub fn cursor_img(dir: &Path, want: u32) -> Option<Img> {
    ["left_ptr", "default", "arrow"].iter().find_map(|n| xcursor(&dir.join("cursors").join(n), want))
}

/// файл Xcursor: заголовок «Xcur», оглавление (тип, номинальный размер, смещение), картинки
/// ARGB с предумноженной альфой — как холст шелла
fn xcursor(p: &Path, want: u32) -> Option<Img> {
    let d = yami_px::paths::read_capped(p, 8 << 20)?;
    if d.get(..4)? != b"Xcur" {
        return None;
    }
    let u = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (hsize, ntoc) = (u(4)? as usize, u(12)?.min(4096) as usize);
    let best = (0..ntoc)
        .filter_map(|i| {
            let o = hsize + i * 12;
            (u(o)? == 0xfffd0002).then_some((u(o + 4)?, u(o + 8)? as usize))
        })
        .min_by_key(|(nom, _)| (*nom as i64 - want as i64).abs())?;
    let o = best.1;
    let (w, h) = (u(o + 16)? as i32, u(o + 20)? as i32);
    if !(1..=256).contains(&w) || !(1..=256).contains(&h) {
        return None;
    }
    let px: Option<Vec<u32>> = (0..(w * h) as usize).map(|i| u(o + 36 + i * 4)).collect();
    let img = Img { w, h, px: px? };
    // крупнее want — вдвое меньше выборкой (курсоры рисуют целыми пикселями)
    Some(if w > want as i32 * 3 / 2 { half(&img) } else { img })
}

fn half(i: &Img) -> Img {
    let (w, h) = ((i.w / 2).max(1), (i.h / 2).max(1));
    let px = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| i.px[((y * 2) * i.w + x * 2) as usize]).collect();
    Img { w, h, px }
}

/// свой набор из архива (.tar.*, .zip, .7z — bsdtar, иначе tar или unzip) или из папки: в
/// ~/.local/share/icons, по папке на набор (в ней index.theme или cursors/). Свой с тем же
/// именем заменяется, системный остаётся (свой его перекрывает). Возвращает имена наборов
pub fn install(src: &Path) -> Result<Vec<String>, String> {
    let dest = yami_px::paths::data_dir().join("icons");
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let meta = std::fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    // распаковка — рядом с целью: переезд на место тогда rename, а не копия
    let work = dest.join(format!(".yami-add-{}-{}", std::process::id(), crate::clock::unix_now()));
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&work).map_err(|e| e.to_string())?;
    }
    let res = (|| {
        let from = if meta.is_dir() {
            src.to_path_buf()
        } else {
            if meta.len() > 1 << 30 {
                return Err(crate::tf!("архив больше 1 ГБ", "the archive is over 1 GB", "o arquivo passa de 1 GB"));
            }
            unpack(src, &work)?;
            work.clone()
        };
        let found = theme_dirs(&from);
        if found.is_empty() {
            return Err(crate::tf!("здесь нет набора: нужна папка с index.theme или cursors/", "no set here: a folder with index.theme or cursors/ is needed", "nenhum conjunto aqui: é preciso uma pasta com index.theme ou cursors/"));
        }
        let mut names = Vec::new();
        for d in found {
            let name = d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !good_name(&name) {
                continue;
            }
            let to = dest.join(&name);
            // своя копия папки (из папки — копия, из архива — переезд), потом на место старой
            let stage = work.join(format!(".stage-{name}"));
            if meta.is_dir() {
                let ok = std::process::Command::new("cp").arg("-a").arg("--").arg(&d).arg(&stage).status().map(|s| s.success()).unwrap_or(false);
                if !ok {
                    continue;
                }
            } else if std::fs::rename(&d, &stage).is_err() {
                continue;
            }
            if std::fs::symlink_metadata(&to).is_ok() {
                let _ = if to.is_dir() && !std::fs::symlink_metadata(&to).map(|m| m.file_type().is_symlink()).unwrap_or(false) { std::fs::remove_dir_all(&to) } else { std::fs::remove_file(&to) };
            }
            if std::fs::rename(&stage, &to).is_ok() {
                names.push(name);
            }
        }
        if names.is_empty() {
            return Err(crate::tf!("набор не поставился (имя или права)", "the set was not installed (name or permissions)", "o conjunto não foi instalado (nome ou permissões)"));
        }
        Ok(names)
    })();
    let _ = std::fs::remove_dir_all(&work);
    res
}

/// архив в папку to. bsdtar и GNU tar не пишут за её пределы (`..` и абсолютные пути режутся),
/// unzip — тоже
fn unpack(src: &Path, to: &Path) -> Result<(), String> {
    let low = src.to_string_lossy().to_lowercase();
    let mut c = if crate::proc::have("bsdtar") {
        let mut c = std::process::Command::new("bsdtar");
        c.arg("-xf").arg(src).arg("-C").arg(to);
        c
    } else if low.ends_with(".zip") {
        let mut c = std::process::Command::new("unzip");
        c.arg("-qq").arg("-o").arg(src).arg("-d").arg(to);
        c
    } else {
        let mut c = std::process::Command::new("tar");
        c.arg("-xf").arg(src).arg("-C").arg(to).arg("--no-same-owner");
        c
    };
    let out = c.stdin(std::process::Stdio::null()).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("unpack failed").chars().take(160).collect())
    }
}

/// папки наборов: сама папка, если это набор, иначе наборы в ней (до трёх уровней)
fn theme_dirs(root: &Path) -> Vec<PathBuf> {
    let is_set = |d: &Path| d.join("index.theme").is_file() || d.join("cursors").is_dir();
    if is_set(root) {
        return vec![root.to_path_buf()];
    }
    let mut out = Vec::new();
    let mut q = vec![(root.to_path_buf(), 0)];
    while let Some((d, depth)) = q.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) || e.file_name().to_string_lossy().starts_with(".stage-") {
                continue;
            }
            if is_set(&p) {
                out.push(p);
            } else if depth < 3 {
                q.push((p, depth + 1));
            }
        }
    }
    out.truncate(32);
    out
}

// ── настройки → вид ─────────────────────────────────────────────────────────

/// сетка карточек; sel — имя выбранной; act — что делает клик по i-й
fn cards(ui: &mut Ui, cards: &[Card], sel: &str, b: R, y: i32, act: fn(usize) -> Act, cursor: bool) -> i32 {
    let t = ui.t;
    let cols = if b.w >= 600 { 3 } else { 2 };
    let gap = 10;
    let cw = (b.w - (cols - 1) * gap) / cols;
    let ch = 58;
    for (i, c) in cards.iter().enumerate() {
        let r = R::new(b.x + (i as i32 % cols) * (cw + gap), y + (i as i32 / cols) * (ch + gap), cw, ch);
        let on = c.name == sel;
        let hov = ui.hover(r);
        let bg = if on { t.selection.alpha(0.55) } else if hov { t.surface_hi } else { t.surface };
        ui.cv.pxbox(r, &BoxStyle::new(bg, if on { t.red } else { t.border }));
        if cursor {
            // стрелка слева, подпись справа
            let pr = R::new(r.x + 6, r.y + 6, 44, ch - 12);
            ui.cv.pxbox(pr, &BoxStyle::new(t.code_bg, t.border));
            match c.name.as_str() {
                "yami" => yami_arrow(ui, pr, 3),
                "system" => ui.cv.icon("cursor", pr.x + (pr.w - 14) / 2, pr.y + (pr.h - 14) / 2, 2, t.text_faint),
                _ => {
                    if let Some(img) = c.imgs.first() {
                        ui.cv.push_clip(pr.inset(2));
                        ui.cv.image(img, pr.x + (pr.w - img.w) / 2, pr.y + (pr.h - img.h) / 2, 1.0);
                        ui.cv.pop_clip();
                    }
                }
            }
            ui.text_in(PX, R::new(r.x + 58, r.y + 6, cw - 66, ch - 12), &c.title, if on { t.text } else { t.text_dim });
        } else {
            ui.text_in(PX, R::new(r.x + 8, r.y + 3, cw - 16, 20), &c.title, if on { t.text } else { t.text_dim });
            let mut x = r.x + 8;
            for img in &c.imgs {
                if x + img.w > r.right() - 6 {
                    break;
                }
                ui.cv.image(img, x, r.y + 26, 1.0);
                x += img.w + 6;
            }
        }
        ui.zone(r, act(i), Cur::Pointer);
    }
    y + (cards.len() as i32 + cols - 1) / cols * (ch + gap)
}

/// пиксельная стрелка yami (та же, что рисует tools/theme.py) клетками по k px в r
pub fn yami_arrow(ui: &mut Ui, r: R, k: i32) {
    const ARROW: [&str; 12] = ["o...........", "oo..........", "obo.........", "oaao........", "oaaao.......", "oaaaao......", "oaaaaao.....", "oaaaaaao....", "oaaaoooo....", "oaoao.......", "oo.oao......", "o...oo......"];
    let t = ui.t;
    let (ox, oy) = (r.x + (r.w - 8 * k) / 2, r.y + (r.h - 12 * k) / 2);
    for (j, row) in ARROW.iter().enumerate() {
        for (i, ch) in row.bytes().enumerate() {
            let c = match ch {
                b'o' => t.bg.lerp(crate::gfx::C(0xff000000), 0.5),
                b'a' => t.red,
                b'b' => t.ember,
                _ => continue,
            };
            ui.cv.fill(R::new(ox + i as i32 * k, oy + j as i32 * k, k, k), c);
        }
    }
}

/// «значки»: вид (пиксельные, в тонах палитры) и наборы карточками
pub fn draw_icons(ui: &mut Ui, st: &State, b: R, mut y: i32) -> i32 {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    ui.toggle(R::new(b.x, y, half, 40), "пиксельные", "чистая сетка, свои цвета", s.icon_pixel, Act::SBool("icon_pixel"));
    if s.icon_pixel {
        ui.toggle(R::new(b.x + half + 20, y, half, 40), "в тонах палитры", "четыре тона акцента", s.icon_tint, Act::SBool("icon_tint"));
    }
    y += 50;
    match &st.sets {
        Some(sets) => y = cards(ui, &sets.icons, &s.icon_theme, b, y, Act::SIconSet, false),
        None => {
            ui.text_in(PX, R::new(b.x, y, b.w, 20), "собираю наборы…", ui.t.text_faint);
            y += 28;
        }
    }
    add_row(ui, st, b, y)
}

/// «курсор»: размер и наборы карточками
pub fn draw_cursors(ui: &mut Ui, st: &State, b: R, mut y: i32) -> i32 {
    let s = &st.set;
    crate::settings::opt_btn(ui, R::new(b.x, y, (b.w - 20) / 2, 26), "размер", &format!("{} px", s.cursor_size), Act::SCycle("cursor_size"));
    y += 36;
    if let Some(sets) = &st.sets {
        let sel = if s.cursor.is_empty() { "yami" } else { s.cursor.as_str() };
        y = cards(ui, &sets.cursors, sel, b, y, Act::SCursorSet, true);
    }
    add_row(ui, st, b, y)
}

/// «поставить свой набор…» и итог последней установки
fn add_row(ui: &mut Ui, st: &State, b: R, y: i32) -> i32 {
    let t = ui.t;
    let bw = 230;
    ui.button(R::new(b.x, y, bw, 26), "поставить свой набор…", "plus", Tone::Plain, false, !st.sets_busy, Act::SCycle("set_add"));
    let note = if st.sets_note.is_empty() { crate::tf!("архив или папка → ~/.local/share/icons", "an archive or a folder → ~/.local/share/icons", "um arquivo ou uma pasta → ~/.local/share/icons") } else { st.sets_note.clone() };
    ui.text_in(PX, R::new(b.x + bw + 12, y, b.w - bw - 12, 26), &note, t.text_faint);
    y + 40
}

impl App {
    /// превью наборов — в нити; уже идёт — не второй раз
    pub fn sets_load(&mut self) {
        if self.st.sets_busy {
            return;
        }
        self.st.sets_busy = true;
        let (ramp, pixel, tint) = (self.st.t.icon_ramp(), self.st.set.icon_pixel, self.st.set.icon_tint);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Sets(Box::new(build(ramp, pixel, tint))));
        });
    }

    pub fn sets_free(&mut self) {
        self.st.sets = None;
        self.st.sets_note.clear();
    }

    /// свой набор: окно выбора yami-files (настройки на это время закрыты — они слой поверх
    /// окон), потом установка в нити и снова «вид»
    pub fn sets_add(&mut self) {
        self.close_modal();
        let tx = self.tx.clone();
        let from = yami_px::paths::xdg_dir("DOWNLOAD").filter(|d| d.is_dir()).unwrap_or_else(crate::config::home);
        let title = crate::i18n::tr("набор значков или курсоров").to_string();
        std::thread::spawn(move || {
            let pats = ["tar.gz", "tgz", "tar.xz", "txz", "tar.bz2", "tar.zst", "tar", "zip", "7z"].map(|e| format!("*.{e}")).join(";");
            let filter = format!("Archives|{pats}|application/x-compressed-tar;application/x-xz-compressed-tar;application/x-bzip-compressed-tar;application/x-zstd-compressed-tar;application/x-tar;application/zip;application/x-7z-compressed");
            let got = crate::proc::files_pick("open", &from, &["--title", &title, "--filter", &filter]);
            let res = got.map(|p| install(Path::new(&p)));
            let _ = tx.send(Msg::SetAdded(res));
        });
    }

    pub fn sets_added(&mut self, res: Option<Result<Vec<String>, String>>) {
        self.st.sets_note = match res {
            None => String::new(),
            Some(Ok(n)) => crate::tf!("поставлено: {}", "installed: {}", "instalado: {}", n.join(", ")),
            Some(Err(e)) => e,
        };
        self.st.sets = None;
        self.st.sets_busy = false;
        self.open_settings_tab("look");
    }

    /// набор значков выбран: шелл сразу, GTK и Qt — theme.py --icons
    pub fn icons_apply(&mut self) {
        self.settings_changed();
        let j = self.st.set.to_json();
        let _ = crate::config::write_atomic(&crate::config::settings_path(), &j);
        self.set_saved = j;
        std::thread::spawn(|| {
            let _ = crate::proc::theme_py().arg("--icons").status();
        });
    }
}

// ── yami icons / yami cursor ────────────────────────────────────────────────

/// yami icons [ИМЯ|auto|add ПУТЬ] и yami cursor [ИМЯ|yami|system|size N|add ПУТЬ]
pub fn cli(y: &crate::cmd::Sty, cursor: bool, args: &[&str]) -> i32 {
    let mut s = crate::config::Settings::load();
    let all = installed();
    let save = |s: &crate::config::Settings, flag: &str| -> i32 {
        if crate::config::write_atomic(&crate::config::settings_path(), &s.to_json()).is_err() {
            return 1;
        }
        let _ = crate::proc::theme_py().arg(flag).status();
        0
    };
    match args {
        [] => {
            if cursor {
                y.section(&crate::tf!("курсоры", "cursors", "cursores"));
                let cur = if s.cursor.is_empty() { "yami" } else { s.cursor.as_str() };
                let row = |name: &str, note: &str| println!("   {} {}  {}", if name == cur { y.acc("●") } else { " ".into() }, y.bold(&format!("{name:<24}")), y.dim(note));
                row("yami", &crate::tf!("пиксельный, в цвет палитры", "pixel, in the palette", "pixel, na paleta"));
                for f in all.iter().filter(|f| f.cursors) {
                    row(&f.name, &f.dir.display().to_string());
                }
                row("system", &crate::tf!("не трогать", "leave alone", "não mexer"));
                println!("\n   {}\n", y.dim(&crate::tf!("сменить: yami cursor ИМЯ  ·  размер: yami cursor size 32  ·  свой: yami cursor add АРХИВ", "switch: yami cursor NAME  ·  size: yami cursor size 32  ·  your own: yami cursor add ARCHIVE", "trocar: yami cursor NOME  ·  tamanho: yami cursor size 32  ·  o seu: yami cursor add ARQUIVO")));
            } else {
                y.section(&crate::tf!("наборы значков", "icon sets", "conjuntos de ícones"));
                let row = |name: &str, shown: &str, note: &str| println!("   {} {}  {}", if name == s.icon_theme { y.acc("●") } else { " ".into() }, y.bold(&format!("{shown:<24}")), y.dim(note));
                row("", "auto", &crate::tf!("как в системе: {}", "as the system: {}", "como no sistema: {}", crate::icons::system_theme()));
                for f in all.iter().filter(|f| f.icons) {
                    row(&f.name, &f.name, &f.dir.display().to_string());
                }
                println!("\n   {}\n", y.dim(&crate::tf!("сменить: yami icons ИМЯ  ·  как в системе: yami icons auto  ·  свой: yami icons add АРХИВ", "switch: yami icons NAME  ·  as the system: yami icons auto  ·  your own: yami icons add ARCHIVE", "trocar: yami icons NOME  ·  como no sistema: yami icons auto  ·  o seu: yami icons add ARQUIVO")));
            }
            0
        }
        ["add", path @ ..] if !path.is_empty() => match install(Path::new(&path.join(" "))) {
            Ok(n) => {
                println!(" {} {}", y.ok("✓"), crate::tf!("поставлено: {}", "installed: {}", "instalado: {}", n.join(", ")));
                0
            }
            Err(e) => {
                eprintln!(" {} {e}", y.bad("✕"));
                1
            }
        },
        ["size", n] if cursor => match n.parse::<i32>() {
            Ok(v) if (16..=96).contains(&v) => {
                s.cursor_size = v;
                save(&s, "--cursor")
            }
            _ => {
                eprintln!(" {} yami cursor size 16…96", y.bad("✕"));
                2
            }
        },
        [name] => {
            let name = *name;
            let ok = if cursor { matches!(name, "yami" | "system") || all.iter().any(|f| f.cursors && f.name == name) } else { name == "auto" || all.iter().any(|f| f.icons && f.name == name) };
            if !ok {
                eprintln!(" {} {}", y.bad("✕"), crate::tf!("нет такого набора: {} (список — без аргумента)", "no such set: {} (run without arguments for the list)", "conjunto inexistente: {} (sem argumentos mostra a lista)", name));
                return 1;
            }
            if cursor {
                s.cursor = name.into();
                save(&s, "--cursor")
            } else {
                s.icon_theme = if name == "auto" { String::new() } else { name.into() };
                save(&s, "--icons")
            }
        }
        _ => {
            eprintln!(" {} yami {} [{}]", y.bad("✕"), if cursor { "cursor" } else { "icons" }, if cursor { "NAME | yami | system | size N | add PATH" } else { "NAME | auto | add PATH" });
            2
        }
    }
}
