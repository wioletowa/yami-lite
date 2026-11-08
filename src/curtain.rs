// Шторка для трансляций (Mod+Shift+H, `yami-shell msg curtain`): зрители стрима, демки в
// Discord или записи OBS видят застывший экран — размытым, крупными пикселями или одни обои,
// без надписей, — а ты работаешь как обычно. На yami-niri шторка — слой с правилом
// `show-only-in "screencast"` (его видит только трансляция), над ней — значок «скрыто» только
// для тебя (`block-out-from`). На niri без `show-only-in` (сток, старый форк) слой показался бы и
// тебе, поэтому там вместо шторки окна на время закрываются в трансляции чёрным.

use crate::app::{App, Msg, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, R};
use crate::ui::{Act, Cur, Ui};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::collections::HashMap;
use std::rc::Rc;

pub const STYLES: &[(&str, &str)] = &[("blur", "размытие"), ("pixels", "пиксели"), ("wall", "обои")];
pub const NS: &str = "yami-curtain";
pub const BADGE_NS: &str = "yami-curtain-badge";
const BADGE_TEXT: [&str; 2] = ["экран скрыт в трансляции", "показать экран"];

/// шторка включена; картинки по мониторам (пусто — стиль «обои» или обычный niri)
#[derive(Default)]
pub struct Curtain {
    pub imgs: HashMap<String, Rc<Img>>,
    /// прежние слои шторки: уходят, когда новый (поверх свежей панели) уже нарисован
    old: Vec<u32>,
}

pub fn style_name(id: &str) -> &'static str {
    STYLES.iter().find(|s| s.0 == id).map(|s| s.1).unwrap_or(STYLES[0].1)
}

pub fn next_style(id: &str) -> &'static str {
    let i = STYLES.iter().position(|s| s.0 == id).unwrap_or(0);
    STYLES[(i + 1) % STYLES.len()].0
}

/// правила niri: слой шторки — только в трансляции, значок и (если так выбрано) уведомления —
/// только на экране. `hide-from` (форк ≥ 5) их просто не рисует в захвате; `block-out-from`
/// старого niri заливает весь слой чёрным, вместе с полями тени
pub fn kdl(lvl: u32, on: bool, notif_cast: bool, l: &mut Vec<String>) {
    let hide = if lvl >= 5 { "    hide-from \"screen-capture\"" } else { "    block-out-from \"screen-capture\"" };
    l.push("// privacy curtain (settings → screenshots), notifications (settings → services)".into());
    for s in ["layer-rule {", &format!("    match namespace=\"^{BADGE_NS}$\""), hide, "}"] {
        l.push(s.into());
    }
    if !notif_cast {
        for s in ["layer-rule {", "    match namespace=\"^yami-notif$\"", hide, "}"] {
            l.push(s.into());
        }
    }
    if lvl >= 4 {
        for s in ["layer-rule {", &format!("    match namespace=\"^{NS}$\""), "    show-only-in \"screencast\"", "}"] {
            l.push(s.into());
        }
    } else if on {
        // обычный niri: показать шторку только трансляции он не умеет — прячем сами окна
        for s in ["window-rule {", "    block-out-from \"screencast\"", "}"] {
            l.push(s.into());
        }
    }
    l.push(String::new());
}

/// застывший кадр монитора → то, что увидят зрители: только картинка, без надписей
pub fn cover_of(src: &Img, style: &str, cell: i32) -> Option<Img> {
    let raw: Vec<u8> = src.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]).collect();
    let img = image::RgbaImage::from_raw(src.w as u32, src.h as u32, raw)?;
    let (w, h) = (src.w as u32, src.h as u32);
    let out = if style == "pixels" {
        let c = cell.clamp(4, 96) as u32;
        image::imageops::resize(&img, (w / c).max(1), (h / c).max(1), image::imageops::FilterType::Triangle)
    } else {
        // размытие: сильное уменьшение и мягкое увеличение до четверти (на экране ×4)
        let small = image::imageops::resize(&img, (w / 32).max(1), (h / 32).max(1), image::imageops::FilterType::Triangle);
        image::imageops::resize(&small, (w / 4).max(1), (h / 4).max(1), image::imageops::FilterType::CatmullRom)
    };
    drop(img);
    Some(crate::icons::from_rgba(&out))
}

/// шторка одного монитора (видят только зрители): кадр, а без него — обои
pub fn draw(ui: &mut Ui, st: &State, out: &str, w: i32, h: i32) {
    ui.cv.clear(ui.t.bg);
    if let Some(img) = st.curtain.as_ref().and_then(|c| c.imgs.get(out)).or(st.wall.as_ref()) {
        crate::wall::cover(ui, img, w, h);
    }
}

/// ширина значка по надписи (язык и шрифт шелла бывают разные): точка, текст, поля, тень
pub fn badge_w(f: &mut crate::font::Fonts) -> i32 {
    BADGE_TEXT.iter().map(|s| f.width(PX, crate::i18n::tr(s))).max().unwrap_or(180) + 28 + 10 + 4
}

/// значок для тебя: шторка опущена, щелчок поднимает
pub fn draw_badge(ui: &mut Ui, w: i32, h: i32) {
    let t = ui.t;
    let r = R::new(0, 0, w - 4, h - 4);
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), if hov { t.ember } else { t.red }).shadow(4));
    ui.cv.fill(R::new(r.x + 12, r.y + (r.h - 8) / 2, 8, 8), t.red);
    ui.text_in(PX, R::new(r.x + 28, r.y, r.w - 38, r.h), BADGE_TEXT[hov as usize], if hov { t.text } else { t.text_dim });
    ui.zone(r, Act::Curtain, Cur::Pointer);
}

impl App {
    pub fn curtain_on(&self) -> bool {
        self.st.curtain.is_some()
    }

    /// `curtain`, `curtain on|off`
    pub fn curtain_cmd(&mut self, arg: &str) {
        let on = match arg.trim() {
            "on" => true,
            "off" => false,
            _ => !self.curtain_on(),
        };
        if on == self.curtain_on() {
            return;
        }
        if !on {
            // слой шторки на мониторе не рисуется — закрывать с анимацией нечего, сразу убрать
            let old = self.st.curtain.take().map(|c| c.old).unwrap_or_default();
            self.curtain_wanted = 0;
            for id in self.find(&SKind::Curtain).into_iter().chain(old) {
                self.destroy(id);
            }
            self.close_kind(&SKind::CurtainBadge);
            self.write_fx();
            self.show_osd("curtain");
            return;
        }
        self.st.curtain = Some(Curtain::default());
        self.curtain_wanted += 1;
        self.write_fx();
        self.show_osd("curtain");
        let out = self.focus_out();
        let (bw, bh) = (badge_w(&mut self.res.fonts), 34);
        let top = self.bar_bottom(&out) + 8;
        self.mk(SKind::CurtainBadge, &out, Layer::Overlay, BADGE_NS, Anchor::TOP, (bw, bh), (top, 0, 0, 0), -1, KeyboardInteractivity::None);
        if !self.st.niri.cast_only {
            return;
        }
        let style = self.st.set.curtain_style.clone();
        if style == "wall" {
            self.curtain_show();
            return;
        }
        // кадр — до того, как что-то успеет поменяться; обработка в нити
        let (n, tx, cell, outs) = (self.curtain_wanted, self.tx.clone(), self.st.set.curtain_cell, self.output_names());
        std::thread::spawn(move || {
            let imgs: HashMap<String, Img> = outs.into_iter().filter_map(|o| crate::snap::grab(&o).and_then(|f| cover_of(&f, &style, cell)).map(|i| (o, i))).collect();
            let _ = tx.send(Msg::Curtain(n, imgs));
        });
    }

    pub fn curtain_ready(&mut self, n: u64, imgs: HashMap<String, Img>) {
        if n != self.curtain_wanted {
            return;
        }
        if let Some(c) = self.st.curtain.as_mut() {
            c.imgs = imgs.into_iter().map(|(k, v)| (k, Rc::new(v))).collect();
            self.curtain_show();
        }
    }

    /// слои шторки на мониторах, где их ещё нет (и на новом мониторе): ввод проходит насквозь
    pub fn curtain_show(&mut self) {
        if !self.curtain_on() || !self.st.niri.cast_only {
            return;
        }
        for o in self.output_names() {
            if self.find_on(&SKind::Curtain, &o).is_none() {
                self.curtain_mk(&o);
            }
        }
    }

    fn curtain_mk(&mut self, out: &str) {
        let id = self.mk(SKind::Curtain, out, Layer::Overlay, NS, Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
        if let Some(s) = self.surfs.get_mut(&id) {
            s.input = Some(vec![]);
        }
    }

    /// панель шелла в слое overlay открылась поверх шторки (порядок слоя — по созданию), и
    /// зрители увидели бы её: новая шторка встаёт выше, старая уходит после её первого кадра
    pub fn curtain_raise(&mut self) {
        let cur = self.find(&SKind::Curtain);
        if cur.is_empty() {
            return;
        }
        if let Some(c) = self.st.curtain.as_mut() {
            c.old.extend(cur);
        }
        for o in self.output_names() {
            self.curtain_mk(&o);
        }
    }

    /// слой шторки нарисован: прежние на этом мониторе больше не нужны
    pub fn curtain_drawn(&mut self, id: u32, out: &str) {
        let Some(c) = self.st.curtain.as_mut() else { return };
        let surfs = &self.surfs;
        let (gone, keep): (Vec<u32>, Vec<u32>) = c.old.iter().partition(|o| **o != id && surfs.get(o).map(|s| s.output == out).unwrap_or(true));
        c.old = keep;
        for o in gone {
            self.destroy(o);
        }
    }
}
