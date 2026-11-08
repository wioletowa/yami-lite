// Ответ прямо из уведомления (возможность «inline-reply» org.freedesktop.Notifications): у
// мессенджера, что её предлагает (Telegram Desktop, некоторые Matrix- и почтовые клиенты), на
// карточке кнопка «ответить»; она открывает это окошко, Enter отправляет текст сигналом
// NotificationReplied. Текст не попадает ни на диск, ни в буфер обмена.
// i18n: inline pairs

use crate::app::{App, Modal, SKind, State};
use crate::font::PX;
use crate::gfx::R;
use crate::ui::{Act, Field, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

pub const SIZE: (i32, i32) = (460, 150);

/// кому отвечаем: уведомление, программа и заголовок (над полем)
#[derive(Default)]
pub struct Reply {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub field: Field,
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let r = &st.reply;
    let fr = R::new(0, 0, w - 8, h - 8);
    ui.frame(fr, true);
    ui.header(R::new(1, 1, fr.w - 2, 30), "send", &crate::tf!("ответ · {}", "reply · {}", "responder · {}", r.app), "");
    ui.ghost(R::new(fr.right() - 28, 5, 24, 22), "close", Act::CloseModal);
    // о чём ответ: заголовок уведомления, как прислали (не переводится)
    ui.cv.text_in(ui.f, PX, R::new(14, 40, fr.w - 28, 20), &r.summary, t.text_dim);
    ui.field(R::new(14, 64, fr.w - 28, 30), &r.field, "напиши ответ… Enter — отправить", true, false, Act::Block);
    let bw = 120;
    ui.button(R::new(fr.right() - 14 - bw, 104, bw, 26), "отправить", "send", Tone::Primary, false, !r.field.text.trim().is_empty(), Act::ReplySend);
    ui.button(R::new(fr.right() - 22 - 2 * bw, 104, bw, 26), "отмена", "close", Tone::Plain, false, true, Act::CloseModal);
}

impl App {
    /// «ответить» на карточке: окно ответа на это уведомление
    pub fn reply_open(&mut self, id: u32) {
        let (app, summary) = self.st.notifs.get(id).map(|n| (n.app.clone(), n.summary.clone())).unwrap_or_default();
        self.st.reply = Reply { id, app, summary, field: Field::default() };
        self.toggle_modal(Modal::Reply, false);
    }

    pub fn reply_send(&mut self) {
        let text = self.st.reply.field.text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let id = self.st.reply.id;
        crate::dbus::notif_replied(id, &text);
        self.st.reply.field.wipe();
        self.st.notifs.remove(id, false);
        crate::dbus::notif_closed(id, 2);
        self.sync_notifs();
        self.close_modal();
    }

    pub fn reply_key(&mut self, e: &KeyEvent) {
        match e.keysym {
            Keysym::Escape => self.close_modal(),
            Keysym::Return | Keysym::KP_Enter => self.reply_send(),
            _ => {
                let mods = self.mods;
                App::edit_field(&mut self.st.reply.field, e, &mods);
            }
        }
        self.dirty(&SKind::Modal);
    }
}
