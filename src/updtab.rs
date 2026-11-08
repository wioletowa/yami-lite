// Настройки → обновления: что установлено, канал и проверки, консоль (хвост update.log с
// прогрессом [k/n]), заметки к выпуску и выбор версии.

use crate::update::*;
use crate::app::State;
use crate::gfx::{BoxStyle, R};
use crate::settings::{opt_btn, title};
use crate::ui::{Act, Tone, Ui};

pub(crate) fn ago(at: u64) -> String {
    let d = now().saturating_sub(at);
    if d < 90 {
        crate::tf!("только что", "just now", "agora mesmo")
    } else if d < 5400 {
        crate::tf!("{} мин назад", "{} min ago", "há {} min", d / 60)
    } else if d < 172_800 {
        crate::tf!("{} ч назад", "{} h ago", "há {} h", d / 3600)
    } else {
        crate::tf!("{} дн назад", "{} days ago", "há {} dias", d / 86400)
    }
}

/// github.com/user/repo из адреса
pub fn short_repo(u: &str) -> String {
    let u = if u.is_empty() { REPO } else { u };
    u.trim_start_matches("https://").trim_start_matches("git@").replace(':', "/").trim_end_matches(".git").to_string()
}

/// мини-консоль: хвост update.log, шаги акцентом, готовое зелёным, ошибки красным; пока идёт
/// установка — мигающий блок-курсор в конце
pub(crate) fn console(ui: &mut Ui, r: R, log: &[String], busy: bool) {
    let t = ui.t;
    let px = crate::font::PX;
    ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
    let head = R::new(r.x + 10, r.y + 4, r.w - 20, 18);
    ui.cv.fill(R::new(head.x, head.y + 6, 6, 6), if busy { t.red } else { t.text_faint });
    let name = log_path().display().to_string().replace(&std::env::var("HOME").unwrap_or_default(), "~");
    ui.cv.text_in(ui.f, px, R::new(head.x + 12, head.y, head.w - 12, 18), &name, t.text_faint);
    ui.cv.fill(R::new(r.x + 1, r.y + 24, r.w - 2, 1), t.border);
    let rows = ((r.h - 34) / 18).max(1) as usize;
    let skip = log.len().saturating_sub(rows);
    let mut y = r.y + 28;
    let n = log.len() - skip;
    for (i, l) in log[skip..].iter().enumerate() {
        let (c, mark) = if l.starts_with('✓') {
            (t.ok, true)
        } else if l.starts_with('✕') || l.starts_with('✗') || l.to_lowercase().contains("error") {
            (t.warn, true)
        } else if l.starts_with('▪') || l.starts_with('▸') {
            (t.red, true)
        } else {
            (t.text_dim, false)
        };
        let x = r.x + 10 + if mark { 0 } else { 12 };
        let line = ui.f.elide(px, l, r.right() - 10 - x - 14);
        let w = ui.cv.text_in(ui.f, px, R::new(x, y, r.right() - 10 - x, 18), &line, c);
        if busy && i + 1 == n && ui.blink {
            ui.cv.fill(R::new(x + w + 4, y + 3, 7, 12), t.ember);
        }
        y += 18;
    }
}

/// выбор версии: прошлая сборка (сразу) и подписанные выпуски; ставится только кнопкой «поставить»
pub(crate) fn picker(ui: &mut Ui, st: &State, r: R) {
    let t = ui.t;
    let u = &st.upd;
    let px = crate::font::PX;
    let cur = u.stamp.version.clone();
    let mut y = title(ui, r.x, r.y, r.w, "на какую версию перейти");
    let foot = r.bottom() - 28;
    let mut rows: Vec<(String, String, String, bool)> = vec![];
    if let Some(p) = prev_version() {
        rows.push(("prev".into(), crate::tf!("прошлая сборка  {}", "previous build  {}", "compilação anterior  {}", p), crate::tf!("сразу, без сборки", "instant, no build", "instantâneo, sem compilar"), true));
    }
    if let Some(i) = &u.info {
        for rel in &i.releases {
            let ok = Info { sig: rel.sig.clone(), ..Default::default() }.sig_ok(&st.set.update_verify);
            let here = rel.version == cur;
            let note = if here {
                crate::tf!("стоит сейчас", "installed now", "instalado agora")
            } else if !ok {
                crate::tf!("{} — не ставится", "{}: will not install", "{}: não será instalado", sig_text(&rel.sig))
            } else {
                format!("{}  ·  {}", sig_text(&rel.sig), date_of(rel.date))
            };
            rows.push((rel.version.clone(), rel.tag.clone(), note, ok && !here));
        }
    }
    if rows.is_empty() {
        let msg = if u.checking { crate::tf!("загружаю список выпусков…", "loading the releases…", "carregando as versões…") } else { crate::tf!("переходить некуда: выпусков и прошлой сборки нет", "nothing to switch to: no releases and no previous build", "não há outra versão: sem versões publicadas ou compilação anterior") };
        ui.text_in(px, R::new(r.x, y, r.w, 20), &msg, t.text_dim);
    }
    for (id, name, note, can) in &rows {
        if y + 26 > foot - 6 {
            break;
        }
        let rr = R::new(r.x, y, r.w, 24);
        ui.button(rr, "", "", Tone::Ghost, u.sel == *id, *can, Act::UpdSel(id.clone()));
        let c = if !can { t.text_faint.fade(0.6) } else { t.text };
        ui.cv.fill(R::new(rr.x + 8, rr.cy() - 3, 6, 6), if u.sel == *id { t.red } else if *can { t.border } else { t.surface });
        ui.cv.text_in(ui.f, px, R::new(rr.x + 22, rr.y, 170, rr.h), name, c);
        let note = ui.f.elide(px, note, rr.w - 200);
        ui.cv.text_in(ui.f, px, R::new(rr.x + 196, rr.y, rr.w - 204, rr.h), &note, if *can { t.text_dim } else { t.text_faint.fade(0.6) });
        y += 28;
    }
    let half = (r.w - 6) / 2;
    let go = if u.sel.is_empty() {
        crate::tf!("выбери версию", "pick a version", "escolher versão")
    } else if u.sel == "prev" {
        crate::tf!("вернуть прошлую сборку", "restore the previous build", "restaurar a compilação anterior")
    } else {
        crate::tf!("поставить {}", "install {}", "instalar {}", u.sel)
    };
    ui.button(R::new(r.x, foot, half, 28), &go, "check", Tone::Primary, false, !u.sel.is_empty(), Act::SCycle("update_pick_go"));
    ui.button(R::new(r.x + half + 6, foot, half, 28), "отмена", "close", Tone::Plain, false, true, Act::SCycle("update_pick_cancel"));
}

/// дата тега: «3 ноя 2026»
pub(crate) fn date_of(at: u64) -> String {
    if at == 0 {
        return String::new();
    }
    let days = (at / 86400) as i64;
    let (y, m, d) = civil(days);
    const RU: [&str; 12] = ["янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"]; // i18n-ok
    const EN: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    const PT: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
    let mon = if crate::i18n::pt_br() { PT[(m - 1) as usize] } else if crate::i18n::en() { EN[(m - 1) as usize] } else { RU[(m - 1) as usize] };
    format!("{d} {mon} {y}")
}

/// дни от 1970-01-01 → (год, месяц, день) (Howard Hinnant)
pub(crate) fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (era * 400 + yoe + i64::from(m <= 2), m, d)
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let u = &st.upd;
    let half = (b.w - 20) / 2;
    let px = crate::font::PX;
    let mut y = title(ui, b.x, b.y, b.w, "установлено");
    let ver = if u.stamp.version.is_empty() { env!("CARGO_PKG_VERSION").to_string() } else { u.stamp.version.clone() };
    let commit: String = u.stamp.commit.chars().take(8).collect();
    let mut cur = ver;
    if !commit.is_empty() {
        cur += &format!("  ·  {commit}");
    }
    if u.stamp.dirty {
        cur += &crate::tf!("  ·  с правками", "  ·  with local edits", "  ·  com alterações locais");
    }
    for (k, v) in [("версия", cur), ("источник", short_repo(if s.update_repo.is_empty() { &u.stamp.repo } else { &s.update_repo }))] {
        ui.text_in(px, R::new(b.x, y, 160, 22), k, t.text_faint);
        ui.text_in(px, R::new(b.x + 170, y, b.w - 170, 22), &v, t.text);
        y += 24;
    }
    y += 10;
    y = title(ui, b.x, y, b.w, "обновления");
    if !ENABLED {
        ui.text_in(px, R::new(b.x, y, b.w, 22), &managed_message(), t.text_dim);
        return;
    }
    let chan = if s.update_channel == "main" { "каждый коммит" } else { "выпуски" };
    opt_btn(ui, R::new(b.x, y, half, 26), "канал", chan, Act::SCycle("update_channel"));
    let ver_l = match s.update_verify.as_str() {
        "require" => "всегда",
        "off" => "выкл",
        _ => "по ключам",
    };
    opt_btn(ui, R::new(b.x + half + 20, y, half, 26), "проверка подписей", ver_l, Act::SCycle("update_verify"));
    y += 34;
    let every = crate::tf!("раз в {} ч", "every {} h", "a cada {} h", s.update_every);
    ui.toggle(R::new(b.x, y, half, 40), "проверять сама", "только проверка: ставишь ты", s.update_check, Act::SBool("update_check"));
    opt_btn(ui, R::new(b.x + half + 20, y + 7, half, 26), "как часто", &every, Act::SCycle("update_every"));
    y += 50;
    // состояние: идёт установка / проверка / итог
    let (line, col) = if let Some(p) = &u.busy {
        (crate::tf!("ставлю: {}", "installing: {}", "instalando: {}", step_of(p).1), t.red)
    } else if let Some((ok, txt)) = &u.done {
        (txt.clone(), if *ok { t.ok } else { t.warn })
    } else if u.checking {
        (crate::tf!("проверяю…", "checking…", "verificando…"), t.text_dim)
    } else if let Some(i) = &u.info {
        if !i.err.is_empty() {
            (crate::tf!("не вышло проверить: {}", "check failed: {}", "falha na verificação: {}", i.err), t.warn)
        } else if i.available {
            (crate::tf!("доступно {}  ·  {}", "{} is available  ·  {}", "{} está disponível  ·  {}", i.latest, sig_text(&i.sig)), if i.sig_ok(&s.update_verify) { t.ok } else { t.warn })
        } else if i.reference.is_empty() {
            (crate::tf!("выпусков ещё нет  ·  проверено {}", "no releases yet  ·  checked {}", "ainda sem versões publicadas  ·  verificado {}", ago(i.at)), t.text_dim)
        } else {
            (crate::tf!("всё свежее  ·  проверено {}", "up to date  ·  checked {}", "atualizado  ·  verificado {}", ago(i.at)), t.text_dim)
        }
    } else {
        (crate::tf!("ещё не проверялось", "not checked yet", "ainda não verificado"), t.text_dim)
    };
    ui.cv.fill(R::new(b.x, y + 7, 6, 6), col);
    ui.text_in(px, R::new(b.x + 14, y, b.w - 14, 20), &line, col);
    y += 24;
    // новым функциям не хватает пакетов или экран входа не поставлен: обновление без sudo их не
    // ставит — одна кнопка (терминал, sudo спросит там)
    if let Some(n) = &u.need {
        ui.button(R::new(b.right() - 130, y, 130, 26), "поставить", "down", Tone::Primary, false, u.busy.is_none(), Act::SCycle("need_run"));
        for (i, txt) in n.items().iter().enumerate() {
            let txt = ui.f.elide(px, txt, b.w - 150);
            let ty = y + 3 + i as i32 * 22;
            ui.cv.fill(R::new(b.x, ty + 7, 6, 6), t.warn);
            ui.cv.text_in(ui.f, px, R::new(b.x + 14, ty, b.w - 150, 20), &txt, t.warn);
        }
        y += 34 + (n.items().len() as i32 - 1).max(0) * 22;
    }
    let by = b.bottom() - 62;
    // прогресс по шагам установщика («[k/n]»); до них — первые деления
    if let Some(p) = &u.busy {
        let v = match step_of(p).0 {
            Some((k, n)) => (k.saturating_sub(1) as f32 + 0.5) / n as f32,
            None => 0.04,
        };
        ui.cv.gauge(R::new(b.x, y, b.w, 8), 40, v.clamp(0.0, 1.0), t.red, t.surface.lerp(t.border, 0.3), 2);
        y += 16;
    }
    let notes = u.info.as_ref().filter(|i| i.available && i.err.is_empty() && u.busy.is_none() && u.done.is_none());
    if u.pick && u.busy.is_none() {
        picker(ui, st, R::new(b.x, y, b.w, by - y - 8));
    } else if let Some(i) = notes {
        // что нового: строки CHANGELOG выпуска на языке интерфейса (CHANGELOG.ru.md), мимо tr()
        y = title(ui, b.x, y, b.w, &crate::tf!("что нового в {}", "what is new in {}", "novidades em {}", i.latest));
        for n in i.notes() {
            if y + 18 > by - 8 {
                break;
            }
            ui.cv.fill(R::new(b.x + 2, y + 7, 4, 4), t.red);
            let line = ui.f.elide(px, n, b.w - 14);
            ui.cv.text_in(ui.f, px, R::new(b.x + 14, y, b.w - 14, 18), &line, t.text_dim);
            y += 18;
        }
    } else if !u.log.is_empty() && by - y > 60 {
        console(ui, R::new(b.x, y + 2, b.w, by - y - 12), &u.log, u.busy.is_some());
    }
    let bw = (b.w - 12) / 3;
    let busy = u.busy.is_some() || u.checking;
    let can = u.info.as_ref().map(|i| i.available && i.err.is_empty() && i.sig_ok(&s.update_verify)).unwrap_or(false);
    ui.button(R::new(b.x, by, bw, 28), "проверить", "search", Tone::Plain, false, !busy, Act::SCycle("update_now"));
    ui.button(R::new(b.x + bw + 6, by, bw, 28), "обновить", "down", Tone::Primary, false, can && !busy, Act::SCycle("update_apply"));
    let any = u.rollback || u.info.as_ref().map(|i| !i.releases.is_empty()).unwrap_or(true);
    ui.button(R::new(b.x + 2 * (bw + 6), by, bw, 28), "откатить", "redo", Tone::Plain, u.pick, any && !busy, Act::SCycle("update_rollback"));
    ui.text_in(px, R::new(b.x, by + 36, b.w, 20), "или в терминале: yami update", t.text_faint);
}
