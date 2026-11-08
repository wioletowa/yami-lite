// Выпадающие списки: кнопка выбора (opt_btn с Act::Drop) открывает список вариантов под собой — в
// окне настроек или во всплывашке, поверх содержимого; щелчок — выбор, мимо или Esc — закрыть.
// Варианты, текущее значение и применение — по ключу здесь, в одном месте.

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Ui};

/// открытый список: ключ, кнопка (в координатах поверхности), где открыт, первая видимая строка
#[derive(Clone, Debug)]
pub struct Drop {
    pub key: &'static str,
    pub at: R,
    pub kind: SKind,
    pub top: usize,
}

const ROW: i32 = 26;

/// всего два варианта: кнопка просто переключает, без списка
pub fn pair(key: &str) -> bool {
    matches!(key, "bar_center_align" | "bar_pos" | "ws_style" | "temp_style" | "wall_fit")
}
const SHOWN: usize = 10;

type Opts = Vec<(String, String)>;

fn list(l: &[(&str, &str)]) -> Opts {
    l.iter().map(|(v, n)| (v.to_string(), crate::i18n::tr(n).to_string())).collect()
}

fn nums(l: &[u32]) -> Opts {
    l.iter().map(|n| (n.to_string(), n.to_string())).collect()
}

/// варианты и текущее значение по ключу
pub fn spec(st: &State, key: &str) -> Option<(Opts, String)> {
    let s = &st.set;
    let w = s.widgets.iter().find(|w| w.id == st.sel_widget);
    let mut eq = crate::widgets::STYLES.to_vec();
    eq.push(("off", "выкл"));
    Some(match key {
        "notif_pos" => (list(crate::notif::POS), s.notif_pos.clone()),
        "osd_pos" => (list(crate::notif::OSD_POS), s.osd_pos.clone()),
        "notif_max" => (nums(&[1, 2, 3, 4, 5, 6, 8]), s.notif_max.to_string()),
        "notif_history" => (nums(&[10, 20, 50, 100, 200]), s.notif_history.to_string()),
        "reveal" => (list(&[("dither", "дизеринг"), ("scan", "развёртка"), ("noise", "шум"), ("wipe", "диагональ"), ("blocks", "блоки")]), s.reveal.clone()),
        "lang" => (vec![(String::new(), crate::i18n::tr("авто").to_string()), ("ru".into(), "Русский".into()), ("en".into(), "English".into())], s.lang.clone()), // i18n-ok: language names
        "dock_side" => (list(&[("bottom", "снизу"), ("left", "слева"), ("right", "справа"), ("top", "сверху")]), s.dock_side.clone()),
        "dock_hide" => (list(&[("smart", "умно — виден на пустом столе"), ("always", "всегда — выезжает у края"), ("never", "никогда (занимает место)")]), s.dock_hide.clone()),
        "dock_anim" => (list(&[("grow", "рост и подъём"), ("wave", "волна — соседи тоже растут"), ("lift", "подъём без роста"), ("none", "нет")]), s.dock_anim.clone()),
        "bar_center_align" => (list(&[("center", "по центру"), ("after-left", "за началом")]), s.bar_center_align.clone()),
        "bar_pos" => (list(&[("top", "сверху"), ("bottom", "снизу")]), s.bar_pos.clone()),
        "wall_fit" => (list(crate::wall::FITS), s.wall_fit.clone()),
        "wall_tone" => (list(crate::wall::TONES), s.wall_tone.clone()),
        "wall_fade" => (list(crate::wall::FADES), s.wall_fade.clone()),
        "ws_style" => (list(&[("boxes", "квадраты с номером"), ("dots", "точки")]), s.ws_style.clone()),
        "cpu_style" => (list(&[("gauge", "шкала"), ("graph", "график"), ("number", "число")]), s.cpu_style.clone()),
        "temp_style" => (list(&[("full", "как у процессора"), ("compact", "коротко")]), s.temp_style.clone()),
        "tray_mode" => (list(&[("icons", "все значки в баре"), ("hybrid", "закреплённые в баре, остальные под стрелкой"), ("arrow", "всё под стрелкой")]), s.tray_mode.clone()),
        "media_viz" => (list(&eq), s.media_viz.clone()),
        "media_view" => (list(crate::config::MEDIA_VIEWS), s.media_view.clone()),
        "bar_sep" => (list(&[("dots", "точки"), ("line", "черта"), ("blocks", "клетки"), ("gap", "пусто")]), s.bar_sep.clone()),
        "bar_sep_color" => (list(&[("border", "рамка"), ("accent", "акцент"), ("dim", "тусклый")]), s.bar_sep_color.clone()),
        "w_source" => (list(crate::widgets::SOURCES), w?.source.clone()),
        "w_style" => (list(crate::widgets::STYLES), w?.style.clone()),
        "w_anim" => (list(crate::widgets::ANIMS), w?.anim.clone()),
        "w_size" => (list(crate::widgets::SIZES), w?.size.clone()),
        "w_color" => (list(crate::widgets::COLORS), w?.color.clone()),
        "wh_sort" => (list(crate::wallhaven::SORTS), crate::wallhaven::SORTS[st.wh.sort].0.to_string()),
        "wh_ratio" => (list(crate::wallhaven::RATIOS), crate::wallhaven::RATIOS[st.wh.ratio].0.to_string()),
        _ => return None,
    })
}

/// список поверх поверхности: под кнопкой (или над ней, если внизу не влезает)
pub fn draw(ui: &mut Ui, st: &State, kind: &SKind, w: i32, h: i32) {
    let Some(d) = st.drop.as_ref().filter(|d| &d.kind == kind) else { return };
    let Some((opts, cur)) = spec(st, d.key) else { return };
    let t = ui.t;
    // щелчок мимо — закрыть
    ui.zone(R::new(0, 0, w, h), Act::DropClose, Cur::Default);
    let n = opts.len().min(SHOWN);
    let lh = n as i32 * ROW + 8;
    let below = d.at.bottom() + 4 + lh <= h - 4;
    let y = if below { d.at.bottom() + 4 } else { (d.at.y - 4 - lh).max(4) };
    let lw = opts.iter().map(|o| ui.w(PX, &o.1) + 50).max().unwrap_or(0).max(d.at.w).min(w - 8);
    let x = d.at.x.min(w - lw - 4).max(4);
    let r = R::new(x, y, lw, lh);
    ui.cv.pxbox(r, &BoxStyle::new(t.glass_panel(), t.red).shadow(4));
    ui.zone(r, Act::DropScroll, Cur::Default);
    let top = d.top.min(opts.len().saturating_sub(SHOWN));
    for (i, (v, name)) in opts.iter().enumerate().skip(top).take(SHOWN) {
        let rr = R::new(r.x + 4, r.y + 4 + (i - top) as i32 * ROW, r.w - 8, ROW);
        let on = *v == cur;
        if on || ui.hover(rr) {
            ui.cv.fill(rr, if on { t.selection } else { t.surface_hi });
        }
        if on {
            ui.cv.icon("check", rr.x + 6, rr.cy() - 7, 2, t.ember);
        }
        ui.cv.text_in(ui.f, PX, R::new(rr.x + 28, rr.y, rr.w - 32, rr.h), name, if on { t.text_hi } else { t.text });
        ui.zone(rr, Act::DropPick(v.clone()), Cur::Pointer);
    }
    if opts.len() > SHOWN {
        let th = (r.h - 8) * SHOWN as i32 / opts.len() as i32;
        let ty = r.y + 4 + (r.h - 8 - th) * top as i32 / (opts.len() - SHOWN) as i32;
        ui.cv.fill(R::new(r.right() - 5, ty, 3, th), t.red);
    }
}

impl App {
    /// открыть список у кнопки (zone — её прямоугольник на поверхности sid)
    pub fn drop_open(&mut self, key: &'static str, sid: u32, zone: R) {
        let Some(kind) = self.surf_kind(sid) else { return };
        // из двух — сразу другое
        if let Some((opts, cur)) = spec(&self.st, key).filter(|s| s.0.len() == 2) {
            let other = opts.into_iter().map(|o| o.0).find(|v| *v != cur).unwrap_or_default();
            self.st.drop = Some(Drop { key, at: zone, kind, top: 0 });
            return self.drop_pick(other);
        }
        if self.st.drop.as_ref().is_some_and(|d| d.key == key) {
            self.drop_close();
            return;
        }
        self.st.drop = Some(Drop { key, at: zone, kind, top: 0 });
        self.st.field_focus = None;
        self.dirty_where(|k| matches!(k, SKind::Settings | SKind::Popup | SKind::Modal));
    }

    pub fn drop_close(&mut self) {
        if self.st.drop.take().is_some() {
            self.dirty_where(|k| matches!(k, SKind::Settings | SKind::Popup | SKind::Modal));
        }
    }

    pub fn drop_scroll(&mut self, d: i32) {
        let Some(key) = self.st.drop.as_ref().map(|x| x.key) else { return };
        let n = spec(&self.st, key).map_or(0, |s| s.0.len());
        if let Some(dr) = self.st.drop.as_mut() {
            dr.top = (dr.top as i32 + d).clamp(0, n.saturating_sub(SHOWN) as i32) as usize;
        }
        self.dirty_where(|k| matches!(k, SKind::Settings | SKind::Popup | SKind::Modal));
    }

    /// выбор из списка: значение в настройки (или в окно Wallhaven), всё зависящее — следом
    pub fn drop_pick(&mut self, v: String) {
        let Some(key) = self.st.drop.take().map(|d| d.key) else { return };
        let s = &mut self.st.set;
        let num = v.parse::<u32>().unwrap_or(0);
        match key {
            "notif_pos" => s.notif_pos = v,
            "osd_pos" => s.osd_pos = v,
            "notif_max" => s.notif_max = num.max(1),
            "notif_history" => s.notif_history = num.max(1),
            "reveal" => s.reveal = v,
            "lang" => s.lang = v,
            "dock_side" => s.dock_side = v,
            "dock_hide" => s.dock_hide = v,
            "dock_anim" => s.dock_anim = v,
            "bar_center_align" => s.bar_center_align = v,
            "bar_pos" => s.bar_pos = v,
            "wall_fit" => s.wall_fit = v,
            "wall_tone" => s.wall_tone = v,
            "wall_fade" => s.wall_fade = v,
            "ws_style" => s.ws_style = v,
            "cpu_style" => s.cpu_style = v,
            "temp_style" => s.temp_style = v,
            "tray_mode" => s.tray_mode = v,
            "media_viz" => s.media_viz = v,
            "media_view" => s.media_view = v,
            "bar_sep" => s.bar_sep = v,
            "bar_sep_color" => s.bar_sep_color = v,
            k if k.starts_with("w_") => {
                let id = self.st.sel_widget;
                if let Some(w) = self.st.set.widget_mut(id) {
                    crate::widgets::set_opt(w, &k[2..], &v);
                }
            }
            "wh_sort" | "wh_ratio" => {
                let l = if key == "wh_sort" { crate::wallhaven::SORTS } else { crate::wallhaven::RATIOS };
                let i = l.iter().position(|x| x.0 == v).unwrap_or(0);
                if key == "wh_sort" {
                    self.st.wh.sort = i;
                } else {
                    self.st.wh.ratio = i;
                }
                self.wh_search(1);
                return;
            }
            _ => return,
        }
        // поверхности, которые от этого пересоздаются на новом месте
        match key {
            "notif_pos" => self.renew_notif(SKind::Notifs),
            "osd_pos" => self.renew_notif(SKind::Osd),
            "dock_side" | "dock_hide" => {
                for k in [SKind::Dock, SKind::DockEdge] {
                    for id in self.find(&k) {
                        self.destroy(id);
                    }
                }
            }
            _ => {}
        }
        if matches!(key, "media_viz" | "media_view") {
            self.media_sync();
        }
        self.settings_changed();
    }
}
