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
                let had_windows = !self.st.niri.wins.is_empty();
                // первый полный список окон: только теперь видно, что уже открыто
                let full_list = v.get("WindowsChanged").is_some();
                let evs = self.st.niri.handle(v);
                for e in evs {
                    match e {
                        niri::Ev::KbSwitched => self.show_osd("layout"),
                        niri::Ev::NewWindow(id) => self.on_new_window(id),
                        niri::Ev::Changed => {}
                    }
                }
                if !layout_only {
                    self.st.session.dirty = true;
                }
                if full_list {
                    self.session_restore();
                }
                // «стол» при входе: окна пришли — делаем их плавающими
                if !had_windows && !self.st.niri.wins.is_empty() && self.st.niri.stacking {
                    let on = self.st.niri.stacking;
                    self.st.niri.set_stacking(on);
                }
                self.game_sync();
                self.sync_titles();
                if !layout_only {
                    self.sync_dock();
                    // окна и столы видны только в баре и доке: виджеты и всплывашки их не рисуют,
                    // а события niri идут на каждую смену фокуса
                    self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Dock));
                }
            }
            Msg::Ipc(cmd) => self.ipc(&cmd),
            Msg::Audio(a) => {
                let touched = self.st.audio.ready && (a.volume != self.st.audio.volume || a.muted != self.st.audio.muted);
                self.st.audio = a;
                if touched && self.st.started.elapsed() > Duration::from_millis(2500) {
                    self.show_osd("volume");
                }
                self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Popup | SKind::Osd));
            }
            Msg::Viz(v) => {
                // ~30 раз в секунду: перерисовать только то, где эквалайзер (всплывашка с плеером,
                // виджет музыки, бар с модулем media), а не все бары и виджеты на всех мониторах
                self.st.viz = v;
                let (s, pop) = (&self.st.set, crate::popups::shows_viz(&self.st));
                let bar = s.on_bar("media") && s.media_viz != "off";
                let ws: Vec<u32> = s.widgets.iter().filter(|w| w.on && w.kind == "media").map(|w| w.id).collect();
                self.dirty_where(|k| match k {
                    SKind::Popup => pop,
                    SKind::Bar => bar,
                    SKind::Widget(id) => ws.contains(id),
                    _ => false,
                });
            }
            Msg::Media(m) => self.media_got(m),
            Msg::Players(l) => self.media_players(l),
            Msg::Themes(l, note, applied) => self.themes_listed(l, note, applied),
            Msg::Weather(w) => self.weather_got(w),
            Msg::Lyrics(l) => {
                // ответ на прошлый трек — мимо
                if l.key == self.st.lyrics.key {
                    self.st.lyrics = l;
                    // текста нет — всплывашка показывает обложку, и эквалайзеру нужен звук
                    self.viz_sync();
                    self.dirty(&SKind::Popup);
                }
            }
            Msg::Art(url, img) => {
                self.st.art.insert(url, img.map(std::rc::Rc::new));
                self.dirty_where(|k| matches!(k, SKind::Popup | SKind::Widget(_)));
            }
            Msg::Notif(n) => self.on_notif(n),
            Msg::Volumes(v) => self.automount_volumes(v),
            Msg::AutoMounted(v, res) => self.automount_done(v, res),
            Msg::Update(i) => self.on_update(i),
            Msg::Snap(imgs, wins, mode) => self.on_snap_ready(imgs, wins, mode),
            Msg::SnapDone { path, copied, err, thumb } => self.on_snap_done(path, copied, err, thumb),
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
                self.st.bt = b;
                self.dirty_where(|k| matches!(k, SKind::Popup | SKind::Bar));
            }
            Msg::Pam(ok) => self.on_pam(ok),
            Msg::Auth(e) => self.on_auth(e),
            Msg::Prof(p) => self.on_prof(p),
            Msg::ScreenPick(types, s) => self.screen_ask(types, s),
            Msg::Clip(lines) => {
                self.st.launcher.clip_lines = lines;
                self.st.launcher.refresh(&self.st.apps, &self.st.niri.wins);
                if self.st.modal == Some(Modal::Launcher) {
                    self.st.launcher.thumbs_start(self.tx.clone());
                }
                self.dirty(&SKind::Modal);
            }
            Msg::ClipThumb(k, img) => {
                if self.st.modal == Some(Modal::Launcher) {
                    self.st.launcher.thumbs.insert(k, img);
                    if self.st.launcher.clip {
                        self.dirty(&SKind::Modal);
                    }
                }
            }
            Msg::Plugin(id, p) => {
                if self.st.plugins.shown.get(&id) != Some(&p) {
                    self.st.plugins.shown.insert(id, p);
                    self.dirty(&SKind::Bar);
                }
            }
            Msg::SnapText(qr, text) => self.on_snap_text(qr, text),
            Msg::LFiles(q, found) => {
                self.st.launcher.files = (q, found);
                if self.st.modal == Some(Modal::Launcher) {
                    self.st.launcher.refresh(&self.st.apps, &self.st.niri.wins);
                    self.dirty(&SKind::Modal);
                }
            }
            Msg::Wall(n, img, accent) => self.wall_ready(n, img, accent),
            Msg::WallsListed(gen, dir, files, more) => self.walls_listed(gen, dir, files, more),
            Msg::WallThumb(gen, i, img) => self.walls_thumb(gen, i, img),
            Msg::WallsAsked(folder, got, back) => self.walls_asked(folder, got, back),
            Msg::SkwdLayers(drawn) => self.skwd_layers(drawn),
            Msg::Curtain(n, imgs) => self.curtain_ready(n, imgs),
            Msg::Theme => {
                self.sync_wall();
                self.res.icons.drop_cache();
                self.st.restyle = crate::restyle::load();
                self.dirty(&SKind::Modal);
            }
            Msg::Restyled => {
                self.st.restyle = crate::restyle::load();
                self.dirty(&SKind::Modal);
            }
            Msg::Sets(b) => {
                self.st.sets_busy = false;
                // настройки успели закрыться — превью не нужны
                if self.st.modal == Some(Modal::Settings) {
                    self.st.sets = Some(*b);
                    self.dirty(&SKind::Modal);
                }
            }
            Msg::SetAdded(r) => self.sets_added(r),
        }
    }

    pub(crate) fn on_new_window(&mut self, id: u64) {
        // в режиме «стол»: под заголовок нужно место сверху
        self.fit_title(id);
        self.session_place(id);
    }
}
