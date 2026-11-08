// Яркость экрана и лампочки клавиатуры (Caps Lock, Num Lock) для OSD: sysfs не шлёт событий, поэтому
// нить читает несколько маленьких файлов раз в 150 мс и сообщает только об изменениях.

use crate::app::Msg;
use calloop::channel::Sender;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Leds {
    /// яркость 0..1; None — подсветки нет (стационарный монитор)
    pub bright: Option<f32>,
    pub caps: bool,
    pub num: bool,
}

fn read_num(p: &Path) -> Option<u64> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// первая подсветка экрана: (brightness, max_brightness)
fn backlight() -> Option<(PathBuf, u64)> {
    let d = std::fs::read_dir("/sys/class/backlight").ok()?.flatten().next()?.path();
    let max = read_num(&d.join("max_brightness")).filter(|m| *m > 0)?;
    Some((d.join("brightness"), max))
}

/// лампочки клавиатур с этим суффиксом (input3::capslock…): горит хоть одна
fn leds(suffix: &str) -> Vec<PathBuf> {
    std::fs::read_dir("/sys/class/leds")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(suffix)))
        .map(|p| p.join("brightness"))
        .collect()
}

fn any_on(l: &[PathBuf]) -> bool {
    l.iter().any(|p| read_num(p).is_some_and(|v| v > 0))
}

pub fn spawn(tx: Sender<Msg>) {
    let _ = std::thread::Builder::new().name("leds".into()).spawn(move || {
        let bl = backlight();
        // клавиатуру могут подключить позже: список лампочек — заново раз в ~5 с
        let (mut caps, mut num) = (leds("::capslock"), leds("::numlock"));
        let mut last: Option<Leds> = None;
        for n in 0u64.. {
            if n % 33 == 32 {
                (caps, num) = (leds("::capslock"), leds("::numlock"));
            }
            let now = Leds { bright: bl.as_ref().and_then(|(p, max)| read_num(p).map(|v| v as f32 / *max as f32)), caps: any_on(&caps), num: any_on(&num) };
            if last != Some(now) {
                if tx.send(Msg::Leds(now)).is_err() {
                    return;
                }
                last = Some(now);
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });
}

/// яркость экрана 0..1 через logind (SetBrightness сессии): без прав на /sys и без brightnessctl.
/// Одна нить на всё время: ползунок шлёт значения пачкой, уходит последнее
pub fn set_brightness(v: f32) {
    static TX: std::sync::OnceLock<std::sync::mpsc::Sender<f32>> = std::sync::OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<f32>();
        let _ = std::thread::Builder::new().name("brightness".into()).spawn(move || {
            let Some((path, max)) = backlight() else { return };
            let Some(name) = path.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()) else { return };
            let Ok(c) = zbus::blocking::Connection::system() else { return };
            while let Ok(mut v) = rx.recv() {
                while let Ok(n) = rx.try_recv() {
                    v = n;
                }
                let val = ((v.clamp(0.01, 1.0) * max as f32).round() as u32).max(1);
                let _ = c.call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1/session/auto", Some("org.freedesktop.login1.Session"), "SetBrightness", &("backlight", name.as_str(), val));
            }
        });
        tx
    });
    let _ = tx.send(v);
}
