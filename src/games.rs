// Игры: правила niri для игровых окон (на весь экран, VRR, разрывы на yami-niri, кеш шейдеров)
// и игровой режим шелла: пока игра (или любое окно на весь экран) в фокусе — уведомления
// молча уходят в историю, а профиль питания — «производительность»; после — как было.

use crate::app::{App, State};
use crate::gfx::R;
use crate::settings::title;
use crate::ui::{Act, Ui};

/// app-id игр по умолчанию (Steam/Proton, gamescope) — как регулярки niri
const GAME_IDS: &[&str] = &["^steam_app_", "^gamescope$"];

/// свои app-id из настроек: через запятую или пробел; `osu!` — точно, `minecraft*` — начало
fn user_ids(s: &str) -> Vec<String> {
    s.split([',', ' ']).map(|x| x.trim()).filter(|x| !x.is_empty()).map(String::from).collect()
}

fn regex_escape(s: &str) -> String {
    s.chars().map(|c| if r"\.+*?()|[]{}^$#".contains(c) { format!("\\{c}") } else { c.to_string() }).collect()
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

/// часть yami-fx.kdl: правило игровых окон и кеш шейдеров
pub fn kdl(set: &crate::config::Settings, yami: bool, l: &mut Vec<String>) {
    let tearing = set.game_tearing && yami;
    // скруглённые углы обрезали бы угол картинки и интерфейса игры
    let square = set.num("win_radius") > 0;
    if set.game_fullscreen || set.game_vrr || tearing || square {
        l.push("// games (settings → games)".into());
        l.push("window-rule {".into());
        for id in GAME_IDS {
            l.push(format!("    match app-id=r#\"{id}\"#"));
        }
        for u in user_ids(&set.game_apps) {
            let re = match u.strip_suffix('*') {
                Some(p) => format!("^{}", regex_escape(p)),
                None => format!("^{}$", regex_escape(&u)),
            };
            l.push(format!("    match app-id=r#\"{re}\"#"));
        }
        if set.game_fullscreen {
            l.push("    open-fullscreen true".into());
        }
        if set.game_vrr {
            // VRR, когда игра на экране и у монитора variable-refresh-rate on-demand=true
            l.push("    variable-refresh-rate true".into());
        }
        if tearing {
            // yami-niri: кадр — сразу, без ожидания vblank (меньше задержка, возможны разрывы)
            l.push("    allow-tearing true".into());
        }
        if square {
            l.push("    geometry-corner-radius 0".into());
        }
        l.push("    border { off; }".into());
        l.push("}".into());
        l.push(String::new());
    }
    let mut env: Vec<String> = vec![];
    if set.game_cache {
        // кеш шейдеров побольше и без чистки: меньше фризов при первом запуске сцены
        env.push("    __GL_SHADER_DISK_CACHE_SIZE \"12000000000\"".into());
        env.push("    __GL_SHADER_DISK_CACHE_SKIP_CLEANUP \"1\"".into());
        env.push("    MESA_SHADER_CACHE_MAX_SIZE \"12G\"".into());
    }
    crate::gameboost::env_kdl(set, &mut env);
    if !env.is_empty() {
        l.push("environment {".into());
        l.append(&mut env);
        l.push("}".into());
        l.push(String::new());
    }
}

/// игровой режим шелла
#[derive(Default)]
pub struct GameMode {
    /// сейчас в фокусе игра / окно на весь экран
    pub game: bool,
    pub fullscreen: bool,
    /// профиль до игры (чтобы вернуть)
    pub prof_before: Option<String>,
    /// игра в фокусе у GameMode (gameboost.rs)
    pub boost: crate::gameboost::Boost,
}

impl App {
    /// после событий niri: что в фокусе — игра, на весь экран?
    pub fn game_sync(&mut self) {
        let (game, full, pid) = match self.st.niri.focused_win() {
            Some(w) => {
                let out = w.ws.and_then(|i| self.st.niri.ws.iter().find(|x| x.id == i)).map(|x| x.output.clone()).unwrap_or_default();
                let (ow, oh) = self.output_size(&out);
                let full = w.layout.size.0 >= ow as f64 && w.layout.size.1 >= oh as f64;
                let game = is_game(&self.st.set, &w.app_id);
                (game, full, if game { w.pid } else { None })
            }
            None => (false, false, None),
        };
        crate::gameboost::focus(&self.st.set, &mut self.st.game.boost, pid);
        let was = std::mem::replace(&mut self.st.game.fullscreen, full);
        if was && !full {
            // вышли из полноэкранного: накопившееся под ним — нарисовать
            self.flush();
        }
        let gm = &mut self.st.game;
        if game == gm.game {
            return;
        }
        gm.game = game;
        if !self.st.set.game_profile || !self.st.prof.present {
            return;
        }
        if game {
            if self.st.prof.active != "performance" && self.st.prof.list.iter().any(|p| p == "performance") {
                self.st.game.prof_before = Some(self.st.prof.active.clone());
                self.prof_set("performance".into());
            }
        } else if let Some(p) = self.st.game.prof_before.take() {
            self.prof_set(p);
        }
    }

    /// уведомления сейчас только в историю: «не беспокоить» или игра на весь экран
    pub fn quiet(&self) -> bool {
        self.st.set.dnd || (self.st.set.game_dnd && (self.st.game.game || self.st.game.fullscreen))
    }
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let p = &st.game_probe;
    let half = (b.w - 20) / 2;
    let (l, r) = (b.x, b.x + half + 20);
    // окна игр: как их открывает niri
    let mut y = title(ui, b.x, b.y, b.w, "игровые окна");
    ui.toggle(R::new(l, y, half, 40), "сразу на весь экран", "Steam, gamescope и свои ниже", s.game_fullscreen, Act::SBool("game_fullscreen"));
    ui.toggle(R::new(r, y, half, 40), "VRR в играх", "экраны → VRR «в играх»", s.game_vrr, Act::SBool("game_vrr"));
    y += 46;
    let hint = if st.niri.yami { "меньше задержка, возможны разрывы" } else { "нужен yami-niri (наш форк)" };
    ui.toggle(R::new(l, y, half, 40), "разрывы (tearing)", hint, s.game_tearing, Act::SBool("game_tearing"));
    y += 46;
    // свои игры: какие ещё окна считать играми
    let focused = st.field_focus == Some("game_apps");
    ui.field(R::new(b.x, y, b.w - 130, 26), &st.game_field, "свои игры: app-id через запятую, osu!, minecraft*", focused, false, Act::SField("game_apps"));
    ui.button(R::new(b.right() - 120, y, 120, 26), "применить", "check", crate::ui::Tone::Plain, false, true, Act::SCycle("game_apply"));
    y += 30;
    ui.text_in(crate::font::PX, R::new(b.x, y, b.w, 18), "app-id окна видно в «niri msg windows». * в конце — по началу имени.", t.text_faint);
    y += 28;
    // пока игра в фокусе — и обратно после
    y = title(ui, b.x, y, b.w, "пока играешь");
    ui.toggle(R::new(l, y, half, 40), "не беспокоить", "уведомления — молча в историю", s.game_dnd, Act::SBool("game_dnd"));
    let ph = if st.prof.present { "потом — как было" } else { "нет power-profiles-daemon" };
    ui.toggle(R::new(r, y, half, 40), "профиль «производительность»", ph, s.game_profile, Act::SBool("game_profile"));
    y += 46;
    let gm = if p.gamemode { "игра в фокусе — сама" } else { "нет gamemoded" };
    ui.toggle(R::new(l, y, half, 40), "GameMode", gm, s.game_gamemode, Act::SBool("game_gamemode"));
    y += 52;
    // переменные окружения: действуют со следующего входа
    y = title(ui, b.x, y, b.w, "Proton и драйвер (со следующего входа)");
    let nt = if p.ntsync { "PROTON_USE_NTSYNC=1" } else { "нет /dev/ntsync" };
    ui.toggle(R::new(l, y, half, 40), "ntsync для Proton", nt, s.game_ntsync, Act::SBool("game_ntsync"));
    ui.toggle(R::new(r, y, half, 40), "большой кеш шейдеров", "меньше фризов в новых сценах", s.game_cache, Act::SBool("game_cache"));
}
