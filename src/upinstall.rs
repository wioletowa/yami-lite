// Установка и откат обновлений: процесс `yami update apply | to | rollback` (отдельно от шелла),
// его замок, прогресс в update.log, копии прежних бинарей, снимок настроек перед переходом на
// более старый выпуск, возвращение шелла после.

use crate::update::*;
use crate::config::Settings;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── установка и откат (процесс `yami update apply`, отдельно от шелла) ─────
/// замок: две установки сразу не пойдут
/// файл держится открытым, пока жив замок: закрытие снимает flock
pub struct Lock(#[allow(dead_code)] std::fs::File);
pub fn lock() -> Option<Lock> {
    use std::os::unix::io::AsRawFd;
    let p = yami_px::paths::cache_dir().join("yami/update.lock");
    let _ = std::fs::create_dir_all(p.parent()?);
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(p).ok()?;
    // SAFETY: flock на своём открытом файле; снимается при закрытии
    (unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0).then_some(Lock(f))
}

/// шаг установки: в терминал и в шелл (вкладка показывает, где мы)
pub(crate) fn progress(s: &str) {
    println!("  ▪ {s}");
    let _ = crate::ipc::send(&format!("update-progress {s}"));
}

/// последние строки update.log (без цветов и пустых) и размер файла; читается только хвост
pub fn log_tail() -> (Vec<String>, u64) {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(log_path()) else { return (vec![], 0) };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(16 * 1024);
    let mut buf = vec![];
    if f.seek(SeekFrom::Start(from)).is_err() || f.take(16 * 1024).read_to_end(&mut buf).is_err() {
        return (vec![], len);
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().skip(usize::from(from > 0)).map(|l| strip_ansi(l).trim().to_string()).filter(|l| !l.is_empty()).collect();
    let keep = lines.len().saturating_sub(40);
    lines.drain(..keep);
    (lines, len)
}

/// «[3/8] шаг» → (3, 8) и текст без счётчика
pub fn step_of(s: &str) -> (Option<(u32, u32)>, &str) {
    let Some(rest) = s.strip_prefix('[') else { return (None, s) };
    let Some((nums, text)) = rest.split_once(']') else { return (None, s) };
    let Some((k, n)) = nums.split_once('/') else { return (None, s) };
    match (k.trim().parse(), n.trim().parse()) {
        (Ok(k), Ok(n)) if n > 0 => (Some((k, n)), text.trim()),
        _ => (None, s),
    }
}

pub(crate) fn copy_bins(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for b in ["yami-shell", "yami-files", "yami-view", "yami-play"] {
        let src = from.join(b);
        if src.exists() {
            // через временное имя: работающий бинарь подменяется целиком (rename), не переписывается
            let tmp = to.join(format!(".{b}.new"));
            std::fs::copy(&src, &tmp)?;
            std::fs::rename(&tmp, to.join(b))?;
        }
    }
    Ok(())
}

/// поставить найденное обновление; Err — почему нет
pub fn apply(set: &Settings) -> Result<String, String> {
    require_enabled()?;
    let Some(_l) = lock() else { return Err(crate::tf!("обновление уже идёт", "an update is already running", "uma atualização já está em andamento")) };
    progress(&crate::tf!("проверяю, что нового", "checking what is new", "verificando novidades"));
    let i = check(set);
    if !i.err.is_empty() {
        return Err(i.err);
    }
    if !i.available {
        return Ok(crate::tf!("уже свежее: {}", "already up to date: {}", "já está atualizado: {}", installed().version));
    }
    if !i.sig_ok(&set.update_verify) {
        return Err(crate::tf!("{} — не ставлю (настройки → обновления → подписи)", "{}: not installing (settings → updates → signatures)", "{}: não será instalado (configurações → atualizações → assinaturas)", sig_text(&i.sig)));
    }
    build_install(&i)?;
    Ok(crate::tf!("установлено: {}", "installed: {}", "instalado: {}", i.latest))
}

/// перейти на выбранный выпуск (тег vX.Y.Z или X.Y.Z), старее или новее установленного.
/// Ставится только то, что проходит правило подписей, как и обычное обновление
pub fn install_tag(set: &Settings, want: &str) -> Result<String, String> {
    require_enabled()?;
    let Some(_l) = lock() else { return Err(crate::tf!("обновление уже идёт", "an update is already running", "uma atualização já está em andamento")) };
    let tag = format!("v{}", want.trim().trim_start_matches('v'));
    if ver(&tag).is_empty() || !tag[1..].chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(crate::tf!("нет такой версии: {}", "no such version: {}", "versão inexistente: {}", want));
    }
    progress(&crate::tf!("ищу {}", "looking for {}", "procurando {}", tag));
    let st = installed();
    let dir = ensure_clone(set, &st)?;
    git(&dir, &["fetch", "-q", "--tags", "origin"]).map_err(explain)?;
    let Ok(target) = git(&dir, &["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}^{{commit}}")]) else {
        return Err(crate::tf!("нет такой версии: {}", "no such version: {}", "versão inexistente: {}", tag));
    };
    let known = has_commit(&dir, &st.commit);
    let mut i = Info { latest: tag[1..].to_string(), reference: tag.clone(), target, available: true, at: now(), ..Default::default() };
    i.sig = verify(&dir, &st, known, &set.update_verify, "verify-tag", &tag);
    if !i.sig_ok(&set.update_verify) {
        return Err(crate::tf!("{} — не ставлю (настройки → обновления → подписи)", "{}: not installing (settings → updates → signatures)", "{}: não será instalado (configurações → atualizações → assinaturas)", sig_text(&i.sig)));
    }
    if st.commit == i.target {
        return Ok(crate::tf!("{} уже стоит", "{} is already installed", "{} já está instalado", tag));
    }
    build_install(&i)?;
    Ok(crate::tf!("установлено: {}", "installed: {}", "instalado: {}", i.latest))
}

/// сборка и установка коммита i.target из своего клона; прежние бинари — в откат
pub(crate) fn build_install(i: &Info) -> Result<(), String> {
    require_enabled()?;
    let dir = clone_dir();
    if !git(&dir, &["status", "--porcelain", "--untracked-files=no"]).unwrap_or_default().is_empty() {
        return Err(crate::tf!("в {} есть правки — убери их, и обновлю", "{} has local edits: remove them and update again", "{} tem alterações locais: remova-as e atualize novamente", dir.display()));
    }
    let from = installed().version;
    progress(&crate::tf!("перехожу на {}", "switching to {}", "trocando para {}", i.reference));
    git(&dir, &["-c", "advice.detachedHead=false", "checkout", "-q", "--detach", &i.target]).map_err(explain)?;
    // прежние бинари и запись об установке — в сторону; точкой отката станут, только если установщик
    // бинари заменил: упавшая сборка не стирает прежнюю точку отката
    let stage = stage_dir();
    let _ = std::fs::remove_dir_all(&stage);
    copy_bins(&bin_dir(), &stage).map_err(|e| e.to_string())?;
    let _ = std::fs::copy(stamp_path(), stage.join("install.json"));
    // старая версия не знает новых настроек и при сохранении их выкинет: снимок, вернутся при обновлении
    if older(&i.latest, &from) {
        snapshot_settings(&from);
    }
    progress(&crate::tf!("сборка и установка (несколько минут)", "build and install (a few minutes)", "compilar e instalar (alguns minutos)"));
    let mut ch = Command::new("bash")
        .arg(dir.join("install.sh"))
        .arg("--update")
        .env("YAMI_UPDATE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(out) = ch.stdout.take() {
        use std::io::BufRead;
        for l in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            // шаги установщика («▪ …») — в шелл, остальное — только в лог
            let clean: String = strip_ansi(&l);
            let c = clean.trim();
            if let Some(s) = c.strip_prefix('▪').or_else(|| c.strip_prefix('▸')).or_else(|| c.strip_prefix('✗')) {
                progress(s.trim());
            } else if !c.is_empty() {
                println!("    {c}");
            }
        }
    }
    let ok = ch.wait().map(|s| s.success()).unwrap_or(false);
    keep_stage(&stage);
    // установщик выбранной версии мог не поднять шелл (у старых был такой баг): поднимаем сами
    ensure_shell();
    if !ok {
        return Err(crate::tf!("установка не удалась — подробности в {}", "install failed: details in {}", "falha na instalação: detalhes em {}", log_path().display()));
    }
    if !older(&i.latest, &from) {
        restore_settings(&i.latest);
    }
    Ok(())
}

fn stage_dir() -> PathBuf {
    prev_dir().with_file_name("prev.new")
}

/// бинари в bin_dir уже не те, что отложены перед установкой: отложенные — новая точка отката;
/// те же — установка до них не дошла, прежняя точка отката остаётся
pub(crate) fn keep_stage(stage: &Path) {
    let same = |a: &Path, b: &Path| match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    };
    let old = stage.join("yami-shell");
    if old.exists() && !same(&old, &bin_dir().join("yami-shell")) {
        let _ = std::fs::remove_dir_all(prev_dir());
        if std::fs::rename(stage, prev_dir()).is_ok() {
            return;
        }
    }
    let _ = std::fs::remove_dir_all(stage);
}

/// a старее b (по номерам версий; пустое — не старее)
pub(crate) fn older(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && ver(a) < ver(b)
}

/// шелл отвечает на сокете; если нет за ~6 с — новый цикл автозапуска niri (второй экземпляр
/// уходит сам). Игру не трогаем: при ней установщик шелл не перезапускал, он жив
pub(crate) fn ensure_shell() {
    let up = || (0..60).any(|_| crate::ipc::send("ping") || { std::thread::sleep(std::time::Duration::from_millis(100)); false });
    if up() || std::env::var_os("NIRI_SOCKET").is_none() {
        return;
    }
    let bin = bin_dir().join("yami-shell").to_string_lossy().replace('\'', "'\\''");
    println!("  {}", crate::tf!("шелл не ответил — запускаю его снова", "the shell did not answer: starting it again", "o shell não respondeu: iniciando novamente"));
    let _ = Command::new("niri").args(["msg", "action", "spawn-sh", "--", &format!("until '{bin}'; do sleep 1; done")]).stdin(Stdio::null()).stdout(Stdio::null()).status();
    if !up() {
        println!("  {}", crate::tf!("шелл так и не запустился: yami run в терминале покажет почему", "the shell did not start: yami run in a terminal shows why", "o shell não iniciou: yami run no terminal mostra o motivo"));
    }
}

pub(crate) fn snap_path() -> PathBuf {
    crate::config::state_dir().join("settings-newer.json")
}

/// перед переходом на старую версию: настройки, как есть, с версией, из которой ушли
pub(crate) fn snapshot_settings(from: &str) {
    let Ok(text) = std::fs::read_to_string(crate::config::settings_path()) else { return };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    // снимок уже есть (откат через несколько версий) — первый, самый полный, не трогаем
    if snap_path().exists() {
        return;
    }
    let _ = crate::config::write_atomic(&snap_path(), &(serde_json::json!({ "version": from, "settings": v }).to_string() + "\n"));
}

/// вернулись на версию не старее снимка: ключи, которых нет в настройках, — из снимка
pub(crate) fn restore_settings(now_ver: &str) {
    let Ok(text) = std::fs::read_to_string(snap_path()) else { return };
    let Ok(snap) = serde_json::from_str::<serde_json::Value>(&text) else {
        let _ = std::fs::remove_file(snap_path());
        return;
    };
    if older(now_ver, snap["version"].as_str().unwrap_or("")) {
        return;
    }
    let p = crate::config::settings_path();
    let mut cur = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let mut added = 0;
    if let Some(old) = snap["settings"].as_object() {
        for (k, v) in old {
            if !cur.contains_key(k) {
                cur.insert(k.clone(), v.clone());
                added += 1;
            }
        }
    }
    if added > 0 {
        let _ = crate::config::write_atomic(&p, &(serde_json::to_string_pretty(&serde_json::Value::Object(cur)).unwrap_or_default() + "\n"));
    }
    let _ = std::fs::remove_file(snap_path());
}

/// версия прошлой сборки (для мгновенного отката), если она есть
pub fn prev_version() -> Option<String> {
    if !prev_dir().join("yami-shell").exists() {
        return None;
    }
    let s: Stamp = std::fs::read_to_string(prev_dir().join("install.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Some(if s.version.is_empty() { "?".into() } else { s.version })
}

pub(crate) fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for c in s.chars() {
        match c {
            '\x1b' => esc = true,
            c if esc && c.is_ascii_alphabetic() => esc = false,
            _ if esc => {}
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

/// вернуть бинари, что стояли до последнего обновления
pub fn rollback() -> Result<String, String> {
    require_enabled()?;
    let Some(_l) = lock() else { return Err(crate::tf!("обновление уже идёт", "an update is already running", "uma atualização já está em andamento")) };
    let p = prev_dir();
    if !p.join("yami-shell").exists() {
        return Err(crate::tf!("откатывать не на что: прежней сборки нет", "nothing to roll back to: no previous build", "não há como reverter: sem compilação anterior"));
    }
    let from = installed().version;
    let to = prev_version().unwrap_or_default();
    if older(&to, &from) {
        snapshot_settings(&from);
    }
    copy_bins(&p, &bin_dir()).map_err(|e| e.to_string())?;
    if p.join("install.json").exists() {
        let _ = std::fs::copy(p.join("install.json"), stamp_path());
    }
    let _ = std::fs::remove_dir_all(&p);
    if !older(&to, &from) {
        restore_settings(&to);
    }
    restart_shell();
    Ok(crate::tf!("вернул {}", "rolled back to {}", "revertido para {}", installed().version))
}

/// перезапуск шелла, если он работает и игра не в фокусе
/// перезапустить шелл новым бинарём и дождаться именно нового: сокет создаётся заново (другой
/// inode) и отвечает. Не дождались за ~8 с — ensure_shell. При игре не трогаем
pub fn restart_shell() {
    if crate::niri::game_focused() {
        println!("{}", crate::tf!("  игра в фокусе — шелл перезапустится сам при следующем входе (или: yami restart)", "  a game has focus: the shell restarts at the next login (or: yami restart)", "  um jogo está em foco: o shell reinicia no próximo login (ou: yami restart)"));
        return;
    }
    use std::os::unix::fs::MetadataExt;
    let ino = || std::fs::metadata(crate::ipc::path()).map(|m| m.ino()).ok();
    let before = ino();
    if !crate::ipc::send("restart") {
        ensure_shell();
        return;
    }
    for _ in 0..80 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if ino() != before && crate::ipc::send("ping") {
            return;
        }
    }
    ensure_shell();
}

/// записать, что установлено (из дерева dir): установщик и tools/deploy.sh
pub fn stamp(dir: &Path, components: &str) -> Result<(), String> {
    let version = std::fs::read_to_string(dir.join("VERSION")).map(|v| v.trim().to_string()).unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let commit = git(dir, &["rev-parse", "HEAD"]).unwrap_or_default();
    let dirty = !git(dir, &["status", "--porcelain", "--untracked-files=no"]).unwrap_or_default().is_empty();
    let mut repo = git(dir, &["remote", "get-url", "origin"]).unwrap_or_default();
    // свой клон обновлений: адрес тот же, что у исходной установки
    if repo.is_empty() || repo.starts_with('/') {
        repo = installed().repo;
    }
    let old = installed();
    let components = if components.is_empty() { old.components } else { components.to_string() };
    save_json(&stamp_path(), &Stamp { version, commit, dirty, repo, components, date: now() });
    Ok(())
}
