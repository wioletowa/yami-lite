// Сообщения нитей (Msg): данные источников (niri, звук, сеть, D-Bus, сканы) приходят сюда и
// ложатся в State; что поменялось — помечается к перерисовке.

use crate::app::*;
use crate::{dbus, niri};
use std::time::Duration;

impl App {
    // ── сообщения нитей ─────────────────────────────────────────────────────
    pub(crate) fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Niri(v) => {
                let layout_only = v.get("WindowLayoutsChanged").is_some();
                let overview = self.st.niri.overview;
                let evs = self.st.niri.handle(v);
                if self.st.niri.overview != overview {
                    // поверх игры кайма видна только в обзоре: захват звука — тоже
                    self.viz_sync();
                }
                for e in evs {
                    match e {
                        niri::Ev::KbSwitched => self.show_osd("layout"),
                        niri::Ev::NewWindow(id) => self.on_new_window(id),
                        niri::Ev::Changed => {}
                    }
                }
                self.game_sync();
                if !layout_only {
                    self.sync_dock();
                    // окна и столы видны только в баре и доке: виджеты и всплывашки их не рисуют,
                    // а события niri идут на каждую смену фокуса
                    self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Dock));
                }
            }
            Msg::Ipc(cmd) => self.ipc(&cmd),
            Msg::Audio(mut a) => {
                // тянут ползунок панели: свежее — его значение, а не запоздалый ответ
                if self.ctl_holding() {
                    a.volume = self.st.audio.volume;
                }
                let old = &self.st.audio;
                let touched = old.ready && (a.volume != old.volume || a.muted != old.muted);
                let mic = old.ready && (a.mic != old.mic || a.mic_muted != old.mic_muted);
                self.st.audio = a;
                // первые секунды — состояние при старте, а не действие человека
                if self.st.started.elapsed() > Duration::from_millis(2500) {
                    if touched {
                        self.show_osd("volume");
                    } else if mic {
                        self.show_osd("mic");
                    }
                }
                self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Popup | SKind::Osd));
            }
            Msg::Viz(v) => {
                // ~30 раз в секунду: перерисовать только то, где эквалайзер (всплывашка с плеером,
                // виджет музыки, бар с модулем media), а не все бары и виджеты на всех мониторах
                // кайма по краям — от любого звука; эквалайзеры плеера — пока он играет
                let edge = crate::edge::on(&self.st) && self.st.edge.feed(&v, self.st.set.edge_viz_smooth);
                self.st.viz = v;
                let (s, pop) = (&self.st.set, crate::popups::shows_viz(&self.st));
                // эквалайзерам плеера хватит ~30 fps: каждый второй кадр
                let playing = self.st.media.playing() && self.st.viz_at.elapsed().as_millis() >= 30;
                if playing {
                    self.st.viz_at = std::time::Instant::now();
                }
                let bar = playing && s.on_bar("media") && s.media_viz != "off";
                let ws: Vec<u32> = if playing { s.widgets.iter().filter(|w| w.on && w.kind == "media").map(|w| w.id).collect() } else { vec![] };
                self.dirty_where(|k| match k {
                    SKind::Popup => pop && playing,
                    SKind::Bar => bar,
                    SKind::Widget(id) => ws.contains(id),
                    SKind::Edge => edge,
                    _ => false,
                });
            }
            Msg::Media(m) => self.media_got(m),
            Msg::Players(l) => self.media_players(l),
            Msg::Themes(l, note, applied) => self.themes_listed(l, note, applied),
            Msg::Art(url, img) => {
                self.st.art.insert(url, img.map(std::rc::Rc::new));
                self.dirty_where(|k| matches!(k, SKind::Popup | SKind::Widget(_)));
            }
            Msg::Notif(n) => self.on_notif(n),
            Msg::NotifClose(id) => {
                self.st.notifs.remove(id, false);
                self.sync_notifs();
                // по спецификации: закрыто вызовом CloseNotification
                dbus::notif_closed(id, 3);
            }
            Msg::Tray(e) => {
                dbus::apply(&mut self.st.tray, e);
                self.dirty(&SKind::Bar);
                if self.st.popup_is("tray") {
                    self.dirty(&SKind::Popup);
                }
            }
            Msg::TrayMenu(svc, items) => self.open_tray_menu(&svc, items),
            Msg::Net(n) => {
                // сети не хватило пароля — поле под ней, курсор в нём
                if !n.ask.is_empty() && self.st.popup_is("net") {
                    self.st.wifi_target = n.ask.clone();
                    self.st.wifi_pass.clear();
                    self.st.field_focus = Some("wifi");
                }
                self.st.net = n;
                self.dirty(&SKind::Popup);
            }
            Msg::Bt(mut b) => {
                // сигналы BlueZ не знают, что идёт сопряжение: его состояние держим до ответа
                if !b.op {
                    b.msg = std::mem::take(&mut self.st.bt.msg);
                    b.busy = std::mem::take(&mut self.st.bt.busy);
                }
                // адаптер включился (или шелл запустился с включённым): доверенные — подключить
                let came_up = b.present && b.powered && !(self.st.bt.present && self.st.bt.powered);
                self.st.bt = b;
                if came_up && self.st.set.bt_connect_on {
                    crate::net::bt_connect_pass(self.tx.clone());
                }
                self.dirty_where(|k| matches!(k, SKind::Popup | SKind::Bar));
            }
            Msg::Pam(ok) => self.on_pam(ok),
            Msg::WhResults(gen, r) => self.wh_results(gen, r),
            Msg::WhThumb(gen, id, img) => self.wh_thumb(gen, id, img),
            Msg::WhApplied(r) => self.wh_applied(r),
            Msg::Camera(u) => {
                self.st.camera = u;
                self.dirty(&SKind::Bar);
            }
            Msg::Leds(mut l) => {
                if self.ctl_holding() {
                    l.bright = self.st.leds.bright;
                }
                let old = std::mem::replace(&mut self.st.leds, l);
                if self.st.started.elapsed() > Duration::from_millis(2500) {
                    if old.caps != l.caps {
                        self.show_osd("caps");
                    } else if old.num != l.num {
                        self.show_osd("num");
                    } else if old.bright.is_some() && old.bright != l.bright {
                        self.show_osd("bright");
                    }
                }
            }
            Msg::Wall(n, img, accent) => self.wall_ready(n, img, accent),
            Msg::Weather(w) => self.weather_got(w),
            Msg::Desk(dir, items) => self.desk_got(dir, items),
            Msg::DeskThumb(k, img) => self.desk_thumb(k, img),
            Msg::DeskDone(err) => self.desk_done(err),
            Msg::DeskDropped(paths, at, mv) => self.desk_dropped(paths, at, mv),
            Msg::DeskRenamed(dir, pairs) => self.desk_renamed(dir, pairs),
            Msg::DeskPlaced(names, at, err) => self.desk_placed(names, at, err),
            Msg::DeskInfo(s) => self.desk_info(s),
            Msg::PrepareSleep => self.prepare_sleep(),
            Msg::HidePicked(app) => self.hide_toggle(app),
            Msg::Clip(e) => self.clip_event(e),
            Msg::ClipThumb(b, img) => self.clip_thumb(b, img),
            Msg::ClipFiles(id, v) => self.clip_files(id, v),
            Msg::IconsRecolored => {
                self.res.icons.drop_cache();
                self.dirty_all();
            }
            Msg::DeskPasted(paths, dest, cut, cell) => self.desk_pasted(paths, dest, cut, cell),
            Msg::WallsListed(gen, dir, dirs, files, more) => self.walls_listed(gen, dir, dirs, files, more),
            Msg::WallThumb(gen, i, img) => self.walls_thumb(gen, i, img),
        }
    }

    pub(crate) fn on_new_window(&mut self, _id: u64) {}
}
