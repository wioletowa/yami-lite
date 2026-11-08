// Вкладка «цвет» → «темы» (#8): свои темы из ~/.config/yami/themes (bundle.rs) карточками,
// щелчок применяет, × удаляет, кнопка сохраняет текущий вид. Файлы тем бывают по десятку МБ
// (картинка обоев внутри): читаются и пишутся в нити, ответ — Msg::Themes.

use crate::app::{App, Msg, SKind, State};
use crate::gfx::{BoxStyle, C, R};
use crate::ui::{Act, Cur, Tone, Ui};
use yami_px::font::PX;

const COLS: i32 = 4;
const CARD_H: i32 = 58;

pub fn tab(ui: &mut Ui, st: &State, b: R, head_w: i32) {
    let t = ui.t;
    let mut y = crate::settings::title(ui, b.x, b.y, head_w, "свои темы — щелчок применяет");
    let cw = (b.w - (COLS - 1) * 8) / COLS;
    // ряды, что помещаются над кнопкой сохранения
    let rows = ((b.bottom() - 92 - y) / (CARD_H + 8)).max(1);
    let shown = (rows * COLS) as usize;
    if st.themes.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 22), "пока пусто — сохрани текущий вид кнопкой ниже", t.text_faint);
    }
    for (i, c) in st.themes.iter().take(shown).enumerate() {
        let r = R::new(b.x + (i as i32 % COLS) * (cw + 8), y + (i as i32 / COLS) * (CARD_H + 8), cw, CARD_H);
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(t.surface, if hov { t.red } else { t.border }).bevel(true).shadow(2));
        ui.zone(r, Act::STheme(i), Cur::Pointer);
        // образец: акцент на фоне своей темы (светлый или тёмный)
        let sw = R::new(r.x + 8, r.y + 8, r.w - 16, 22);
        let bg = yami_px::theme::Theme::new(c.accent, 1.0, 1.0, 0.0, 1.0, c.light).panel;
        let ac = C(0xff000000 | c.accent);
        ui.cv.pxbox(sw, &BoxStyle::new(bg, t.border).bevel(true));
        ui.cv.fill(R::new(sw.x + 4, sw.y + 4, sw.w / 2 - 4, sw.h - 8), ac);
        if c.wall {
            ui.cv.icon("image", sw.right() - 18, sw.y + 4, 2, ac);
        }
        // имя — как назвал владелец, не переводится
        ui.cv.text_in(ui.f, PX, R::new(r.x + 8, r.y + 34, r.w - 30, 18), &c.name, if hov { t.text_hi } else { t.text });
        let x = R::new(r.right() - 22, r.y + 33, 16, 18);
        let xh = ui.hover(x);
        ui.cv.icon("close", x.x + 1, x.y + 2, 2, if xh { t.ember } else { t.text_faint });
        ui.zone(x, Act::SThemeDel(i), Cur::Pointer);
    }
    if st.themes.len() > shown {
        let more = crate::tf!("ещё {} — yami theme list", "{} more: yami theme list", "mais {}: yami theme list", st.themes.len() - shown);
        ui.cv.text_in(ui.f, PX, R::new(b.x, y + rows * (CARD_H + 8), b.w, 18), &more, t.text_faint);
    }
    y = b.bottom() - 80;
    ui.button(R::new(b.x, y, 220, 28), "сохранить текущий вид", "save", Tone::Primary, false, true, Act::SCycle("theme_save"));
    ui.cv.text_in(ui.f, PX, R::new(b.x + 232, y, b.w - 232, 28), &st.theme_note, t.text_dim);
    ui.text_in(PX, R::new(b.x, y + 36, b.w, 20), "поделиться своей: yami theme export ФАЙЛ", t.text_faint);
    ui.text_in(PX, R::new(b.x, y + 56, b.w, 20), "поставить чужую: yami theme import ФАЙЛ или ССЫЛКА", t.text_faint);
}

impl App {
    /// list | save | apply | remove (i — карточка). Тема читает настройки с диска: сначала
    /// записывается то, что сейчас в памяти
    pub fn theme_do(&mut self, op: &'static str, i: usize) {
        let name = self.st.themes.get(i).map(|c| c.name.clone()).unwrap_or_default();
        if matches!(op, "apply" | "remove") && name.is_empty() {
            return;
        }
        self.save_now();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let r = match op {
                "apply" => crate::bundle::apply(&name).map(|_| crate::tf!("применена: {}", "applied: {}", "aplicado: {}", name)),
                "remove" => crate::bundle::remove(&name).map(|_| crate::tf!("удалена: {}", "removed: {}", "removido: {}", name)),
                "save" => crate::bundle::save("").map(|n| crate::tf!("сохранена: {}", "saved: {}", "salvo: {}", n)),
                _ => Ok(String::new()),
            };
            let _ = tx.send(Msg::Themes(crate::bundle::cards(), r.unwrap_or_else(|e| e), op == "apply"));
            // тема с картинкой — мегабайты JSON: куча шелла не должна их держать
            yami_px::trim_heap();
        });
    }

    pub fn themes_listed(&mut self, list: Vec<crate::bundle::Card>, note: String, applied: bool) {
        self.st.themes = list;
        self.st.theme_note = note;
        // применённая тема уже в файле настроек: перечитать сейчас, не ждать тика
        if applied {
            self.check_settings_file();
        }
        self.dirty(&SKind::Modal);
    }
}
