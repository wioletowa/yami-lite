// Настройки: рисование простых вкладок (цвет, виджеты, вид, система, док, окна, скриншоты,
// о программе). Вкладки побольше — в своих файлах (bartab, fonttab, games, keys, displays,
// agents, updtab, wall); клики всех вкладок — setclick.rs.

use crate::app::State;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, C, R};
use crate::settings::{num_btn, opt_btn, title};
use crate::theme::{parse_hex, PRESETS, WHITE};
use crate::ui::{Act, Cur, Tone, Ui};
use crate::widgets::{self, name_of, ANIMS, COLORS, KINDS, SIZES, SOURCES, STYLES};

pub(crate) fn tab_color(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    // три раздела: палитра, свои темы (#8), программы в цветах палитры (#17)
    // кнопки разделов справа налево, каждая по своей надписи (по-английски она короче)
    let mut x = b.right();
    for (i, (name, icon, act)) in [("палитра", "drop", "color_page_pal"), ("темы", "grid", "color_page_themes"), ("программы", "term", "color_page_apps")].into_iter().enumerate().rev() {
        let bw = ui.w(PX, name) + 36;
        x -= bw;
        ui.button(R::new(x, b.y - 3, bw, 22), name, icon, Tone::Ghost, st.color_page == i as u8, true, Act::SCycle(act));
        x -= 6;
    }
    let tw = x - b.x - 6;
    match st.color_page {
        1 => return crate::themetab::tab(ui, st, b, tw),
        2 => return app_themes(ui, st, b, tw),
        _ => {}
    }
    let mut y = title(ui, b.x, b.y, tw, "палитра — весь райс перекрашивается сразу");
    let cw = (b.w - 6 * 8) / 7;
    for (i, p) in PRESETS.iter().enumerate() {
        let r = R::new(b.x + i as i32 * (cw + 8), y, cw, 64);
        let sel = s.accent.is_empty() && s.palette == p.id;
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(if sel { t.selection } else { t.surface }, if sel { t.ember } else if hov { t.red } else { t.border }).bevel(true).shadow(2));
        let sw = R::new(r.x + 8, r.y + 8, r.w - 16, 26);
        let c = C(0xff000000 | p.accent);
        ui.cv.pxbox(sw, &BoxStyle::new(c, c.lerp(WHITE, 0.3)).bevel(true));
        ui.text_center(PX, R::new(r.x, r.y + 38, r.w, 22), p.name, if sel { t.text_hi } else { t.text_dim });
        ui.zone(r, Act::SPreset(i), Cur::Pointer);
    }
    y += 76;
    // палитра из обоев (#26): акцент из картинки, от него фон, панели, рамки и 16 цветов терминала;
    // та же галочка, что во вкладке «обои», пресет её снимает
    let ph = if s.wall_skwd && s.wall_skwd_colors { "палитру ведёт skwd-wall" } else { "акцент из картинки, с новыми обоями — заново" };
    ui.toggle(R::new(b.x, y, b.w, 40), "палитра из обоев", ph, s.wall_accent, Act::SWall("accent"));
    y += 50;
    y = title(ui, b.x, y, b.w, "свой цвет");
    let ok = parse_hex(&st.hex.text).is_some();
    let focused = st.field_focus == Some("hex");
    ui.field(R::new(b.x, y, 200, 30), &st.hex, "#rrggbb", focused, false, Act::SField("hex"));
    if let Some(h) = parse_hex(&st.hex.text) {
        let c = C(0xff000000 | h);
        ui.cv.pxbox(R::new(b.x + 210, y, 30, 30), &BoxStyle::new(c, t.border).bevel(true));
    }
    ui.button(R::new(b.x + 250, y + 2, 110, 26), "применить", "check", Tone::Primary, false, ok, Act::SCycle("hex_apply"));
    ui.text_in(PX, R::new(b.x + 370, y, b.w - 370, 30), if s.wall_accent { "сейчас: из обоев" } else if s.accent.is_empty() { "сейчас: пресет" } else { "сейчас: свой" }, t.text_faint);
    y += 46;
    y = title(ui, b.x, y, b.w, "оттенки");
    // по два ползунка в ряд: цвет фона слева, прозрачность справа
    let hw = (b.w - 24) / 2;
    let (lx, rx) = (b.x, b.x + hw + 24);
    let pct = |v: f32| format!("{:.0}%", v * 100.0);
    ui.slider(R::new(lx, y, hw, 40), "насыщенность фона", s.tint / 2.0, &format!("{:.1}", s.tint), 12, Act::SSlider("tint", 0.0, 2.0, 20));
    ui.slider(R::new(rx, y, hw, 40), "плотность панелей", (s.glass - 0.15) / 0.85, &pct(s.glass), 12, Act::SSlider("glass", 0.15, 1.0, 17));
    y += 50;
    ui.slider(R::new(lx, y, hw, 40), "яркость панелей", s.lift, &pct(s.lift), 12, Act::SSlider("lift", 0.0, 1.0, 20));
    ui.slider(R::new(rx, y, hw, 40), "карточки и графики", (s.inner - 0.2) / 0.8, &pct(s.inner), 12, Act::SSlider("inner", 0.2, 1.0, 16));
    y += 50;
    ui.slider(R::new(lx, y, hw, 40), "затемнение под меню", s.dim / 0.8, &pct(s.dim), 12, Act::SSlider("dim", 0.0, 0.8, 16));
    // тема: тёмная или светлая — та же палитра, весь райс вместе с шеллом
    ui.text_in(PX, R::new(rx, y, hw, 20), "тема", t.text);
    let bw = (hw - 8) / 2;
    ui.button(R::new(rx, y + 20, bw, 24), "тёмная", "moon", Tone::Plain, !s.light, true, Act::SCycle("theme_dark"));
    ui.button(R::new(rx + bw + 8, y + 20, bw, 24), "светлая", "sun", Tone::Plain, s.light, true, Act::SCycle("theme_light"));
    y += 54;
    // образец
    let r = R::new(b.x, y, b.w, (b.bottom() - y).min(70));
    if r.h > 30 {
        ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), t.border));
        let mut x = r.x + 10;
        for c in [t.bg, t.panel, t.surface, t.surface_hi, t.border, t.dim, t.blood, t.red, t.glow, t.ember, t.text_dim, t.text] {
            ui.cv.pxbox(R::new(x, r.y + 10, 26, r.h - 20), &BoxStyle::new(c, t.border));
            x += 32;
        }
    }
}

/// чужие программы в цветах палитры (themeApps, #17): каждую можно выключить — тогда yami её не трогает
/// (у kitty уходит наша строка include); шелл, экран входа, рамки niri и свои шаблоны — всегда
fn app_themes(ui: &mut Ui, st: &State, b: R, tw: i32) {
    let t = ui.t;
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, tw, "программы в цветах палитры");
    for (i, (id, name, hint)) in crate::config::THEME_APPS.iter().enumerate() {
        let on = s.theme_apps.iter().any(|a| a == id);
        let x = if i % 2 == 0 { b.x } else { b.x + half + 20 };
        ui.toggle(R::new(x, y, half, 40), name, hint, on, Act::SThemeApp(id));
        if i % 2 == 1 {
            y += 46;
        }
    }
    if crate::config::THEME_APPS.len() % 2 == 1 {
        y += 46;
    }
    ui.wrap(PX, R::new(b.x, y, b.w, 40), "выключенную программу yami не трогает; всегда в палитре: шелл, экран входа, рамки niri, свои шаблоны. Курсор — окна → курсор", t.text_faint, 2);
    y += 48;
    // свой конфиг fastfetch, своя палитра starship (#27): yami их не перезаписывает и говорит об этом
    for n in crate::restyle::app_notes(&st.restyle) {
        ui.wrap(PX, R::new(b.x, y, b.w, 40), &n, t.text_dim, 2);
        y += 40;
    }
    y += 6;
    y = title(ui, b.x, y, b.w, "свои шаблоны");
    let help = "файл в ~/.config/yami/templates — конфиг программы, где цвета записаны как {{accent}}, {{bg}}, {{text}}, {{color1}}… (.strip — без #, .rgb — r, g, b). Строка «yami-output: ~/путь» — куда писать, «yami-reload: USR1 программа» — кому сказать перечитать. Изменённый руками файл не перезаписывается, шаблон с неизвестным цветом не пишется; правка в папке шаблонов применяется сама.";
    ui.wrap(PX, R::new(b.x, y, b.w, 120), help, t.text_dim, 6);
    y += 124;
    ui.button(R::new(b.x, y, 220, 28), "папка шаблонов", "folder", Tone::Plain, false, true, Act::SCycle("templates_open"));
    ui.button(R::new(b.x + 230, y, 220, 28), "перекрасить сейчас", "reboot", Tone::Plain, false, true, Act::SCycle("theme_rerun"));
    y += 40;
    // что вышло у каждого шаблона в последний раз (restyle.json)
    crate::restyle::draw(ui, st, b, y);
}

pub(crate) fn tab_widgets(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let y0 = title(ui, b.x, b.y, b.w, "виджеты рабочего стола");
    // список слева
    let lw = 200;
    let mut y = y0;
    for w in &s.widgets {
        let r = R::new(b.x, y, lw, 28);
        let sel = st.sel_widget == w.id;
        ui.button(r, "", "", Tone::Ghost, sel, true, Act::SWidgetSel(w.id));
        let c = if w.on { t.ok } else { t.text_faint };
        ui.cv.fill(R::new(r.x + 8, r.cy() - 3, 6, 6), c);
        let extra = if w.kind == "graph" { format!(" · {}", crate::i18n::tr(name_of(SOURCES, &w.source))) } else { String::new() };
        ui.text_in(PX, R::new(r.x + 22, r.y, r.w - 26, r.h), &format!("{}{}", crate::i18n::tr(name_of(KINDS, &w.kind)), extra), if sel { t.text_hi } else if w.on { t.text } else { t.text_faint });
        y += 30;
    }
    y += 8;
    ui.text_in(PX, R::new(b.x, y, lw, 20), "добавить:", t.text_dim);
    y += 22;
    for (i, (k, n)) in KINDS.iter().enumerate() {
        let r = R::new(b.x + (i as i32 % 2) * (lw / 2 + 2), y + (i as i32 / 2) * 28, lw / 2 - 2, 24);
        ui.button(r, n, "", Tone::Plain, false, true, Act::SWidgetAdd(k));
    }
    // параметры справа
    let px = b.x + lw + 20;
    let pw = b.right() - px;
    let Some(w) = s.widgets.iter().find(|w| w.id == st.sel_widget) else {
        ui.text_in(PX, R::new(px, y0, pw, 20), "выбери виджет слева", t.text_faint);
        return;
    };
    let id = w.id;
    let mut y = y0;
    ui.toggle(R::new(px, y, pw, 26), "показывать", "", w.on, Act::SWidgetOpt(id, "on"));
    y += 34;
    let row = |ui: &mut Ui, y: &mut i32, label: &str, val: &str, key: &'static str| {
        opt_btn(ui, R::new(px, *y, pw, 26), label, val, Act::SWidgetOpt(id, key));
        *y += 30;
    };
    match w.kind.as_str() {
        "graph" => {
            row(ui, &mut y, "источник", name_of(SOURCES, &w.source), "source");
            row(ui, &mut y, "вид графика", name_of(STYLES, &w.style), "style");
            row(ui, &mut y, "анимация", name_of(ANIMS, &w.anim), "anim");
        }
        "net" => {
            row(ui, &mut y, "вид графика", name_of(STYLES, &w.style), "style");
            row(ui, &mut y, "анимация", name_of(ANIMS, &w.anim), "anim");
        }
        "media" => row(ui, &mut y, "эквалайзер", name_of(STYLES, &w.style), "style"),
        "script" => {
            // имя файла — как есть, не переводится
            opt_btn(ui, R::new(px, y, pw, 26), "скрипт", if w.source.is_empty() { "—" } else { &w.source }, Act::SWidgetOpt(id, "source"));
            y += 30;
            if crate::plugins::scripts().is_empty() {
                ui.wrap(crate::font::PX, R::new(px, y, pw, 40), "положи исполняемый скрипт в ~/.config/yami/modules (как модуль бара)", t.text_faint, 2);
                y += 40;
            }
        }
        _ => {}
    }
    row(ui, &mut y, "размер", name_of(SIZES, &w.size), "size");
    row(ui, &mut y, "цвет", name_of(COLORS, &w.color), "color");
    y += 4;
    ui.toggle(R::new(px, y, pw / 2, 26), "подложка", "", w.frame, Act::SWidgetOpt(id, "frame"));
    if matches!(w.kind.as_str(), "graph" | "net" | "media") {
        ui.toggle(R::new(px + pw / 2, y, pw / 2, 26), "подпись", "", w.label, Act::SWidgetOpt(id, "label"));
    }
    if w.kind == "clock" {
        ui.toggle(R::new(px + pw / 2, y, pw / 4, 26), "секунды", "", w.seconds, Act::SWidgetOpt(id, "seconds"));
        ui.toggle(R::new(px + pw * 3 / 4, y, pw / 4, 26), "дата", "", w.date, Act::SWidgetOpt(id, "date"));
    }
    y += 40;
    // живое превью графика
    if matches!(w.kind.as_str(), "graph" | "net") {
        let r = R::new(px, y, pw, 70);
        ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
        let (ser, full) = if w.kind == "net" { (&st.sys.h_rx, st.sys.h_rx.max().max(65536.0)) } else { st.sys.series(&w.source) };
        let data: Vec<f32> = ser.tail(160).iter().map(|v| v / full).collect();
        widgets::graph(ui, r.inset(6), &data, &w.style, &w.anim, st.sys_phase(), widgets::color(&t, &w.color), None);
        y += 80;
    }
    let bw = (pw - 12) / 3;
    ui.button(R::new(px, y, bw, 28), "расставить", "drag", Tone::Primary, false, true, Act::SEdit);
    ui.button(R::new(px + bw + 6, y, bw, 28), "на место", "reboot", Tone::Plain, false, true, Act::SResetPos);
    ui.button(R::new(px + 2 * (bw + 6), y, bw, 28), "убрать", "trash", Tone::Danger, false, true, Act::SWidgetDel(id));
    y += 44;
    // для всех виджетов без подложки: читаемы на любых обоях
    y = title(ui, px, y, pw, "без подложки");
    ui.toggle(R::new(px, y, pw, 40), "в тон обоям", "на светлом месте — тёмный текст, у букв тень", s.widget_contrast, Act::SBool("widget_contrast"));
}

/// пиксельность одним ползунком: 0 — гладко, 1 — мягче, 2 — только yami, 3 — всё пиксельное (по умолчанию)
pub(crate) const PIXEL_LEVELS: [&str; 4] = ["гладко", "мягче", "только yami", "всё пиксельное"];

/// где сейчас ползунок пиксельности: по шрифтам, значкам и курсору (свои сочетания — ближайшее)
pub(crate) fn pixel_level(s: &crate::config::Settings) -> usize {
    let smooth = |f: &str| !f.is_empty() && !font::is_pixel_family(f);
    if smooth(&s.font_shell) {
        0
    } else if !s.icon_pixel || smooth(&s.font_files) || smooth(&s.font_view) {
        1
    } else if smooth(&s.font_ui) {
        2
    } else {
        3
    }
}

pub(crate) fn tab_look(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    // пиксельность и лёгкий режим — первыми: их ищут, когда стиль надоел или стол тормозит
    let mut y = title(ui, b.x, b.y, b.w, "пиксельность и нагрузка");
    let lvl = pixel_level(s);
    ui.slider(R::new(b.x, y, half, 40), "пиксельность", lvl as f32 / 3.0, PIXEL_LEVELS[lvl], 4, Act::SSlider("pixel_level", 0.0, 3.0, 3));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "лёгкий режим", "слабый ПК, виртуалка: без размытия и анимаций", s.lite, Act::SBool("lite"));
    y += 48;
    // размытие рисует niri (ext-background-effect), каждый кадр
    ui.toggle(R::new(b.x, y, half, 26), "размытие под панелями", "", s.blur_shell, Act::SBool("blur_shell"));
    ui.toggle(R::new(b.x + half + 20, y, half, 26), "размытие под терминалами", "", s.blur_term, Act::SBool("blur_term"));
    y += 38;
    y = title(ui, b.x, y, b.w, "анимации");
    ui.toggle(R::new(b.x, y, half, 26), "анимации", "", s.animations, Act::SBool("animations"));
    ui.toggle(R::new(b.x + half + 20, y, half, 26), "анимация закрытия", "", s.anim_close, Act::SBool("anim_close"));
    y += 34;
    ui.slider(R::new(b.x, y, b.w, 40), "скорость анимаций (меньше — быстрее)", (s.anim_speed - 0.3) / 1.7, &format!("×{:.1}", s.anim_speed), 17, Act::SSlider("anim_speed", 0.3, 2.0, 17));
    y += 50;
    // узор проявления и крупность клеток — во всех программах yami; развёртка — только шелл
    opt_btn(ui, R::new(b.x, y, half, 26), "появление", reveal_name(&s.reveal), Act::SCycle("reveal"));
    num_btn(ui, R::new(b.x + half + 20, y, half, 26), "размер клеток", s.num("reveal_grain"), "reveal_grain");
    y += 34;
    ui.toggle(R::new(b.x, y, b.w, 26), "строки развёртки на панелях шелла (как на старом мониторе)", "", s.scanlines, Act::SBool("scanlines"));
    y += 38;
    y = title(ui, b.x, y, b.w, "значки программ");
    y = crate::iconsets::draw_icons(ui, st, b, y);
    y = title(ui, b.x, y, b.w, "курсор");
    crate::iconsets::draw_cursors(ui, st, b, y);
}

/// подпись узора проявления (gfx::Reveal)
pub(crate) fn reveal_name(id: &str) -> &'static str {
    match id {
        "scan" => "развёртка",
        "noise" => "шум",
        "wipe" => "диагональ",
        "blocks" => "блоки",
        _ => "дизеринг",
    }
}

pub(crate) fn tab_system(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = title(ui, b.x, b.y, b.w, "простой");
    let mins = |m: u32| if m == 0 { "никогда".to_string() } else { crate::tf!("{m} мин", "{m} min", "{m} min") };
    ui.slider(R::new(lx, y, half, 40), "заблокировать через", s.lock_after as f32 / 60.0, &mins(s.lock_after), 12, Act::SSlider("lock_after", 0.0, 60.0, 20));
    ui.slider(R::new(rx, y, half, 40), "погасить экран через", s.screen_off_after as f32 / 60.0, &mins(s.screen_off_after), 12, Act::SSlider("screen_off_after", 0.0, 60.0, 20));
    y += 52;
    // ночной свет: wlsunset; по координатам — от заката до восхода, иначе по часам
    y = title(ui, b.x, y, b.w, "ночной свет: по солнцу или по часам");
    let has = crate::doctor::which("wlsunset");
    ui.toggle(R::new(lx, y, half, 40), "теплее вечером", if has { "wlsunset" } else { "нужен wlsunset" }, s.night, Act::SBool("night"));
    num_btn(ui, R::new(rx, y + 6, half, 26), "температура, K", s.num("night_temp"), "night_temp");
    y += 46;
    // координаты есть — по закату и восходу, часы тогда не нужны
    let gf = st.field_focus == Some("night_geo");
    let by_sun = crate::night::geo(&s.night_geo).is_some();
    ui.field(R::new(lx, y, half, 26), &st.geo_field, if s.night_geo.is_empty() { "широта, долгота — по солнцу" } else { &s.night_geo }, gf, false, Act::SField("night_geo"));
    if by_sun {
        ui.text_in(PX, R::new(rx + 8, y, half - 8, 26), "по закату и восходу", ui.t.text_faint);
    } else {
        let qw = (half - 8) / 2;
        num_btn(ui, R::new(rx, y, qw, 26), "с", s.num("night_on_h"), "night_on_h");
        num_btn(ui, R::new(rx + qw + 8, y, qw, 26), "до", s.num("night_off_h"), "night_off_h");
    }
    y += 40;
    y = title(ui, b.x, y, b.w, "язык и вход");
    let lang = match s.lang.as_str() {
        "ru" => "Русский", // i18n-ok: name of the language
        "en" => "English",
        "pt-BR" => "Português (Brasil)",
        _ => "авто",
    };
    opt_btn(ui, R::new(lx, y, half, 26), "язык", lang, Act::SCycle("lang"));
    ui.toggle(R::new(rx, y, half, 26), "вернуть программы при входе", "", s.session_restore, Act::SBool("session_restore"));
    y += 40;
    y = title(ui, b.x, y, b.w, "быстро");
    let bw = (b.w - 18) / 4;
    for (i, (label, icon, act)) in [("заблокировать", "lock", Act::SCycle("do_lock")), ("микшер", "speaker", Act::Pavu), ("процессы", "cpu", Act::SysMon), ("перечитать", "reboot", Act::SCycle("reload"))].into_iter().enumerate() {
        ui.button(R::new(b.x + i as i32 * (bw + 6), y, bw, 28), label, icon, Tone::Plain, false, true, act);
    }
    y += 34;
    // для тех, кто правит руками: файл открывается в редакторе, справка — yami-shell config
    ui.button(R::new(b.x, y, bw * 2 + 6, 28), "файл настроек", "pen", Tone::Plain, false, true, Act::SCycle("open_config"));
    ui.text_in(PX, R::new(b.x + bw * 2 + 18, y, b.w - bw * 2 - 18, 28), "или: yami-shell config list", ui.t.text_faint);
}

/// уведомления: как ведут себя карточки, где они и где громкость с яркостью
pub(crate) fn tab_notif(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = title(ui, b.x, b.y, b.w, "карточки");
    ui.toggle(R::new(lx, y, half, 40), "не беспокоить", "только срочные", s.dnd, Act::SBool("dnd"));
    ui.slider(R::new(rx, y, half, 40), "сколько висит", (s.notif_timeout as f32 - 2.0) / 28.0, &crate::tf!("{} с", "{} s", "{} s", s.notif_timeout), 12, Act::SSlider("notif_timeout", 2.0, 30.0, 14));
    y += 48;
    ui.toggle(R::new(lx, y, half, 40), "картинки в карточках", "значки и превью от программ", s.notif_images, Act::SBool("notif_images"));
    ui.toggle(R::new(rx, y, half, 40), "в записи и трансляциях", "выкл — зрители их не видят", s.notif_cast, Act::SBool("notif_cast"));
    y += 46;
    opt_btn(ui, R::new(lx, y, half, 26), "на экране сразу", &s.notif_max.to_string(), Act::SCycle("notif_max"));
    opt_btn(ui, R::new(rx, y, half, 26), "в истории", &s.notif_history.to_string(), Act::SCycle("notif_history"));
    y += 40;
    y = title(ui, b.x, y, b.w, "где на экране");
    opt_btn(ui, R::new(lx, y, half, 26), "карточки", crate::widgets::name_of(crate::notif::POS, &s.notif_pos), Act::SCycle("notif_pos"));
    opt_btn(ui, R::new(rx, y, half, 26), "громкость и яркость", crate::widgets::name_of(crate::notif::OSD_POS, &s.osd_pos), Act::SCycle("osd_pos"));
    y += 32;
    num_btn(ui, R::new(lx, y, half, 26), "от края", s.num("notif_offset"), "notif_offset");
    if s.osd_pos != "center" {
        num_btn(ui, R::new(rx, y, half, 26), "от края", s.num("osd_offset"), "osd_offset");
    }
    y += 40;
    y = title(ui, b.x, y, b.w, "сервер");
    ui.toggle(R::new(lx, y, b.w, 40), "сервер уведомлений yami", "выкл — mako, dunst и т.п. (после перезапуска шелла)", s.agent_notif, Act::SBool("agent_notif"));
}

pub(crate) fn tab_dock(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, b.w, "док");
    ui.toggle(R::new(b.x, y, half, 40), "показывать док", "закреплённое и открытые окна", s.dock, Act::SBool("dock"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "прыжок при запуске", "значок подскакивает ступенями", s.dock_bounce, Act::SBool("dock_bounce"));
    y += 46;
    ui.toggle(R::new(b.x, y, half, 40), "открытые программы", "не только закреплённые", s.dock_running, Act::SBool("dock_running"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "подписи", "имя или заголовок окна", s.dock_labels, Act::SBool("dock_labels"));
    y += 46;
    ui.toggle(R::new(b.x, y, half, 40), "кнопка программ", "в начале дока", s.dock_launcher, Act::SBool("dock_launcher"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "корзина", "в конце дока", s.dock_trash, Act::SBool("dock_trash"));
    y += 48;
    let side = match s.dock_side.as_str() {
        "top" => "сверху",
        "left" => "слева",
        "right" => "справа",
        _ => "снизу",
    };
    let hide = match s.dock_hide.as_str() {
        "never" => "никогда (занимает место)",
        "always" => "всегда — выезжает у края",
        _ => "умно — виден на пустом столе",
    };
    opt_btn(ui, R::new(b.x, y, b.w, 26), "край экрана", side, Act::SCycle("dock_side"));
    y += 32;
    opt_btn(ui, R::new(b.x, y, b.w, 26), "прятать", hide, Act::SCycle("dock_hide"));
    y += 32;
    let anim = match s.dock_anim.as_str() {
        "wave" => "волна — соседи тоже растут",
        "lift" => "подъём без роста",
        "none" => "нет",
        _ => "рост и подъём",
    };
    opt_btn(ui, R::new(b.x, y, b.w, 26), "при наведении", anim, Act::SCycle("dock_anim"));
    y += 34;
    ui.slider(R::new(b.x, y, b.w, 40), "размер значков", (s.dock_size as f32 - 24.0) / 40.0, &format!("{} px", s.dock_size), 10, Act::SSlider("dock_size", 24.0, 64.0, 10));
    y += 50;
    y = title(ui, b.x, y, b.w, "закреплённые");
    let mut x = b.x;
    for id in s.dock_pins.iter() {
        let Some(a) = st.apps.iter().find(|a| &a.id == id) else { continue };
        let r = R::new(x, y, 30, 30);
        if x + 30 > b.right() {
            break;
        }
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(if hov { t.surface_hi } else { t.surface }, if hov { t.red } else { t.border }));
        ui.app_icon(&a.icon, r.x + 3, r.y + 3, 24, 1.0);
        ui.zone(r, Act::DockPin(id.clone(), 0), Cur::Pointer);
        x += 36;
    }
    ui.text_in(PX, R::new(b.x, y + 36, b.w, 18), "щелчок — убрать · порядок и закрепление — ПКМ по значку в доке", t.text_faint);
}

pub(crate) fn tab_windows(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    // отступы, углы и рамка окон: niri gaps/struts и правила в yami-fx.kdl (применяются сразу)
    let mut y = title(ui, b.x, b.y, b.w, "окна: отступы от краёв и между, углы, рамка");
    let cw = (b.w - 24) / 3;
    let col = |i: i32| b.x + i * (cw + 12);
    let nums = [("win_left", "слева"), ("win_right", "справа"), ("win_gap", "между"), ("win_top", "сверху"), ("win_bottom", "снизу"), ("win_radius", "углы"), ("win_border", "рамка окна")];
    for (i, (k, label)) in nums.into_iter().enumerate() {
        let i = i as i32;
        num_btn(ui, R::new(col(i % 3), y + i / 3 * 32, cw, 26), label, s.num(k), k);
    }
    let t = ui.t;
    let corners = if s.num("win_radius") == 0 { "углы прямые, как пиксели" } else { "игры и окна на весь экран — с прямыми углами" };
    ui.text_in(PX, R::new(col(1) + 4, y + 64, cw * 2 + 8, 26), corners, t.text_faint);
    y += 100;
    y = title(ui, b.x, y, b.w, "плавающие окна");
    ui.toggle(R::new(b.x, y, half, 40), "фото и видео", "просмотр в Telegram, yami-view, mpv…", s.float_media, Act::SBool("float_media"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "картинка в картинке", "в правом нижнем углу", s.float_pip, Act::SBool("float_pip"));
    y += 46;
    ui.slider(R::new(b.x, y, half, 40), "ширина", (s.float_w - 0.3) / 0.65, &format!("{:.0}%", s.float_w * 100.0), 13, Act::SSlider("float_w", 0.3, 0.95, 13));
    ui.slider(R::new(b.x + half + 20, y, half, 40), "высота", (s.float_h - 0.3) / 0.65, &format!("{:.0}%", s.float_h * 100.0), 13, Act::SSlider("float_h", 0.3, 0.95, 13));
    y += 50;
    y = title(ui, b.x, y, b.w, "свои программы");
    let focused = st.field_focus == Some("float_apps");
    ui.field(R::new(b.x, y, b.w - 130, 30), &st.float_field, "app-id через запятую: org.telegram.desktop, imv", focused, false, Act::SField("float_apps"));
    ui.button(R::new(b.right() - 120, y + 2, 120, 26), "применить", "check", Tone::Primary, false, true, Act::SCycle("float_apply"));
    y += 40;
    ui.text_in(PX, R::new(b.x, y, b.w, 20), "app-id окна: niri msg windows · правила пишутся в niri/yami-fx.kdl", t.text_faint);
    y += 28;
    y = title(ui, b.x, y, b.w, "режим «стол»");
    ui.toggle(R::new(b.x, y, half, 40), "окна плавают, как в DE", "Mod+Shift+Space — назад к ленте", s.stacking, Act::SBool("stacking"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "заголовки окон", "полоса с кнопками над окном", s.titlebars, Act::SBool("titlebars"));
}

pub(crate) fn tab_shots(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, b.w, "куда");
    ui.toggle(R::new(b.x, y, half, 40), "в буфер обмена", "сразу вставляй куда угодно", s.shot_copy, Act::SBool("shot_copy"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "в папку", "файл png с датой в имени", s.shot_save, Act::SBool("shot_save"));
    y += 50;
    let focused = st.field_focus == Some("shot_dir");
    let shown = crate::snap::shots_dir(s).display().to_string().replace(&crate::config::home().to_string_lossy().to_string(), "~");
    ui.field(R::new(b.x, y, b.w - 130, 30), &st.shot_field, &shown, focused, false, Act::SField("shot_dir"));
    ui.button(R::new(b.right() - 120, y + 2, 120, 26), "применить", "check", Tone::Primary, false, true, Act::SCycle("shot_dir_apply"));
    y += 46;
    y = title(ui, b.x, y, b.w, "оверлей");
    ui.toggle(R::new(b.x, y, half, 40), "лупа", "пиксели под курсором, ctrl+c — цвет", s.shot_loupe, Act::SBool("shot_loupe"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "уведомление", "с миниатюрой и кнопками", s.shot_notify, Act::SBool("shot_notify"));
    y += 50;
    let d = crate::tf!("{} с", "{} s", "{} s", s.shot_delay);
    opt_btn(ui, R::new(b.x, y, b.w, 26), "таймер (кнопка ◷ в оверлее)", &d, Act::SCycle("shot_delay"));
    y += 32;
    let conf = if s.shot_confirm == "instant" { "сразу, как отпустил мышь" } else { "пробелом или enter" };
    opt_btn(ui, R::new(b.x, y, b.w, 26), "снимок", conf, Act::SCycle("shot_confirm"));
    y += 40;
    // шторка: зрители трансляции видят её, ты — экран (curtain.rs)
    y = title(ui, b.x, y, b.w, "скрыть экран в трансляции");
    opt_btn(ui, R::new(b.x, y, b.w, 26), "шторка", crate::curtain::style_name(&s.curtain_style), Act::SCycle("curtain_style"));
    y += 34;
    ui.slider(R::new(b.x, y, b.w, 40), "размер пикселей шторки", (s.curtain_cell as f32 - 8.0) / 56.0, &format!("{} px", s.curtain_cell), 14, Act::SSlider("curtain_cell", 8.0, 64.0, 14));
    y += 46;
    let t = ui.t;
    if !st.niri.cast_only {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "этот niri не умеет шторку: окна в трансляции станут чёрными", t.warn);
        y += 24;
    }
    y = title(ui, b.x, y, b.w, "клавиши");
    for (k, v) in [("print", "область · окно · экран"), ("mod+shift+h", "скрыть экран в трансляции"), ("в оверлее", "1–7 инструменты · tab режим · enter готово · ctrl+s сохранить как")] {
        ui.text_in(PX, R::new(b.x, y, 130, 20), k, t.red);
        ui.text_in(PX, R::new(b.x + 140, y, b.w - 140, 20), v, t.text_dim);
        y += 22;
    }
}

pub(crate) fn tab_about(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let mut y = b.y;
    let face = font::px(32);
    ui.text(face, b.x, y, "YAMI", t.red);
    y += ui.f.line(face) + 4;
    ui.text_in(PX, R::new(b.x, y, b.w, 20), &crate::tf!("yami-shell {} — лёгкий Wayland-шелл для niri на Rust", "yami-shell {} — a lightweight Wayland shell for niri, written in Rust", "yami-shell {} — um shell Wayland leve para niri, escrito em Rust", env!("CARGO_PKG_VERSION")), t.text_dim);
    y += 34;
    let up = st.started.elapsed().as_secs();
    let rows = [
        ("память шелла", crate::tf!("{:.1} МБ", "{:.1} MB", "{:.1} MB", st.rss)),
        ("работает", crate::tf!("{} ч {} мин", "{} h {} min", "{} h {} min", up / 3600, up / 60 % 60)),
        ("niri", if st.niri.yami { "yami-niri (форк)".to_string() } else { "стоковый".to_string() }),
        ("окон", st.niri.wins.len().to_string()),
        ("уведомлений в истории", st.notifs.history.len().to_string()),
        ("настройки", "~/.config/yami-shell.json".to_string()),
        ("команды", "yami-shell msg launcher|power|settings|lock|…".to_string()),
    ];
    for (k, v) in rows {
        ui.text_in(PX, R::new(b.x, y, 200, 22), k, t.text_faint);
        ui.text_in(PX, R::new(b.x + 210, y, b.w - 210, 22), &v, t.text);
        y += 26;
    }
    y += 10;
    // шкала памяти: шелл против бюджета 64 МБ
    ui.cv.gauge(R::new(b.x, y, b.w, 12), 32, (st.rss / 64.0).min(1.0), t.red, t.surface.lerp(t.border, 0.3), 2);
}
