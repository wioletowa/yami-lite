// Вкладка «шрифты»: свой шрифт шеллу и отдельно файлам и просмотрщику, программам (GTK, Qt),
// терминалу; браузеру — только меню ПКМ в палитре и, по желанию, пиксельный шрифт.
// Список — из установленных (fc-list) среди известных; любое другое семейство:
// yami-shell config set fontShell "Имя".

use crate::app::{App, State};
use crate::gfx::R;
use crate::settings::{opt_btn, title};
use crate::ui::{Act, Ui};
use std::sync::OnceLock;

const SANS: &[&str] = &["Inter", "Noto Sans", "DejaVu Sans", "Cantarell", "Roboto", "Ubuntu", "IBM Plex Sans", "Fira Sans", "Source Sans 3", "Liberation Sans", "Open Sans", "Lato"];
const PIXEL: &[&str] = &["Monocraft", "Terminess Nerd Font Mono", "DotGothic16", "Silkscreen", "Cozette"];
const MONO: &[&str] = &["JetBrains Mono", "Fira Code", "DejaVu Sans Mono", "Noto Sans Mono", "Hack", "Iosevka", "Cascadia Code", "Source Code Pro", "Liberation Mono"];

/// установленные семейства (один раз за жизнь шелла)
fn installed() -> &'static Vec<String> {
    static L: OnceLock<Vec<String>> = OnceLock::new();
    L.get_or_init(|| {
        let out = std::process::Command::new("fc-list").args([":", "family"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let mut v: Vec<String> = out.lines().flat_map(|l| l.split(',')).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        v.sort();
        v.dedup();
        v
    })
}

/// варианты для поля: "" (по умолчанию), особые значения, известные установленные, текущее
fn options(key: &str, cur: &str) -> Vec<String> {
    let have = installed();
    let pick = |xs: &[&str]| xs.iter().filter(|x| have.iter().any(|h| h.eq_ignore_ascii_case(x))).map(|x| x.to_string()).collect::<Vec<_>>();
    let mut v: Vec<String> = vec![String::new()];
    match key {
        "font_ui" => {
            v.push("sans".into());
            v.extend(pick(SANS));
            v.extend(pick(PIXEL));
        }
        "font_mono" => {
            v.extend(pick(PIXEL).into_iter().filter(|x| x != "DotGothic16"));
            v.extend(pick(MONO));
        }
        _ => {
            v.extend(pick(PIXEL));
            v.extend(pick(SANS));
        }
    }
    if !cur.is_empty() && !v.iter().any(|x| x == cur) {
        v.push(cur.to_string());
    }
    v.dedup();
    v
}

/// подпись значения
fn shown(key: &str, v: &str) -> String {
    if !v.is_empty() {
        return if v == "sans" { crate::i18n::tr("системный").to_string() } else { v.to_string() };
    }
    crate::i18n::tr(match key {
        "font_files" | "font_view" => "как у шелла",
        "font_mono" => "Monocraft (пиксельный)",
        _ => "Tiny5 (пиксельный)",
    })
    .to_string()
}

fn field<'a>(set: &'a mut crate::config::Settings, key: &str) -> Option<&'a mut String> {
    Some(match key {
        "font_shell" => &mut set.font_shell,
        "font_files" => &mut set.font_files,
        "font_view" => &mut set.font_view,
        "font_ui" => &mut set.font_ui,
        "font_mono" => &mut set.font_mono,
        _ => return None,
    })
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, b.w, "шелл и приложения yami");
    for (key, label, cur) in [("font_shell", "шелл", &s.font_shell), ("font_files", "файлы", &s.font_files), ("font_view", "просмотр картинок", &s.font_view)] {
        opt_btn(ui, R::new(b.x, y, b.w, 26), label, &shown(key, cur), Act::SCycle(key));
        y += 32;
    }
    y += 8;
    y = title(ui, b.x, y, b.w, "программы");
    opt_btn(ui, R::new(b.x, y, b.w, 26), "интерфейс (GTK, Qt)", &shown("font_ui", &s.font_ui), Act::SCycle("font_ui"));
    y += 32;
    let pixel_ui = s.font_ui.is_empty() || ["Monocraft", "Terminess", "DotGothic", "Silkscreen", "Cozette"].iter().any(|p| s.font_ui.contains(p));
    let size = if pixel_ui {
        if s.font_size == 0 { crate::i18n::tr("авто").to_string() } else { format!("{} px", s.font_size) }
    } else if s.font_pt == 0 {
        "11 pt".to_string()
    } else {
        format!("{} pt", s.font_pt)
    };
    opt_btn(ui, R::new(b.x, y, b.w, 26), "размер", &size, Act::SCycle(if pixel_ui { "font_size" } else { "font_pt" }));
    y += 40;
    y = title(ui, b.x, y, b.w, "терминал");
    opt_btn(ui, R::new(b.x, y, b.w, 26), "шрифт", &shown("font_mono", &s.font_mono), Act::SCycle("font_mono"));
    y += 32;
    ui.toggle(R::new(b.x, y, b.w, 40), "расширенный fastfetch", "ещё ядро, пакеты, процессор, видеокарта, память, диск, экран", s.fetch_full, Act::SBool("fetch_full"));
    y += 48;
    y = title(ui, b.x, y, b.w, "браузер (Firefox, LibreWolf, Floorp, Zen)");
    ui.toggle(R::new(b.x, y, half, 40), "меню ПКМ в палитре", "тема браузера — твоя, не трогаем", s.browser_menus, Act::SBool("browser_menus"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "пиксельный шрифт", "в меню и интерфейсе, не на сайтах", s.browser_font, Act::SBool("browser_font"));
    y += 48;
    ui.wrap(crate::font::PX, R::new(b.x, y, b.w, 40), "браузер подхватит при следующем запуске. Любой другой шрифт: yami-shell config set fontShell \"Имя\"", t.text_faint, 2);
}

impl App {
    /// SCycle по шрифтовым полям; true — обработано
    pub fn font_cycle(&mut self, key: &str) -> bool {
        let s = &mut self.st.set;
        match key {
            "font_pt" => {
                s.font_pt = match s.font_pt {
                    0 => 12,
                    12 => 13,
                    13 => 14,
                    14 => 10,
                    _ => 0,
                };
            }
            _ => {
                let Some(cur) = field(s, key) else { return false };
                let opts = options(key, cur);
                let i = opts.iter().position(|x| x == cur).unwrap_or(0);
                *cur = opts[(i + 1) % opts.len()].clone();
            }
        }
        self.settings_changed();
        if matches!(key, "font_ui" | "font_mono" | "font_pt") {
            self.apply_fonts();
        }
        true
    }
}
