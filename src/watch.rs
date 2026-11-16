// Вкладка «часы» панели управления: слева таймер, справа секундомер с кругами. Таймер — тот же,
// что виджет (timer.rs, id TAB): кольцо, колесо, «запустить» / «сбросить», сигнал — громкостью
// виджета-таймера. Время ещё тянут мышью вверх-вниз и вписывают: двойной щелчок по кольцу, цифры
// справа налево, как на кухонном таймере (5, 3, 0 → 05:30), Enter. Секундомер живёт до выхода.

use crate::app::{App, SKind, State};
use crate::font::{self, PX};
use crate::gfx::R;
use crate::timer::{self, TAB};
use crate::ui::{Act, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::time::{Duration, Instant};

/// два щелчка по кольцу не дальше этого — ввод времени
const DOUBLE: Duration = Duration::from_millis(400);
/// цифр во вводе: чч мм сс
const DIGITS: usize = 6;

#[derive(Default)]
pub struct Watch {
    /// секундомер идёт с этого момента (плюс acc)
    pub start: Option<Instant>,
    /// натикало до последней паузы
    pub acc: Duration,
    /// круги: время секундомера на каждом, по порядку
    pub laps: Vec<Duration>,
    /// ввод времени таймера: набранные цифры
    pub edit: Option<String>,
    /// прошлый щелчок по кольцу — для двойного
    pub click_at: Option<Instant>,
    /// тянут кольцо: время таймера в начале
    pub drag_from: Option<u32>,
}

impl Watch {
    pub fn elapsed(&self) -> Duration {
        self.acc + self.start.map_or(Duration::ZERO, |s| s.elapsed())
    }
}

/// «01:05.37», «1:01:05.37»
pub fn fmt_sw(d: Duration) -> String {
    let cs = d.as_millis() / 10;
    let (h, m, s, c) = (cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}.{c:02}")
    } else {
        format!("{m:02}:{s:02}.{c:02}")
    }
}

/// набранные цифры — секунды: справа налево сс, мм, чч (минуты и секунды больше 59 — пересчитать)
pub fn edit_secs(d: &str) -> u32 {
    let n: u32 = d.parse().unwrap_or(0);
    (n / 10000 * 3600 + n / 100 % 100 * 60 + n % 100).min(timer::MAX)
}

/// набранные цифры на экране: «00:05:30»
pub fn edit_text(d: &str) -> String {
    let p = format!("{d:0>6}");
    format!("{}:{}:{}", &p[..2], &p[2..4], &p[4..])
}

/// сдвиг мышью на dy px (вверх — больше): по 15 с, дальше по минуте, потом по 5 мин
pub fn drag_secs(dy: f64) -> i64 {
    let n = (dy.abs() / 4.0) as i64;
    let s = 15 * n.min(20) + 60 * (n - 20).clamp(0, 40) + 300 * (n - 60).max(0);
    if dy < 0.0 {
        s
    } else {
        -s
    }
}

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let (l, r) = (R::new(b.x, b.y, half, b.h), R::new(b.x + half + 20, b.y, b.w - half - 20, b.h));
    ui.cv.fill(R::new(r.x - 10, b.y, 1, b.h), t.border);
    // ── таймер ──
    head(ui, l, "timer", "таймер");
    let color = t.red;
    timer::draw(ui, st, TAB, "l", R::new(l.x, l.y + 34, l.w, l.h - 34), color);
    // ── секундомер ──
    head(ui, r, "clock", "секундомер");
    let w = &st.watch;
    let run = w.start.is_some();
    let el = w.elapsed();
    if run {
        ui.anim = true;
    }
    let face = font::px(48);
    let fh = ui.f.line(face);
    let ty = r.y + 44;
    ui.text_center(face, R::new(r.x, ty, r.w, fh), &fmt_sw(el), if run || el.is_zero() { t.text_hi } else { t.text_dim });
    let by = ty + fh + 14;
    let bw = (r.w - 8) / 2;
    let go = if run { "пауза" } else if el.is_zero() { "старт" } else { "дальше" };
    ui.button(R::new(r.x, by, bw, 28), go, "", Tone::Primary, false, true, Act::SwGo);
    // идёт — «круг», стоит — «сброс»
    let (lap, can) = if run { ("круг", true) } else { ("сброс", !el.is_zero()) };
    ui.button(R::new(r.x + bw + 8, by, r.w - bw - 8, 28), lap, "", Tone::Plain, false, can, Act::SwLap);
    // круги, свежий сверху: номер, сколько длился, время на нём
    let mut y = by + 40;
    ui.cv.fill(R::new(r.x, y, r.w, 1), t.border);
    y += 6;
    if w.laps.is_empty() {
        ui.text_center(PX, R::new(r.x, y + 12, r.w, 18), "кругов пока нет", t.text_faint);
        return;
    }
    // самый быстрый и самый долгий — цветом (когда их хотя бы два)
    let dur = |i: usize| w.laps[i] - if i > 0 { w.laps[i - 1] } else { Duration::ZERO };
    let n = w.laps.len();
    let best = (0..n).min_by_key(|&i| dur(i)).filter(|_| n > 1);
    let worst = (0..n).max_by_key(|&i| dur(i)).filter(|_| n > 1);
    ui.cv.push_clip(R::new(r.x, y, r.w, r.bottom() - y));
    for i in (0..n).rev() {
        if y > r.bottom() {
            break;
        }
        let col = if Some(i) == best { t.ok } else if Some(i) == worst { t.ember } else { t.text };
        ui.text_in(PX, R::new(r.x + 4, y, 80, 18), &crate::tf!("круг {}", "lap {}", i + 1), t.text_dim);
        ui.text_center(PX, R::new(r.x, y, r.w, 18), &format!("+{}", fmt_sw(dur(i))), col);
        ui.text_right(PX, R::new(r.x, y, r.w - 4, 18), &fmt_sw(w.laps[i]), t.text_dim);
        y += 22;
    }
    ui.cv.pop_clip();
}

/// заголовок половины: значок и название, черта под ними
fn head(ui: &mut Ui, r: R, icon: &str, title: &str) {
    let t = ui.t;
    ui.cv.icon(icon, r.x, r.y + 3, 2, t.red);
    ui.text_in(PX, R::new(r.x + 22, r.y, r.w - 22, 20), title, t.text);
    ui.cv.fill(R::new(r.x, r.y + 26, r.w, 1), t.border);
}

impl App {
    /// щелчок по кольцу таймера вкладки: второй подряд — ввод времени (у стоящего таймера)
    pub fn watch_time_click(&mut self) {
        let w = &mut self.st.watch;
        let double = w.click_at.take().is_some_and(|t| t.elapsed() < DOUBLE);
        let tm = self.st.timers.get(&TAB).cloned().unwrap_or_default();
        if double && !tm.running() && tm.left == 0 {
            w.edit = Some(String::new());
        } else if !double {
            w.click_at = Some(Instant::now());
        }
        self.dirty(&SKind::Popup);
    }

    /// ввод времени закончен: ok — поставить набранное (пустое — оставить как было)
    pub fn watch_edit_done(&mut self, ok: bool) {
        let Some(d) = self.st.watch.edit.take() else { return };
        if ok && !d.is_empty() {
            self.timer_set(TAB, edit_secs(&d));
        }
        self.dirty(&SKind::Popup);
    }

    /// тянут кольцо: dy — от места нажатия
    pub fn watch_drag(&mut self, dy: f64) {
        let tm = self.st.timers.get(&TAB).cloned().unwrap_or_default();
        if tm.running() || tm.left > 0 {
            return;
        }
        let from = *self.st.watch.drag_from.get_or_insert(tm.set);
        let v = (from as i64 + drag_secs(dy)).clamp(0, timer::MAX as i64) as u32;
        if v != tm.set {
            self.timer_set(TAB, v - v % 15);
        }
    }

    pub fn sw_go(&mut self) {
        let w = &mut self.st.watch;
        match w.start.take() {
            Some(s) => w.acc += s.elapsed(),
            None => w.start = Some(Instant::now()),
        }
        self.dirty(&SKind::Popup);
    }

    /// идёт — отметить круг, стоит — обнулить
    pub fn sw_lap(&mut self) {
        let w = &mut self.st.watch;
        if w.start.is_some() {
            let el = w.elapsed();
            w.laps.push(el);
        } else {
            (w.acc, w.laps) = (Duration::ZERO, vec![]);
        }
        self.dirty(&SKind::Popup);
    }

    /// клавиатура во вкладке: цифры, Backspace, Enter, Esc — пока вписывают время
    pub fn watch_key_event(&mut self, e: &KeyEvent) -> bool {
        if !self.st.popup_is("clock") {
            return false;
        }
        let Some(d) = self.st.watch.edit.as_mut() else { return false };
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter => self.watch_edit_done(true),
            Keysym::Escape => self.watch_edit_done(false),
            Keysym::BackSpace => {
                d.pop();
            }
            _ => match e.utf8.as_deref().and_then(|s| s.parse::<u8>().ok()).filter(|&n| n < 10) {
                // ведущие нули — не цифры: «0», «5» → 5 с
                Some(n) if d.len() < DIGITS && !(d.is_empty() && n == 0) => d.push((b'0' + n) as char),
                Some(_) => {}
                None => return false,
            },
        }
        self.dirty(&SKind::Popup);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_time() {
        assert_eq!(edit_secs("530"), 330);
        assert_eq!(edit_secs("13000"), 3600 + 1800);
        assert_eq!(edit_secs("90"), 90);
        assert_eq!(edit_secs(""), 0);
        assert_eq!(edit_text("530"), "00:05:30");
        assert_eq!(fmt_sw(Duration::from_millis(65_370)), "01:05.37");
        assert_eq!(drag_secs(-8.0), 30);
        assert_eq!(drag_secs(8.0), -30);
        assert_eq!(drag_secs(-100.0), 15 * 20 + 60 * 5);
    }
}
