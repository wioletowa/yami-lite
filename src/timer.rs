// Виджет «таймер»: квадрат с кольцом-циферблатом из 60 делений (оборот — час) и временем в центре.
// Колесо над кольцом выбирает время: медленно — по 15 с, быстрее — по минуте, совсем быстро — по
// 5 мин (до 23:59:45). Под кольцом — «запустить» / «пауза» / «продолжить» и «сбросить». Вышло время
// — чиптюн один раз (sound.rs) и уведомление. Идущий таймер переживает перезапуск шелла: конец
// хранится временем на часах (~/.local/state/yami-lite/timers.json).

use crate::app::{App, SKind, State};
use crate::font::{self, PX};
use crate::gfx::{C, R};
use crate::ui::{Act, Cur, Tone, Ui};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// больше — некуда: 23:59:45
const MAX: u32 = 24 * 3600 - 15;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timer {
    /// выбранное время, с
    pub set: u32,
    /// идёт: когда кончится (unix, с); 0 — стоит
    pub end: u64,
    /// на паузе: сколько осталось, с (0 — не запускали)
    pub left: u32,
    /// время вышло (мигает, пока не сбросят)
    pub done: bool,
}

impl Timer {
    /// сколько показывать сейчас, с
    pub fn shown(&self, now: u64) -> u32 {
        if self.end > 0 {
            self.end.saturating_sub(now) as u32
        } else if self.left > 0 {
            self.left
        } else if self.done {
            0
        } else {
            self.set
        }
    }
    pub fn running(&self) -> bool {
        self.end > 0
    }
}

/// шаг колеса по тому, как быстро крутят: пауза между щелчками
pub fn wheel_step(gap_ms: u128) -> u32 {
    match gap_ms {
        0..=70 => 300,
        71..=180 => 60,
        _ => 15,
    }
}

/// «05:30», «1:05:30»
pub fn fmt(s: u32) -> String {
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m:02}:{sec:02}")
    }
}

fn path() -> std::path::PathBuf {
    yami_px::paths::state_dir().join("timers.json")
}

pub fn load() -> HashMap<u32, Timer> {
    std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(t: &HashMap<u32, Timer>) {
    if let Ok(s) = serde_json::to_string(t) {
        let _ = yami_px::paths::write_atomic(&path(), &s);
    }
}

fn now() -> u64 {
    yami_px::time::unix_now()
}

/// размер: квадрат
pub fn size(size: &str) -> (i32, i32) {
    let s = match size {
        "s" => 180,
        "l" => 280,
        _ => 220,
    };
    (s, s)
}

/// кольцо и время, кнопки под ним
pub fn draw(ui: &mut Ui, st: &State, id: u32, size: &str, r: R, c: C) {
    let t = ui.t;
    let tm = st.timers.get(&id).cloned().unwrap_or_default();
    let n = now();
    let shown = tm.shown(n);
    let ring = R::new(r.x, r.y, r.w, r.h - 38);
    let (cx, cy) = (ring.x + ring.w / 2, ring.y + ring.h / 2);
    let rad = (ring.w.min(ring.h) / 2 - 6) as f32;
    let dot = if size == "l" { 6 } else if size == "s" { 4 } else { 5 };
    // заполнено: идёт — доля оставшегося от выбранного, стоит — выбранное от часа
    let frac = if tm.running() || tm.left > 0 { shown as f32 / tm.set.max(1) as f32 } else { (shown.min(3600)) as f32 / 3600.0 };
    let lit = (frac * 60.0).ceil() as i32;
    // вышло время: кольцо мигает раз в полсекунды
    let blink = tm.done && st.blink;
    for i in 0..60 {
        let a = (i as f32 * 6.0 - 90.0).to_radians();
        let (x, y) = (cx as f32 + a.cos() * rad, cy as f32 + a.sin() * rad);
        let big = i % 5 == 0;
        let d = if big { dot + 1 } else { dot - 1 };
        let col = if tm.done {
            if blink { t.ember } else { t.border }
        } else if i < lit {
            c
        } else {
            t.border.fade(0.8)
        };
        ui.cv.fill(R::new(x.round() as i32 - d / 2, y.round() as i32 - d / 2, d, d), col);
    }
    let face = font::px(match size {
        "s" => 24,
        "l" => 48,
        _ => 32,
    });
    let s = fmt(shown);
    let lh = ui.f.line(face);
    let col = if tm.done && blink { t.ember } else { t.text_hi };
    ui.text_center(face, R::new(ring.x, cy - lh / 2 - 6, ring.w, lh), &s, col);
    let hint = if tm.done {
        crate::i18n::tr("время!")
    } else if tm.running() {
        crate::i18n::tr("идёт")
    } else if tm.left > 0 {
        crate::i18n::tr("пауза")
    } else {
        crate::i18n::tr("крути колесо")
    };
    ui.text_center(PX, R::new(ring.x, cy + lh / 2 - 2, ring.w, 18), hint, t.text_faint);
    // колесо — над кольцом (пока не идёт)
    ui.zone(ring, Act::TimerWheel(id), Cur::Default);
    let by = r.bottom() - 28;
    let bw = (r.w - 8) / 2;
    // подписи без значков: «запустить» и «сбросить» влезают и в маленький таймер
    let label = if tm.running() {
        "пауза"
    } else if tm.left > 0 {
        "дальше"
    } else {
        "запустить"
    };
    let can = tm.running() || tm.left > 0 || (tm.set > 0 && !tm.done);
    ui.button(R::new(r.x, by, bw, 28), label, "", Tone::Primary, false, can, Act::TimerGo(id));
    ui.button(R::new(r.x + bw + 8, by, r.w - bw - 8, 28), "сбросить", "", Tone::Plain, false, tm.set > 0 || tm.done || tm.left > 0, Act::TimerReset(id));
}

impl App {
    fn timer_mut(&mut self, id: u32) -> &mut Timer {
        self.st.timers.entry(id).or_default()
    }

    fn timer_changed(&mut self, id: u32) {
        self.st.timers.retain(|k, _| self.st.set.widgets.iter().any(|w| w.id == *k));
        save(&self.st.timers);
        self.dirty(&SKind::Widget(id));
    }

    /// колесо над кольцом: шаг по скорости, вверх — больше
    pub fn timer_wheel(&mut self, id: u32, up: bool) {
        let now = Instant::now();
        let gap = self.st.timer_wheel_at.insert(id, now).map_or(u128::MAX, |t| now.duration_since(t).as_millis());
        let step = wheel_step(gap);
        let t = self.timer_mut(id);
        if t.running() || t.left > 0 {
            return;
        }
        t.done = false;
        t.set = if up { (t.set + step).min(MAX) } else { t.set.saturating_sub(step) };
        // ровно по шагу: 1:07 после быстрой прокрутки → 1:00/1:15, а не «грязные» секунды
        t.set -= t.set % 15;
        self.timer_changed(id);
    }

    /// запустить, на паузу, продолжить
    pub fn timer_go(&mut self, id: u32) {
        let n = now();
        let t = self.timer_mut(id);
        if t.running() {
            t.left = t.end.saturating_sub(n) as u32;
            t.end = 0;
        } else if t.left > 0 {
            t.end = n + t.left as u64;
            t.left = 0;
        } else if t.set > 0 {
            t.done = false;
            t.end = n + t.set as u64;
        }
        self.timer_changed(id);
    }

    pub fn timer_reset(&mut self, id: u32) {
        let t = self.timer_mut(id);
        let (keep, was_idle) = (t.set, !t.running() && t.left == 0 && !t.done);
        // второй «сбросить» у стоящего — ноль
        *t = Timer { set: if was_idle { 0 } else { keep }, ..Default::default() };
        self.timer_changed(id);
    }

    /// из тика: идущие — перерисовать, вышедшие — сигнал и уведомление
    pub fn timer_tick(&mut self) {
        let n = now();
        let mut rang = vec![];
        for (id, t) in self.st.timers.iter_mut() {
            if t.running() && n >= t.end {
                t.end = 0;
                t.left = 0;
                t.done = true;
                rang.push((*id, t.set));
            }
        }
        let live: Vec<u32> = self.st.timers.iter().filter(|(_, t)| t.running() || t.done).map(|(id, _)| *id).collect();
        for id in live {
            self.dirty(&SKind::Widget(id));
        }
        if !rang.is_empty() {
            save(&self.st.timers);
            for &(id, _) in &rang {
                let vol = self.st.set.widgets.iter().find(|w| w.id == id).map_or(0.7, |w| w.volume);
                crate::sound::chime(vol);
            }
            for (_, set) in rang {
                self.toast_note(crate::i18n::tr("таймер"), &crate::tf!("прошло {}", "{} is up", fmt(set)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_speeds_up_and_time_formats() {
        assert_eq!(wheel_step(500), 15);
        assert_eq!(wheel_step(120), 60);
        assert_eq!(wheel_step(30), 300);
        assert_eq!(fmt(330), "05:30");
        assert_eq!(fmt(3930), "1:05:30");
    }

    #[test]
    fn shown_time_by_state() {
        let t = Timer { set: 300, ..Default::default() };
        assert_eq!(t.shown(1000), 300);
        let t = Timer { set: 300, end: 1100, ..Default::default() };
        assert_eq!(t.shown(1000), 100);
        let t = Timer { set: 300, left: 42, ..Default::default() };
        assert_eq!(t.shown(1000), 42);
        let t = Timer { set: 300, done: true, ..Default::default() };
        assert_eq!(t.shown(1000), 0);
    }
}
