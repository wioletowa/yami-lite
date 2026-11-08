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
