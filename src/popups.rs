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
pub const KINDS: &[&str] = &["cal", "sound", "media", "net", "bt", "sys", "bat", "tray", "control", "wallhaven", "fan"];

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

/// открытая всплывашка рисует эквалайзер (ему нужен захват звука)
pub fn shows_viz(st: &State) -> bool {
    st.set.media_viz != "off" && ((st.popup_is("media") && media_view(st) == "cover") || (st.popup_is("sound") && sound_media(st)))
}

/// вид всплывашки музыки (#13); текста у трека нет — обложка
pub fn media_view(st: &State) -> &str {
    let v = st.set.media_view.as_str();
    match v {
        "lyrics" | "both" if lyrics_none(st) => "cover",
        "lyrics" | "both" | "minimal" => v,
        _ => "cover",
    }
}

/// текст хотели показать, искали и не нашли
fn lyrics_none(st: &State) -> bool {
    matches!(st.set.media_view.as_str(), "lyrics" | "both") && st.lyrics.done && st.lyrics.lines.is_empty() && !st.media.title.is_empty()
}

/// высота строк текста песни: в виде «текст» и в виде «обложка и текст»
const LYR_TALL: i32 = 232;
const LYR_SHORT: i32 = 160;

pub fn size(kind: &str, st: &State) -> (i32, i32) {
    match kind {
        "cal" => (660, 372),
        "sound" => {
            let apps = st.audio.streams.len().min(6) as i32;
            let list = |n: usize| if n > 1 { n.min(5) as i32 * 26 + 30 } else { 0 };
            let media = if sound_media(st) { 170 } else { 0 };
            (420, 44 + 56 + list(st.audio.sinks.len()) + 60 + list(st.audio.sources.len()) + if apps > 0 { 30 + apps * 46 } else { 0 } + media + 50)
        }
        "media" => {
            let time = if st.media.length > 0.0 { 24 } else { 0 };
            let card = match media_view(st) {
                "cover" if st.set.media_viz != "off" => 156,
                _ => 70,
            };
            let rest = match media_view(st) {
                "lyrics" => LYR_TALL,
                "both" => LYR_SHORT,
                _ if lyrics_none(st) => 22,
                _ => 0,
            };
            (420, 44 + 36 + card + time + rest + 4)
        }
        "net" => {
            let msg = if st.net.msg.is_empty() { 0 } else { 24 };
            let wifi = if st.net.wifi_present { 40 + msg + if st.net.wifi_on { st.net.nets.len().min(8) as i32 * 28 + if st.wifi_target.is_empty() { 0 } else { 44 } } else { 0 } } else { 0 };
            (420, 44 + 24 + st.net.active.len().max(1) as i32 * 22 + 130 + wifi + 50)
        }
        "bt" => (380, 44 + 40 + if st.bt.msg.is_empty() { 0 } else { 24 } + if st.bt.powered { st.bt.devices.len().clamp(1, 8) as i32 * 34 } else { 30 } + 50),
        "sys" => (440, 44 + 4 * 74 + 40 + 50),
        "tray" => (300, 66 + tray_items(st).filter(|it| !tray_in_bar(st, it)).count().clamp(1, 12) as i32 * 32),
        "bat" => (340, 44 + 40),
        "control" => (crate::control::SIZE.0 + 38, crate::control::SIZE.1 + 30),
        "wallhaven" => (crate::wallhaven::SIZE.0 + 38, crate::wallhaven::SIZE.1 + 30),
        "fan" => (crate::fan::SIZE.0 + 38, crate::fan::SIZE.1 + 30),
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
    let body = R::new(16, 12, fr.w - 32, fr.h - 24);
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
        "wallhaven" => crate::wallhaven::draw(ui, st, body),
        "fan" => crate::fan::draw(ui, st, body),
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
        y += ch + 6;
    }
    ui.cv.pop_clip();
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
    let view = media_view(st);
    // шапка: справа — какой вид (щелчок — следующий), между — имя плеера
    let name = crate::widgets::name_of(crate::config::MEDIA_VIEWS, &st.set.media_view);
    let bw = ui.w(PX, name) + 20;
    let mut y = head(ui, b, "music", "музыка", "");
    let tw = ui.w(PX, "музыка");
    let nr = R::new(b.x + 30 + tw, b.y, b.w - bw - 40 - tw, 20);
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
    ui.button(R::new(b.right() - bw, b.y - 2, bw, 22), name, "", Tone::Ghost, false, true, Act::MediaView);
    // обложка с эквалайзером; без эквалайзера и в остальных видах — строка плеера высотой в обложку
    let viz = if view == "cover" { st.set.media_viz.as_str() } else { "off" };
    let art = matches!(view, "cover" | "both");
    if viz != "off" {
        widgets::media_card(ui, st, R::new(b.x, y, b.w, 150), viz, t.red, true, art);
        y += 156;
    } else {
        widgets::media_card(ui, st, R::new(b.x, y, b.w, 94), "off", t.red, true, art);
        y += 70;
    }
    if m.length > 0.0 {
        // время: где сейчас (или куда тянут) и сколько всего
        let at = st.seek.map(|f| f as f64 * m.length).unwrap_or(m.now_pos());
        ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w / 2, 20), &yami_px::clock(at), if st.seek.is_some() { t.red } else { t.text_dim });
        ui.cv.text_right(ui.f, PX, R::new(b.x, y, b.w, 20), &yami_px::clock(m.length), t.text_faint);
        y += 24;
    }
    match view {
        "lyrics" => lyrics(ui, st, R::new(b.x, y, b.w, LYR_TALL)),
        "both" => lyrics(ui, st, R::new(b.x, y, b.w, LYR_SHORT)),
        _ if lyrics_none(st) => {
            ui.text_in(PX, R::new(b.x, y, b.w, 20), "текста не нашлось", t.text_faint);
        }
        _ => {}
    }
}

/// текст песни: с метками времени — звучащая строка в акценте и держится на трети высоты,
/// спетые — тусклее; простой текст листается вместе с треком
fn lyrics(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let ly = &st.lyrics;
    ui.cv.fill(R::new(r.x, r.y, r.w, 1), t.border);
    let r = R::new(r.x, r.y + 8, r.w, r.h - 12);
    if !ly.done {
        ui.text_in(PX, R::new(r.x, r.y, r.w, 20), "ищу текст…", t.text_faint);
        return;
    }
    // строки по ширине: (номер строки текста, кусок); текст — как есть, без перевода
    let lh = ui.f.line(PX) + 6;
    let mut rows: Vec<(usize, String)> = vec![];
    for (i, l) in ly.lines.iter().enumerate() {
        // пустая строка (проигрыш) — один пустой ряд: wrap("") сам отдаёт [""]
        for part in ui.f.wrap(PX, &l.text, r.w - 8, 3) {
            rows.push((i, part));
        }
    }
    let vis = (r.h / lh).max(1) as usize;
    let pos = st.media.now_pos();
    let cur = ly.now(pos);
    let first = match cur {
        Some(c) => rows.iter().position(|(i, _)| *i == c).unwrap_or(0).saturating_sub(vis / 3),
        None if ly.synced => 0,
        None => {
            let k = if st.media.length > 0.0 { (pos / st.media.length).clamp(0.0, 1.0) } else { 0.0 };
            (rows.len().saturating_sub(vis) as f64 * k) as usize
        }
    };
    let first = first.min(rows.len().saturating_sub(vis));
    ui.cv.push_clip(r);
    for (k, (i, s)) in rows.iter().skip(first).take(vis).enumerate() {
        let c = match cur {
            Some(c) if *i == c => t.red,
            Some(c) if *i < c => t.text_faint,
            _ => t.text_dim,
        };
        // звучит проигрыш: на месте строки — многоточие
        let s = if s.is_empty() && cur == Some(*i) { "…" } else { s.as_str() };
        ui.cv.text_in(ui.f, PX, R::new(r.x + 4, r.y + k as i32 * lh, r.w - 8, lh), s, c);
    }
    ui.cv.pop_clip();
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
    let mut y = head(ui, b, "net", "сеть", &sub);
    if st.net.active.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), if st.net.loaded { "не подключено" } else { "смотрю…" }, t.text_faint);
        y += 22;
    }
    for (name, kind, dev) in &st.net.active {
        ui.cv.fill(R::new(b.x, y + 7, 6, 6), t.ok);
        ui.text_in(PX, R::new(b.x + 14, y, b.w - 120, 20), name, t.text);
        ui.text_right(PX, R::new(b.x, y, b.w, 20), &format!("{kind} · {dev}"), t.text_faint);
        y += 22;
    }
    y += 6;
    // ↓↑ график
    let full = s.h_rx.max().max(s.h_tx.max()).max(64.0 * 1024.0);
    let rx: Vec<f32> = s.h_rx.tail(120).iter().map(|v| v / full).collect();
    let tx: Vec<f32> = s.h_tx.tail(120).iter().map(|v| v / full).collect();
    let w1 = ui.text_in(PX, R::new(b.x, y, 160, 20), &crate::tf!("↓ {}/с", "↓ {}/s", crate::sys::rate(s.rx)), t.red);
    ui.text_in(PX, R::new(b.x + w1 + 16, y, 160, 20), &crate::tf!("↑ {}/с", "↑ {}/s", crate::sys::rate(s.tx)), t.text_dim);
    ui.text_right(PX, R::new(b.x, y, b.w, 20), &crate::tf!("пик {}/с", "peak {}/s", crate::sys::rate(full as f64)), t.text_faint);
    y += 24;
    let gr = R::new(b.x, y, b.w, 90);
    ui.cv.pxbox(gr, &BoxStyle::new(t.code_bg, t.border));
    widgets::graph(ui, gr.inset(6), &rx, "line", "scroll", st.sys_phase(), t.red, Some((&tx, t.accent2)));
    y += 100;
    // Wi-Fi
    if st.net.wifi_present {
        ui.toggle(R::new(b.x, y, 140, 26), "wi-fi", "", st.net.wifi_on, Act::Wifi(!st.net.wifi_on));
        if st.net.wifi_on {
            ui.button(R::new(b.right() - 110, y, 110, 24), "искать", "search", Tone::Ghost, false, !st.net.busy, Act::WifiRescan);
        }
        y += 34;
        if !st.net.msg.is_empty() {
            let c = if st.net.busy { t.text_dim } else { t.warn };
            ui.text_in(PX, R::new(b.x, y - 4, b.w, 20), &st.net.msg, c);
            y += 24;
        }
        if st.net.wifi_on {
            for n in st.net.nets.iter().take(8) {
                let r = R::new(b.x, y, b.w, 26);
                ui.button(r, "", "", Tone::Ghost, n.active, !st.net.busy, Act::WifiNet(n.ssid.clone()));
                // сила сигнала — 4 столбика
                for k in 0..4 {
                    let on = n.signal as i32 > k * 25;
                    ui.cv.fill(R::new(r.x + 6 + k * 4, r.y + 18 - k * 3, 3, 3 + k * 3), if on { if n.active { t.ember } else { t.red } } else { t.border });
                }
                ui.text_in(PX, R::new(r.x + 30, r.y, r.w - 90, r.h), &n.ssid, if n.active { t.text_hi } else { t.text });
                if n.secure {
                    ui.cv.icon("lock", r.right() - 40, r.cy() - 7, 2, if n.known { t.text_dim } else { t.text_faint });
                }
                ui.text_right(PX, R::new(r.x, r.y, r.w - 4, r.h), &format!("{}", n.signal), t.text_faint);
                y += 28;
                if st.wifi_target == n.ssid {
                    let focused = st.field_focus == Some("wifi");
                    ui.field(R::new(b.x + 30, y, b.w - 130, 30), &st.wifi_pass, "пароль", focused, true, Act::SField("wifi"));
                    ui.button(R::new(b.right() - 92, y + 2, 92, 26), "войти", "check", Tone::Primary, false, true, Act::WifiNet(n.ssid.clone()));
                    y += 44;
                }
            }
        }
    }
    ui.button(R::new(b.x, b.bottom() - 28, b.w, 28), "все подключения", "gear", Tone::Plain, false, true, Act::NetEditor);
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
    ui.toggle(R::new(b.x, y, 160, 26), "включён", "", s.powered, Act::BtPower(!s.powered));
    if s.powered {
        ui.button(R::new(b.right() - 110, y, 110, 24), if s.discovering { "ищу…" } else { "искать" }, "search", Tone::Ghost, s.discovering, !s.discovering, Act::BtScan);
    }
    y += 36;
    if !s.msg.is_empty() {
        ui.text_in(PX, R::new(b.x, y - 4, b.w, 20), &s.msg, t.warn);
        y += 24;
    }
    if !s.powered {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "выключен", t.text_faint);
    } else if s.devices.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "устройств нет — нажми «искать»", t.text_faint);
    }
    if s.powered {
        for d in s.devices.iter().take(8) {
            let r = R::new(b.x, y, b.w, 30);
            ui.button(r, "", "", Tone::Ghost, d.connected, true, Act::BtDev(d.path.clone()));
            let icon = match d.icon.as_str() {
                i if i.contains("audio") || i.contains("headset") => "music",
                i if i.contains("input") || i.contains("keyboard") || i.contains("mouse") => "grid",
                i if i.contains("phone") => "bell",
                _ => "bt",
            };
            ui.cv.icon(icon, r.x + 8, r.cy() - 7, 2, if d.connected { t.ember } else { t.text_dim });
            ui.text_in(PX, R::new(r.x + 30, r.y, r.w - 130, r.h), &d.name, if d.connected { t.text_hi } else { t.text });
            let st_txt = if s.busy == d.path { "подождите…".to_string() } else if d.connected { "подключено".to_string() } else if d.paired { "сопряжено".to_string() } else { "новое".to_string() };
            let st_txt = match d.battery {
                Some(p) => format!("{p}% · {}", crate::i18n::tr(&st_txt)),
                None => st_txt,
            };
            ui.text_right(PX, R::new(r.x, r.y, r.w - 8, r.h), &st_txt, t.text_faint);
            y += 34;
        }
    }
    ui.button(R::new(b.x, b.bottom() - 28, b.w, 28), "менеджер устройств", "gear", Tone::Plain, false, true, Act::BtManager);
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
    let mut y = head(ui, b, "cpu", "система", &crate::tf!("работает {} ч {} мин", "up {} h {} min", up / 3600, up / 60 % 60));
    let rows: [(&str, &str, String, &str); 4] = [
        ("cpu", "процессор", format!("{:.0}%", s.cpu * 100.0), "cpu"),
        ("ram", "память", crate::tf!("{:.1} / {:.0} ГБ", "{:.1} / {:.0} GB", s.mem_used, s.mem_total), "ram"),
        ("temp", "температура", format!("{:.0}°C", s.temp), "temp"),
        ("temp", "видеокарта", if s.gpu_sleep { "сон".into() } else if s.gpu_temp > 0.0 { format!("{:.0}°C  {:.0}%", s.gpu_temp, s.gpu_load * 100.0) } else { "—".into() }, "gpu"),
    ];
    for (icon, label, val, src) in rows {
        ui.cv.icon(icon, b.x, y + 3, 2, t.red);
        ui.text_in(PX, R::new(b.x + 22, y, 200, 20), label, t.text_dim);
        ui.text_right(PX, R::new(b.x, y, b.w, 20), &val, t.text);
        let (ser, full) = s.series(src);
        let data: Vec<f32> = ser.tail(140).iter().map(|v| v / full).collect();
        let gr = R::new(b.x, y + 24, b.w, 42);
        widgets::graph(ui, gr, &data, "bars", "scroll", st.sys_phase(), if src == "temp" && s.temp > 80.0 { t.ember } else { t.red }, None);
        y += 74;
    }
    ui.text_in(PX, R::new(b.x, y, 140, 20), "диск /", t.text_dim);
    ui.cv.gauge(R::new(b.x + 90, y + 4, b.w - 150, 12), 24, s.disk, t.red, t.surface.lerp(t.border, 0.3), 2);
    ui.text_right(PX, R::new(b.x, y, b.w, 20), &format!("{:.0}%", s.disk * 100.0), t.text);
    ui.button(R::new(b.x, b.bottom() - 28, b.w, 28), "процессы", "cpu", Tone::Plain, false, true, Act::SysMon);
}

impl App {
    pub fn popup_key(&mut self, e: &KeyEvent) {
        if self.st.drop.is_some() && e.keysym == Keysym::Escape {
            return self.drop_close();
        }
        if self.wh_key(e) {
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
