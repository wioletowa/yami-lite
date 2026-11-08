// Шрифт шелла: Tiny5 (вшит), пиксельные и обычные из установленных (fc-list) среди известных;
// любое другое семейство — yami-lite config set fontShell "Имя".

use crate::app::{App, State};
use crate::gfx::R;
use crate::settings::opt_btn;
use crate::ui::{Act, Ui};
use std::sync::OnceLock;

const SANS: &[&str] = &["Inter", "Noto Sans", "DejaVu Sans", "Cantarell", "Roboto", "Ubuntu", "IBM Plex Sans", "Fira Sans", "Source Sans 3", "Liberation Sans", "Open Sans", "Lato"];
const PIXEL: &[&str] = &["Monocraft", "Terminess Nerd Font Mono", "DotGothic16", "Silkscreen", "Cozette"];

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

/// варианты: "" (Tiny5), известные установленные, текущее
fn options(cur: &str) -> Vec<String> {
    let have = installed();
    let mut v: Vec<String> = vec![String::new()];
    v.extend(PIXEL.iter().chain(SANS).filter(|x| have.iter().any(|h| h.eq_ignore_ascii_case(x))).map(|x| x.to_string()));
    if !cur.is_empty() && !v.iter().any(|x| x == cur) {
        v.push(cur.to_string());
    }
    v
}

/// строка выбора шрифта во вкладке «вид»
pub fn row(ui: &mut Ui, st: &State, r: R) {
    let cur = &st.set.font_shell;
    let shown = if cur.is_empty() { crate::i18n::tr("Tiny5 (пиксельный)").to_string() } else { cur.clone() };
    opt_btn(ui, r, "шрифт шелла", &shown, Act::SCycle("font_shell"));
}

impl App {
    pub fn font_cycle(&mut self) {
        let opts = options(&self.st.set.font_shell);
        let i = opts.iter().position(|x| *x == self.st.set.font_shell).unwrap_or(0);
        self.st.set.font_shell = opts[(i + 1) % opts.len()].clone();
        self.settings_changed();
    }
}
