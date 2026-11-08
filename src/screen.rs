// Окно «что показать» для показа экрана (OBS, Discord, браузеры): портал yami-files
// (screencast.rs) спрашивает по сокету шелла `pick-screen <типы>` и ждёт строку ответа:
// «monitor DP-1», «window 42» или «cancel». Мониторы — карточками, окна — списком со значками.

use crate::app::{App, Modal, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Tone, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use std::io::Write;
use std::os::unix::net::UnixStream;

const W: i32 = 560;
/// окон в списке не больше — дальше колесо
const ROWS: usize = 8;

pub struct Item {
    /// «monitor DP-1» / «window 42» — то, что уйдёт порталу
    pub reply: String,
    pub monitor: bool,
    pub title: String,
    pub sub: String,
    pub icon: String,
    /// монитор: размер для мини-схемы
    pub size: (i32, i32),
}

pub struct Pick {
    pub stream: UnixStream,
    pub items: Vec<Item>,
    pub sel: usize,
    pub scroll: usize,
}

pub fn size(p: &Pick) -> (i32, i32) {
    let mons = p.items.iter().any(|i| i.monitor);
    let wins = p.items.iter().filter(|i| !i.monitor).count().min(ROWS) as i32;
    (W, 44 + if mons { 30 + 96 + 12 } else { 0 } + if wins > 0 { 30 + wins * 32 + 8 } else { 0 } + 52)
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let Some(p) = &st.screen_pick else { return };
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, true);
    ui.header(R::new(1, 1, fr.w - 2, 30), "screen", "что показать", "");
    let x = 16;
    let iw = fr.w - 32;
    let mut y = 42;
    let mons: Vec<(usize, &Item)> = p.items.iter().enumerate().filter(|(_, i)| i.monitor).collect();
    if !mons.is_empty() {
        y = crate::settings::title(ui, x, y, iw, "экраны");
        let cw = ((iw - (mons.len() as i32 - 1) * 10) / mons.len() as i32).min(170);
        for (k, (i, it)) in mons.iter().enumerate() {
            let r = R::new(x + k as i32 * (cw + 10), y, cw, 92);
            let sel = p.sel == *i;
            let hov = ui.hover(r);
            ui.cv.pxbox(r, &BoxStyle::new(if sel { t.selection } else { t.surface }, if sel { t.ember } else if hov { t.red } else { t.border }).bevel(true).shadow(2));
            // мини-монитор в пропорциях экрана
            let (mw, mh) = it.size;
            let k2 = (46.0 / mh.max(1) as f32).min((cw - 24) as f32 / mw.max(1) as f32);
            let (sw, sh) = (((mw as f32 * k2) as i32).max(8), ((mh as f32 * k2) as i32).max(6));
            let m = R::new(r.x + (r.w - sw) / 2, r.y + 8, sw, sh);
            ui.cv.pxbox(m, &BoxStyle::new(t.code_bg, if sel { t.ember } else { t.border }));
            ui.cv.fill(R::new(m.x + m.w / 2 - 6, m.bottom(), 12, 3), t.border);
            ui.cv.text_center(ui.f, PX, R::new(r.x, r.y + 58, r.w, 16), &it.title, t.text);
            ui.cv.text_center(ui.f, PX, R::new(r.x, r.y + 74, r.w, 16), &it.sub, if sel { t.text_dim } else { t.text_faint });
            ui.zone(r, Act::ScreenSel(*i), Cur::Pointer);
        }
        y += 104;
    }
    let wins: Vec<(usize, &Item)> = p.items.iter().enumerate().filter(|(_, i)| !i.monitor).collect();
    if !wins.is_empty() {
        let more = if wins.len() > ROWS { crate::tf!("{} — колесо листает", "{} — scroll for more", "{} — role para ver mais", wins.len()) } else { String::new() };
        y = crate::settings::title(ui, x, y, iw, "окна");
        if !more.is_empty() {
            ui.text_right(PX, R::new(x, y - 34, iw, 20), &more, t.text_faint);
        }
        for (i, it) in wins.iter().skip(p.scroll).take(ROWS).map(|(i, it)| (*i, *it)) {
            let r = R::new(x, y, iw, 30);
            let sel = p.sel == i;
            ui.button(r, "", "", Tone::Ghost, sel, true, Act::ScreenSel(i));
            ui.app_icon(&it.icon, r.x + 8, r.y + 7, 16, 1.0);
            ui.cv.text_in(ui.f, PX, R::new(r.x + 32, r.y, r.w - 200, r.h), &it.title, if sel { t.text } else { t.text_dim });
            ui.cv.text_right(ui.f, PX, R::new(r.x, r.y, r.w - 10, r.h), &it.sub, t.text_faint);
            y += 32;
        }
    }
    let by = fr.h - 40;
    ui.button(R::new(fr.w - 16 - 260, by, 124, 28), "отмена", "close", Tone::Plain, false, true, Act::ScreenCancel);
    ui.button(R::new(fr.w - 16 - 130, by, 130, 28), "показать", "check", Tone::Primary, false, !p.items.is_empty(), Act::ScreenOk);
    ui.text_in(PX, R::new(x, by, fr.w - 300, 28), "enter — показать · esc — отмена", t.text_faint);
}

impl App {
    /// запрос от портала: собрать мониторы и окна, показать окно выбора
    pub fn screen_ask(&mut self, types: u32, stream: UnixStream) {
        if let Some(old) = self.st.screen_pick.take() {
            let _ = (&old.stream).write_all(b"cancel\n");
        }
        if self.st.locked {
            let _ = (&stream).write_all(b"cancel\n");
            return;
        }
        let mut items = vec![];
        if types & 1 != 0 || types == 0 {
            for o in crate::displays::load().into_iter().filter(|o| o.on) {
                let (w, h) = o.size();
                items.push(Item { reply: format!("monitor {}", o.name), monitor: true, title: o.name.clone(), sub: format!("{w}×{h}"), icon: String::new(), size: (w, h) });
            }
        }
        if types & 2 != 0 {
            let mut wins = self.st.niri.wins.clone();
            wins.sort_by(|a, b| b.ts.partial_cmp(&a.ts).unwrap_or(std::cmp::Ordering::Equal));
            for w in wins {
                let icon = crate::icons::icon_for(&self.st.apps, &w.app_id);
                let title = if w.title.is_empty() { w.app_id.clone() } else { w.title.clone() };
                items.push(Item { reply: format!("window {}", w.id), monitor: false, title, sub: w.app_id.clone(), icon, size: (0, 0) });
            }
        }
        // по умолчанию — экран в фокусе
        let focus = self.st.niri.focused_output();
        let sel = items.iter().position(|i| i.monitor && i.title == focus).unwrap_or(0);
        let pick = crate::screen::Pick { stream, items, sel, scroll: 0 };
        let (w, h) = size(&pick);
        self.st.screen_pick = Some(pick);
        if self.st.apps.is_empty() {
            self.st.apps = crate::icons::load_apps();
        }
        // поверх всего, как окно пароля (оно важнее — не перебиваем его)
        if self.st.modal == Some(Modal::Auth) {
            return;
        }
        self.close_all();
        let out = self.focus_out();
        self.st.modal = Some(Modal::Screen);
        self.mk(SKind::Dim, &out, Layer::Overlay, "yami-dim", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
        self.mk(SKind::Modal, &out, Layer::Overlay, "yami-screen", Anchor::empty(), (w, h + 12), (0, 0, 0, 0), -1, KeyboardInteractivity::Exclusive);
    }

    /// ответ порталу и закрыть окно (None — отмена)
    pub fn screen_reply(&mut self, ok: bool) {
        let Some(p) = self.st.screen_pick.take() else { return };
        let line = match p.items.get(p.sel) {
            Some(i) if ok => format!("{}\n", i.reply),
            _ => "cancel\n".to_string(),
        };
        let _ = (&p.stream).write_all(line.as_bytes());
        if self.st.modal == Some(Modal::Screen) {
            self.close_modal();
        }
    }

    pub fn screen_click(&mut self, a: &Act, double: bool) {
        match a {
            Act::ScreenSel(i) => {
                if let Some(p) = self.st.screen_pick.as_mut() {
                    p.sel = *i;
                }
                if double {
                    self.screen_reply(true);
                    return;
                }
            }
            Act::ScreenOk => self.screen_reply(true),
            Act::ScreenCancel => self.screen_reply(false),
            _ => {}
        }
        self.dirty(&SKind::Modal);
    }

    pub fn screen_wheel(&mut self, up: bool) {
        if let Some(p) = self.st.screen_pick.as_mut() {
            let n = p.items.iter().filter(|i| !i.monitor).count();
            p.scroll = if up { p.scroll.saturating_sub(1) } else { (p.scroll + 1).min(n.saturating_sub(ROWS)) };
        }
        self.dirty(&SKind::Modal);
    }

    pub fn screen_key(&mut self, e: &KeyEvent) {
        let Some(p) = self.st.screen_pick.as_mut() else { return };
        let n = p.items.len().max(1);
        match e.keysym {
            Keysym::Escape => return self.screen_reply(false),
            Keysym::Return | Keysym::KP_Enter => return self.screen_reply(true),
            Keysym::Down | Keysym::Right | Keysym::Tab => p.sel = (p.sel + 1) % n,
            Keysym::Up | Keysym::Left | Keysym::ISO_Left_Tab => p.sel = (p.sel + n - 1) % n,
            _ => {}
        }
        // выбранное окно — в видимой части списка
        let first_win = p.items.iter().position(|i| !i.monitor).unwrap_or(p.items.len());
        if p.sel >= first_win {
            let k = p.sel - first_win;
            if k < p.scroll {
                p.scroll = k;
            } else if k >= p.scroll + ROWS {
                p.scroll = k + 1 - ROWS;
            }
        }
        self.dirty(&SKind::Modal);
    }
}
