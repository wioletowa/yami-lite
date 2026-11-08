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

    pub fn on_click(&mut self, a: Act, button: u32, sid: u32, zone: R, double: bool) {
        let kind = self.surf_kind(sid);
        // клик мимо всплывашки/меню
        if kind == Some(SKind::Catcher) {
            self.close_popup();
            self.close_menu();
            return;
        }
        if self.settings_click(&a) {
            return;
        }
        let (out, ax, ay) = self.anchor(sid, zone);
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
            Act::WallsDir => self.walls_ask(true),
            Act::WallsFile => self.walls_ask(false),
            // ── бар ─────────────────────────────────────────────────────────
            Act::Launcher => self.toggle_modal(Modal::Launcher, button == RMB),
            Act::Plug(cmd) => App::sh(&cmd),
            Act::NightToggle => self.night_toggle(),
            Act::ReplySend => self.reply_send(),
            Act::Power => self.toggle_modal(Modal::Power, false),
            Act::Settings => self.toggle_modal(Modal::Settings, false),
            Act::Curtain => self.curtain_cmd(""),
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
                self.tray_anchor = Some(if in_popup { (out.clone(), ax - zone.w / 2 + 8, ay + 2) } else { (out.clone(), ax - 20, if crate::bar::low(&self.st.set) { ay } else { self.bar_bottom(&out) + 2 }) });
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
            Act::Mode => self.ipc("stacking"),
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
                ("profile", RMB) => self.prof_step(1),
                _ => self.open_popup(k, &out, ax),
            },
            // ── лаунчер и питание ───────────────────────────────────────────
            Act::LRun(i) => {
                if button == RMB {
                    self.launcher_pin(i);
                } else {
                    self.launcher_run(i);
                }
            }
            Act::LTab(clip) => self.toggle_modal(Modal::Launcher, clip),
            Act::LPin(i) => self.launcher_pin(i),
            Act::LDel(i) => self.launcher_del(i),
            Act::LWipe => self.launcher_wipe(),
            Act::PowerDo(i) => self.power_do(i),
            // ── меню ────────────────────────────────────────────────────────
            Act::Menu(i) => self.menu_pick(i),
            Act::MenuBack => self.menu_pick(0),
            // ── всплывашки ──────────────────────────────────────────────────
            Act::CalMonth(d) => {
                self.st.cal_month = if d == 0 { 0 } else { self.st.cal_month + d };
            }
            Act::VolSlider | Act::MicSlider | Act::AppVol(_) => {}
            Act::VolMute => audio::toggle_mute(),
            Act::MicMute => audio::toggle_mic(),
            Act::AppMute(i) => audio::mute_stream(i),
            Act::Sink(name) => audio::set_sink(&name),
            Act::Source(name) => audio::set_source(&name),
            Act::MediaPlay => self.media_ctl("play-pause"),
            Act::MediaNext => self.media_ctl("next"),
            Act::MediaPrev => self.media_ctl("previous"),
            // шапка всплывашки музыки: ЛКМ — следующий плеер, ПКМ — прошлый
            Act::MediaPlayer => self.media_step(if button == RMB { -1 } else { 1 }),
            Act::MediaView => self.media_view_next(),
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
                net::bt_toggle_dev(d, self.tx.clone(), self.st.bt.clone());
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
                if !self.shot_note_action(id, &key) && !self.update_note_action(id) && !self.automount_note_action(id) {
                    crate::dbus::notif_action(id, &key);
                }
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
            Act::SnapBtn(a) => self.snap_btn(a),
            Act::DockItem(i) => self.dock_click(i, button, sid, zone),
            Act::DockLauncher => self.toggle_modal(Modal::Launcher, button == RMB),
            Act::DockTrash => App::open_dir("trash:///".into()),
            Act::Pavu => {
                self.close_all();
                App::spawn(&["pavucontrol".into()]);
            }
            Act::SysMon => {
                self.close_all();
                self.open_sysmon();
            }
            // ── заголовки окон ──────────────────────────────────────────────
            Act::TDrag(id) => match button {
                RMB => self.open_window_menu(id, &out, ax - zone.w / 2 + 40, ay + 2),
                MMB => self.st.niri.close_window(id),
                _ => {
                    if double {
                        self.window_op(id, "maximize");
                    } else {
                        self.st.niri.act(serde_json::json!({"FocusWindow": {"id": id}}));
                    }
                }
            },
            Act::TSize(id) => self.st.niri.act(serde_json::json!({"FocusWindow": {"id": id}})),
            Act::TMin(id) => self.st.niri.minimize(id),
            Act::TMax(id) => self.window_op(id, "maximize"),
            Act::TClose(id) => self.st.niri.close_window(id),
            // ── виджеты ─────────────────────────────────────────────────────
            Act::WDrag(id) | Act::WMenu(id) => {
                if button == RMB || matches!(a, Act::WMenu(_)) {
                    self.open_widget_menu(id);
                }
            }
            Act::EditDone => self.set_editing(false),
            Act::LockField | Act::AuthField => {}
            Act::AuthOk => self.auth_submit(),
            Act::AuthCancel => self.auth_cancel(),
            Act::Prof(p) => self.prof_set(p),
            Act::ScreenSel(_) | Act::ScreenOk | Act::ScreenCancel => self.screen_click(&a, double),
            Act::Disp(_) | Act::DispSel(_) | Act::DispDrag(_) => self.disp_click(&a),
            _ => {}
        }
        self.dirty_all();
    }

    pub fn on_wheel(&mut self, a: Act, d: i32) {
        let up = d < 0;
        match a {
            Act::Popup("sound" | "media") | Act::VolSlider | Act::VolMute => audio::step_volume(if up { 1 } else { -1 }),
            Act::WallPick(_) | Act::WallsScroll => self.walls_scroll(if up { -1 } else { 1 }),
            Act::MicSlider | Act::MicMute => {
                let v = (self.st.audio.mic + if up { 0.05 } else { -0.05 }).clamp(0.0, 1.0);
                audio::set_mic(v);
            }
            Act::AppVol(i) | Act::AppMute(i) => {
                if let Some(s) = self.st.audio.streams.iter().find(|s| s.idx == i) {
                    audio::set_stream(i, (s.vol + if up { 0.05 } else { -0.05 }).clamp(0.0, 1.0));
                }
            }
            Act::KeyScroll | Act::KeyRec(_) | Act::KeyReset(_) | Act::KeyDel(_) => self.keys_wheel(d),
            Act::Ws(_) => {
                self.st.niri.act(if up { serde_json::json!({"FocusWorkspaceUp": {}}) } else { serde_json::json!({"FocusWorkspaceDown": {}}) });
            }
            Act::DockItem(i) => self.dock_wheel(i, up),
            Act::Popup("profile") | Act::Prof(_) => self.prof_step(if up { -1 } else { 1 }),
            Act::ScreenSel(_) => self.screen_wheel(up),
            Act::Kb => self.st.niri.act(serde_json::json!({"SwitchLayout": {"layout": if up { "Prev" } else { "Next" }}})),
            Act::CalMonth(_) | Act::Popup("cal") => {
                self.st.cal_month += if up { -1 } else { 1 };
                self.dirty(&SKind::Popup);
            }
            Act::LRun(_) => {
                let n = self.st.launcher.items.len();
                if n > 0 {
                    let c = self.st.launcher.cur as i32 + if up { -1 } else { 1 };
                    self.st.launcher.cur = c.clamp(0, n as i32 - 1) as usize;
                    self.st.launcher.keep_visible();
                    self.dirty(&SKind::Modal);
                }
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
        if let Some(g) = gui.iter().find(|b| crate::doctor::which(b)) {
            App::spawn(&[g.to_string()]);
            return true;
        }
        let Some(bin) = tui.iter().find(|b| crate::doctor::which(b)) else { return false };
        let term = crate::launcher::term();
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

