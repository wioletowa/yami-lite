// Питание: столбик кнопок.
// Стрелки/Tab — выбор, Enter — выполнить, буква — сразу, Esc — закрыть.

use crate::app::{App, State};
use crate::font::PX;
use crate::gfx::R;
use crate::ui::{Act, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

pub const SIZE: (i32, i32) = (306, 272);

pub const ACTIONS: &[(&str, &str, &str, &str)] = &[
    ("lock", "заблокировать", "L", "lock"),
    ("moon", "сон", "S", "sleep"),
    ("exit", "выйти", "E", "logout"),
    ("reboot", "перезагрузка", "R", "reboot"),
    ("power", "выключить", "P", "poweroff"),
];

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, false);
    ui.header(R::new(1, 1, fr.w - 2, 30), "power", "питание", "");
    for (i, (icon, text, key, _)) in ACTIONS.iter().enumerate() {
        let r = R::new(16, 44 + i as i32 * 38, fr.w - 32, 32);
        let danger = i == ACTIONS.len() - 1;
        let cur = st.power_cur == i;
        ui.button(r, text, icon, if danger { Tone::Danger } else { Tone::Plain }, false, true, Act::PowerDo(i));
        if cur {
            ui.cv.outline(r, t.ember, 1);
        }
        ui.text_right(PX, R::new(r.x, r.y, r.w - 10, r.h), key, t.text_faint);
    }
}

impl App {
    pub fn power_do(&mut self, i: usize) {
        self.close_modal();
        if let Some(a) = ACTIONS.get(i) {
            self.sys_action(a.3);
        }
    }

    /// действие меню питания (и строки ACTIONS)
    pub fn sys_action(&mut self, a: &str) {
        match a {
            "lock" => self.lock_now(),
            "sleep" => {
                self.lock_now();
                App::sh("sleep 0.6; systemctl suspend");
            }
            "logout" => self.st.niri.act(serde_json::json!({"Quit": {"skip_confirmation": true}})),
            "reboot" => App::spawn(&["systemctl".into(), "reboot".into()]),
            "poweroff" => App::spawn(&["systemctl".into(), "poweroff".into()]),
            _ => {}
        }
    }

    pub fn power_key(&mut self, e: &KeyEvent) {
        let n = ACTIONS.len();
        match e.keysym {
            Keysym::Escape => self.close_modal(),
            Keysym::Down | Keysym::Right | Keysym::Tab => self.st.power_cur = (self.st.power_cur + 1) % n,
            Keysym::Up | Keysym::Left | Keysym::ISO_Left_Tab => self.st.power_cur = (self.st.power_cur + n - 1) % n,
            Keysym::Return | Keysym::KP_Enter | Keysym::space => {
                let c = self.st.power_cur;
                self.power_do(c);
            }
            _ => {
                // буква — и в русской раскладке (по месту клавиши), и в AZERTY (по подписи)
                if let Some(c) = crate::keys::latin_letter(e) {
                    if let Some(i) = ACTIONS.iter().position(|a| a.2.starts_with(c)) {
                        self.power_do(i);
                    }
                }
            }
        }
        self.dirty_panels();
    }
}
