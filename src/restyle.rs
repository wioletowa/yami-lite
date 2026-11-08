// Итог перекраски программ и своих шаблонов (#27). tools/theme.py пишет его в
// ~/.local/state/yami/restyle.json: шаблон записан, изменён руками (не перезаписан), с опечаткой в
// {{цвете}}, без строки yami-output, с путём вне разрешённых папок; fastfetch со своим config.jsonc,
// starship со своей палитрой. Шелл показывает это в настройках → цвет → программы, yami templates —
// в терминале. Папку шаблонов шелл сверяет раз в 5 с: изменилась — theme.py --templates.

use crate::app::*;
use crate::font::PX;
use crate::gfx::R;
use crate::ui::{Act, Tone, Ui};

#[derive(Clone, Default)]
pub struct Tpl {
    pub file: String,
    pub output: String,
    pub state: String,
    pub keys: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Status {
    /// fastfetch: свой config.jsonc не тронут, наш вид — в этом файле
    pub fetch: Option<String>,
    /// starship: выбрана своя палитра (не yami)
    pub starship: Option<String>,
    pub tpls: Vec<Tpl>,
}

pub fn load() -> Status {
    let p = crate::config::state_dir().join("restyle.json");
    let Ok(t) = std::fs::read_to_string(p) else { return Status::default() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) else { return Status::default() };
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").chars().take(200).collect::<String>();
    let keys = |v: &serde_json::Value| v.as_array().map(|k| k.iter().filter_map(|x| x.as_str().map(|x| x.chars().take(40).collect())).take(8).collect()).unwrap_or_default();
    Status {
        fetch: v["apps"]["fastfetch"]["file"].as_str().map(String::from),
        starship: v["apps"]["starship"]["palette"].as_str().map(String::from),
        tpls: v["templates"].as_array().map(|a| a.iter().take(64).map(|r| Tpl { file: s(&r["file"]), output: s(&r["output"]), state: s(&r["state"]), keys: keys(&r["keys"]) }).collect()).unwrap_or_default(),
    }
}

/// слепок папки шаблонов: имена, размеры, время правки — меняется от любой правки, нового или
/// удалённого файла
pub fn stamp() -> u64 {
    use std::hash::{Hash, Hasher};
    let mut v: Vec<(String, u64, u128)> = std::fs::read_dir(crate::config::config_dir().join("yami/templates"))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let m = e.metadata().ok()?;
                    let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
                    Some((e.file_name().to_string_lossy().into_owned(), m.len(), t))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// что с шаблоном, словами
pub fn note(t: &Tpl) -> String {
    match t.state.as_str() {
        "ok" => crate::tf!("записан: {}", "written: {}", "gravado: {}", t.output),
        "hand" => crate::tf!("изменён руками — не перезаписан", "changed by hand: not overwritten", "alterado à mão: não sobrescrito"),
        "unknown" => crate::tf!("нет таких цветов: {} — файл не записан", "no such colours: {}; the file is not written", "cores inexistentes: {}; o arquivo não foi gravado", t.keys.join(", ")),
        "nooutput" => crate::tf!("нет строки «yami-output: ~/путь» — некуда писать", "no “yami-output: ~/path” line: nowhere to write", "sem a linha “yami-output: ~/caminho”: não há onde gravar"),
        "outside" => crate::tf!("{} вне ~/.config, ~/.local/share и ~/.cache — не пишется", "{} is outside ~/.config, ~/.local/share and ~/.cache: not written", "{} fica fora de ~/.config, ~/.local/share e ~/.cache: não gravado", t.output),
        _ => crate::tf!("не читается (не текст?)", "unreadable (not text?)", "ilegível (não é texto?)"),
    }
}

/// заметки к программам: свой конфиг fastfetch, своя палитра starship
pub fn app_notes(s: &Status) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(f) = &s.fetch {
        v.push(crate::tf!("fastfetch: твой config.jsonc не тронут, вид yami — в {} (fastfetch -c {})", "fastfetch: your config.jsonc is kept, yami's look is in {} (fastfetch -c {})", "fastfetch: seu config.jsonc foi mantido, o visual do yami está em {} (fastfetch -c {})", f, f));
    }
    if let Some(p) = &s.starship {
        v.push(crate::tf!("starship: выбрана твоя палитра «{}»; цвета yami рядом — palette = \"yami\"", "starship: your palette “{}” stays chosen; yami's colours are next to it: palette = \"yami\"", "starship: sua paleta “{}” continua escolhida; as cores do yami estão ao lado: palette = \"yami\"", p));
    }
    v
}

/// строки шаблонов под кнопками вкладки: имя, итог, «перезаписать» у изменённого руками
pub fn draw(ui: &mut Ui, st: &State, b: R, mut y: i32) -> i32 {
    let t = ui.t;
    for (i, tp) in st.restyle.tpls.iter().enumerate() {
        let r = R::new(b.x, y, b.w, 24);
        ui.cv.fill(R::new(r.x, r.cy() - 2, 4, 4), if tp.state == "ok" { t.ok } else { t.warn });
        let nw = ui.text(PX, r.x + 12, r.y + 6, &tp.file, t.text);
        let bw = if tp.state == "hand" { 150 } else { 0 };
        let nx = r.x + 12 + nw.max(120) + 12;
        ui.text_in(PX, R::new(nx, r.y, r.right() - nx - bw - 8, r.h), &note(tp), if tp.state == "ok" { t.text_faint } else { t.text_dim });
        if bw > 0 {
            ui.button(R::new(r.right() - bw, r.y, bw, 24), "перезаписать", "reboot", Tone::Plain, false, true, Act::STplForce(i));
        }
        y += 28;
    }
    y
}

impl App {
    /// свои шаблоны заново (force — имя изменённого руками: перезаписать, прежний — *.bak-yami)
    pub fn templates_run(&mut self, force: Option<String>) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut c = crate::proc::theme_py();
            c.arg("--templates");
            if let Some(f) = force {
                c.arg("--force").arg(f);
            }
            let _ = c.status();
            let _ = tx.send(Msg::Restyled);
        });
    }

    /// раз в 5 с: папку шаблонов поменяли (новый файл, правка) — отрисовать их заново, не дожидаясь смены палитры
    pub fn templates_tick(&mut self) {
        let s = stamp();
        match self.st.tpl_stamp {
            Some(old) if old != s => {
                self.st.tpl_stamp = Some(s);
                self.templates_run(None);
            }
            None => self.st.tpl_stamp = Some(s),
            _ => {}
        }
    }
}

/// yami templates [force ИМЯ]: отрисовать свои шаблоны и сказать, что вышло
pub fn cli(y: &crate::cmd::Sty, args: &[&str]) -> i32 {
    let mut c = crate::proc::theme_py();
    c.arg("--templates");
    match args {
        [] => {}
        ["force", name] => {
            c.arg("--force").arg(name);
        }
        _ => {
            eprintln!(" {} yami templates [force {}]", y.bad("✕"), crate::tf!("ИМЯ", "NAME", "NOME"));
            return 2;
        }
    }
    let _ = c.status();
    let s = load();
    y.section(&crate::tf!("свои шаблоны", "your templates", "seus modelos"));
    if s.tpls.is_empty() {
        println!("   {}", y.dim(&crate::tf!("нет: файлы кладутся в ~/.config/yami/templates (пример — example.md)", "none: files go into ~/.config/yami/templates (see example.md)", "nenhum: os arquivos ficam em ~/.config/yami/templates (veja example.md)")));
    }
    let mut bad = 0;
    for t in &s.tpls {
        let mark = if t.state == "ok" { y.ok("✓") } else { bad += 1; y.warn("!") };
        println!(" {mark} {}  {}", t.file, y.dim(&note(t)));
    }
    for n in app_notes(&s) {
        println!("   {}", y.dim(&n));
    }
    println!();
    i32::from(bad > 0)
}
