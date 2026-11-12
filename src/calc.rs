// Калькулятор: вкладка панели управления. Слева — кнопки (цифры, + − × ÷, скобки, степени, √, %,
// π), справа — выражение с ответом и история под ним. Считает по мере набора (ответ бледный), «=»
// кладёт строку в историю; щелчок по строке истории — её ответ обратно в выражение. С клавиатуры —
// тоже: цифры, знаки, Enter, Backspace, Delete. История — в ~/.local/state/yami-lite/calc.json.

use crate::app::{App, SKind, State};
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Ui};
use serde::{Deserialize, Serialize};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

/// сколько строк истории хранить
const KEEP: usize = 50;
const ROW: i32 = 40;

#[derive(Default, Serialize, Deserialize)]
pub struct Calc {
    /// выражение, как набрано
    #[serde(skip)]
    pub expr: String,
    /// только что нажали «=»: цифра начнёт новое выражение, знак — продолжит ответ
    #[serde(skip)]
    pub done: bool,
    /// пропущено строк истории (колесо)
    #[serde(skip)]
    pub scroll: usize,
    /// когда ответ ушёл с экрана в историю (новый ввод после «=»): строка съезжает вниз
    #[serde(skip)]
    pub fly: Option<std::time::Instant>,
    /// выражение и ответ, свежие — первыми; пока done, первая — на экране, а не в списке
    pub history: Vec<(String, String)>,
}

fn path() -> std::path::PathBuf {
    yami_px::paths::state_dir().join("calc.json")
}

impl Calc {
    pub fn load() -> Calc {
        std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self) {
        if cfg!(test) {
            return;
        }
        if let Ok(s) = serde_json::to_string(self) {
            let _ = yami_px::paths::write_atomic(&path(), &s);
        }
    }

    /// кнопка или клавиша: "0"…"9", ".", знаки, "(", ")", "√", "π", "x²", "C", "del", "="
    pub fn press(&mut self, k: &str) {
        let op = matches!(k, "+" | "−" | "×" | "÷" | "^" | "%" | "x²");
        let was_done = self.done;
        if self.done && !op && !matches!(k, "del" | "=") {
            self.expr.clear();
        }
        self.done = false;
        match k {
            "C" => self.expr.clear(),
            "del" => {
                self.expr.pop();
            }
            "=" if was_done => self.done = true,
            "=" => {
                let Ok(v) = eval(&self.expr) else { return };
                let ans = fmt(v);
                if self.history.first().map(|h| (&h.0, &h.1)) != Some((&self.expr, &ans)) {
                    self.history.insert(0, (std::mem::take(&mut self.expr), ans.clone()));
                    self.history.truncate(KEEP);
                    self.save();
                }
                self.expr = ans;
                self.scroll = 0;
                self.done = true;
            }
            "x²" => self.expr.push('²'),
            // два знака подряд — второй заменяет первый (минус после ×÷^ — знак числа)
            "+" | "−" | "×" | "÷" | "^" => {
                let last = self.expr.chars().last();
                let unary = k == "−" && matches!(last, Some('×' | '÷' | '^' | '('));
                if !unary && matches!(last, Some('+' | '−' | '×' | '÷' | '^')) {
                    self.expr.pop();
                }
                self.expr.push_str(k);
            }
            _ => self.expr.push_str(k),
        }
        if was_done && !self.done {
            self.fly = Some(std::time::Instant::now());
        }
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.scroll = 0;
        self.save();
    }
}

// ── вычисление ──────────────────────────────────────────────────────────────
// разбор сверху вниз: сумма → произведение → знак → степень (справа налево) → %, ² → число,
// скобки, π, √. Число вплотную к скобке, π или √ — умножение: 2π, 3(1+2), 2√9

struct P<'a> {
    s: &'a [char],
    i: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        let ok = self.peek() == Some(c);
        self.i += ok as usize;
        ok
    }

    fn sum(&mut self) -> Result<f64, ()> {
        let mut v = self.product()?;
        loop {
            if self.eat('+') {
                v += self.product()?;
            } else if self.eat('−') || self.eat('-') {
                v -= self.product()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn product(&mut self) -> Result<f64, ()> {
        let mut v = self.unary()?;
        loop {
            if self.eat('×') || self.eat('*') {
                v *= self.unary()?;
            } else if self.eat('÷') || self.eat('/') {
                v /= self.unary()?;
            } else if matches!(self.peek(), Some('(' | 'π' | '√')) || self.peek().is_some_and(|c| c.is_ascii_digit()) {
                v *= self.power()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn unary(&mut self) -> Result<f64, ()> {
        if self.eat('−') || self.eat('-') {
            return Ok(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<f64, ()> {
        let b = self.postfix()?;
        if self.eat('^') {
            // 2^-1, 2^3^2 = 2^9
            return Ok(b.powf(self.unary()?));
        }
        Ok(b)
    }

    fn postfix(&mut self) -> Result<f64, ()> {
        let mut v = self.atom()?;
        loop {
            if self.eat('%') {
                v /= 100.0;
            } else if self.eat('²') {
                v *= v;
            } else {
                return Ok(v);
            }
        }
    }

    fn atom(&mut self) -> Result<f64, ()> {
        if self.eat('(') {
            let v = self.sum()?;
            // незакрытая скобка в конце — как закрытая: «2×(3+4» уже считается
            if !self.eat(')') && self.peek().is_some() {
                return Err(());
            }
            return Ok(v);
        }
        if self.eat('π') {
            return Ok(std::f64::consts::PI);
        }
        if self.eat('√') {
            return Ok(self.postfix()?.sqrt());
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == ',') {
            self.i += 1;
        }
        // порядок из ответа: 1e20, 1e−7
        if self.i > start && self.eat('e') {
            let _ = self.eat('−') || self.eat('-') || self.eat('+');
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let num: String = self.s[start..self.i].iter().map(|&c| match c {
            ',' => '.',
            '−' => '-',
            c => c,
        }).collect();
        num.parse().map_err(|_| ())
    }
}

pub fn eval(expr: &str) -> Result<f64, ()> {
    let s: Vec<char> = expr.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return Err(());
    }
    let mut p = P { s: &s, i: 0 };
    let v = p.sum()?;
    if p.i < s.len() || !v.is_finite() {
        return Err(());
    }
    Ok(v)
}

/// ответ: целое — без точки, иначе до 10 значащих цифр; очень большое и малое — с порядком
pub fn fmt(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let a = v.abs();
    if a != 0.0 && !(1e-6..1e15).contains(&a) {
        let s = format!("{:.9e}", v);
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let m = m.trim_end_matches('0').trim_end_matches('.');
        return format!("{m}e{e}").replace('-', "−");
    }
    let digits = (9 - a.log10().floor().max(0.0) as i32).max(0) as usize;
    let s = format!("{:.*}", digits, v);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('-', "−")
}

// ── рисование ───────────────────────────────────────────────────────────────

/// кнопки по рядам
const KEYS: [[&str; 5]; 5] = [
    ["C", "del", "(", ")", "÷"],
    ["7", "8", "9", "^", "×"],
    ["4", "5", "6", "√", "−"],
    ["1", "2", "3", "x²", "+"],
    ["%", "0", ".", "π", "="],
];

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let c = &st.calc;
    // ── кнопки: левая половина, плитки без рамок ──
    const GAP: i32 = 6;
    let kw = (b.w - 20) / 2;
    let cw = (kw - 4 * GAP) / 5;
    let ch = (b.h - 4 * GAP) / 5;
    let digit = font::px(24);
    for (j, row) in KEYS.iter().enumerate() {
        for (i, &k) in row.iter().enumerate() {
            let r = R::new(b.x + i as i32 * (cw + GAP), b.y + j as i32 * (ch + GAP), cw, ch);
            let hov = ui.hover(r);
            let num = k.chars().all(|c| c.is_ascii_digit() || c == '.');
            let eq = k == "=";
            let (bg, fg) = if eq {
                (if hov { t.red.lerp(t.ember, 0.45) } else { t.red }, t.on_accent)
            } else if num {
                (if hov { t.surface_hi } else { t.surface }, t.text_hi)
            } else {
                (if hov { t.surface_hi } else { t.code_bg }, if k == "C" { t.ember } else { t.red })
            };
            ui.cv.pxbox(r, &BoxStyle::new(bg, bg));
            if k == "del" {
                ui.cv.icon("left", r.x + (r.w - 14) / 2, r.y + (r.h - 14) / 2, 2, fg);
            } else {
                ui.text_center(if num || eq { digit } else { op_face(k) }, r, k, fg);
            }
            ui.zone(r, Act::CalcKey(k), Cur::Pointer);
        }
    }
    // ── ответ: правая половина сверху ──
    let rx = b.x + kw + 20;
    ui.cv.fill(R::new(rx - 10, b.y, 1, b.h), t.border);
    let rw = b.right() - rx;
    // правый край — как у ответов в истории
    let inner = R::new(rx + 8, b.y + 4, rw - 16, 92);
    let val = eval(&c.expr);
    // выражение — хвостом, если не влезает
    let mut shown = c.expr.as_str();
    while ui.w(PX, shown) > inner.w && !shown.is_empty() {
        let mut it = shown.chars();
        it.next();
        shown = it.as_str();
    }
    let expr_line = if c.done { c.history.first().map_or("", |h| h.0.as_str()) } else { shown };
    ui.text_right(PX, R::new(inner.x, inner.y, inner.w, 18), expr_line, t.text_dim);
    let (ans, col) = match (&val, c.expr.is_empty()) {
        (_, true) => ("0".to_string(), t.text_faint),
        (Ok(v), _) if c.done => (fmt(*v), t.text_hi),
        (Ok(v), _) => (fmt(*v), t.text_dim),
        (Err(_), _) => ("…".to_string(), t.text_faint),
    };
    let ans = if c.done { c.expr.clone() } else { ans };
    // крупно, а длинное — мельче, чтобы влезло
    let face = [48, 32, 24, 16].into_iter().map(font::px).find(|&f| ui.w(f, &ans) <= inner.w).unwrap_or(PX);
    let fh = ui.f.line(face);
    ui.text_right(face, R::new(inner.x, inner.bottom() - fh, inner.w, fh), &ans, col);
    // ── история ──
    let hy = b.y + 112;
    ui.cv.icon("clock", rx, hy + 3, 2, t.red);
    // пока ответ на экране, его строки в списке нет
    let first = c.done as usize;
    let n = c.history.len();
    ui.text_in(PX, R::new(rx + 22, hy, rw - 60, 20), "история", t.text);
    ui.button(R::new(b.right() - 26, hy - 2, 26, 24), "", "trash", crate::ui::Tone::Ghost, false, n > 0, Act::CalcClear);
    ui.cv.fill(R::new(rx, hy + 26, rw, 1), t.border);
    let list = R::new(rx, hy + 34, rw, b.bottom() - hy - 34);
    ui.zone(list, Act::CalcList, Cur::Default);
    // ушедший с экрана ответ съезжает на первую строку и тускнеет, остальные сдвигаются вниз
    let dur = 0.26 * st.set.anim_speed.max(0.1);
    let fly = c.fly.filter(|_| st.set.anim() && !c.done && c.scroll == 0 && n > 0).map(|at| (at.elapsed().as_secs_f32() / dur).min(1.0)).filter(|&k| k < 1.0);
    let k = fly.map(|k| 1.0 - (1.0 - k).powi(3));
    if fly.is_some() {
        ui.anim = true;
    }
    if n <= first {
        ui.text_center(PX, R::new(list.x, list.cy() - 10, list.w, 20), "пока пусто", t.text_faint);
        return;
    }
    ui.cv.push_clip(list);
    let mut y = list.y - k.map_or(0, |k| ((1.0 - k) * ROW as f32).round() as i32);
    for (i, (e, a)) in c.history.iter().enumerate().skip(first + c.scroll.min(n - 1 - first)) {
        if y > list.bottom() {
            break;
        }
        let r = R::new(list.x, y, list.w, ROW - 4);
        if !(i == 0 && k.is_some()) {
            history_row(ui, r, i, e, a, 1.0);
        }
        y += ROW;
    }
    ui.cv.pop_clip();
    // летящая строка: от ответа на экране до своего места, поверх разделителя
    if let (Some(k), Some((e, a))) = (k, c.history.first()) {
        let from = inner.bottom() - 26;
        let r = R::new(list.x, from + ((list.y - from) as f32 * k).round() as i32, list.w, ROW - 4);
        ui.cv.push_clip(R::new(rx, b.y, rw, b.h));
        history_row(ui, r, 0, e, a, k);
        ui.cv.pop_clip();
    }
}

/// строка истории без рамки: выражение бледно, ответ справа; k < 1 — ещё летит (ярче, не жмётся)
fn history_row(ui: &mut Ui, r: R, i: usize, e: &str, a: &str, k: f32) {
    let t = ui.t;
    let hov = k >= 1.0 && ui.hover(r);
    if hov {
        ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, t.surface_hi));
    }
    let a = format!("= {a}");
    let aw = ui.w(PX, &a);
    ui.text_in(PX, R::new(r.x + 8, r.y + 9, r.w - aw - 24, 18), e, t.text_dim.lerp(t.text_faint, k));
    ui.text_right(PX, R::new(r.x + 8, r.y + 9, r.w - 16, 18), &a, if hov { t.text_hi } else { t.text_hi.lerp(t.text_dim, k) });
    if k >= 1.0 {
        ui.zone(r, Act::CalcHist(i), Cur::Pointer);
    }
}

/// знаки — тем же крупным, а двухбуквенные (x²) — помельче, чтобы не упирались в края
fn op_face(k: &str) -> font::Face {
    if k.chars().count() > 1 { PX } else { font::PX24 }
}

impl App {
    pub fn calc_key(&mut self, k: &str) {
        self.st.calc.press(k);
        self.dirty(&SKind::Popup);
    }

    pub fn calc_hist(&mut self, i: usize) {
        let c = &mut self.st.calc;
        if let Some((_, a)) = c.history.get(i).cloned() {
            // после знака — дописать к выражению, иначе заменить
            if c.done || !c.expr.ends_with(['+', '−', '×', '÷', '^', '(']) {
                c.expr.clear();
            }
            c.expr.push_str(&a);
            if c.done {
                c.fly = Some(std::time::Instant::now());
            }
            c.done = false;
        }
        self.dirty(&SKind::Popup);
    }

    pub fn calc_scroll(&mut self, up: bool) {
        let c = &mut self.st.calc;
        c.scroll = if up { c.scroll.saturating_sub(1) } else { (c.scroll + 1).min(c.history.len().saturating_sub(1)) };
        self.dirty(&SKind::Popup);
    }

    /// клавиатура во вкладке калькулятора: true — нажатие его
    pub fn calc_key_event(&mut self, e: &KeyEvent) -> bool {
        if !self.st.popup_is("calc") || self.st.field_focus.is_some() {
            return false;
        }
        let k = match e.keysym {
            Keysym::Return | Keysym::KP_Enter => "=",
            Keysym::BackSpace => "del",
            Keysym::Delete => "C",
            _ => match e.utf8.as_deref() {
                Some(s) => match s {
                    "0" => "0",
                    "1" => "1",
                    "2" => "2",
                    "3" => "3",
                    "4" => "4",
                    "5" => "5",
                    "6" => "6",
                    "7" => "7",
                    "8" => "8",
                    "9" => "9",
                    "." | "," => ".",
                    "+" => "+",
                    "-" => "−",
                    "*" => "×",
                    "/" => "÷",
                    "^" => "^",
                    "%" => "%",
                    "(" => "(",
                    ")" => ")",
                    "=" => "=",
                    "p" => "π",
                    "r" => "√",
                    "c" => "C",
                    _ => return false,
                },
                None => return false,
            },
        };
        self.calc_key(k);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates() {
        let e = |s: &str| eval(s).map(fmt);
        assert_eq!(e("2+3×4").as_deref(), Ok("14"));
        assert_eq!(e("(2+3)×4").as_deref(), Ok("20"));
        assert_eq!(e("2^3^2").as_deref(), Ok("512"));
        assert_eq!(e("−2^2").as_deref(), Ok("−4"));
        assert_eq!(e("2^−1").as_deref(), Ok("0.5"));
        assert_eq!(e("3²").as_deref(), Ok("9"));
        assert_eq!(e("√16+1").as_deref(), Ok("5"));
        assert_eq!(e("2√9").as_deref(), Ok("6"));
        assert_eq!(e("50%×80").as_deref(), Ok("40"));
        assert_eq!(e("2×(3+4").as_deref(), Ok("14"));
        assert_eq!(e("1÷3").as_deref(), Ok("0.333333333"));
        assert_eq!(e("0.1+0.2").as_deref(), Ok("0.3"));
        assert_eq!(e("10^20").as_deref(), Ok("1e20"));
        assert_eq!(e("1e20×2").as_deref(), Ok("2e20"));
        assert_eq!(e("1e−7×2").as_deref(), Ok("2e−7"));
        assert!(eval("1÷0").is_err());
        assert!(eval("2+").is_err());
        assert!(eval("").is_err());
    }

    #[test]
    fn presses() {
        let mut c = Calc::default();
        for k in ["2", "+", "×", "3", "="] {
            c.press(k);
        }
        assert_eq!(c.expr, "6");
        c.press("+");
        c.press("1");
        assert_eq!(c.expr, "6+1");
        c.press("=");
        c.press("=");
        assert_eq!(c.history.len(), 2);
        c.press("5");
        assert_eq!(c.expr, "5");
    }
}
