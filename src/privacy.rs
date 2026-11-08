// Индикаторы приватности в баре: микрофон (записи PipeWire не с мониторов), показ экрана (потоки
// niri) и камера (camera.rs). Горят красным, пока кто-то пользуется; наведение — подсказка, кто.

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, C, R};
use crate::ui::Ui;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};

/// красный приватности: один при любой палитре
pub const RED: C = C(0xffe0302a);

/// кто пользуется: имена программ
pub fn users(st: &State, kind: &str) -> Vec<String> {
    match kind {
        "mic" => st.audio.recorders.clone(),
        "cam" => st.camera.clone(),
        _ => {
            if !st.niri.casting {
                return vec![];
            }
            // у потоков PipeWire niri программу не знает (pid есть только у wlr-screencopy)
            let named: Vec<String> = st.niri.casts.iter().filter_map(|c| c.1).filter_map(|p| std::fs::read_to_string(format!("/proc/{p}/comm")).ok()).map(|s| s.trim().to_string()).collect();
            if named.is_empty() {
                vec![crate::i18n::tr("через портал").to_string()]
            } else {
                named
            }
        }
    }
}

fn title(kind: &str) -> &'static str {
    match kind {
        "mic" => "микрофон",
        "cam" => "камера",
        _ => "показ экрана",
    }
}

pub fn draw_tip(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let Some((kind, _)) = &st.tip else { return };
    let t = ui.t;
    let fr = R::new(0, 0, w - 4, h - 4);
    ui.cv.pxbox(fr, &BoxStyle::new(t.glass_panel(), RED).shadow(4));
    ui.cv.fill(R::new(10, 13, 6, 6), RED);
    ui.text_in(PX, R::new(22, 6, fr.w - 30, 20), title(kind), t.text);
    let list = users(st, kind).join(", ");
    ui.cv.text_in(ui.f, PX, R::new(10, 28, fr.w - 20, 20), &list, t.text_dim);
}

impl App {
    /// подсказка под (над) индикатором: sid — бар, zone — сам индикатор
    pub fn privacy_tip(&mut self, kind: &'static str, sid: u32, zone: R) {
        if self.st.tip.as_ref().is_some_and(|t| t.0 == kind) && !self.find(&SKind::Tip).is_empty() {
            return;
        }
        self.privacy_tip_close();
        let Some(s) = self.surfs.get(&sid) else { return };
        let (out, (mx, _)) = (s.output.clone(), s.margin);
        let list = users(&self.st, kind).join(", ");
        let w = (self.res.fonts.width(PX, &list).max(self.res.fonts.width(PX, crate::i18n::tr(title(kind))) + 12) + 24).clamp(140, 420) + 4;
        let h = 56;
        let (ow, _) = self.output_size(&out);
        let x = (mx + zone.x + zone.w / 2 - w / 2).clamp(6, (ow - w - 6).max(6));
        let low = crate::bar::low(&self.st.set);
        let (anchor, margin) = if low { (Anchor::BOTTOM | Anchor::LEFT, (0, 0, self.bar_reserve_bottom(&out) + 6, x)) } else { (Anchor::TOP | Anchor::LEFT, (self.bar_bottom(&out) + 6, 0, 0, x)) };
        self.st.tip = Some((kind, zone));
        let id = self.mk(SKind::Tip, &out, Layer::Overlay, "yami-tip", anchor, (w, h), margin, -1, KeyboardInteractivity::None);
        // мышь сквозь подсказку
        if let Some(s) = self.surfs.get_mut(&id) {
            s.input = Some(vec![]);
        }
    }

    pub fn privacy_tip_close(&mut self) {
        self.st.tip = None;
        for id in self.find(&SKind::Tip) {
            self.destroy(id);
        }
    }
}
