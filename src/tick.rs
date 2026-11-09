// Тик шелла: раз в секунду (100 мс, пока что-то движется) — часы, датчики, батарея, файл
// настроек; таймеры простоя (замок, гашение экрана).

use crate::app::*;
use crate::clock::local_now;
use crate::proc::reap_inherited;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

impl App {
    // ── тик: часы, датчики, файл настроек ───────────────────────────────────
    pub(crate) fn tick_interval(&self) -> Duration {
        let fast = !self.st.notifs.popups.is_empty() || self.st.osd.is_some() || self.st.field_focus.is_some() || self.st.locked;
        let fast = fast || self.st.wall_fade.is_some() || self.st.dock_leave.is_some() || !self.st.dock_bounce.is_empty();
        // смена обоев: свой темп кадров (обои под окнами niri будит редко)
        let ms = if self.st.wall_fade.is_some() { 33 } else if fast { 100 } else { 1000 };
        Duration::from_millis(ms)
    }

    /// автоподключение: отмеченные сопряжённые устройства, пока не подключены, — попытка раз
    /// в 30 с, пока Bluetooth включён и не идёт сопряжение
    fn bt_auto_tick(&mut self) {
        let b = &self.st.bt;
        if !b.powered || !b.busy.is_empty() || self.st.set.bt_auto.is_empty() {
            return;
        }
        let now = Instant::now();
        // подключённые — из памяти попыток (отключатся — пробовать сразу) и из отключённых руками
        let on: Vec<&String> = b.devices.iter().filter(|d| d.connected).map(|d| &d.path).collect();
        self.st.bt_auto_try.retain(|p, _| !on.contains(&p));
        self.st.bt_auto_hold.retain(|p| !on.contains(&p));
        let due = bt_due(&self.st.bt.devices, &self.st.set.bt_auto, &self.st.bt_auto_try, &self.st.bt_auto_hold, now);
        for p in due {
            self.st.bt_auto_try.insert(p.clone(), now);
            crate::net::bt_connect_quiet(p, self.tx.clone());
        }
    }

    /// батарея садится: одно уведомление на 10% и одно на 5%, пока не поставят на зарядку
    pub(crate) fn bat_check(&mut self) {
        let Some(b) = self.st.sys.bat else { return };
        if b.charging || b.plugged {
            self.st.bat_warn = 0;
            return;
        }
        let lvl = if b.pct <= 5 { 2 } else if b.pct <= 10 { 1 } else { 0 };
        if lvl > self.st.bat_warn {
            self.st.bat_warn = lvl;
            let left = b.mins.map(|m| crate::tf!("осталось {} мин", "{} min left", m)).unwrap_or_default();
            self.toast_note(&crate::tf!("батарея {}%", "battery {}%", b.pct), if left.is_empty() { "подключи зарядку" } else { &left });
        }
    }

    pub(crate) fn tick(&mut self) {
        reap_inherited();
        let now = Instant::now();
        // старт позади (конфиги, темы значков, шрифты, все модули) или окно закрыто полминуты
        // назад: их код больше не исполняется — из памяти; нужное вернётся из кеша страниц
        if self.shed_at.is_some_and(|t| now >= t) && self.st.modal.is_none() && self.st.popup.is_none() {
            self.shed_at = None;
            yami_px::shed_code();
        }
        // мигание курсора ввода — раз в полсекунды
        let blink = (self.st.started.elapsed().as_millis() / 500) % 2 == 0;
        if blink != self.st.blink {
            self.st.blink = blink;
            if self.st.field_focus.is_some() || self.st.locked {
                self.dirty_panels();
                self.dirty_where(|k| matches!(k, SKind::Widget(_) | SKind::Popup));
                for l in self.lock_surfs.iter_mut() {
                    l.dirty = true;
                }
            }
        }
        if now.duration_since(self.last_sample) >= Duration::from_millis(990) {
            self.last_sample = now;
            self.weather_tick();
            self.bt_auto_tick();
            // игра на весь экран закрывает все бары: датчики видеокарты не нужны, а nvidia-smi раз в
            // 3 с под игрой — лишняя работа драйвера (рывки кадров)
            let fo = self.focus_out();
            let hidden = self.st.game.fullscreen && self.output_names().iter().all(|n| *n == fo || !self.bar_on(n));
            self.st.sys.want_gpu = self.st.sys.has_gpu && self.uses("gpu") && !hidden;
            self.st.sys.sample();
            if self.st.popup_is("sys") {
                self.st.procs.sample();
            }
            // вентилятор: история копится, пока модуль в баре, — открытая всплывашка видит прошлое
            if self.st.set.on_bar("fan") || self.st.popup_is("fan") {
                self.st.fan.sample();
                if self.st.popup_is("fan") {
                    self.dirty(&SKind::Popup);
                }
            }
            self.wall_park();
            self.bat_check();
            // память видна только в настройках: не читать её каждую секунду зря
            if self.st.settings_shown || self.st.rss == 0.0 {
                self.st.rss = crate::sys::self_mem_mb();
            }
            // плеер во всплывашке: время и полоса позиции идут (с эквалайзером их двигают его кадры)
            if crate::popups::shows_player(&self.st) && self.st.media.playing() && !crate::popups::shows_viz(&self.st) {
                self.dirty(&SKind::Popup);
            }
            let lt = local_now();
            let sec_needed = self.st.set.clock_seconds || self.st.set.widgets.iter().any(|w| w.on && (w.kind == "clock" || w.kind == "graph" || w.kind == "net"));
            if sec_needed || lt.min != self.last_min {
                self.last_min = lt.min;
                self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Widget(_) | SKind::Popup));
                for l in self.lock_surfs.iter_mut() {
                    l.dirty = true;
                }
            } else {
                self.dirty(&SKind::Bar);
            }
            self.check_settings_file();
            // ночной свет упал сам — значок в баре гаснет
            let n = self.st.night.running();
            if n != self.st.night.on {
                self.st.night.on = n;
                self.dirty(&SKind::Bar);
            }
        }
        // смена обоев: карточка панели управления идёт вместе с обоями; конец — отпустить прошлые
        if crate::wall::fade_k(&self.st).is_some() {
            if self.st.popup_is("control") {
                self.dirty(&SKind::Popup);
            }
            // кадр обоев — по таймеру, не дожидаясь ответа niri на прошлый
            for s in self.surfs.values_mut().filter(|s| s.kind == SKind::Wall) {
                s.dirty = true;
                s.frame_pending = false;
            }
            // стол закрыт окном на весь экран: под ним не рисуем, и смена сама не дойдёт до конца —
            // время вышло, значит, она закончена; откроют стол — сразу новые обои, без доигрывания
            if self.st.game.fullscreen && crate::wall::fade_k(&self.st).is_some_and(|k| k >= 1.0) {
                crate::wall::finish_fade(&self.st);
            }
            if crate::wall::fade_done(&self.st) {
                self.st.wall_prev = None;
                self.st.wall_fade = None;
                *self.st.wall_mix.borrow_mut() = None;
                self.dirty(&SKind::Wall);
            }
        } else if self.st.wall_fade.is_some() {
            self.st.wall_fade = None;
        }
        // уведомления и всплывашка громкости гаснут сами
        if self.notif_tick() {
            self.sync_notifs();
        }
        self.dock_tick();
        if let Some((_, t)) = self.st.osd {
            if t.elapsed().as_secs_f32() > self.st.set.osd_time {
                self.st.osd = None;
                self.close_kind(&SKind::Osd);
            }
        }
    }

    pub(crate) fn uses(&self, src: &str) -> bool {
        let s = &self.st.set;
        s.widgets.iter().any(|w| w.on && w.kind == "graph" && w.source == src) || (src == "gpu" && (self.st.popup_is("sys") || s.on_bar("gpu")))
    }

    // ── простой: замок и гашение экрана ─────────────────────────────────────
    pub(crate) fn setup_idle(&mut self) {
        let cfg = (self.st.set.lock_after, self.st.set.screen_off_after);
        if cfg == self.idle_cfg && !self.idle_notes.is_empty() {
            return;
        }
        self.idle_cfg = cfg;
        for n in self.idle_notes.drain(..) {
            n.destroy();
        }
        let (Some(idle), Some(seat)) = (self.idle.clone(), self.seat.seats().next()) else { return };
        if cfg.0 > 0 {
            self.idle_notes.push(idle.get_idle_notification(cfg.0 * 60_000, &seat, &self.qh, 0));
        }
        if cfg.1 > 0 {
            self.idle_notes.push(idle.get_idle_notification(cfg.1 * 60_000, &seat, &self.qh, 1));
        }
    }
}

/// кого автоподключать сейчас: отмеченные сопряжённые, не подключённые и не отключённые руками; то
/// же устройство уже подключено через другой адаптер — не трогать; попытка не чаще раза в 30 с
fn bt_due(devs: &[crate::net::BtDev], auto: &[String], tried: &HashMap<String, Instant>, hold: &HashSet<String>, now: Instant) -> Vec<String> {
    use crate::net::bt_key;
    devs.iter()
        .filter(|d| d.paired && !d.connected && !hold.contains(&d.path) && auto.iter().any(|k| k == bt_key(&d.path)))
        .filter(|d| !devs.iter().any(|o| o.connected && bt_key(&o.path) == bt_key(&d.path)))
        .filter(|d| tried.get(&d.path).is_none_or(|t| now.duration_since(*t).as_secs() >= 30))
        .map(|d| d.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::BtDev;

    fn dev(path: &str, connected: bool) -> BtDev {
        BtDev { path: path.into(), connected, paired: true, ..Default::default() }
    }

    #[test]
    fn auto_connect_skips_held_and_the_same_device_on_another_adapter() {
        let now = Instant::now();
        let auto = vec!["dev_AA".to_string()];
        let (tried, mut hold) = (HashMap::new(), HashSet::new());
        let one = [dev("/org/bluez/hci0/dev_AA", false)];
        assert_eq!(bt_due(&one, &auto, &tried, &hold, now), vec!["/org/bluez/hci0/dev_AA".to_string()]);
        hold.insert("/org/bluez/hci0/dev_AA".to_string());
        assert!(bt_due(&one, &auto, &tried, &hold, now).is_empty(), "disconnected by hand");
        hold.clear();
        let two = [dev("/org/bluez/hci0/dev_AA", false), dev("/org/bluez/hci1/dev_AA", true)];
        assert!(bt_due(&two, &auto, &tried, &hold, now).is_empty(), "already connected via hci1");
        let tried = HashMap::from([("/org/bluez/hci0/dev_AA".to_string(), now)]);
        assert!(bt_due(&one, &auto, &tried, &hold, now + Duration::from_secs(10)).is_empty());
        assert_eq!(bt_due(&one, &auto, &tried, &hold, now + Duration::from_secs(30)).len(), 1);
    }
}
