// Выбор шрифта списком с поиском: шелла (вкладка «вид») и виджетов стола (вкладка «виджеты»).
// Первым — Tiny5 (вшит), вторым — системный (sans-serif), дальше установленные семейства (fc-list);
// у виджетов сверху ещё «как у шелла».

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::settings::opt_btn;
use crate::ui::{Act, Cur, Ui};
use std::sync::OnceLock;

/// строк списка на виду
const ROWS: usize = 10;
const ROW_H: i32 = 26;

/// установленные семейства (один раз за жизнь шелла)
fn installed() -> &'static Vec<String> {
    static L: OnceLock<Vec<String>> = OnceLock::new();
    L.get_or_init(|| {
        let out = std::process::Command::new("fc-list").args([":", "family"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        // «DejaVu Sans,DejaVu Sans Condensed» — основное имя первым
        let mut v: Vec<String> = out.lines().filter_map(|l| l.split(',').next()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("tiny5")).collect();
        v.sort_by_key(|s| s.to_lowercase());
        v.dedup();
        v
    })
}

/// подпись значения (у виджетов "" — как у шелла)
fn shown(key: &str, v: &str) -> String {
    match v {
        "" if key == "font_widgets" => crate::i18n::tr("как у шелла").to_string(),
        "" => crate::i18n::tr("Tiny5 (пиксельный)").to_string(),
        "sans" => crate::i18n::tr("системный").to_string(),
        "tiny5" => crate::i18n::tr("Tiny5 (пиксельный)").to_string(),
        _ => v.to_string(),
    }
}

/// варианты под поиск: "" (Tiny5 или «как у шелла»), у виджетов ещё Tiny5, "sans", установленные
fn options(key: &str, q: &str) -> Vec<String> {
    let q = q.trim().to_lowercase();
    let hit = |s: &str| q.is_empty() || s.to_lowercase().contains(&q);
    let head: &[&str] = if key == "font_widgets" { &["", "tiny5", "sans"] } else { &["", "sans"] };
    let mut v: Vec<String> = head.iter().filter(|x| hit(&shown(key, x))).map(|x| x.to_string()).collect();
    v.extend(installed().iter().filter(|s| hit(s)).cloned());
    v
}

fn value<'a>(st: &'a State, key: &str) -> &'a str {
    if key == "font_widgets" { &st.set.font_widgets } else { &st.set.font_shell }
}

/// строка выбора шрифта (key — font_shell или font_widgets); открыт список — поиск и семейства под
/// ней. Возвращает высоту
pub fn row(ui: &mut Ui, st: &State, r: R, key: &'static str, label: &str) -> i32 {
    let t = ui.t;
    opt_btn(ui, r, label, &shown(key, value(st, key)), Act::SFontOpen(key));
    if st.font_open != Some(key) {
        return r.h;
    }
    let mut y = r.bottom() + 6;
    let focused = st.field_focus == Some("font_search");
    ui.field(R::new(r.x, y, r.w, 28), &st.font_search, "поиск шрифта", focused, false, Act::SField("font_search"));
    y += 34;
    let list = options(key, &st.font_search.text);
    let box_r = R::new(r.x, y, r.w, ROWS as i32 * ROW_H + 4);
    ui.cv.pxbox(box_r, &BoxStyle::new(t.code_bg, t.border));
    ui.zone(box_r, Act::SFontScroll, Cur::Default);
    let top = st.font_scroll.min(list.len().saturating_sub(ROWS));
    for (i, name) in list.iter().enumerate().skip(top).take(ROWS) {
        let rr = R::new(box_r.x + 2, box_r.y + 2 + (i - top) as i32 * ROW_H, box_r.w - 4, ROW_H);
        let on = name.eq_ignore_ascii_case(value(st, key));
        if on || ui.hover(rr) {
            ui.cv.fill(rr, if on { t.selection } else { t.surface_hi });
        }
        // имена семейств — как есть; Tiny5 и «системный» переводятся
        ui.cv.text_in(ui.f, PX, R::new(rr.x + 10, rr.y, rr.w - 40, rr.h), &shown(key, name), if on { t.text_hi } else { t.text });
        if on {
            ui.cv.icon("check", rr.right() - 22, rr.cy() - 7, 2, t.ember);
        }
        ui.zone(rr, Act::SFont(name.clone()), Cur::Pointer);
    }
    if list.is_empty() {
        ui.text_in(PX, R::new(box_r.x + 10, box_r.y + 6, box_r.w - 20, 20), "ничего не нашлось", t.text_faint);
    }
    if list.len() > ROWS {
        let h = box_r.h - 4;
        let th = (h * ROWS as i32 / list.len() as i32).max(12);
        let ty = box_r.y + 2 + (h - th) * top as i32 / (list.len() - ROWS) as i32;
        ui.cv.fill(R::new(box_r.right() - 5, ty, 3, th), t.red);
    }
    box_r.bottom() + 6 - r.y
}

impl App {
    pub fn font_pick(&mut self, name: String) {
        let Some(key) = self.st.font_open else { return };
        // у виджетов Tiny5 — отдельный пункт: "" там значит «как у шелла»
        let v = if key == "font_widgets" && name == "tiny5" { "Tiny5".to_string() } else { name };
        if key == "font_widgets" {
            self.st.set.font_widgets = v;
        } else {
            self.st.set.font_shell = v;
        }
        self.st.font_open = None;
        self.st.field_focus = None;
        self.settings_changed();
    }

    pub fn font_toggle(&mut self, key: &'static str) {
        self.st.font_open = if self.st.font_open == Some(key) { None } else { Some(key) };
        self.st.font_search.clear();
        self.st.font_scroll = 0;
        self.st.field_focus = self.st.font_open.map(|_| "font_search");
        self.dirty(&SKind::Settings);
    }

    pub fn font_scroll(&mut self, d: i32) {
        let n = options(self.st.font_open.unwrap_or("font_shell"), &self.st.font_search.text).len();
        self.st.font_scroll = (self.st.font_scroll as i32 + d * 3).clamp(0, n.saturating_sub(ROWS) as i32) as usize;
        self.dirty(&SKind::Settings);
    }

    /// Enter в поиске — первый найденный
    pub fn font_first(&mut self) {
        if let Some(n) = options(self.st.font_open.unwrap_or("font_shell"), &self.st.font_search.text).into_iter().next() {
            self.font_pick(n);
        }
    }
}
