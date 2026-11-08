// Профиль питания (power-profiles-daemon, он же tuned-ppd): экономия / баланс / производительность.
// Читаем и меняем по системной шине короткими нитями; без демона модуль бара прячется.

use crate::app::{App, Msg, SKind};
use calloop::channel::Sender;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use zbus::blocking::Proxy;
use zbus::zvariant::{OwnedValue, Value};

/// (имя на шине, путь, интерфейс): новое имя и старое
const BUS: &[(&str, &str, &str)] = &[
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles"),
];

/// порядок, значок и подпись профилей
pub const KINDS: &[(&str, &str, &str)] = &[("power-saver", "leaf", "экономия"), ("balanced", "gauge", "баланс"), ("performance", "bolt", "производительность")];

static BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Default, Clone, PartialEq)]
pub struct Prof {
    pub present: bool,
    pub active: String,
    pub list: Vec<String>,
    /// почему «производительность» урезана (перегрев, ноутбук на коленях); пусто — всё в порядке
    pub degraded: String,
}

impl Prof {
    pub fn icon(&self) -> &'static str {
        KINDS.iter().find(|k| k.0 == self.active).map(|k| k.1).unwrap_or("gauge")
    }
}

pub fn label(p: &str) -> &'static str {
    KINDS.iter().find(|k| k.0 == p).map(|k| k.2).unwrap_or("баланс")
}

fn proxy<'a>(c: &'a zbus::blocking::Connection) -> Option<Proxy<'a>> {
    for (name, path, iface) in BUS {
        if let Ok(p) = Proxy::new(c, *name, *path, *iface) {
            if p.get_property::<String>("ActiveProfile").is_ok() {
                return Some(p);
            }
        }
    }
    None
}

fn query() -> Prof {
    let Some(c) = crate::net::system() else { return Prof::default() };
    let Some(p) = proxy(&c) else { return Prof::default() };
    let list = p
        .get_property::<Vec<HashMap<String, OwnedValue>>>("Profiles")
        .unwrap_or_default()
        .iter()
        .filter_map(|m| match m.get("Profile").map(|v| &**v) {
            Some(Value::Str(s)) => Some(s.to_string()),
            _ => None,
        })
        .collect();
    Prof { present: true, active: p.get_property("ActiveProfile").unwrap_or_default(), list, degraded: p.get_property("PerformanceDegraded").unwrap_or_default() }
}

/// перечитать (и, если задан, сначала включить профиль); один запрос за раз
pub fn refresh(set: Option<String>, tx: Sender<Msg>) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        if let (Some(v), Some(c)) = (set, crate::net::system()) {
            if let Some(p) = proxy(&c) {
                let _ = p.set_property("ActiveProfile", v);
            }
        }
        let _ = tx.send(Msg::Prof(query()));
        BUSY.store(false, Ordering::SeqCst);
    });
}

impl App {
    pub fn on_prof(&mut self, p: Prof) {
        if p != self.st.prof {
            self.st.prof = p;
            self.dirty(&SKind::Bar);
            self.dirty(&SKind::Popup);
        }
    }

    /// следующий (или предыдущий) профиль по кругу
    pub fn prof_step(&mut self, d: i32) {
        let ps: Vec<&str> = KINDS.iter().map(|k| k.0).filter(|k| self.st.prof.list.iter().any(|l| l == k)).collect();
        if ps.is_empty() {
            return;
        }
        let i = ps.iter().position(|k| *k == self.st.prof.active).unwrap_or(0) as i32;
        let n = ps[(i + d).rem_euclid(ps.len() as i32) as usize].to_string();
        self.prof_set(n);
    }

    pub fn prof_set(&mut self, p: String) {
        // сразу показать выбор, демон подтвердит
        self.st.prof.active = p.clone();
        self.dirty(&SKind::Bar);
        self.dirty(&SKind::Popup);
        refresh(Some(p), self.tx.clone());
    }
}
