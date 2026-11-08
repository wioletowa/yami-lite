// Игровой режим шелла: пока игра (или любое окно на весь экран) в фокусе — уведомления молча
// уходят в историю, а то, что под игрой, шелл не перерисовывает. Правила окон niri — не здесь:
// они в твоём config.kdl.

use crate::app::{App, State};
use crate::gfx::R;
use crate::settings::title;
use crate::ui::{Act, Ui};

/// свои app-id из настроек: через запятую или пробел; `osu!` — точно, `minecraft*` — начало
fn user_ids(s: &str) -> Vec<String> {
    s.split([',', ' ']).map(|x| x.trim()).filter(|x| !x.is_empty()).map(String::from).collect()
}

/// это игра? (по app-id)
pub fn is_game(set: &crate::config::Settings, app_id: &str) -> bool {
    app_id.starts_with("steam_app_")
        || app_id == "gamescope"
        || user_ids(&set.game_apps).iter().any(|u| match u.strip_suffix('*') {
            Some(p) => app_id.starts_with(p),
            None => app_id == u,
        })
}

/// игровой режим шелла
#[derive(Default)]
pub struct GameMode {
    /// сейчас в фокусе игра / окно на весь экран
    pub game: bool,
    pub fullscreen: bool,
}

impl App {
    /// после событий niri: что в фокусе — игра, на весь экран?
    pub fn game_sync(&mut self) {
        let (game, full) = match self.st.niri.focused_win() {
            Some(w) => {
                let out = w.ws.and_then(|i| self.st.niri.ws.iter().find(|x| x.id == i)).map(|x| x.output.clone()).unwrap_or_default();
                let (ow, oh) = self.output_size(&out);
                let full = w.layout.size.0 >= ow as f64 && w.layout.size.1 >= oh as f64;
                (is_game(&self.st.set, &w.app_id), full)
            }
            None => (false, false),
        };
        let was = std::mem::replace(&mut self.st.game.fullscreen, full);
        if was && !full {
            // вышли из полноэкранного: накопившееся под ним — нарисовать
            self.flush();
        }
        self.st.game.game = game;
    }

    /// уведомления сейчас только в историю: «не беспокоить» или игра на весь экран
    pub fn quiet(&self) -> bool {
        self.st.set.dnd || (self.st.set.game_dnd && (self.st.game.game || self.st.game.fullscreen))
    }
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let mut y = title(ui, b.x, b.y, b.w, "пока играешь");
    ui.toggle(R::new(b.x, y, b.w, 40), "не беспокоить", "уведомления — молча в историю, пока игра или окно на весь экран в фокусе", s.game_dnd, Act::SBool("game_dnd"));
    y += 52;
    y = title(ui, b.x, y, b.w, "что считать игрой");
    let focused = st.field_focus == Some("game_apps");
    ui.field(R::new(b.x, y, b.w - 130, 26), &st.game_field, "свои игры: app-id через запятую, osu!, minecraft*", focused, false, Act::SField("game_apps"));
    ui.button(R::new(b.right() - 120, y, 120, 26), "применить", "check", crate::ui::Tone::Plain, false, true, Act::SCycle("game_apply"));
    y += 30;
    ui.text_in(crate::font::PX, R::new(b.x, y, b.w, 18), "Steam и gamescope — всегда. app-id окна видно в «niri msg windows»", t.text_faint);
}
