// Клики и колесо: зона под курсором → действие. ЛКМ — основное, ПКМ — меню / второе,
// СКМ — третье (закрыть окно, свернуть). Колесо — громкость, столы, месяцы.

use crate::app::{App, Modal, SKind};
use crate::gfx::R;
use crate::ui::Act;
use crate::{audio, net};

const LMB: u32 = 0x110;
const RMB: u32 = 0x111;
const MMB: u32 = 0x112;

impl App {
    /// откуда открывать всплывашку: центр зоны в координатах монитора
    fn anchor(&self, sid: u32, zone: R) -> (String, i32, i32) {
        let s = self.surfs.get(&sid);
        let out = s.map(|s| s.output.clone()).unwrap_or_else(|| self.focus_out());
        let (mx, my) = s.map(|s| s.margin).unwrap_or((0, 0));
        (out, mx + zone.x + zone.w / 2, my + zone.bottom())
    }

    pub fn on_click(&mut self, a: Act, button: u32, sid: u32, zone: R) {
        let kind = self.surf_kind(sid);
        // клик мимо всплывашки/меню
        if kind == Some(SKind::Catcher) {
            self.close_popup();
            self.close_menu();
            if self.st.walls_anchored {
                self.close_modal();
            }
            return;
        }
        match &a {
            Act::Drop(k) => return self.drop_open(k, sid, zone),
            Act::DropPick(v) => return self.drop_pick(v.clone()),
            Act::DropClose | Act::DropScroll => return self.drop_close(),
            _ => {}
        }
        if self.settings_click(&a) || self.wh_click(&a) {
            return;
        }
        let (out, ax, ay) = self.anchor(sid, zone);
        // ПКМ по бару: окна и трей — свои меню, в остальном месте — панель управления
        if kind == Some(SKind::Bar) && button == RMB && !matches!(a, Act::Task(_) | Act::Tray(_)) {
            let (mx, _) = self.surfs.get(&sid).map(|s| s.margin).unwrap_or((0, 0));
            let px = self.ptr.filter(|p| p.0 == sid).map_or(zone.x + zone.w / 2, |p| p.1 as i32);
            let py = if crate::bar::low(&self.st.set) { ay } else { self.bar_bottom(&out) + 2 };
            let _ = py;
            self.toggle_control(&out, mx + px);
            return;
        }
        match a {
            Act::Block => {
                if self.st.field_focus.is_some() && kind != Some(SKind::Modal) {
                    self.st.field_focus = None;
                }
            }
            Act::CloseModal => self.close_modal(),
            // ── выбор обоев ─────────────────────────────────────────────────
            Act::WallPick(i) => self.walls_pick(i),
            Act::WallsBack => self.walls_back(),
            Act::WallsUp => self.walls_up(),
            Act::WallsHere => self.walls_here(),
            Act::WallsSort => {
                self.st.set.walls_sort = if self.st.set.walls_sort == "name" { "new" } else { "name" }.into();
                let d = self.st.walls.dir.clone();
                self.walls_go(d);
            }
            // ── бар ─────────────────────────────────────────────────────────
            Act::Launcher | Act::DockLauncher => App::sh(&self.st.set.launcher_cmd.clone()),
            Act::NightToggle => self.night_toggle(),
            Act::BarWalls => self.walls_at(&out, ax),
            Act::CtlTile(k) => self.control_tile(&k),
            Act::CtlTab(k) => self.switch_tab(k),
            Act::CtlViz => {
                // стили по кругу, после последнего — «выкл» (захват звука не нужен), затем снова первый
                let st = crate::widgets::STYLES;
                self.st.set.ctl_viz = match st.iter().position(|(k, _)| *k == self.st.set.ctl_viz) {
                    Some(i) if i + 1 < st.len() => st[i + 1].0.into(),
                    Some(_) => "off".into(),
                    None => st[0].0.into(),
                };
                self.media_sync();
                self.settings_changed();
            }
            Act::CtlWeatherSet => {
                self.close_popup();
                self.open_settings_tab("system");
                self.st.field_focus = Some("weather_city");
            }
            Act::CtlWall => {
                self.close_popup();
                self.walls_open(false, false);
            }
            Act::ReplySend => self.reply_send(),
            Act::Power => self.toggle_modal(Modal::Power),
            Act::Settings => self.toggle_settings(),
            Act::Ws(id) => {
                if let Some(ws) = self.st.niri.ws.iter().find(|w| w.id == id).cloned() {
                    self.st.niri.focus_ws(&ws);
                }
            }
            Act::Task(id) => match button {
                RMB => self.open_window_menu(id, &out, ax - 20, ay + 2),
                MMB => self.st.niri.close_window(id),
                _ => self.st.niri.toggle_minimize(id),
            },
            Act::Tray(svc) => {
                let Some(it) = self.st.tray.iter().find(|t| t.service == svc) else { return };
                let (bus, path, menu, is_menu) = (it.bus.clone(), it.path.clone(), it.menu.clone(), it.is_menu);
                // меню программы: под значком в баре или под её строкой во всплывашке трея
                let in_popup = kind == Some(SKind::Popup);
                // из списка трея — меню над курсором; из бара — под баром (над нижним)
                let at_ptr = self.ptr.filter(|p| p.0 == sid).zip(self.surfs.get(&sid)).map(|(p, s)| (s.margin.0 + p.1 as i32, s.margin.1 + p.2 as i32));
                self.tray_anchor = Some(match (in_popup, at_ptr) {
                    (true, Some((px, py))) => (out.clone(), px - 12, py - 4, true),
                    (true, None) => (out.clone(), ax - zone.w / 2 + 8, ay - zone.h, true),
                    _ => (out.clone(), ax - 20, if crate::bar::low(&self.st.set) { ay } else { self.bar_bottom(&out) + 2 }, false),
                });
                if button == RMB || (button == LMB && is_menu) {
                    if !menu.is_empty() {
                        crate::dbus::tray_menu(&svc, &bus, &menu, self.tx.clone());
                    } else {
                        crate::dbus::tray_call(&bus, &path, "ContextMenu", ax, ay);
                    }
                } else {
                    crate::dbus::tray_call(&bus, &path, if button == MMB { "SecondaryActivate" } else { "Activate" }, ax, ay);
                    // программа открыла окно: всплывашка трея своё сделала
                    if in_popup {
                        self.close_popup();
                    }
                }
            }
            Act::Kb => self.st.niri.act(serde_json::json!({"SwitchLayout": {"layout": "Next"}})),
            Act::Popup(k) => match (k, button) {
                ("sound", RMB) => audio::toggle_mute(),
                ("sound", MMB) => App::spawn(&["pavucontrol".into()]),
                // музыка: ПКМ — пауза/играть, СКМ — следующий трек
                ("media", RMB) => self.media_ctl("play-pause"),
                ("media", MMB) => self.media_ctl("next"),
                ("net", RMB) | ("bt", RMB) => App::spawn(&[if k == "net" { "nm-connection-editor" } else { "blueman-manager" }.into()]),
                ("cal", RMB) => {
                    self.st.set.dnd = !self.st.set.dnd;
                    self.settings_changed();
                }
                ("sys", RMB) => self.open_sysmon(),
                _ => self.open_popup(k, &out, ax),
            },
            // ── питание ─────────────────────────────────────────────────────
            Act::PowerDo(i) => self.power_do(i),
            // ── меню ────────────────────────────────────────────────────────
            Act::Menu(i) => self.menu_pick(i),
            Act::MenuBack => self.menu_pick(0),
            // ── всплывашки ──────────────────────────────────────────────────
            Act::CalMonth(d) => {
                self.st.cal_month = if d == 0 { 0 } else { self.st.cal_month + d };
                // «к сегодня» — и выбор снимается
                if d == 0 {
                    self.st.cal_pick = None;
                }
            }
            Act::CalDay(day) => {
                self.st.cal_pick = if self.st.cal_pick == Some(day) { None } else { Some(day) };
            }
            Act::VolSlider | Act::MicSlider | Act::AppVol(_) => {}
            Act::VolMute => audio::toggle_mute(),
            Act::MicMute => audio::toggle_mic(),
            Act::AppMute(i) => audio::mute_stream(i),
            Act::Sink(name) => audio::set_sink(&name),
            Act::Source(name) => audio::set_source(&name),
            Act::MediaPlay => self.media_ctl("play-pause"),
            // сразу на экране и в списке плееров (из него собирается показанный), плеер подтвердит
            // своей строкой
            Act::MediaRepeat => {
                let rep: String = crate::media::repeat_next(&self.st.media.inst, &self.st.media.repeat).into();
                self.media_patch(|m| m.repeat = rep.clone());
            }
            Act::MediaShuffle => {
                crate::media::shuffle_toggle(&self.st.media.inst);
                self.media_patch(|m| m.shuffle = m.shuffle.map(|v| !v));
            }
            Act::MediaNext => self.media_ctl("next"),
            Act::MediaPrev => self.media_ctl("previous"),
            // шапка всплывашки музыки: ЛКМ — следующий плеер, ПКМ — прошлый
            Act::MediaPlayer => self.media_step(if button == RMB { -1 } else { 1 }),
            Act::Wifi(on) => net::wifi_cmd(vec!["radio".into(), "wifi".into(), if on { "on" } else { "off" }.into()], self.tx.clone(), self.st.net.clone()),
            Act::WifiNet(ssid) => self.wifi_connect(&ssid),
            Act::WifiRescan => {
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::app::Msg::Net(net::query_wifi(true)));
                });
            }
            Act::NetEditor => {
                self.close_popup();
                App::open_tool(&["nm-connection-editor"], &["nmtui"]);
            }
            Act::BtManager => {
                self.close_popup();
                App::open_tool(&["blueman-manager", "overskride"], &["bluetoothctl"]);
            }
            Act::BtPower(on) => net::bt_power(&self.st.bt.adapter.clone(), on, self.tx.clone()),
            Act::BtDev(path) => {
                let Some(d) = self.st.bt.devices.iter().find(|d| d.path == path).cloned() else { return };
                // отключил руками — автоподключение не возвращает, пока не подключат снова; занято
                // сразу, чтобы тик не увидел его ещё подключённым и не снял запрет
                if d.connected {
                    self.st.bt_auto_hold.insert(path.clone());
                } else {
                    self.st.bt_auto_hold.remove(&path);
                }
                self.st.bt.busy = path;
                net::bt_toggle_dev(d, self.tx.clone(), self.st.bt.clone());
            }
            Act::BtOpen(path) => {
                self.st.bt_open = if self.st.bt_open == path { String::new() } else { path };
            }
            Act::BtTrust(path) => {
                let Some(d) = self.st.bt.devices.iter().find(|d| d.path == path).cloned() else { return };
                let on = !d.trusted;
                net::bt_trust(d, on, self.tx.clone());
            }
            Act::BtAuto(path) => {
                let key = net::bt_key(&path).to_string();
                self.st.bt_auto_hold.remove(&path);
                let list = &mut self.st.set.bt_auto;
                match list.iter().position(|k| *k == key) {
                    Some(i) => {
                        list.remove(i);
                    }
                    None => {
                        list.push(key);
                        self.st.bt_auto_try.remove(&path);
                    }
                }
                self.settings_changed();
            }
            Act::BtForget(path) => {
                let Some(d) = self.st.bt.devices.iter().find(|d| d.path == path).cloned() else { return };
                self.st.bt_open.clear();
                self.st.bt_auto_hold.remove(&path);
                // забытое не подключать
                if let Some(i) = self.st.set.bt_auto.iter().position(|k| k == net::bt_key(&path)) {
                    self.st.set.bt_auto.remove(i);
                    self.settings_changed();
                }
                net::bt_remove(&self.st.bt.adapter.clone(), d, self.tx.clone());
            }
            Act::BtScan => net::bt_scan(&self.st.bt.adapter.clone(), self.tx.clone()),
            Act::Dnd => {
                self.st.set.dnd = !self.st.set.dnd;
                self.settings_changed();
            }
            Act::NotifClear => {
                for id in self.st.notifs.history.iter().map(|n| n.id).collect::<Vec<_>>() {
                    crate::dbus::notif_closed(id, 2);
                }
                self.st.notifs.history.clear();
                self.st.notifs.popups.clear();
                self.sync_notifs();
                self.resize_popup();
            }
            Act::NotifDel(id) => {
                self.st.notifs.remove(id, kind == Some(SKind::Popup));
                crate::dbus::notif_closed(id, 2);
                self.sync_notifs();
            }
            Act::NotifAct(id, key) if key == "inline-reply" => self.reply_open(id),
            Act::NotifAct(id, key) => {
                crate::dbus::notif_action(id, &key);
                self.st.notifs.remove(id, false);
                crate::dbus::notif_closed(id, 2);
                self.sync_notifs();
            }
            Act::NotifClick(id) => {
                if button == RMB {
                    self.st.notifs.remove(id, false);
                    crate::dbus::notif_closed(id, 2);
                    self.sync_notifs();
                } else {
                    self.notif_click(id, true);
                    if kind == Some(SKind::Popup) {
                        self.close_popup();
                    }
                }
            }
            Act::DockItem(i) => self.dock_click(i, button, sid, zone),
            Act::DockTrash => App::open_dir("trash:///".into()),
            Act::Pavu => {
                self.close_all();
                App::spawn(&["pavucontrol".into()]);
            }
            Act::SysMon => {
                self.close_all();
                self.open_sysmon();
            }
            // ── виджеты ─────────────────────────────────────────────────────
            Act::WDrag(id) | Act::WMenu(id) => {
                if button == RMB || matches!(a, Act::WMenu(_)) {
                    self.open_widget_menu(id);
                }
            }
            Act::EditDone => self.set_editing(false),
            Act::LockField => {}
            _ => {}
        }
        self.dirty_all();
    }

    pub fn on_wheel(&mut self, a: Act, d: i32) {
        let up = d < 0;
        match a {
            Act::Popup("sound" | "media") | Act::VolSlider | Act::VolMute => audio::step_volume(if up { 1 } else { -1 }),
            Act::CtlBright => self.ctl_brightness(self.st.leds.bright.unwrap_or(0.5) + if up { 0.05 } else { -0.05 }),
            Act::CtlVol => self.ctl_volume((self.st.audio.volume + if up { 0.05 } else { -0.05 }).clamp(0.0, 1.0)),
            Act::Popup("fan") | Act::FanLevel => {
                self.st.fan.step(up);
                self.dirty(&SKind::Popup);
                self.dirty(&SKind::Bar);
            }
            Act::WallPick(_) | Act::WallsScroll => self.walls_scroll(if up { -1 } else { 1 }),
            Act::DropPick(_) | Act::DropScroll => self.drop_scroll(if up { -1 } else { 1 }),
            Act::SFont(_) | Act::SFontScroll => self.font_scroll(if up { -1 } else { 1 }),
            Act::MicSlider | Act::MicMute => {
                let v = (self.st.audio.mic + if up { 0.05 } else { -0.05 }).clamp(0.0, 1.0);
                audio::set_mic(v);
            }
            Act::AppVol(i) | Act::AppMute(i) => {
                if let Some(s) = self.st.audio.streams.iter().find(|s| s.idx == i) {
                    audio::set_stream(i, (s.vol + if up { 0.05 } else { -0.05 }).clamp(0.0, 1.0));
                }
            }
            Act::Ws(_) => {
                self.st.niri.act(if up { serde_json::json!({"FocusWorkspaceUp": {}}) } else { serde_json::json!({"FocusWorkspaceDown": {}}) });
            }
            Act::DockItem(i) => self.dock_wheel(i, up),
            Act::CtlTab(_) => {
                if let Some(k) = self.st.popup.as_ref().map(|p| crate::control::tab_step(p.kind, up)) {
                    self.switch_tab(k);
                }
            }
            Act::Kb => self.st.niri.act(serde_json::json!({"SwitchLayout": {"layout": if up { "Prev" } else { "Next" }}})),
            Act::CalMonth(_) | Act::Popup("cal") => {
                self.st.cal_month += if up { -1 } else { 1 };
                self.dirty(&SKind::Popup);
            }
            Act::Tray(svc) => {
                if let Some(it) = self.st.tray.iter().find(|t| t.service == svc) {
                    crate::dbus::tray_call(&it.bus, &it.path, "Scroll", if up { -120 } else { 120 }, 0);
                }
            }
            Act::SSlider(key, from, to, seg) => {
                let cur = crate::prefs::settings_get_f32(&self.st.set, key);
                let step = (to - from) / seg as f32;
                self.slider_set(key, (cur + if up { step } else { -step }).clamp(from, to));
            }
            Act::SNum(k, _) => {
                self.st.set.step_num(k, if up { 1 } else { -1 });
                self.settings_changed();
            }
            _ => {}
        }
    }

    /// первая найденная графическая программа, иначе текстовая в терминале: на чистой системе
    /// nm-connection-editor и blueman часто не стоят, а nmtui и bluetoothctl идут с сервисами
    fn open_tool(gui: &[&str], tui: &[&str]) -> bool {
        if let Some(g) = gui.iter().find(|b| crate::proc::which(b)) {
            App::spawn(&[g.to_string()]);
            return true;
        }
        let Some(bin) = tui.iter().find(|b| crate::proc::which(b)) else { return false };
        let term = crate::proc::term();
        let mut v = vec![term.clone()];
        if !term.ends_with("kitty") {
            v.push("-e".into());
        }
        v.push(bin.to_string());
        App::spawn(&v);
        true
    }

    fn open_sysmon(&self) {
        if !App::open_tool(&[], &["btop", "htop"]) {
            App::spawn(&["plasma-systemmonitor".into()]);
        }
    }
}

