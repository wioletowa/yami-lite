// Приватность окон: приложение прячется из трансляции экрана — в демонстрации на его месте чёрное.
// Делает это сам niri: правило окна `block-out-from "screencast"` по app-id. Шелл ведёт список
// приложений (настройка hideApps) и пишет правила в ~/.config/niri/yami-lite-hidden.kdl, который
// подключён к config.kdl строкой include; niri перечитывает конфиг. Выбрать окно — бинд
// `yami-lite msg hide` или ЛКМ по замку в баре: прицел niri (pick-window), щелчок по окну прячет его
// приложение, по уже спрятанному — возвращает. ПКМ по замку — список спрятанных.

use crate::app::{App, Msg, SKind, State};
use crate::font::PX;
use crate::gfx::R;
use crate::ui::{Act, Tone, Ui};

const FILE: &str = "yami-lite-hidden.kdl";

fn niri_dir() -> std::path::PathBuf {
    yami_px::paths::config_dir().join("niri")
}

/// строка в регулярном выражении niri: служебные символы — с обратной чертой, в KDL — кавычки тоже
fn regex_exact(s: &str) -> String {
    let mut out = String::from("^");
    for c in s.chars() {
        if ".^$*+?()[]{}|\\/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('$');
    out.replace('\\', "\\\\").replace('"', "\\\"")
}

/// файл правил: по правилу на приложение
pub fn rules(apps: &[String]) -> String {
    let mut t = String::from("// yami-lite: windows hidden from screencasts (shell → bar → lock); rewritten by the shell\n");
    for a in apps {
        t.push_str(&format!("window-rule {{\n    match app-id=\"{}\"\n    block-out-from \"screencast\"\n}}\n", regex_exact(a)));
    }
    t
}

/// записать правила, подключить файл к config.kdl (один раз) и попросить niri перечитать
fn apply(apps: Vec<String>) {
    std::thread::spawn(move || {
        let dir = niri_dir();
        let conf = dir.join("config.kdl");
        if !conf.exists() {
            return;
        }
        let _ = yami_px::paths::write_atomic(&dir.join(FILE), &rules(&apps));
        let line = format!("include \"{FILE}\"");
        let t = std::fs::read_to_string(&conf).unwrap_or_default();
        if !t.lines().any(|l| l.trim() == line) {
            let sep = if t.is_empty() || t.ends_with('\n') { "" } else { "\n" };
            let _ = yami_px::paths::write_atomic(&conf, &format!("{t}{sep}\n// yami-lite: windows hidden from screencasts\n{line}\n"));
        }
        let _ = std::process::Command::new("niri").args(["msg", "action", "load-config-file"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    });
}

impl App {
    /// прицел niri: щелчок по окну — его app-id (Esc — ничего)
    pub fn hide_pick(&mut self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let o = std::process::Command::new("niri").args(["msg", "--json", "pick-window"]).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output();
            let app = o.ok().and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok()).and_then(|v| v["app_id"].as_str().map(String::from)).filter(|a| !a.is_empty());
            if let Some(a) = app {
                let _ = tx.send(Msg::HidePicked(a));
            }
        });
    }

    /// выбрали окно: спрятать его приложение или, если уже спрятано, вернуть
    pub fn hide_toggle(&mut self, app: String) {
        let list = &mut self.st.set.hide_apps;
        let hidden = match list.iter().position(|a| *a == app) {
            Some(i) => {
                list.remove(i);
                false
            }
            None => {
                list.push(app.clone());
                true
            }
        };
        let name = app_label(&self.st.apps, &app);
        let msg = if hidden { crate::tf!("{} — скрыто из трансляции", "{} — hidden from screencasts", name) } else { crate::tf!("{} — снова видно в трансляции", "{} — visible in screencasts again", name) };
        self.toast_note(crate::i18n::tr("приватность"), &msg);
        self.hide_sync();
    }

    pub fn hide_show(&mut self, app: &str) {
        self.st.set.hide_apps.retain(|a| a != app);
        self.hide_sync();
    }

    fn hide_sync(&mut self) {
        apply(self.st.set.hide_apps.clone());
        self.settings_changed();
        self.resize_popup();
        self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Popup));
    }

    /// при запуске: файл правил — по настройке (её могли поменять, пока шелла не было)
    pub fn hide_init(&self) {
        let want = rules(&self.st.set.hide_apps);
        if std::fs::read_to_string(niri_dir().join(FILE)).ok().as_deref() != Some(want.as_str()) && (!self.st.set.hide_apps.is_empty() || niri_dir().join(FILE).exists()) {
            apply(self.st.set.hide_apps.clone());
        }
    }
}

/// имя программы по app-id: из её .desktop (по имени файла или StartupWMClass), иначе сам app-id
fn app_label(apps: &[yami_px::icons::App], id: &str) -> String {
    let low = id.to_lowercase();
    apps.iter().find(|a| a.id.to_lowercase() == low || a.wm_class.to_lowercase() == low || a.id.to_lowercase().ends_with(&format!(".{low}"))).map(|a| a.name.clone()).unwrap_or_else(|| id.to_string())
}

/// всплывашка «скрытые окна»: значок, имя и крестик (вернуть в трансляцию)
pub fn size(st: &State) -> (i32, i32) {
    (300, 44 + st.set.hide_apps.len().max(1) as i32 * 32 + 40)
}

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    ui.cv.icon("lock", b.x, b.y + 3, 2, t.red);
    ui.text_in(PX, R::new(b.x + 22, b.y, b.w - 22, 20), "скрыто из трансляции", t.text);
    ui.cv.fill(R::new(b.x, b.y + 26, b.w, 1), t.border);
    let mut y = b.y + 36;
    if st.set.hide_apps.is_empty() {
        ui.text_in(PX, R::new(b.x, y, b.w, 24), "ничего не скрыто", t.text_faint);
    }
    for a in &st.set.hide_apps {
        let icon = yami_px::icons::icon_for(&st.apps, a);
        ui.app_icon(&icon, b.x, y + 4, 16, 1.0);
        ui.text_in(PX, R::new(b.x + 24, y, b.w - 56, 28), &app_label(&st.apps, a), t.text);
        // крестик — вернуть приложение в трансляцию
        ui.ghost(R::new(b.right() - 24, y + 4, 22, 20), "close", Act::HideShow(a.clone()));
        y += 32;
    }
    ui.button(R::new(b.x, b.bottom() - 28, b.w, 28), "скрыть окно…", "lock", Tone::Plain, false, true, Act::HidePick);
}

#[cfg(test)]
mod tests {
    #[test]
    fn rules_escape_app_ids() {
        let t = super::rules(&["org.telegram.desktop".into()]);
        assert!(t.contains(r#"match app-id="^org\\.telegram\\.desktop$""#), "{t}");
        assert!(t.contains("block-out-from \"screencast\""));
        assert_eq!(super::rules(&[]).lines().count(), 1);
    }
}
