// Всплывашки бара (ЛКМ по модулю): календарь + история уведомлений, звук (выход, микрофон,
// громкость программ), музыка (обложка, кнопки, перемотка, эквалайзер), сеть (график ↓↑, Wi-Fi),
// bluetooth, система, трей (trayMode = arrow: значки программ списком).
// Клик мимо — закрыть (ловец кликов в app.rs), Esc — тоже.

use crate::app::{App, SKind, State};
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Tone, Ui};
use crate::widgets;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

/// виды всплывашек: `popup KIND`, `yami open KIND`, shot
pub const KINDS: &[&str] = &["cal", "sound", "media", "net", "bt", "sys", "bat", "tray", "control", "wallhaven", "fan", "weather", "calc", "hidden"];

impl State {
    /// открыта всплывашка этого вида
    pub fn popup_is(&self, kind: &str) -> bool {
        self.popup.as_ref().is_some_and(|p| p.kind == kind)
    }
}

/// открытая всплывашка рисует плеер (время, полоса позиции)
pub fn shows_player(st: &State) -> bool {
    st.popup_is("media") || (st.popup_is("sound") && sound_media(st))
}

/// открытая всплывашка рисует эквалайзер (ему нужен захват звука): у вкладки музыки свой (ctl_viz),
/// у плеера во всплывашке звука — тот же, что в баре (media_viz)
pub fn shows_viz(st: &State) -> bool {
    (st.popup_is("media") && st.set.ctl_viz != "off") || (st.popup_is("sound") && sound_media(st) && st.set.media_viz != "off")
}

/// размер всплывашки; вкладки панели управления — все одного размера, со столбиком вкладок
pub fn size(kind: &str, st: &State) -> (i32, i32) {
    if crate::control::is_tab(kind) {
        (crate::control::TAB_SIZE.0 + 38 + crate::control::RAIL, crate::control::TAB_SIZE.1 + 30)
    } else {
        base_size(kind, st)
    }
}

fn base_size(kind: &str, st: &State) -> (i32, i32) {
    match kind {
        "sound" => {
            let apps = st.audio.streams.len().min(6) as i32;
            let list = |n: usize| if n > 1 { n.min(5) as i32 * 26 + 30 } else { 0 };
            let media = if sound_media(st) { 170 } else { 0 };
            (420, 44 + 56 + list(st.audio.sinks.len()) + 60 + list(st.audio.sources.len()) + if apps > 0 { 30 + apps * 46 } else { 0 } + media + 50)
        }
        "tray" => (300, 66 + tray_items(st).filter(|it| !tray_in_bar(st, it)).count().clamp(1, 12) as i32 * 32),
        "bat" => (340, 44 + 40),
        "wallhaven" => (crate::wallhaven::SIZE.0 + 38, crate::wallhaven::SIZE.1 + 30),
        "fan" => (crate::fan::SIZE.0 + 38, crate::fan::SIZE.1 + 30),
        "hidden" => {
            let (w, h) = crate::hide::size(st);
            (w + 38, h + 30)
        }
        _ => (300, 200),
    }
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let Some(p) = &st.popup else { return };
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, true);
    // «язычок» к модулю бара
    let t = ui.t;
    ui.cv.fill(R::new(2, 0, fr.w - 4, 2), t.red);
    let tab = crate::control::is_tab(p.kind);
    let rail = if tab { crate::control::RAIL } else { 0 };
    if tab {
        crate::control::rail(ui, p.kind, R::new(12, 12, rail - 8, h - 30));
    }
    let body = R::new(16 + rail, 12, w - 38 - rail, h - 30);
    match p.kind {
        "cal" => cal(ui, st, body),
        "sound" => sound(ui, st, body),
        "media" => media(ui, st, body),
        "net" => net(ui, st, body),
        "bt" => bt(ui, st, body),
        "sys" => sys(ui, st, body),
        "bat" => battery(ui, st, body),
        "tray" => tray(ui, st, body),
        "control" => crate::control::draw(ui, st, body),
        "weather" => crate::weather::draw(ui, st, body),
        "calc" => crate::calc::draw(ui, st, body),
        "wallhaven" => crate::wallhaven::draw(ui, st, body),
        "fan" => crate::fan::draw(ui, st, body),
        "hidden" => crate::hide::draw(ui, st, body),
        _ => {}
    }
    crate::dropdown::draw(ui, st, &crate::app::SKind::Popup, w, h);
}

fn head(ui: &mut Ui, r: R, icon: &str, title: &str, sub: &str) -> i32 {
    let t = ui.t;
    ui.cv.icon(icon, r.x, r.y + 3, 2, t.red);
    let w = ui.text_in(PX, R::new(r.x + 22, r.y, r.w - 22, 20), title, t.text);
    if !sub.is_empty() {
        ui.text_in(PX, R::new(r.x + 30 + w, r.y, r.w - 40 - w, 20), sub, t.text_faint);
    }
    ui.cv.fill(R::new(r.x, r.y + 26, r.w, 1), t.border);
    r.y + 36
}


// ── календарь ───────────────────────────────────────────────────────────────
fn cal(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let now = crate::clock::local_now();
    let face = font::px(32);
    ui.text(face, b.x, b.y - 2, &format!("{:02}:{:02}", now.hour, now.min), t.text);
    let date = crate::tf!("{}, {} {}", "{}, {} {}", crate::i18n::tr(crate::clock::RU_DAY_FULL[now.wday as usize]), now.day, crate::i18n::tr(crate::clock::RU_MON[(now.mon - 1) as usize]));
    let dw = ui.text_in(PX, R::new(b.x, b.y + 34, 300, 20), &date, t.text_dim);
    // выбранный день: через сколько он (или сколько прошло)
    if let Some(day) = st.cal_pick {
        let left = widgets::days_until(day - widgets::days_from_civil(now.year, now.mon as u32, now.day as u32));
        ui.text_in(PX, R::new(b.x + dw + 12, b.y + 34, 290 - dw, 20), &left, t.text_faint);
    }
    let cr = R::new(b.x, b.y + 66, 290, b.h - 66);
    widgets::calendar(ui, st, cr, st.cal_month, t.red, true);
    // уведомления
    let nx = b.x + 310;
    ui.cv.fill(R::new(nx - 10, b.y, 1, b.h), t.border);
    let nr = R::new(nx, b.y, b.right() - nx, b.h);
    let n = st.notifs.history.len();
    ui.cv.icon(if st.quiet() { "bellx" } else { "bell" }, nr.x, nr.y + 3, 2, t.red);
    let tw = ui.text_in(PX, R::new(nr.x + 22, nr.y, nr.w - 90, 20), "уведомления", t.text);
    // число — плашкой, а не текстом через пробел
    if n > 0 {
        let num = n.to_string();
        let bw = ui.w(PX, &num) + 10;
        let br = R::new(nr.x + 30 + tw, nr.y + 1, bw, 18);
        ui.cv.fill(br, t.red);
        ui.text_center(PX, br, &num, t.on_accent);
    }
    // справа — значки: «не беспокоить» (горит, когда включено) и «очистить»
    ui.button(R::new(nr.right() - 58, nr.y - 2, 26, 24), "", if st.set.dnd { "bellx" } else { "bell" }, Tone::Ghost, st.set.dnd, true, Act::Dnd);
    ui.button(R::new(nr.right() - 26, nr.y - 2, 26, 24), "", "trash", Tone::Ghost, false, n > 0, Act::NotifClear);
    ui.cv.fill(R::new(nr.x, nr.y + 26, nr.w, 1), t.border);
    let mut y = nr.y + 34;
    if n == 0 {
        // пусто: значок и подпись посередине свободного места
        let free = R::new(nr.x, y, nr.w, nr.bottom() - y);
        let cy = free.cy();
        ui.cv.icon_in(if st.set.dnd { "bellx" } else { "bell" }, R::new(free.x, cy - 34, free.w, 28), 4, t.border);
        ui.text_center(PX, R::new(free.x, cy + 4, free.w, 20), "тихо. ничего нового", t.text_faint);
        if st.set.dnd {
            ui.text_center(PX, R::new(free.x, cy + 24, free.w, 20), "не беспокоить включено", t.text_faint.fade(0.7));
        }
        return;
    }
    ui.cv.push_clip(R::new(nr.x, y, nr.w, nr.bottom() - y));
    let hide = st.auto_quiet();
    for note in &st.notifs.history {
        if y > nr.bottom() {
            break;
        }
        let ch = if note.body.is_empty() { 52 } else { 70 };
        let r = R::new(nr.x, y, nr.w, ch);
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(if hov { t.surface_hi } else { t.surface }, if note.critical { t.ember } else { t.border }));
        ui.zone(r, Act::NotifClick(note.id), Cur::Pointer);
        let mut x = r.x + 8;
        if !note.icon.is_empty() {
            let icon = note.icon.clone();
            ui.app_icon(&icon, x, r.y + 6, 16, 1.0);
            x += 22;
        }
        let app = if note.app.is_empty() { "система".to_string() } else { note.app.to_lowercase() };
        let aw = ui.text_in(PX, R::new(x, r.y + 4, r.w / 2, 18), &app, t.red);
        ui.text_in(PX, R::new(x + aw + 8, r.y + 4, 60, 18), &note.time, t.text_faint);
        ui.ghost(R::new(r.right() - 24, r.y + 4, 20, 18), "close", Act::NotifDel(note.id));
        ui.text_in(PX, R::new(r.x + 8, r.y + 26, r.w - 16, 18), &note.summary, t.text);
        if !note.body.is_empty() {
            ui.text_in(PX, R::new(r.x + 8, r.y + 46, r.w - 16, 18), &note.body.replace('\n', " "), t.text_dim);
        }
        // тихо само: текст под пиксельным шумом, наведение его рассыпает, уход — собирает
        if hide {
            let tr = R::new(r.x + 6, r.y + 24, r.w - 12, ch - 28);
            spoiler(ui, st, tr, note.id, hov);
        }
        y += ch + 6;
    }
    ui.cv.pop_clip();
}

/// ключ спойлера для строки (имя сети): не пересекается с id уведомлений
fn spoiler_key(s: &str) -> u32 {
    (yami_px::fnv(yami_px::FNV0, s.as_bytes()) as u32) | 0x8000_0000
}

/// сколько спойлера открыто сейчас: к цели за четверть секунды
fn spoiler_open(e: &(bool, f32, std::time::Instant), now: std::time::Instant) -> f32 {
    let k = (now.duration_since(e.2).as_secs_f32() / 0.25).min(1.0);
    let k = 1.0 - (1.0 - k) * (1.0 - k);
    e.1 + ((if e.0 { 1.0 } else { 0.0 }) - e.1) * k
}

/// спойлер над текстом в r (тихо само: игра, весь экран, демонстрация): размыт, пока не наведут
/// open_on; открывается и закрывается за четверть секунды. bg — фон под текстом
fn spoiler(ui: &mut Ui, st: &State, r: R, id: u32, open_on: bool) {
    let now = std::time::Instant::now();
    let open = {
        let mut m = st.spoilers.borrow_mut();
        // старые (ушедшие уведомления, сети) не копить
        if m.len() > 256 {
            m.retain(|_, e| e.0 || now.duration_since(e.2).as_secs() < 60);
        }
        let e = m.entry(id).or_insert((false, 0.0, now));
        if e.0 != open_on {
            let cur = spoiler_open(e, now);
            *e = (open_on, cur, now);
        }
        spoiler_open(e, now)
    };
    // кадры — только пока проявляется или прячется
    if open > 0.001 && open < 0.999 {
        ui.anim = true;
    }
    spoiler_cover(ui, r, 1.0 - open);
}

/// размытие текста в r: cover 1 — сильное (не прочесть, но видно, сколько строк и какой длины),
/// 0 — как есть. Ящичное размытие по буферу в два прохода (по строкам, потом по столбцам), радиус
/// растёт с cover
fn spoiler_cover(ui: &mut Ui, r: R, cover: f32) {
    let rad = (cover * 6.0).round() as i32;
    if rad <= 0 {
        return;
    }
    let (ox, oy, cw) = (ui.cv.ox, ui.cv.oy, ui.cv.w);
    let area = R::new(r.x + ox, r.y + oy, r.w, r.h).inter(&ui.cv.clip_rect());
    if area.w <= 0 || area.h <= 0 {
        return;
    }
    let buf = &mut *ui.cv.buf;
    let ch = |p: u32, s: u32| (p >> s) & 255;
    // одна линия: n пикселей с шагом step от start; скользящая сумма окна 2·rad+1 (края — повтором)
    let mut line = |start: usize, step: usize, n: usize| {
        let src: Vec<u32> = (0..n).map(|i| buf[start + i * step]).collect();
        let win = (2 * rad + 1) as u32;
        let at = |i: i32| src[i.clamp(0, n as i32 - 1) as usize];
        let mut sum = [0u32; 4];
        for i in -rad..=rad {
            let p = at(i);
            for (k, s) in [24, 16, 8, 0].into_iter().enumerate() {
                sum[k] += ch(p, s);
            }
        }
        for i in 0..n as i32 {
            buf[start + i as usize * step] = (sum[0] / win) << 24 | (sum[1] / win) << 16 | (sum[2] / win) << 8 | sum[3] / win;
            let (out, inn) = (at(i - rad), at(i + rad + 1));
            for (k, s) in [24, 16, 8, 0].into_iter().enumerate() {
                sum[k] = sum[k] + ch(inn, s) - ch(out, s);
            }
        }
    };
    for _ in 0..2 {
        for y in area.y..area.bottom() {
            line((y * cw + area.x) as usize, 1, area.w as usize);
        }
        for x in area.x..area.right() {
            line((area.y * cw + x) as usize, cw as usize, area.h as usize);
        }
    }
}

// ── звук ────────────────────────────────────────────────────────────────────
fn sound(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let a = &st.audio;
    let sink = a.sinks.iter().find(|s| s.name == a.sink).map(|s| crate::audio::label(&st.set, s)).unwrap_or_default();
    let mut y = head(ui, b, "speaker", "звук", &sink);
    // выход
    ui.button(R::new(b.x, y + 18, 26, 22), "", if a.muted { "mute" } else { "speaker" }, Tone::Ghost, a.muted, true, Act::VolMute);
    let v = if a.muted { "тишина".to_string() } else { format!("{:.0}%", a.volume * 100.0) };
    ui.slider(R::new(b.x + 34, y, b.w - 34, 40), "громкость", a.volume.min(1.0) as f32, &v, 20, Act::VolSlider);
    y += 50;
    y = devices(ui, st, b, y, "выход", &a.sinks, &a.sink, Act::Sink);
    // микрофон
    ui.button(R::new(b.x, y + 18, 26, 22), "", if a.mic_muted { "mute" } else { "mic" }, Tone::Ghost, a.mic_muted, true, Act::MicMute);
    let mv = if a.mic_muted { "выкл".to_string() } else { format!("{:.0}%", a.mic * 100.0) };
    ui.slider(R::new(b.x + 34, y, b.w - 34, 40), "микрофон", a.mic.min(1.0) as f32, &mv, 20, Act::MicSlider);
    y += 54;
    y = devices(ui, st, b, y, "вход", &a.sources, &a.source, Act::Source);
    // программы
    if !a.streams.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "программы", t.text_dim);
        y += 26;
        for s in a.streams.iter().take(6) {
            // первое имя, у которого есть свой значок (через .desktop или тему значков)
            let names: Vec<String> = s.icon.iter().map(|n| crate::icons::icon_for(&st.apps, n)).collect();
            let icon = names.iter().find(|n| ui.ic.has_app(n)).or(names.first()).cloned().unwrap_or_default();
            ui.app_icon(&icon, b.x + 4, y + 20, 16, if s.muted { 0.4 } else { 1.0 });
            let label = if s.title.is_empty() || s.title == s.app { s.app.clone() } else { format!("{} — {}", s.app, s.title) };
            let v = if s.muted { "тихо".to_string() } else { format!("{:.0}%", s.vol * 100.0) };
            ui.slider(R::new(b.x + 34, y, b.w - 34 - 30, 40), &label, s.vol.min(1.0) as f32, &v, 20, Act::AppVol(s.idx));
            ui.ghost(R::new(b.right() - 24, y + 18, 24, 22), if s.muted { "mute" } else { "speaker" }, Act::AppMute(s.idx));
            y += 46;
        }
    }
    // музыка — здесь, только если ей больше негде быть (бар по умолчанию без `media`)
    if sound_media(st) {
        ui.cv.fill(R::new(b.x, y, b.w, 1), t.border);
        y += 10;
        widgets::media_card(ui, st, R::new(b.x, y, b.w, 150), &st.set.media_viz, t.red, true, true);
    }
    ui.button(R::new(b.x, b.bottom() - 28, b.w, 28), "микшер pavucontrol", "speaker", Tone::Plain, false, true, Act::Pavu);
}

/// выбор устройства (выход или микрофон), когда их больше одного: не больше пяти строк
fn devices(ui: &mut Ui, st: &State, b: R, mut y: i32, title: &str, list: &[crate::audio::Dev], cur: &str, act: fn(String) -> Act) -> i32 {
    if list.len() < 2 {
        return y;
    }
    let t = ui.t;
    ui.text_in(PX, R::new(b.x, y, b.w, 20), title, t.text_dim);
    y += 24;
    for d in list.iter().take(5) {
        let on = d.name == cur;
        let r = R::new(b.x, y, b.w, 24);
        ui.button(r, "", "", Tone::Ghost, on, true, act(d.name.clone()));
        ui.cv.fill(R::new(r.x + 8, r.cy() - 3, 6, 6), if on { t.ember } else { t.border });
        ui.text_in(PX, R::new(r.x + 22, r.y, r.w - 26, r.h), &crate::audio::label(&st.set, d), if on { t.text_hi } else { t.text });
        y += 26;
    }
    y + 6
}

/// плеер во всплывашке звука: что-то играет, а модуля `media` в баре и виджета музыки нет (#10
/// развёл их, но у бара по умолчанию плеер был только здесь)
pub fn sound_media(st: &State) -> bool {
    !st.media.title.is_empty() && !st.set.media_placed()
}

// ── музыка ──────────────────────────────────────────────────────────────────
// модуль `media`: только плеер; громкость — у модуля звука (#10)
fn media(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let m = &st.media;
    let y = head(ui, b, "music", "музыка", "");
    let tw = ui.w(PX, "музыка");
    let nr = R::new(b.x + 30 + tw, b.y, b.w - 40 - tw, 20);
    let n = st.players.len();
    if n > 1 {
        // плееров несколько: имя — переключатель (ЛКМ — следующий, ПКМ — прошлый), «2/3» — который (#24)
        let k = st.players.iter().position(|p| p.inst == m.inst).map_or(1, |i| i + 1);
        let label = format!("{} {k}/{n}", m.player);
        let lw = (ui.w(PX, &label) + 16).min(nr.w);
        ui.button(R::new(nr.x - 8, b.y - 2, lw, 22), &label, "", Tone::Ghost, false, true, Act::MediaPlayer);
    } else {
        ui.cv.text_in(ui.f, PX, nr, &m.player, t.text_faint);
    }
    // слева карточка «сейчас играет», справа узкий эквалайзер
    let rest = R::new(b.x, y, b.w, b.bottom() - y);
    let side = 180;
    let lw = rest.w - side - 12;
    now_card(ui, st, R::new(rest.x, rest.y, lw, rest.h));
    vviz(ui, st, R::new(rest.x + lw + 12, rest.y, side, rest.h));
}

/// карточка плеера: обложка, название, исполнитель, альбом, перемотка и кнопки внизу
fn now_card(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let m = &st.media;
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, t.border));
    let r = r.inset(14);
    ui.text_in(PX, R::new(r.x, r.y, r.w - 24, 20), "сейчас играет", t.text);
    ui.cv.icon("music", r.right() - 14, r.y + 3, 2, t.text_dim);
    if m.title.is_empty() {
        ui.cv.icon_in("music", R::new(r.x, r.y + 30, r.w, r.h - 60), 6, t.text_faint);
        ui.text_center(PX, R::new(r.x, r.bottom() - 24, r.w, 20), "тишина — ничего не играет", t.text_faint);
        return;
    }
    // низ снизу вверх: кнопки, перемотка, подписи; обложка — на то, что осталось
    let by = r.bottom() - 32;
    let sy = by - 30;
    let ty = sy - 66;
    let side = (ty - r.y - 38).min(r.w).max(0);
    let a = R::new(r.x + (r.w - side) / 2, r.y + 30, side, side);
    match st.art.get(&m.art).cloned().flatten() {
        Some(img) => {
            ui.cv.image_scaled(&img, a, 1.0);
            ui.cv.outline(a, t.border, 1);
        }
        None => {
            ui.cv.pxbox(a, &BoxStyle::new(t.code_bg, t.border));
            ui.cv.icon_in("music", a, 5, t.dim);
        }
    }
    ui.text_in(font::PX24, R::new(r.x, ty, r.w, 26), &m.title, t.text_hi);
    ui.text_in(PX, R::new(r.x, ty + 28, r.w, 18), if m.artist.is_empty() { &m.player } else { &m.artist }, t.text_dim);
    if !m.album.is_empty() {
        ui.text_in(PX, R::new(r.x, ty + 46, r.w, 18), &m.album, t.text_faint);
    }
    // перемотка: время по краям, щёлкнуть или потянуть
    if m.length > 0.0 {
        let at = st.seek.map(|f| f as f64 * m.length).unwrap_or(m.now_pos());
        let lw = ui.text_in(PX, R::new(r.x, sy, 60, 20), &yami_px::clock(at), if st.seek.is_some() { t.red } else { t.text_dim });
        let rw = ui.text_right(PX, R::new(r.x, sy, r.w, 20), &yami_px::clock(m.length), t.text_faint);
        let px0 = r.x + lw + 10;
        let pw = r.right() - rw - 10 - px0;
        if pw > 40 {
            widgets::seek_bar(ui, st, R::new(px0, sy - 1, pw, 22), t.red, true);
        }
    }
    // кнопки: повтор, назад, пауза, вперёд, перемешать — по центру
    let rep = m.repeat.as_str();
    let total = 5 * 36 + 8 + 4 * 14;
    let mut x = r.x + (r.w - total) / 2;
    let sel = |on: bool| if on { Tone::Primary } else { Tone::Ghost };
    ui.button(R::new(x, by, 36, 32), "", "repeat", sel(matches!(rep, "Playlist" | "Track")), false, !rep.is_empty(), Act::MediaRepeat);
    if rep == "Track" {
        ui.cv.fill(R::new(x + 28, by + 4, 4, 4), t.ember);
    }
    x += 50;
    ui.button(R::new(x, by, 36, 32), "", "prev", Tone::Ghost, false, true, Act::MediaPrev);
    x += 50;
    ui.button(R::new(x, by, 44, 32), "", if m.playing() { "pause" } else { "play" }, Tone::Primary, false, true, Act::MediaPlay);
    x += 58;
    ui.button(R::new(x, by, 36, 32), "", "next", Tone::Ghost, false, true, Act::MediaNext);
    x += 50;
    ui.button(R::new(x, by, 36, 32), "", "shuffle", sel(m.shuffle == Some(true)), false, m.shuffle.is_some(), Act::MediaShuffle);
}

/// эквалайзер стоймя: полосы частот сверху вниз, каждая — в обе стороны от середины; ЛКМ — стиль
fn vviz(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, if hov { t.red } else { t.border }));
    let inr = r.inset(12);
    // выключен щелчком по нему — звук не захватывается, только пунктир
    let idle = st.set.ctl_viz == "off" || st.viz.is_empty() || !st.media.playing();
    let n = if st.viz.is_empty() { 24 } else { st.viz.len() } as i32;
    let bh = (inr.h / n).max(2);
    let top = inr.y + (inr.h - bh * n) / 2;
    let cx = inr.x + inr.w / 2;
    let c = t.red;
    // тишина — пунктир по середине
    if idle {
        for y in (inr.y..inr.bottom()).step_by(6) {
            ui.cv.fill(R::new(cx - 1, y, 2, 3), t.border);
        }
    }
    let mut prev: Option<(i32, i32)> = None;
    for i in 0..n {
        let v = if idle { 0.0 } else { st.viz.get(i as usize).copied().unwrap_or(0.0) };
        let half = ((v * inr.w as f32 / 2.0).round() as i32).min(inr.w / 2);
        if half <= 0 {
            prev = None;
            continue;
        }
        let y = top + i * bh;
        match st.set.ctl_viz.as_str() {
            "wave" => ui.cv.fill(R::new(cx - half, y + bh / 4, half * 2, (bh / 2).max(1)), c),
            "dots" => {
                ui.cv.fill(R::new(cx - half, y, 2, bh - 1), c);
                ui.cv.fill(R::new(cx + half - 2, y, 2, bh - 1), c);
            }
            "blocks" => {
                for k in 0..(half / 4).max(1) {
                    let col = if k + 1 == (half / 4).max(1) { t.ember } else { c };
                    ui.cv.fill(R::new(cx + k * 4, y, 3, bh - 1), col);
                    ui.cv.fill(R::new(cx - (k + 1) * 4 + 1, y, 3, bh - 1), col);
                }
            }
            "line" => {
                // края — ломаной: от прошлой полосы к этой
                let (l0, r0) = prev.unwrap_or((cx - half, cx + half));
                let (l1, r1) = (cx - half, cx + half);
                ui.cv.fill(R::new(l0.min(l1), y, (l0 - l1).abs() + 2, 2), c);
                ui.cv.fill(R::new(r0.min(r1) - 2, y, (r0 - r1).abs() + 2, 2), c);
                ui.cv.fill(R::new(l1, y, 2, bh), c);
                ui.cv.fill(R::new(r1 - 2, y, 2, bh), c);
            }
            _ => {
                ui.cv.fill(R::new(cx - half, y, half * 2, bh - 1), c);
                ui.cv.fill(R::new(cx - half, y, 2, bh - 1), t.ember);
                ui.cv.fill(R::new(cx + half - 2, y, 2, bh - 1), t.ember);
            }
        }
        prev = Some((cx - half, cx + half));
    }
    if hov {
        let name = if st.set.ctl_viz == "off" { "выкл" } else { crate::widgets::name_of(widgets::STYLES, &st.set.ctl_viz) };
        ui.text_center(PX, R::new(r.x, r.bottom() - 26, r.w, 18), name, t.text_faint);
    }
    ui.zone(r, Act::CtlViz, Cur::Pointer);
}

// ── трей ────────────────────────────────────────────────────────────────────
// trayMode = arrow: в баре одна стрелка, программы — здесь. ЛКМ, ПКМ, СКМ и колесо — как у значка
// в баре; меню программы открывается под её строкой
/// ключ программы трея для закрепления: Id, иначе процесс (имя на шине меняется с каждым запуском)
pub fn tray_key(it: &crate::dbus::TrayItem) -> String {
    if it.id.is_empty() { it.app.clone() } else { it.id.clone() }
}

/// значок стоит в баре: все — в режиме «значки», закреплённые — в смешанном
pub fn tray_in_bar(st: &State, it: &crate::dbus::TrayItem) -> bool {
    match st.set.tray_mode.as_str() {
        "arrow" => false,
        "hybrid" => st.set.tray_pinned.contains(&tray_key(it)),
        _ => true,
    }
}

pub fn tray_items(st: &State) -> impl Iterator<Item = &crate::dbus::TrayItem> {
    // сеть показывает модуль `net`: апплет сети в трее её повторял бы (#16)
    let net = st.set.on_bar("net");
    st.tray.iter().filter(move |i| i.status != "Passive" && !(net && i.net_applet()))
}

/// имя программы трея для людей: из её .desktop (по процессу или Id), иначе подпись, если она не
/// служебная («discord_status_icon_1», «chrome_status_icon_1»), иначе процесс. Как есть, без перевода
pub fn tray_name(st: &State, it: &crate::dbus::TrayItem) -> String {
    let norm = |s: &str| s.to_lowercase().replace(['-', '_', '.', ' '], "");
    let keys: Vec<String> = [it.app.as_str(), it.id.as_str()].iter().map(|k| norm(k)).filter(|k| k.len() >= 3).collect();
    // 1) ярлык с тем же именем или классом окна
    let exact = st.apps.iter().find(|a| [norm(&a.id), norm(&a.wm_class)].iter().any(|i| !i.is_empty() && keys.iter().any(|k| i == k || i.ends_with(k.as_str()))));
    if let Some(a) = exact {
        return a.name.clone();
    }
    // 2) своя подпись программы, если она не служебная
    let service_like = |s: &str| s.is_empty() || s.contains("status_icon") || s.contains("StatusNotifier") || s.starts_with(':') || s.chars().last().is_some_and(|c| c.is_ascii_digit()) && s.contains('_');
    if !service_like(&it.title) {
        return it.title.clone();
    }
    // 3) ярлык, что запускает эту программу; не ярлыки игр и ссылок (steam steam://rungameid/…)
    let by_exec = st.apps.iter().find(|a| {
        let mut w = a.exec.split_whitespace();
        let bin = norm(w.next().unwrap_or("").rsplit('/').next().unwrap_or(""));
        !bin.is_empty() && !w.any(|x| x.contains("://")) && keys.iter().any(|k| bin == *k)
    });
    if let Some(a) = by_exec {
        return a.name.clone();
    }
    let raw = if !it.app.is_empty() { it.app.as_str() } else { it.id.as_str() };
    let clean = raw.split(['_', '-']).next().unwrap_or(raw);
    let mut c = clean.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_else(|| it.icon.clone())
}

fn tray(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let n = tray_items(st).filter(|it| !tray_in_bar(st, it)).count();
    let mut y = head(ui, b, "menu", "трей", &n.to_string());
    if n == 0 {
        ui.text_in(PX, R::new(b.x, y, b.w, 28), "в трее никого", t.text_faint);
        return;
    }
    for it in tray_items(st).filter(|it| !tray_in_bar(st, it)).take(12) {
        let r = R::new(b.x, y, b.w, 28);
        ui.button(r, "", "", Tone::Ghost, false, true, Act::Tray(it.service.clone()));
        crate::bar::tray_icon(ui, it, r.x + 6, r.cy() - 8, st.set.tray_pixel);
        let name = tray_name(st, it);
        let hov = ui.hover(r);
        ui.cv.text_in(ui.f, PX, R::new(r.x + 30, r.y, r.w - 46, r.h), &name, if hov { t.text_hi } else { t.text });
        if it.status == "NeedsAttention" {
            ui.cv.fill(R::new(r.right() - 12, r.cy() - 2, 4, 4), t.ember);
        }
        y += 32;
    }
}

// ── сеть ────────────────────────────────────────────────────────────────────
fn net(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.sys;
    let sub = if s.online { s.ip.clone() } else { "нет сети".to_string() };
    let top = head(ui, b, "net", "сеть", &sub);
    // слева Wi-Fi, справа подключения и график ↓↑
    let lw = b.w * 3 / 5;
    let l = R::new(b.x, top, lw, b.bottom() - top);
    let r = R::new(b.x + lw + 17, top, b.w - lw - 17, b.bottom() - top);
    ui.cv.fill(R::new(l.right() + 8, top, 1, l.h), t.border);
    let mut y = r.y;
    if st.net.active.is_empty() {
        ui.text_in(PX, R::new(r.x, y, r.w, 20), if st.net.loaded { "не подключено" } else { "смотрю…" }, t.text_faint);
        y += 22;
    }
    let hide = st.auto_quiet();
    for (name, kind, dev) in st.net.active.iter().take(3) {
        ui.cv.fill(R::new(r.x, y + 7, 6, 6), t.ok);
        ui.text_in(PX, R::new(r.x + 14, y, r.w - 14, 20), name, t.text);
        // тихо само (демонстрация, игра): имя сети под спойлером; адрес и устройство — нет
        if hide {
            let nr = R::new(r.x + 12, y, (ui.w(PX, name) + 6).min(r.w - 12), 20);
            spoiler(ui, st, nr, spoiler_key(name), ui.hover(nr));
        }
        ui.text_in(PX, R::new(r.x + 14, y + 18, r.w - 14, 18), &format!("{kind} · {dev}"), t.text_faint);
        y += 40;
    }
    let full = s.h_rx.max().max(s.h_tx.max()).max(64.0 * 1024.0);
    let rx: Vec<f32> = s.h_rx.tail(120).iter().map(|v| v / full).collect();
    let tx: Vec<f32> = s.h_tx.tail(120).iter().map(|v| v / full).collect();
    let half = r.w / 2;
    ui.text_in(PX, R::new(r.x, y, half, 20), &crate::tf!("↓ {}/с", "↓ {}/s", crate::sys::rate(s.rx)), t.red);
    ui.text_in(PX, R::new(r.x + half, y, half, 20), &crate::tf!("↑ {}/с", "↑ {}/s", crate::sys::rate(s.tx)), t.accent2);
    y += 22;
    ui.text_in(PX, R::new(r.x, y, r.w, 18), &crate::tf!("пик {}/с", "peak {}/s", crate::sys::rate(full as f64)), t.text_faint);
    y += 24;
    let gr = R::new(r.x, y, r.w, (r.bottom() - 38 - y).max(40));
    ui.cv.pxbox(gr, &BoxStyle::new(t.code_bg, t.border));
    widgets::graph(ui, gr.inset(6), &rx, "line", "scroll", st.sys_phase(), t.red, Some((&tx, t.accent2)));
    ui.button(R::new(r.x, r.bottom() - 28, r.w, 28), "все подключения", "gear", Tone::Plain, false, true, Act::NetEditor);
    // Wi-Fi
    let mut y = l.y;
    if !st.net.wifi_present {
        ui.text_in(PX, R::new(l.x, y, l.w, 20), "Wi-Fi нет", t.text_faint);
        return;
    }
    ui.toggle(R::new(l.x, y, 140, 26), "wi-fi", "", st.net.wifi_on, Act::Wifi(!st.net.wifi_on));
    if st.net.wifi_on {
        ui.button(R::new(l.right() - 110, y, 110, 24), "искать", "search", Tone::Ghost, false, !st.net.busy, Act::WifiRescan);
    }
    y += 34;
    if !st.net.msg.is_empty() {
        let c = if st.net.busy { t.text_dim } else { t.warn };
        ui.text_in(PX, R::new(l.x, y - 4, l.w, 20), &st.net.msg, c);
        y += 24;
    }
    if !st.net.wifi_on {
        return;
    }
    // строк — сколько влезает; под поле пароля — место заранее
    let pass = if st.wifi_target.is_empty() { 0 } else { 44 };
    for n in &st.net.nets {
        if y + 26 + pass > l.bottom() && st.wifi_target != n.ssid {
            continue;
        }
        if y + 26 > l.bottom() {
            break;
        }
        let rr = R::new(l.x, y, l.w, 26);
        ui.button(rr, "", "", Tone::Ghost, n.active, !st.net.busy, Act::WifiNet(n.ssid.clone()));
        // сила сигнала — 4 столбика
        for k in 0..4 {
            let on = n.signal as i32 > k * 25;
            ui.cv.fill(R::new(rr.x + 6 + k * 4, rr.y + 18 - k * 3, 3, 3 + k * 3), if on { if n.active { t.ember } else { t.red } } else { t.border });
        }
        ui.text_in(PX, R::new(rr.x + 30, rr.y, rr.w - 90, rr.h), &n.ssid, if n.active { t.text_hi } else { t.text });
        if hide {
            let hov = ui.hover(rr);
            let sw = (ui.w(PX, &n.ssid) + 6).min(rr.w - 92);
            spoiler(ui, st, R::new(rr.x + 28, rr.y + 4, sw, rr.h - 8), spoiler_key(&n.ssid), hov);
        }
        if n.secure {
            ui.cv.icon("lock", rr.right() - 40, rr.cy() - 7, 2, if n.known { t.text_dim } else { t.text_faint });
        }
        ui.text_right(PX, R::new(rr.x, rr.y, rr.w - 4, rr.h), &format!("{}", n.signal), t.text_faint);
        y += 28;
        if st.wifi_target == n.ssid {
            let focused = st.field_focus == Some("wifi");
            ui.field(R::new(l.x + 30, y, l.w - 130, 30), &st.wifi_pass, "пароль", focused, true, Act::SField("wifi"));
            ui.button(R::new(l.right() - 92, y + 2, 92, 26), "войти", "check", Tone::Primary, false, true, Act::WifiNet(n.ssid.clone()));
            y += 44;
        }
    }
}

// ── bluetooth ───────────────────────────────────────────────────────────────
fn bt(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.bt;
    let conn = s.devices.iter().filter(|d| d.connected).count();
    let sub = if !s.present { "нет адаптера".to_string() } else if conn > 0 { crate::tf!("подключено {conn}", "{conn} connected") } else { String::new() };
    let mut y = head(ui, b, "bt", "bluetooth", &sub);
    if !s.present {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "адаптер не найден (bluez не запущен?)", t.text_faint);
        return;
    }
    ui.toggle(R::new(b.x, y, 150, 26), "включён", "", s.powered, Act::BtPower(!s.powered));
    // как у Noctalia: при включении адаптера — подключить доверенные (автоподключение)
    ui.toggle(R::new(b.x + 160, y, 290, 26), "подключать при включении", "", st.set.bt_connect_on, Act::BtConnectOn);
    if s.powered {
        ui.button(R::new(b.right() - 110, y, 110, 24), if s.discovering { "ищу…" } else { "искать" }, "search", Tone::Ghost, s.discovering, !s.discovering, Act::BtScan);
    }
    if !s.msg.is_empty() {
        ui.text_in(PX, R::new(b.x, b.bottom() - 54, b.w, 20), &s.msg, t.warn);
    }
    y += 36;
    ui.button(R::new(b.x, b.bottom() - 28, b.w, 28), "менеджер устройств", "gear", Tone::Plain, false, true, Act::BtManager);
    if !s.powered {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "выключен", t.text_faint);
        return;
    }
    // слева сопряжённые, справа найденные рядом
    let cw = (b.w - 17) / 2;
    let bottom = b.bottom() - 36;
    ui.cv.fill(R::new(b.x + cw + 8, y, 1, bottom - y), t.border);
    let mut max = [0, 0];
    for (k, (col, mine)) in [(R::new(b.x, y, cw, bottom - y), true), (R::new(b.x + cw + 17, y, cw, bottom - y), false)].into_iter().enumerate() {
        // пустое место колонки: щелчок сворачивает раскрытое, колесо листает (строки — поверх)
        ui.zone(col, Act::BtList(mine), Cur::Default);
        let title = if mine { "мои устройства" } else { "рядом" };
        ui.text_in(PX, R::new(col.x, col.y, col.w, 18), title, t.text_faint);
        let top = col.y + 24;
        let list: Vec<&crate::net::BtDev> = s.devices.iter().filter(|d| d.paired == mine).collect();
        if list.is_empty() {
            let hint = if mine { "пока ни одного" } else if s.discovering { "ищу…" } else { "нажми «искать»" };
            ui.text_in(PX, R::new(col.x, top, col.w, 20), hint, t.text_faint);
        }
        // сколько строк влезает; раскрытое занимает две
        let fit = ((col.bottom() - top + 4) / 34).max(1);
        let extra = list.iter().any(|d| st.bt_open == d.path) as i32;
        max[k] = (list.len() as i32 + extra - fit).max(0);
        let mut skip = st.bt_scroll[k].clamp(0, max[k]);
        // раскрытое не должно уйти за низ колонки: оно занимает две строки — докрутить, чтобы
        // влезло целиком (и не выше его самого)
        if let Some(i) = list.iter().position(|d| st.bt_open == d.path) {
            let i = i as i32;
            skip = skip.max(i + 2 - fit).min(i).clamp(0, max[k]);
        }
        shown[k] = skip;
        let skip = skip as usize;
        let mut y = top;
        let rw = if max[k] > 0 { col.w - 8 } else { col.w };
        for d in list.iter().skip(skip) {
            let open = st.bt_open == d.path;
            if y + 30 + if open { 32 } else { 0 } > col.bottom() {
                break;
            }
            bt_row(ui, st, d, R::new(col.x, y, rw, 30), open);
            y += if open { 66 } else { 34 };
        }
        // полоса прокрутки: где окно среди всего списка
        if max[k] > 0 {
            let track = R::new(col.right() - 3, top, 3, col.bottom() - top);
            let total = list.len() as i32 + extra;
            let th = (track.h * fit / total).max(12);
            let ty = track.y + (track.h - th) * skip as i32 / max[k];
            ui.cv.fill(track, t.border.fade(0.5));
            ui.cv.fill(R::new(track.x, ty, 3, th), t.red);
        }
    }
    st.bt_scroll_max.set(max);
    st.bt_shown.set(shown);
}

/// строка устройства; раскрытая — с кнопками под ней
fn bt_row(ui: &mut Ui, st: &State, d: &crate::net::BtDev, r: R, open: bool) {
    let t = ui.t;
    ui.button(r, "", "", Tone::Ghost, d.connected || open, true, Act::BtOpen(d.path.clone()));
    let (ix, iy) = (r.x + 8, r.cy() - 7);
    ui.cv.icon(crate::net::bt_icon(&d.icon, &d.name), ix, iy, 2, if d.connected { t.ember } else { t.text_dim });
    // метка «автоподключение» (доверено) — галочка в углу значка, как у blueman
    if d.paired && d.trusted {
        let (bx, by) = (ix + 10, iy + 9);
        ui.cv.fill(R::new(bx - 1, by - 1, 7, 6), t.surface);
        for (dx, dy) in [(0, 2), (1, 3), (2, 2), (3, 1), (4, 0)] {
            ui.cv.fill(R::new(bx + dx, by + dy, 1, 1), t.ok);
        }
    }
    // справа: заряд или сигнал, состояние
    let state = if st.bt.busy == d.path {
        crate::i18n::tr("подождите…").to_string()
    } else if d.connected {
        crate::i18n::tr("подключено").to_string()
    } else {
        String::new()
    };
    let right = match (d.battery, d.rssi) {
        (Some(p), _) => format!("{p}%"),
        (None, Some(v)) if !d.paired => format!("{v} dBm"),
        _ => String::new(),
    };
    let info = [right.as_str(), state.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" · ");
    let iw = if info.is_empty() { 0 } else { ui.text_right(PX, R::new(r.x, r.y, r.w - 8, r.h), &info, t.text_faint) };
    if let Some(p) = d.battery {
        ui.cv.gauge(R::new(r.right() - 16 - iw - 26, r.cy() - 4, 20, 8), 4, p as f32 / 100.0, if p <= 15 { t.ember } else { t.ok }, t.surface.lerp(t.border, 0.3), 1);
    }
    let nw = r.w - 40 - iw - if d.battery.is_some() { 30 } else { 0 };
    ui.text_in(PX, R::new(r.x + 30, r.y, nw, r.h), &d.name, if d.connected { t.text_hi } else { t.text });
    if !open {
        return;
    }
    let y = r.bottom() + 4;
    let busy = st.bt.busy == d.path;
    if d.paired {
        // подключить, автоподключение (= доверие в BlueZ, горит — включено) и мусорка — забыть
        let tw = 30;
        let bw = (r.w - tw - 12) / 2;
        ui.button(R::new(r.x, y, bw, 26), if d.connected { "отключить" } else { "подключить" }, "", Tone::Plain, false, !busy, Act::BtDev(d.path.clone()));
        ui.button(R::new(r.x + bw + 6, y, bw, 26), "автоподключение", "", Tone::Plain, d.trusted, !busy, Act::BtTrust(d.path.clone()));
        ui.button(R::new(r.right() - tw, y, tw, 26), "", "trash", Tone::Plain, false, !busy, Act::BtForget(d.path.clone()));
    } else {
        ui.button(R::new(r.x, y, r.w, 26), if busy { "сопрягаю…" } else { "сопрячь и подключить" }, "bt", Tone::Primary, false, !busy, Act::BtDev(d.path.clone()));
    }
}

// ── батарея ─────────────────────────────────────────────────────────────────
fn battery(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let y = head(ui, b, "bolt", "батарея", "");
    let Some(bat) = st.sys.bat else { return };
    let col = if bat.charging || bat.plugged { t.ok } else if bat.pct <= 15 { t.ember } else { t.red };
    ui.cv.battery(b.x, y + 3, 2, bat.pct as f32 / 100.0, t.text_dim, col, t.surface_hi.lerp(t.border, 0.45));
    let state = if bat.charging { "заряжается" } else if bat.plugged { "от сети" } else { "от батареи" };
    let w = ui.text_in(PX, R::new(b.x + 30, y, 60, 20), &format!("{}%", bat.pct), t.text);
    ui.text_in(PX, R::new(b.x + 30 + w + 10, y, b.w - 140, 20), state, t.text_dim);
    if let Some(m) = bat.mins {
        let left = if m >= 60 { crate::tf!("{} ч {} мин", "{} h {} min", m / 60, m % 60) } else { crate::tf!("{} мин", "{} min", m) };
        ui.text_right(PX, R::new(b.x, y, b.w, 20), &left, t.text_faint);
    }
}

// ── система ─────────────────────────────────────────────────────────────────
fn sys(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.sys;
    let up = std::fs::read_to_string("/proc/uptime").ok().and_then(|x| x.split_whitespace().next().and_then(|v| v.parse::<f64>().ok())).unwrap_or(0.0) as u64;
    let y = head(ui, b, "cpu", "система", &crate::tf!("работает {} ч {} мин", "up {} h {} min", up / 3600, up / 60 % 60));
    ui.button(R::new(b.right() - 104, b.y - 2, 104, 22), "процессы", "cpu", Tone::Ghost, false, true, Act::SysMon);
    // 2×2: процессор, видеокарта, память, сеть — подпись, значение и график
    let cw = (b.w - 10) / 2;
    let ch = 84;
    let gpu = if s.gpu_sleep { crate::i18n::tr("сон").to_string() } else if s.gpu_temp > 0.0 { format!("{:.0}%  {:.0}°", s.gpu_load * 100.0, s.gpu_temp) } else { "—".into() };
    let full = s.h_rx.max().max(s.h_tx.max()).max(64.0 * 1024.0);
    let cards: [(&str, &str, String, String); 4] = [
        ("cpu", "CPU", format!("{:.0}%  {:.0}°", s.cpu * 100.0, s.temp), "cpu".into()),
        ("bolt", "GPU", gpu, "gpu_load".into()),
        ("ram", "RAM", crate::tf!("{:.1} / {:.0} ГБ", "{:.1} / {:.0} GB", s.mem_used, s.mem_total), "ram".into()),
        ("net", "NET", format!("↓ {}  ↑ {}", crate::sys::rate(s.rx), crate::sys::rate(s.tx)), "rx".into()),
    ];
    for (i, (icon, label, val, src)) in cards.iter().enumerate() {
        let r = R::new(b.x + (i as i32 % 2) * (cw + 10), y + (i as i32 / 2) * (ch + 8), cw, ch);
        ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
        let inr = r.inset(8);
        ui.cv.icon(icon, inr.x, inr.y + 3, 2, t.red);
        ui.text_in(PX, R::new(inr.x + 20, inr.y, 60, 20), label, t.text_dim);
        ui.text_right(PX, R::new(inr.x, inr.y, inr.w, 20), val, t.text);
        let gr = R::new(inr.x, inr.y + 24, inr.w, inr.h - 24);
        let tail = |ser: &crate::sys::Series, k: f32| ser.tail(140).iter().map(|v| v / k).collect::<Vec<f32>>();
        match src.as_str() {
            // процессор: загрузка и температура второй линией
            "cpu" => widgets::graph(ui, gr, &tail(&s.h_cpu, 1.0), "line", "scroll", st.sys_phase(), if s.temp > 80.0 { t.ember } else { t.red }, Some((&tail(&s.h_temp, 100.0), t.accent2))),
            "gpu_load" => widgets::graph(ui, gr, &tail(&s.h_gpu_load, 1.0), "line", "scroll", st.sys_phase(), t.red, Some((&tail(&s.h_gpu, 100.0), t.accent2))),
            "rx" => widgets::graph(ui, gr, &tail(&s.h_rx, full as f32), "line", "scroll", st.sys_phase(), t.red, Some((&tail(&s.h_tx, full as f32), t.accent2))),
            _ => {
                let (ser, k) = s.series(src);
                widgets::graph(ui, gr, &tail(ser, k), "bars", "scroll", st.sys_phase(), t.red, None);
            }
        }
    }
    // процессы: имя, CPU, память; диск — справа в шапке списка
    let mut y = y + 2 * (ch + 8) + 2;
    let (cx, mx) = (b.right() - 150, b.right() - 70);
    ui.text_in(PX, R::new(b.x, y, 200, 18), "процессы", t.text_faint);
    ui.text_right(PX, R::new(cx, y, 70, 18), "CPU", t.text_faint);
    ui.text_right(PX, R::new(mx, y, 70, 18), "RAM", t.text_faint);
    let dw = ui.text_in(PX, R::new(b.x + 120, y, 60, 18), "диск /", t.text_faint);
    ui.cv.gauge(R::new(b.x + 128 + dw, y + 5, cx - b.x - 160 - dw, 8), 20, s.disk, t.red, t.surface.lerp(t.border, 0.3), 2);
    ui.text_right(PX, R::new(b.x, y, cx - b.x - 8, 18), &format!("{:.0}%", s.disk * 100.0), t.text_dim);
    y += 22;
    for p in &st.procs.top {
        if y + 18 > b.bottom() {
            break;
        }
        ui.text_in(PX, R::new(b.x, y, cx - b.x - 8, 18), &p.name, t.text);
        ui.text_right(PX, R::new(cx, y, 70, 18), &format!("{:.1}%", p.cpu * 100.0), if p.cpu > 0.5 { t.ember } else { t.text_dim });
        ui.text_right(PX, R::new(mx, y, 70, 18), &crate::sys::rate(p.mem), t.text_dim);
        y += 20;
    }
}

impl App {
    pub fn popup_key(&mut self, e: &KeyEvent) {
        if self.st.drop.is_some() && e.keysym == Keysym::Escape {
            return self.drop_close();
        }
        if self.wh_key(e) || self.calc_key_event(e) {
            return;
        }
        if self.st.field_focus == Some("wifi") {
            match e.keysym {
                Keysym::Escape => {
                    self.st.field_focus = None;
                    self.st.wifi_target.clear();
                    self.resize_popup();
                }
                Keysym::Return | Keysym::KP_Enter => {
                    let ssid = self.st.wifi_target.clone();
                    self.wifi_connect(&ssid);
                }
                _ => {
                    let mods = self.mods;
                    App::edit_field(&mut self.st.wifi_pass, e, &mods);
                }
            }
            self.dirty(&SKind::Popup);
            return;
        }
        if e.keysym == Keysym::Escape {
            self.close_popup();
        }
    }

    /// щелчок по сети. Подключённая — отключить; новая с паролем — сначала поле; иначе пробуем,
    /// а если NetworkManager попросит пароль (не сохранён, лежал в KWallet живой сессии, сменили),
    /// поток вернёт `ask` и поле откроется само (app.rs, Msg::Net)
    pub fn wifi_connect(&mut self, ssid: &str) {
        if self.st.net.busy {
            return;
        }
        let Some(n) = self.st.net.nets.iter().find(|n| n.ssid == ssid).cloned() else { return };
        if n.active {
            let by = if n.uuid.is_empty() { ["id".to_string(), n.ssid.clone()] } else { ["uuid".to_string(), n.uuid.clone()] };
            crate::net::wifi_cmd(vec!["connection".into(), "down".into(), by[0].clone(), by[1].clone()], self.tx.clone(), self.st.net.clone());
            return;
        }
        let typed = self.st.wifi_target == ssid && !self.st.wifi_pass.text.is_empty();
        if !n.known && n.secure && !typed && !n.sec.contains("802.1X") {
            self.st.wifi_target = ssid.to_string();
            self.st.wifi_pass.clear();
            self.st.field_focus = Some("wifi");
            self.resize_popup();
            self.dirty(&SKind::Popup);
            return;
        }
        let pass = typed.then(|| std::mem::take(&mut self.st.wifi_pass.text));
        self.st.wifi_target.clear();
        self.st.wifi_pass.clear();
        self.st.field_focus = None;
        crate::net::wifi_connect(n, pass, self.tx.clone(), self.st.net.clone());
        self.resize_popup();
    }
}
