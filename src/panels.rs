// Открытие и закрытие панелей шелла: модальные (питание, настройки, ответ, выбор обоев),
// всплывашки бара и пиксельное меню.

use crate::app::*;
use crate::gfx::R;
use crate::{menu, net, popups};

use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};

impl App {
    // ── модальные панели: питание, ответ, выбор обоев ───────────────────────
    pub fn toggle_modal(&mut self, m: Modal) {
        if self.st.locked {
            return;
        }
        let same = self.st.modal == Some(m);
        self.close_all();
        if same {
            return;
        }
        let out = self.focus_out();
        let (w, h) = match m {
            Modal::Power => crate::power::SIZE,
            Modal::Reply => crate::reply::SIZE,
            Modal::Walls => crate::walls::SIZE,
        };
        self.st.modal = Some(m);
        match m {
            Modal::Power => self.st.power_cur = 0,
            Modal::Reply | Modal::Walls => {}
        }
        self.mk(SKind::Dim, &out, Layer::Overlay, "yami-dim", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
        self.mk(SKind::Modal, &out, Layer::Overlay, match m {
            Modal::Power => "yami-power",
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

    pub fn close_modal(&mut self) {
        if self.st.modal.is_none() {
            return;
        }
        self.st.drop = None;
        // следующее окно снова откроется по центру
        self.modal_at = None;
        if self.st.modal == Some(Modal::Walls) {
            self.walls_free();
        }
        if std::mem::take(&mut self.st.walls_anchored) && self.st.menu.is_none() && self.st.popup.is_none() {
            self.close_kind(&SKind::Catcher);
        }
        // код выбора обоев велик, а нужен изредка: через полминуты — из памяти. После
        // частых окон — нет: выгрузка задевает и горячий код (бар, тик), а он
        // тут же читается с диска обратно
        if self.st.modal == Some(Modal::Walls) {
            self.shed_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
        }
        self.st.modal = None;
        self.st.field_focus = None;
        self.close_kind(&SKind::Modal);
        self.close_kind(&SKind::Dim);
    }

    // ── всплывашки бара ─────────────────────────────────────────────────────
    pub fn open_popup(&mut self, kind: &'static str, out: &str, ax: i32) {
        // островок ещё сворачивается — довернуть сразу: щелчок по нему раскроет заново
        if self.st.island.closing.is_some() {
            self.close_popup_now();
        }
        if !crate::features::popup_on(kind) {
            return;
        }
        let same = self.st.popup_is(kind);
        // панель уже открыта на другой вкладке: переключить на месте — размер у всех один
        if !same && crate::control::is_tab(kind) && self.st.popup.as_ref().is_some_and(|p| crate::control::is_tab(p.kind) && p.out == out) {
            self.switch_tab(kind);
            return;
        }
        // другая всплывашка вместо раскрытого островка: островок уходит сразу — сворачиваться
        // ему было бы нечем (одна всплывашка на шелл, его поверхность нарисовала бы чужую)
        if !same && self.st.popup_is("island") {
            self.close_popup_now();
        } else {
            self.close_popup();
        }
        self.close_menu();
        if same {
            return;
        }
        self.close_modal();
        self.st.popup = Some(Popup { kind, out: out.into() });
        self.popup_opened(kind);
        let (w, h) = popups::size(kind, &self.st);
        let (ow, _) = self.output_size(out);
        if kind == "island" {
            // раскрытый островок — поверх свёрнутого, вниз от верхнего края
            self.mk_catcher_on(out, self.bar_layer());
            self.st.island.open_at = Some(std::time::Instant::now());
            self.st.island.live = None;
            self.st.island.leave = None;
            self.st.island.big = None;
            // у своего края, посередине вдоль него
            let (m, _, _) = crate::bar::geo(&self.st.set, ow);
            let oh = self.output_size(out).1;
            let (x, y) = match crate::island::edge(&self.st.set) {
                crate::island::Edge::Top => ((ow - w) / 2, m),
                crate::island::Edge::Bottom => ((ow - w) / 2, oh - m - h),
            };
            let id = self.mk(SKind::Popup, out, self.bar_layer(), "yami-island", Anchor::TOP | Anchor::LEFT, (w, h), (y, 0, 0, x), -1, KeyboardInteractivity::OnDemand);
            if let Some(s) = self.surfs.get_mut(&id) {
                s.anchor_side = "island".into();
            }
            self.dirty(&SKind::Bar);
            return;
        }
        let x = (ax - w / 2).clamp(6, (ow - w - 6).max(6));
        let (layer, kb) = self.popup_layer(kind);
        self.mk_catcher_on(out, layer);
        // видимый верх — на 6 px ниже бара (где бы он ни был); содержимое всплывашки внутри
        // поверхности опущено на высоту подъёма (12), поэтому сама поверхность — выше.
        // Бар снизу — всплывашка над ним: низ содержимого и есть низ поверхности
        let low = self.bar_reserve_bottom(out);
        let (anchor, margin) = if low > 0 {
            (Anchor::BOTTOM | Anchor::LEFT, (0, 0, low + 6, x))
        } else {
            (Anchor::TOP | Anchor::LEFT, ((self.bar_bottom(out) + 6 - 12).max(0), 0, 0, x))
        };
        let id = self.mk(SKind::Popup, out, layer, "yami-popup", anchor, (w, h + 12), margin, -1, kb);
        if low > 0 {
            // верх поверхности на мониторе: от него меню трея во всплывашке ищет свою строку
            let (_, oh) = self.output_size(out);
            if let Some(s) = self.surfs.get_mut(&id) {
                s.margin = (x, oh - low - 6 - h - 12);
                s.applied = s.margin;
            }
        }
    }

    /// слой и клавиатура всплывашки. Калькулятор зовут и хоткеем поверх игры или видео: слой выше
    /// полноэкранных окон, и клавиатура сразу его — цифры без щелчка. Всплывашки поднятого бара
    /// (peek.rs) — тоже поверх, но клавиатуру не забирают
    fn popup_layer(&self, kind: &str) -> (Layer, KeyboardInteractivity) {
        if kind == "calc" {
            (Layer::Overlay, KeyboardInteractivity::Exclusive)
        } else {
            (self.bar_layer(), KeyboardInteractivity::OnDemand)
        }
    }

    /// всплывашка этого вида только что показана (или панель переключилась на эту вкладку)
    fn popup_opened(&mut self, kind: &'static str) {
        // модули бара рисуют, открыта ли их всплывашка (стрелка трея, календарь)
        self.dirty(&SKind::Bar);
        match kind {
            "net" => net::refresh_wifi(self.tx.clone()),
            "bt" => net::refresh_bt(self.tx.clone()),
            "cal" => {
                // история показана — прочитано
                self.st.notifs.seen = Some(std::time::Instant::now());
                self.st.cal_month = 0;
                self.st.notif_scroll = 0;
                self.st.cal_pick = None;
            }
            "wallhaven" => self.wh_opened(),
            "fan" => self.st.fan.sample(),
            "sys" => self.st.procs.sample(),
            _ => {}
        }
        self.viz_sync();
    }

    /// вкладка панели управления: окно то же, меняется только содержимое
    pub fn switch_tab(&mut self, kind: &'static str) {
        let Some(p) = self.st.popup.as_mut() else { return };
        if p.kind == kind || !crate::control::is_tab(p.kind) {
            return;
        }
        p.kind = kind;
        // на калькуляторе клавиатура — его сразу, на остальных — по щелчку
        let kb = if kind == "calc" { KeyboardInteractivity::Exclusive } else { KeyboardInteractivity::OnDemand };
        for s in self.surfs.values_mut().filter(|s| s.kind == SKind::Popup && !s.closing) {
            s.layer.set_keyboard_interactivity(kb);
            s.layer.commit();
        }
        self.st.drop = None;
        self.st.field_focus = None;
        self.popup_opened(kind);
        self.dirty(&SKind::Popup);
    }

    pub fn resize_popup(&mut self) {
        let Some(p) = &self.st.popup else { return };
        let (w, h) = popups::size(p.kind, &self.st);
        // у всплывашек сверху запас на всплытие (12 px); раскрытый островок не всплывает
        let h = h + if p.kind == "island" { 0 } else { 12 };
        for s in self.surfs.values_mut() {
            if s.kind == SKind::Popup && !s.closing && (s.w != w || s.h != h) {
                s.layer.set_size(w as u32, h as u32);
                s.layer.commit();
            }
        }
    }

    pub fn close_popup(&mut self) {
        // раскрытый островок сворачивается сам: края рассыпаются до свёрнутой полосы
        // (island::draw_panel), поверхность уходит потом (island_tick)
        if self.st.popup_is("island") && self.st.set.anim() {
            if self.st.island.closing.is_none() {
                self.st.island.closing = Some(std::time::Instant::now());
                // меню трея из островка уходит вместе с ним
                self.close_menu();
                self.st.drop = None;
                self.st.field_focus = None;
                if self.st.menu.is_none() {
                    self.close_kind(&SKind::Catcher);
                }
                self.dirty(&SKind::Popup);
            }
            return;
        }
        self.close_popup_now();
    }

    /// закрыть всплывашку сразу, без своего сворачивания островка
    pub fn close_popup_now(&mut self) {
        self.st.island.closing = None;
        if self.st.popup.take().is_some() {
            self.st.drop = None;
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
        self.mk_catcher_on(out, Layer::Top);
    }

    /// ловец на своём слое: над полноэкранным окном — Overlay, как и всплывашка
    pub fn mk_catcher_on(&mut self, out: &str, layer: Layer) {
        if self.find(&SKind::Catcher).is_empty() {
            let id = self.mk(SKind::Catcher, out, layer, "yami-catch", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
            // ловит всё, кроме самого бара: бар живой, а поля вокруг плавающего — нет
            let (w, h) = self.output_size(out);
            let mut rs = vec![R::new(0, 0, w, h)];
            // поверхность бара как она есть (сверху, снизу, сбоку — островок): вокруг неё четыре куска
            if let Some(b) = self.find_on(&SKind::Bar, out).and_then(|id| self.surfs.get(&id)).map(|s| R::new(s.margin.0, s.margin.1, s.w, s.h)) {
                rs = vec![R::new(0, 0, w, b.y), R::new(0, b.y, b.x, b.h), R::new(b.right(), b.y, w - b.right(), b.h), R::new(0, b.bottom(), w, h - b.bottom())];
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
        // над точкой (из списка трея) — низ меню у курсора; нижний бар не закрываем
        let y = if m.above { m.y - h } else { m.y };
        let y = y.clamp(4, (oh - h - 8 - self.bar_reserve_bottom(&out)).max(4));
        let mut m = m;
        m.last = Some((w, h));
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
