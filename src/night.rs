// Ночной свет: вечером цвета теплее через wlsunset (wlr-gamma-control, он у niri есть). От заката
// до восхода по координатам или по часам, если координат нет. Процесс принадлежит шеллу и
// умирает вместе с ним. Модуль бара `night` показывает и переключает.
// i18n: inline pairs

use crate::app::{App, SKind};
use std::process::{Child, Command, Stdio};

#[derive(Default)]
pub struct Night {
    /// работал при последней проверке (по этому рисует бар, обновляет tick)
    pub on: bool,
    child: Option<Child>,
    args: Vec<String>,
}

impl Night {
    pub fn running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }
    fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            // wlsunset сам возвращает гамму, когда завершается
            unsafe { libc::kill(c.id() as i32, libc::SIGTERM) };
            let _ = c.wait();
        }
    }
}

impl Drop for Night {
    fn drop(&mut self) {
        self.stop();
    }
}

/// «55.75, 37.62», «55.75 37.62», «55.75 ,37.62» → (широта, долгота)
pub fn geo(s: &str) -> Option<(f64, f64)> {
    let mut it = s.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|x| !x.is_empty());
    let (a, b) = (it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let (lat, lon) = (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
}

/// аргументы wlsunset по настройкам
pub fn args(set: &crate::config::Settings) -> Vec<String> {
    // ночная температура всегда ниже дневной (6500): иначе wlsunset не запустится
    let mut a = vec!["-t".to_string(), set.num("night_temp").to_string(), "-T".into(), "6500".into()];
    match geo(&set.night_geo) {
        Some((lat, lon)) => a.extend(["-l".into(), format!("{lat}"), "-L".into(), format!("{lon}")]),
        None => {
            let (on, off) = (set.num("night_on_h"), set.num("night_off_h"));
            // одинаковые часы — ночь в один час: включить на час позже, чем выключить
            let on = if on == off { (off + 1) % 24 } else { on };
            let h = |x: i32| format!("{x:02}:00");
            a.extend(["-S".into(), h(off), "-s".into(), h(on)]);
        }
    }
    a
}

impl App {
    pub fn night_sync(&mut self) {
        let s = &self.st.set;
        if !s.night || !crate::proc::which("wlsunset") {
            if self.st.night.on {
                self.st.night.stop();
                self.st.night.on = false;
                self.dirty(&SKind::Bar);
            }
            return;
        }
        let a = args(s);
        if self.st.night.running() && self.st.night.args == a {
            return;
        }
        self.st.night.stop();
        let mut cmd = Command::new("wlsunset");
        cmd.args(&a).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        self.st.night.child = crate::proc::owned(&mut cmd).spawn().ok();
        self.st.night.on = self.st.night.child.is_some();
        self.st.night.args = a;
        self.dirty(&SKind::Bar);
    }

    pub fn night_toggle(&mut self) {
        if !crate::proc::which("wlsunset") {
            self.toast_note("ночной свет", "нужен wlsunset: поставь его из своего дистрибутива");
            return;
        }
        self.st.set.night = !self.st.set.night;
        self.settings_changed();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn coords() {
        assert_eq!(super::geo("55.75, 37.62"), Some((55.75, 37.62)));
        assert_eq!(super::geo("55.75 37.62"), Some((55.75, 37.62)));
        assert_eq!(super::geo("55.75 ,37.62"), Some((55.75, 37.62)));
        assert_eq!(super::geo("-33.9,18.4"), Some((-33.9, 18.4)));
        assert_eq!(super::geo("1, 2, 3"), None);
        assert_eq!(super::geo("95, 0"), None);
        assert_eq!(super::geo(""), None);
    }

    #[test]
    fn wlsunset_args() {
        let mut s = crate::config::Settings { night_temp: 6500, ..Default::default() };
        let a = super::args(&s);
        assert_eq!(&a[..4], ["-t", "6000", "-T", "6500"]);
        s.night_on_h = 7;
        s.night_off_h = 7;
        let a = super::args(&s);
        assert_eq!(&a[4..], ["-S", "07:00", "-s", "08:00"]);
    }
}
