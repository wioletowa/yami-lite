// Настройки: рисование простых вкладок (цвет, виджеты, вид, система, уведомления, док,
// о программе). Вкладки побольше — в своих файлах (bartab, games, wall); клики — setclick.rs.

use crate::app::State;
use crate::font::PX;
use crate::gfx::{BoxStyle, C, R};
use crate::settings::{num_btn, opt_btn, title};
use crate::theme::{parse_hex, PRESETS, WHITE};
use crate::ui::{Act, Cur, Tone, Ui};
use crate::widgets::{self, name_of, ANIMS, COLORS, KINDS, SOURCES, STYLES};

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
    let mut y = title(ui, b.x, b.y, tw, "палитра — шелл и выбранные программы сразу");
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
    ui.toggle(R::new(b.x, y, b.w, 40), "палитра из обоев", "акцент из картинки, с новыми обоями — заново", s.wall_accent, Act::SWall("accent"));
    y += 50;
    // цвета обоев по весу: щелчок — этот акцент (и тот же номер на следующих обоях); справа — второй
    if !st.wall_cands.is_empty() {
        ui.text_in(PX, R::new(b.x, y, 150, 30), "цвета обоев", t.text_dim);
        let pick = (s.wall_pick as usize).min(st.wall_cands.len() - 1);
        for (i, c) in st.wall_cands.iter().enumerate() {
            let r = R::new(b.x + 150 + i as i32 * 44, y, 36, 30);
            let sel = s.wall_accent && i == pick;
            let hov = ui.hover(r);
            let cc = C(0xff000000 | c);
            ui.cv.pxbox(r, &BoxStyle::new(cc, if sel { t.text_hi } else if hov { t.red } else { t.border }).bevel(true));
            if sel {
                ui.cv.fill(R::new(r.x + 4, r.bottom() - 6, r.w - 8, 2), t.text_hi);
            }
            ui.zone(r, Act::SWallPick(i), Cur::Pointer);
        }
        let x2 = b.x + 150 + 5 * 44 + 20;
        ui.text_in(PX, R::new(x2, y, 80, 30), "второй", t.text_dim);
        let c2 = t.accent2;
        ui.cv.pxbox(R::new(x2 + 70, y, 36, 30), &BoxStyle::new(c2, t.border).bevel(true));
        y += 42;
    }
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
    // тема: тёмная или светлая — та же палитра
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

/// чужие программы в цветах палитры (themeApps): только цвета, файл рядом с конфигом и одна строка
/// подключения в нём; выключенная — строка уходит, остальной конфиг не тронут
fn app_themes(ui: &mut Ui, st: &State, b: R, tw: i32) {
    let t = ui.t;
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, tw, "программы в цветах палитры");
    for (i, (id, name, hint)) in crate::recolor::APPS.iter().enumerate() {
        let on = s.theme_apps.iter().any(|a| a == id);
        let x = if i % 2 == 0 { b.x } else { b.x + half + 20 };
        ui.toggle(R::new(x, y, half, 40), name, hint, on, Act::SThemeApp(id));
        if i % 2 == 1 {
            y += 46;
        }
    }
    if crate::recolor::APPS.len() % 2 == 1 {
        y += 46;
    }
    ui.wrap(PX, R::new(b.x, y, b.w, 60), "файл цветов — yami-lite-colors.* рядом с конфигом программы; выключил — строка подключения уходит. Темы, значки и курсоры не ставятся", t.text_faint, 3);
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
        let drop = match key {
            "source" => "w_source",
            "style" => "w_style",
            "anim" => "w_anim",
            "size" => "w_size",
            _ => "w_color",
        };
        opt_btn(ui, R::new(px, *y, pw, 26), label, val, Act::Drop(drop));
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
        _ => {}
    }
    row(ui, &mut y, "размер", name_of(widgets::sizes_of(&w.kind), &w.size), "size");
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
    // для всех виджетов: шрифт и читаемость без подложки
    y = title(ui, px, y, pw, "все виджеты");
    y += crate::fonttab::row(ui, st, R::new(px, y, pw, 26), "font_widgets", "шрифт") + 8;
    ui.toggle(R::new(px, y, pw, 40), "в тон обоям", "на светлом месте — тёмный текст, у букв тень", s.widget_contrast, Act::SBool("widget_contrast"));
}


pub(crate) fn tab_look(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, b.w, "шрифт и значки");
    y += crate::fonttab::row(ui, st, R::new(b.x, y, b.w, 26), "font_shell", "шрифт шелла") + 8;
    ui.toggle(R::new(b.x, y, half, 40), "пиксельные значки", "значки программ — 16×16 в тонах палитры", s.icon_pixel, Act::SBool("icon_pixel"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "значки в цвет палитры", "", s.icon_tint, Act::SBool("icon_tint"));
    y += 50;
    // размытие рисует niri (ext-background-effect), если оно включено в его конфиге
    ui.toggle(R::new(b.x, y, b.w, 26), "размытие под панелями (если включено в niri)", "", s.blur_shell, Act::SBool("blur_shell"));
    y += 38;
    y = title(ui, b.x, y, b.w, "анимации");
    ui.toggle(R::new(b.x, y, half, 26), "анимации", "", s.animations, Act::SBool("animations"));
    ui.toggle(R::new(b.x + half + 20, y, half, 26), "анимация закрытия", "", s.anim_close, Act::SBool("anim_close"));
    y += 34;
    ui.slider(R::new(b.x, y, b.w, 40), "скорость анимаций (меньше — быстрее)", (s.anim_speed - 0.3) / 1.7, &format!("×{:.1}", s.anim_speed), 17, Act::SSlider("anim_speed", 0.3, 2.0, 17));
    y += 50;
    opt_btn(ui, R::new(b.x, y, half, 26), "появление", reveal_name(&s.reveal), Act::Drop("reveal"));
    num_btn(ui, R::new(b.x + half + 20, y, half, 26), "размер клеток", s.num("reveal_grain"), "reveal_grain");
    y += 34;
    ui.toggle(R::new(b.x, y, b.w, 26), "строки развёртки на панелях (как на старом мониторе)", "", s.scanlines, Act::SBool("scanlines"));
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
    let mins = |m: u32| if m == 0 { "никогда".to_string() } else { crate::tf!("{m} мин", "{m} min") };
    ui.slider(R::new(lx, y, half, 40), "заблокировать через", s.lock_after as f32 / 60.0, &mins(s.lock_after), 12, Act::SSlider("lock_after", 0.0, 60.0, 20));
    ui.slider(R::new(rx, y, half, 40), "погасить экран через", s.screen_off_after as f32 / 60.0, &mins(s.screen_off_after), 12, Act::SSlider("screen_off_after", 0.0, 60.0, 20));
    y += 52;
    // ночной свет: wlsunset; по координатам — от заката до восхода, иначе по часам
    y = title(ui, b.x, y, b.w, "ночной свет: по солнцу или по часам");
    let has = crate::proc::which("wlsunset");
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
    // погода — вкладка панели управления; город виден только здесь
    y = title(ui, b.x, y, b.w, "погода");
    let wf = st.field_focus == Some("weather_city");
    ui.field(R::new(lx, y, half, 26), &st.weather_field, if s.weather_city.is_empty() { "город или широта, долгота" } else { &s.weather_city }, wf, false, Act::SField("weather_city"));
    opt_btn(ui, R::new(rx, y, half, 26), "единицы", if s.weather_units == "f" { "°F, mph" } else { "°C, м/с" }, Act::SCycle("weather_units"));
    y += 40;
    y = title(ui, b.x, y, b.w, "язык и музыка");
    let lang = match s.lang.as_str() {
        "ru" => "Русский", // i18n-ok: name of the language
        "en" => "English",
        _ => "авто",
    };
    opt_btn(ui, R::new(lx, y, half, 26), "язык", lang, Act::Drop("lang"));
    y += 34;
    ui.toggle(R::new(lx, y, half, 40), "обложки из сети", "если плеер не дал свою", s.media_art_net, Act::SBool("media_art_net"));
    y += 48;
    y = title(ui, b.x, y, b.w, "быстро");
    let bw = (b.w - 18) / 4;
    for (i, (label, icon, act)) in [("заблокировать", "lock", Act::SCycle("do_lock")), ("микшер", "speaker", Act::Pavu), ("процессы", "cpu", Act::SysMon), ("перечитать", "reboot", Act::SCycle("reload"))].into_iter().enumerate() {
        ui.button(R::new(b.x + i as i32 * (bw + 6), y, bw, 28), label, icon, Tone::Plain, false, true, act);
    }
    y += 34;
    // для тех, кто правит руками: файл открывается в редакторе, справка — yami-lite config
    ui.button(R::new(b.x, y, bw * 2 + 6, 28), "файл настроек", "pen", Tone::Plain, false, true, Act::SCycle("open_config"));
    ui.text_in(PX, R::new(b.x + bw * 2 + 18, y, b.w - bw * 2 - 18, 28), "или: yami-lite config list", ui.t.text_faint);
}

/// уведомления: когда приходят, сколько висят и где; OSD — на что отвечает, где и сколько висит
pub(crate) fn tab_notif(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = title(ui, b.x, b.y, b.w, "когда показывать");
    ui.toggle(R::new(lx, y, half, 40), "не беспокоить", "показываются только срочные", s.dnd, Act::SBool("dnd"));
    ui.toggle(R::new(rx, y, half, 40), "тихо при демонстрации", "пока показываешь экран — сразу в историю", s.cast_dnd, Act::SBool("cast_dnd"));
    y += 48;
    y = title(ui, b.x, y, b.w, "сколько висят");
    ui.slider(R::new(lx, y, half, 40), "время на экране", (s.notif_timeout as f32 - 2.0) / 28.0, &crate::tf!("{} с", "{} s", s.notif_timeout), 14, Act::SSlider("notif_timeout", 2.0, 30.0, 14));
    ui.toggle(R::new(rx, y, half, 40), "пауза под курсором", "навёл — карточка не уходит", s.notif_hover_pause, Act::SBool("notif_hover_pause"));
    y += 48;
    ui.toggle(R::new(lx, y, half, 40), "срочные — до закрытия", "иначе уходят по тому же времени", s.notif_critical_stay, Act::SBool("notif_critical_stay"));
    ui.toggle(R::new(rx, y, half, 40), "время от программы", "если программа просит своё (до минуты)", s.notif_app_timeout, Act::SBool("notif_app_timeout"));
    y += 52;
    y = title(ui, b.x, y, b.w, "карточки");
    opt_btn(ui, R::new(lx, y, half, 26), "на экране сразу", &s.notif_max.to_string(), Act::Drop("notif_max"));
    opt_btn(ui, R::new(rx, y, half, 26), "в истории", &s.notif_history.to_string(), Act::Drop("notif_history"));
    y += 32;
    opt_btn(ui, R::new(lx, y, half, 26), "место", crate::widgets::name_of(crate::notif::POS, &s.notif_pos), Act::Drop("notif_pos"));
    num_btn(ui, R::new(rx, y, half, 26), "сверху / снизу", s.num("notif_offset"), "notif_offset");
    y += 32;
    if s.notif_pos != "top" && s.notif_pos != "bottom" {
        num_btn(ui, R::new(rx, y, half, 26), "от края экрана", s.num("notif_offset_x"), "notif_offset_x");
    }
    y += 32;
    ui.toggle(R::new(lx, y, b.w, 26), "картинки и значки программ", "", s.notif_images, Act::SBool("notif_images"));
    y += 40;
    y = title(ui, b.x, y, b.w, "OSD");
    let cw = (b.w - 24) / 3;
    for (i, (k, name)) in crate::notif::OSD_KINDS.iter().enumerate() {
        let on = s.osd_kinds.iter().any(|x| x == k);
        let r = R::new(b.x + (i as i32 % 3) * (cw + 12), y + (i as i32 / 3) * 32, cw, 26);
        ui.toggle(r, name, "", on, Act::SOsd(k));
    }
    y += 70;
    ui.slider(R::new(lx, y, half, 40), "время на экране", (s.osd_time - 0.5) / 4.5, &crate::tf!("{:.1} с", "{:.1} s", s.osd_time), 9, Act::SSlider("osd_time", 0.5, 5.0, 9));
    opt_btn(ui, R::new(rx, y + 8, half, 26), "место", crate::widgets::name_of(crate::notif::OSD_POS, &s.osd_pos), Act::Drop("osd_pos"));
    y += 48;
    ui.button(R::new(lx, y, half, 26), "показать", "eye", Tone::Plain, false, true, Act::SCycle("osd_test"));
    if s.osd_pos != "center" {
        num_btn(ui, R::new(rx, y, half, 26), "отступ", s.num("osd_offset"), "osd_offset");
    }
    y += 40;
    y = title(ui, b.x, y, b.w, "службы");
    ui.toggle(R::new(lx, y, half, 40), "сервер уведомлений", "выключи, если пользуешься mako или dunst", s.agent_notif, Act::SBool("agent_notif"));
    ui.toggle(R::new(rx, y, half, 40), "трей", "выключи, если трей держит другая программа", s.agent_tray, Act::SBool("agent_tray"));
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
    ui.toggle(R::new(b.x, y, half, 40), "кнопка лаунчера", "команда — в настройках модуля «лаунчер»", s.dock_launcher, Act::SBool("dock_launcher"));
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
    opt_btn(ui, R::new(b.x, y, b.w, 26), "край экрана", side, Act::Drop("dock_side"));
    y += 32;
    opt_btn(ui, R::new(b.x, y, b.w, 26), "прятать", hide, Act::Drop("dock_hide"));
    y += 32;
    let anim = match s.dock_anim.as_str() {
        "wave" => "волна — соседи тоже растут",
        "lift" => "подъём без роста",
        "none" => "нет",
        _ => "рост и подъём",
    };
    opt_btn(ui, R::new(b.x, y, b.w, 26), "при наведении", anim, Act::Drop("dock_anim"));
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



pub(crate) fn tab_about(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let mut y = b.y;
    let face = crate::font::px(32);
    ui.text(face, b.x, y, "YAMI-LITE", t.red);
    y += ui.f.line(face) + 4;
    ui.text_in(PX, R::new(b.x, y, b.w, 20), &format!("yami-lite {}", env!("CARGO_PKG_VERSION")), t.text_dim);
    y += 34;
    let up = st.started.elapsed().as_secs();
    let rows = [
        ("память шелла", crate::tf!("{:.1} МБ", "{:.1} MB", st.rss)),
        ("работает", crate::tf!("{} ч {} мин", "{} h {} min", up / 3600, up / 60 % 60)),
        ("окон", st.niri.wins.len().to_string()),
        ("уведомлений в истории", st.notifs.history.len().to_string()),
        ("настройки", "~/.config/yami-lite/settings.json".to_string()),
        ("команды", "yami-lite msg power|settings|lock|…".to_string()),
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
