// Бар поверх полноэкранного окна (настройки → бар, «бар поверх полноэкранного»): пока в фокусе
// окно на весь экран, у верхнего края над местом бара — невидимая полоса в 2 px на слое Overlay.
// Курсор держат на ней 1.2 с — бар (островок) встаёт заново на Overlay, поверх окна; курсор ушёл с
// него и всплывашек нет — обратно на свой слой.

use crate::app::{App, SKind};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::time::{Duration, Instant};

/// сколько держать курсор у края: над полноэкранным — подольше (чтобы не дёргался в игре),
/// при автоскрытии — почти сразу
const HOLD: Duration = Duration::from_millis(1200);
const HOLD_TUCKED: Duration = Duration::from_millis(250);
/// курсор ушёл с бара — через сколько он прячется обратно
const LEAVE: Duration = Duration::from_millis(800);

#[derive(Default)]
pub struct Peek {
    /// бар сейчас поверх полноэкранного
    pub on: bool,
    /// курсор на полосе с этого момента
    pub at: Option<Instant>,
    /// курсор ушёл с бара тогда
    pub leave: Option<Instant>,
}

impl App {
    /// полоса у края: нужна, пока включено, в фокусе окно на весь экран и бар не поднят
    pub fn peek_sync(&mut self) {
        let out = self.focus_out();
        let base = !self.st.peek.on && !self.st.bar_hidden && self.bar_on(&out) && (crate::island::on(&self.st.set) || !crate::bar::low(&self.st.set));
        let full = self.st.set.bar_peek && self.st.game.fullscreen;
        let want = base && (full || self.bar_tucked(&out));
        // слой полосы: над полноэкранным — Overlay; при автоскрытии — Top, над окнами
        let key = if self.st.game.fullscreen { "over" } else { "top" };
        let mut have: Vec<u32> = self.find(&SKind::PeekEdge);
        // слой сменился или фокус ушёл на другой монитор — полосу заново
        if have.iter().any(|id| self.surfs.get(id).is_some_and(|s| s.anchor_side != key || s.output != out)) {
            for id in have.drain(..) {
                self.destroy(id);
            }
        }
        if !want {
            for id in have {
                self.destroy(id);
            }
            self.st.peek.at = None;
            return;
        }
        if have.is_empty() {
            let (ow, _) = self.output_size(&out);
            let (_, side, _) = crate::bar::geo(&self.st.set, ow);
            let layer = if self.st.game.fullscreen { Layer::Overlay } else { Layer::Top };
            // у того края, где бар: полоса в 2 px над его местом
            let edge = if crate::island::on(&self.st.set) { crate::island::edge(&self.st.set) } else { crate::island::Edge::Top };
            let (anchor, size, margin) = match edge {
                crate::island::Edge::Top => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, 2), (0, side, 0, side)),
                crate::island::Edge::Bottom => (Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT, (0, 2), (0, side, 0, side)),
            };
            let id = self.mk(SKind::PeekEdge, &out, layer, "yami-peek", anchor, size, margin, -1, KeyboardInteractivity::None);
            if let Some(s) = self.surfs.get_mut(&id) {
                s.anchor_side = key.into();
            }
        }
    }

    /// курсор на полосе / ушёл с неё; на баре или его всплывашке / ушёл
    pub fn peek_edge(&mut self, on: bool) {
        self.st.peek.at = on.then(Instant::now);
    }
    pub fn peek_bar(&mut self, on: bool) {
        if self.st.peek.on {
            self.st.peek.leave = (!on).then(Instant::now);
        }
    }

    /// тик: продержали — поднять; ушли и ничего не открыто — опустить; окно больше не на весь экран
    pub fn peek_tick(&mut self) {
        let hold = if self.st.game.fullscreen { HOLD } else { HOLD_TUCKED };
        if self.st.peek.at.is_some_and(|t| t.elapsed() >= hold) {
            self.st.peek.at = None;
            self.peek_set(true);
            return;
        }
        if !self.st.peek.on {
            return;
        }
        // открыта всплывашка, меню или раскрытый островок — держать: им отдан фокус клавиатуры, и
        // niri на это время не видит окна в фокусе (game.fullscreen падает), хотя оно на месте
        let busy = self.st.popup.is_some() || self.st.menu.is_some() || self.st.modal.is_some();
        let gone = self.st.peek.leave.is_some_and(|t| t.elapsed() >= LEAVE);
        if !busy && gone {
            self.peek_set(false);
        }
    }

    /// бар на Overlay (поверх полноэкранного) или обратно на Top
    fn peek_set(&mut self, on: bool) {
        self.st.peek.on = on;
        // поднят: если курсор так и не придёт на бар — спрятать, как если бы он ушёл
        self.st.peek.leave = on.then(Instant::now);
        if !on {
            self.close_popup_now();
            self.close_menu();
        }
        // прежний рассыпается, новый на своём слое проявляется
        self.close_kind(&SKind::Bar);
        self.sync_bar_outputs();
        self.peek_sync();
        self.dirty(&SKind::Bar);
    }

    /// бар спрятан автоскрытием на мониторе out: «всегда» — да; «умно» — если на его столе есть окна
    pub fn bar_tucked(&self, out: &str) -> bool {
        match self.st.set.bar_hide.as_str() {
            "always" => true,
            "smart" => self.st.niri.active_ws(out).is_some_and(|w| self.st.niri.has_windows(w.id)),
            _ => false,
        }
    }

    /// столы или окна сменились: умное автоскрытие — показать или спрятать бар
    pub fn tuck_sync(&mut self) {
        if self.st.set.bar_hide != "smart" || self.st.peek.on {
            return;
        }
        let names = self.output_names();
        // бар есть там, где его быть не должно, или наоборот (как решает sync_bar_outputs)
        let stale = names.iter().any(|n| self.find_on(&SKind::Bar, n).is_some() != (!self.st.bar_hidden && self.bar_on(n) && !self.bar_tucked(n)));
        if stale {
            self.close_kind(&SKind::Bar);
            self.sync_bar_outputs();
            self.peek_sync();
            self.sync_dock();
            self.sync_widgets();
        }
    }

    /// слой бара и его всплывашек: поднят (над полноэкранным или из автоскрытия) — Overlay.
    /// Не по game.fullscreen: открытый раскрытый берёт фокус клавиатуры, niri на это время не
    /// видит окна в фокусе — и раскрытый встал бы на Top, под поднятый бар
    pub fn bar_layer(&self) -> Layer {
        if self.st.peek.on {
            Layer::Overlay
        } else {
            Layer::Top
        }
    }
}
