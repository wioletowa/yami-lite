// Открытие и закрытие панелей шелла: модальные (лаунчер, питание, настройки, пароль, выбор
// экрана, ответ), всплывашки бара и пиксельное меню.

use crate::app::*;
use crate::gfx::R;
use crate::icons::{self};
use crate::{launcher, menu, net, popups};

use smithay_client_toolkit::shell::{
    wlr_layer::{Anchor, KeyboardInteractivity, Layer},
    WaylandSurface,
};

impl App {
    // ── модальные панели: лаунчер, питание, настройки ───────────────────────
    pub fn toggle_modal(&mut self, m: Modal, clip: bool) {
        // окно пароля не уступает место другим панелям
        if self.st.locked || matches!(self.st.modal, Some(Modal::Auth | Modal::Screen)) {
            return;
        }
        let same = self.st.modal == Some(m) && (m != Modal::Launcher || self.st.launcher.clip == clip);
        if self.st.modal.is_some() && !same && m == Modal::Launcher && self.st.modal == Some(Modal::Launcher) {
            // переключение программы ↔ буфер без закрытия
            self.st.launcher.set_mode(clip, &self.st.apps, &self.st.niri.wins);
            if clip {
                launcher::load_clip(self.tx.clone());
            }
            self.dirty(&SKind::Modal);
            return;
        }
        self.close_all();
        if same {
            return;
        }
        let out = self.focus_out();
        let (w, h) = match m {
            Modal::Launcher => launcher::SIZE,
            Modal::Power => crate::power::SIZE,
            Modal::Settings => crate::settings::SIZE,
            Modal::Auth => crate::auth::SIZE,
            Modal::Screen => (0, 0),
            Modal::Reply => crate::reply::SIZE,
            Modal::Walls => crate::walls::SIZE,
        };
        self.st.modal = Some(m);
        match m {
            Modal::Launcher => {
                self.st.apps = icons::load_apps();
                self.st.launcher.open(clip, &self.st.apps, &self.st.niri.wins);
                if clip {
                    launcher::load_clip(self.tx.clone());
                }
            }
            Modal::Power => self.st.power_cur = 0,
            Modal::Auth | Modal::Screen | Modal::Reply | Modal::Walls => {}
            Modal::Settings => {
                self.tab_probe();
                self.st.hex.set(&self.st.set.accent.clone());
                if self.st.sel_widget == 0 {
                    self.st.sel_widget = self.st.set.widgets.first().map(|w| w.id).unwrap_or(0);
                }
            }
        }
        self.mk(SKind::Dim, &out, Layer::Overlay, "yami-dim", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
        self.mk(SKind::Modal, &out, Layer::Overlay, match m {
            Modal::Launcher => "yami-launcher",
            Modal::Power => "yami-power",
            Modal::Settings => "yami-settings",
            Modal::Auth => "yami-auth",
            Modal::Screen => "yami-screen",
            Modal::Reply => "yami-reply",
            Modal::Walls => "yami-walls",
        }, Anchor::empty(), (w, h + 12), (0, 0, 0, 0), -1, KeyboardInteractivity::Exclusive);
    }

    /// тащим настройки за заголовок: окно открывается по центру (без якоря), при первом сдвиге
    /// получает якорь сверху слева и дальше едет полями. Заголовок не уходит за край экрана
    pub fn modal_drag(&mut self, sid: u32, dx: f64, dy: f64) {
        let Some((w, h, out)) = self.surfs.get(&sid).map(|s| (s.w, s.h, s.output.clone())) else { return };
        let (ow, oh) = self.output_size(&out);
        let at = self.modal_at.filter(|m| m.0 == sid).map(|m| m.1);
        let start = *self.mdrag.get_or_insert(at.unwrap_or(((ow - w) / 2, (oh - h) / 2)));
        let nx = (start.0 + dx.round() as i32).clamp(80 - w, ow - 80);
        let ny = (start.1 + dy.round() as i32).clamp(0, (oh - 40).max(0));
        if at.is_none() {
            if let Some(s) = self.surfs.get(&sid) {
                s.layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            }
        }
        self.modal_at = Some((sid, (nx, ny)));
        self.move_surface(sid, nx, ny);
    }

    /// все клавиши — шеллу (запись сочетания): niri не ловит свои бинды, пока фокус у настроек
    pub fn inhibit_shortcuts(&mut self, on: bool) {
        if let Some(i) = self.ksi.take() {
            i.destroy();
        }
        if !on {
            return;
        }
        let surf = self.surfs.values().find(|s| s.kind == SKind::Modal).map(|s| s.layer.wl_surface().clone());
        let seat = self.seat.seats().next();
        if let (Some(mgr), Some(surf), Some(seat)) = (&self.ksi_mgr, surf, seat) {
            self.ksi = Some(mgr.inhibit_shortcuts(&surf, &seat, &self.qh, ()));
        }
    }

    pub fn close_modal(&mut self) {
        if self.st.modal.is_none() {
            return;
        }
        // следующее окно снова откроется по центру
        self.modal_at = None;
        self.keys_capture(None);
        if self.st.modal == Some(Modal::Auth) {
            self.auth_cancel();
        }
        // окно выбора экрана закрыли мимо кнопок — порталу «отмена»
        if self.st.modal == Some(Modal::Screen) {
            if let Some(p) = self.st.screen_pick.take() {
                use std::io::Write;
                let _ = (&p.stream).write_all(b"cancel\n");
            }
        }
        if self.st.modal == Some(Modal::Walls) {
            self.walls_free();
        }
        if self.st.modal == Some(Modal::Settings) {
            self.sets_free();
        }
        if self.st.modal == Some(Modal::Launcher) {
            self.st.launcher.thumbs_free();
        }
        // код настроек и выбора обоев велик, а нужен изредка: через полминуты — из памяти. После
        // лаунчера и прочих частых окон — нет: выгрузка задевает и горячий код (бар, тик), а он
        // тут же читается с диска обратно
        if matches!(self.st.modal, Some(Modal::Settings | Modal::Walls)) {
            self.shed_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        }
        self.st.modal = None;
        self.st.field_focus = None;
        self.close_kind(&SKind::Modal);
        self.close_kind(&SKind::Dim);
    }

    // ── всплывашки бара ─────────────────────────────────────────────────────
    pub fn open_popup(&mut self, kind: &'static str, out: &str, ax: i32) {
        let same = self.st.popup_is(kind);
        self.close_popup();
        self.close_menu();
        if same {
            return;
        }
        self.close_modal();
        self.st.popup = Some(Popup { kind });
        self.lyrics_want();
        // модули бара рисуют, открыта ли их всплывашка (стрелка трея, календарь)
        self.dirty(&SKind::Bar);
        match kind {
            "net" => net::refresh_wifi(self.tx.clone()),
            "bt" => net::refresh_bt(self.tx.clone()),
            "profile" => crate::profile::refresh(None, self.tx.clone()),
            "cal" => self.st.cal_month = 0,
            _ => {}
        }
        self.viz_sync();
        let (w, h) = popups::size(kind, &self.st);
        let (ow, _) = self.output_size(out);
        let x = (ax - w / 2).clamp(6, (ow - w - 6).max(6));
        self.mk_catcher(out);
        // видимый верх — на 6 px ниже бара (где бы он ни был); содержимое всплывашки внутри
        // поверхности опущено на высоту подъёма (12), поэтому сама поверхность — выше.
        // Бар снизу — всплывашка над ним: низ содержимого и есть низ поверхности
        let low = self.bar_reserve_bottom(out);
        let (anchor, margin) = if low > 0 {
            (Anchor::BOTTOM | Anchor::LEFT, (0, 0, low + 6, x))
        } else {
            (Anchor::TOP | Anchor::LEFT, ((self.bar_bottom(out) + 6 - 12).max(0), 0, 0, x))
        };
        let id = self.mk(SKind::Popup, out, Layer::Top, "yami-popup", anchor, (w, h + 12), margin, -1, KeyboardInteractivity::OnDemand);
        if low > 0 {
            // верх поверхности на мониторе: от него меню трея во всплывашке ищет свою строку
            let (_, oh) = self.output_size(out);
            if let Some(s) = self.surfs.get_mut(&id) {
                s.margin = (x, oh - low - 6 - h - 12);
                s.applied = s.margin;
            }
        }
    }

    pub fn resize_popup(&mut self) {
        let Some(p) = &self.st.popup else { return };
        let (w, h) = popups::size(p.kind, &self.st);
        for s in self.surfs.values_mut() {
            if s.kind == SKind::Popup && !s.closing && (s.w != w || s.h != h + 12) {
                s.layer.set_size(w as u32, (h + 12) as u32);
                s.layer.commit();
            }
        }
    }

    pub fn close_popup(&mut self) {
        if self.st.popup.take().is_some() {
            self.dirty(&SKind::Bar);
            self.st.field_focus = None;
            self.close_kind(&SKind::Popup);
            if self.st.menu.is_none() {
                self.close_kind(&SKind::Catcher);
            }
            self.viz_sync();
        }
    }

    /// прозрачный ловец кликов мимо всплывашки (бар остаётся живым)
    pub fn mk_catcher(&mut self, out: &str) {
        if self.find(&SKind::Catcher).is_empty() {
            let id = self.mk(SKind::Catcher, out, Layer::Top, "yami-catch", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
            // ловит всё, кроме самого бара: бар живой, а поля вокруг плавающего — нет
            let (w, h) = self.output_size(out);
            let mut rs = vec![R::new(0, 0, w, h)];
            if !self.st.bar_hidden && self.bar_on(out) {
                let (m, side, _) = crate::bar::geo(&self.st.set, w);
                let top = if crate::bar::low(&self.st.set) { h - m - BAR_H } else { m };
                let bot = top + BAR_H;
                rs = vec![R::new(0, 0, w, top), R::new(0, top, side, BAR_H), R::new(w - side, top, side, BAR_H), R::new(0, bot, w, h - bot)];
                rs.retain(|r| r.w > 0 && r.h > 0);
            }
            if let Some(s) = self.surfs.get_mut(&id) {
                s.input = Some(rs);
            }
        }
    }

    // ── меню ────────────────────────────────────────────────────────────────
    pub fn open_menu(&mut self, m: menu::MenuState) {
        self.close_menu();
        let out = m.output.clone();
        let (w, h) = menu::size(&m, &mut self.res.fonts);
        let (ow, oh) = self.output_size(&out);
        let x = m.x.clamp(4, (ow - w - 8).max(4));
        // нижний бар не закрываем: меню из него встаёт над ним
        let y = m.y.clamp(4, (oh - h - 8 - self.bar_reserve_bottom(&out)).max(4));
        self.st.menu = Some(m);
        self.mk_catcher(&out);
        self.mk(SKind::Menu, &out, Layer::Overlay, "yami-menu", Anchor::TOP | Anchor::LEFT, (w, h + 4), (y, 0, 0, x), -1, KeyboardInteractivity::OnDemand);
    }
    pub fn close_menu(&mut self) {
        if self.st.menu.take().is_some() {
            self.close_kind(&SKind::Menu);
            if self.st.popup.is_none() {
                self.close_kind(&SKind::Catcher);
            }
        }
    }
}
