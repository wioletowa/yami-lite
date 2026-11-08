// Обновления: проверка, установка и откат — из настроек (вкладка «обновления») и из терминала
// (`yami update`). Источник — свой клон репозитория в ~/.local/share/yami/src (не рабочая
// копия разработчика): git fetch, выпуск = самый новый тег vX.Y.Z (канал «main» — вершина
// ветки). Подпись выпуска проверяется ключами keys/allowed_signers из УЖЕ установленной версии
// (доверие с первой установки): подменённый на сервере тег без ключа автора не встанет.
// Ставит install.sh --update (сборка из исходников, без sudo, настройки не трогает), прежние
// бинари — в ~/.local/lib/yami/prev для отката. Что установлено — ~/.local/state/yami/install.json.

pub use crate::upinstall::*;
pub use crate::updtab::{short_repo, tab};
use crate::app::{App, Msg, SKind};
use crate::config::Settings;
use crate::ui::Act;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// ключи подписи выпусков (формат ssh allowed_signers) в дереве репозитория
pub const KEYS: &str = "keys/allowed_signers";
/// откуда ставит установщик по умолчанию; свой адрес — updateRepo
pub const REPO: &str = "https://github.com/Trendorin/yami.git";
const NOTE_ID: u32 = 9_400_001;

pub const ENABLED: bool = !cfg!(feature = "package-managed");

pub fn managed_message() -> String {
    crate::tf!("Обновлениями управляет пакетный менеджер.", "Updates are managed by your package manager.", "As atualizações são gerenciadas pelo seu gerenciador de pacotes.")
}

pub(crate) fn require_enabled() -> Result<(), String> {
    if ENABLED { Ok(()) } else { Err(managed_message()) }
}

/// что установлено: пишет `yami-shell update stamp` (установщик, tools/deploy.sh)
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Stamp {
    pub version: String,
    pub commit: String,
    /// собрано из дерева с несохранёнными правками
    pub dirty: bool,
    pub repo: String,
    pub components: String,
    pub date: u64,
}

/// результат проверки; последний хранится в ~/.local/state/yami/update.json
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Info {
    /// версия выпуска (2.1.0) или короткий коммит (канал main)
    pub latest: String,
    /// тег или origin/main
    pub reference: String,
    pub target: String,
    pub available: bool,
    /// что нового по-английски (CHANGELOG.md) и по-русски (CHANGELOG.ru.md): оба сразу, чтобы смена
    /// языка интерфейса не ждала следующей проверки; показывать — `notes()`
    pub notes: Vec<String>,
    pub notes_ru: Vec<String>,
    /// good:<кто> | unsigned | bad | nokeys | off
    pub sig: String,
    pub err: String,
    pub at: u64,
    /// последние выпуски (новые первыми) — на что можно перейти из вкладки и `yami update to`
    pub releases: Vec<Rel>,
}

impl Info {
    /// что нового на языке интерфейса (старый update.json без русских строк — по-английски)
    pub fn notes(&self) -> &[String] {
        if crate::i18n::translated() || self.notes_ru.is_empty() {
            &self.notes
        } else {
            &self.notes_ru
        }
    }
}

/// выпуск: тег, версия, подпись (как Info::sig), дата тега
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Rel {
    pub tag: String,
    pub version: String,
    pub sig: String,
    pub date: u64,
}

impl Info {
    /// можно ставить: подпись по правилам updateVerify
    pub fn sig_ok(&self, policy: &str) -> bool {
        match policy {
            "off" => true,
            "require" => self.sig.starts_with("good:"),
            // auto: ключей ещё нет — как при первой установке; ключи есть — только подписанное
            _ => self.sig.starts_with("good:") || self.sig == "nokeys" || self.sig == "off",
        }
    }
}

/// состояние для вкладки
#[derive(Default)]
pub struct Upd {
    pub stamp: Stamp,
    pub info: Option<Info>,
    pub checking: bool,
    /// идёт установка: последний шаг
    pub busy: Option<String>,
    /// чем кончилась установка: (успех, текст)
    pub done: Option<(bool, String)>,
    pub rollback: bool,
    /// хвост update.log для консоли вкладки и размер файла, по которому он прочитан
    pub log: Vec<String>,
    pub log_len: u64,
    /// открыт выбор версии для отката и что выбрано («prev» или X.Y.Z); ставится только кнопкой
    /// «поставить», так что случайный щелчок по «откатить» ничего не меняет
    pub pick: bool,
    pub sel: String,
    /// пакеты, которых не хватает выбранным частям (need.rs)
    pub need: Option<crate::need::Need>,
}

impl Upd {
    pub fn load() -> Upd {
        let (log, log_len) = log_tail();
        Upd { stamp: installed(), info: last(), rollback: prev_dir().join("yami-shell").exists(), log, log_len, need: crate::need::load(), ..Default::default() }
    }
    /// для `yami-shell shot settings:updates[-new|-busy]`
    pub fn mock(what: &str) -> Upd {
        let stamp = Stamp { version: "2.0.0".into(), commit: "24da2407c0ffee00".into(), repo: REPO.into(), ..Default::default() };
        let mut u = Upd { stamp, rollback: true, ..Default::default() };
        let mut i = Info { latest: "2.0.0".into(), reference: "v2.0.0".into(), sig: "good:trendorin".into(), at: now() - 600, ..Default::default() };
        if what != "updates" {
            i.latest = "2.1.0".into();
            i.reference = "v2.1.0".into();
            i.available = true;
            i.notes = vec!["yami: one command for the whole desktop".into(), "settings: updates and services tabs".into(), "shell: notification images are size-checked".into()];
        }
        if what == "updates-busy" {
            u.busy = Some("[3/8] shell: build from source".into());
            u.log = ["▪ checking what is new", "▪ switching to v2.1.0", "▪ build and install (a few minutes)", "▪ [1/8] update from ~/.local/share/yami/src", "✓ update from ~/.local/share/yami/src 0s", "▪ [2/8] packages", "✓ packages 3s", "▪ [3/8] shell: build from source", "Compiling yami-px v2.1.0", "Compiling yami-files v2.1.0", "Compiling yami-shell v2.1.0"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        }
        if what == "updates-need" {
            // вышла версия с новыми функциями, а пакетов к ним нет: кнопка «поставить»
            i.available = false;
            u.need = Some(crate::need::Need { pm: "pacman".into(), pkgs: vec!["wlsunset".into(), "tesseract".into(), "tesseract-data-eng".into(), "zbar".into()], login: Some("/home/you/.local/share/yami/src/tools/greet-install.sh".into()) });
        }
        if what == "updates-pick" {
            // выбор версии: прошлой сборки в образце нет, три выпуска, один не подписан
            u.pick = true;
            u.sel = "2.1.0".into();
            i.available = false;
            i.releases = vec![
                Rel { tag: "v2.1.0".into(), version: "2.1.0".into(), sig: "good:trendorin".into(), date: 1_793_721_454 },
                Rel { tag: "v2.0.0".into(), version: "2.0.0".into(), sig: "good:trendorin".into(), date: 1_793_721_346 },
                Rel { tag: "v1.9.0".into(), version: "1.9.0".into(), sig: "unsigned".into(), date: 1_793_600_000 },
            ];
        }
        u.info = Some(i);
        u
    }
}

// ── файлы ───────────────────────────────────────────────────────────────────
pub(crate) fn now() -> u64 {
    crate::clock::unix_now()
}
pub fn stamp_path() -> PathBuf {
    crate::config::state_dir().join("install.json")
}
pub(crate) fn info_path() -> PathBuf {
    crate::config::state_dir().join("update.json")
}
pub fn clone_dir() -> PathBuf {
    crate::config::data_dir().join("yami/src")
}
pub(crate) fn bin_dir() -> PathBuf {
    crate::config::home().join(".local/bin")
}
pub(crate) fn prev_dir() -> PathBuf {
    crate::config::home().join(".local/lib/yami/prev")
}
pub fn log_path() -> PathBuf {
    yami_px::paths::cache_dir().join("yami/update.log")
}
pub fn installed() -> Stamp {
    std::fs::read_to_string(stamp_path()).map(|t| yami_px::paths::lenient_json(&t)).unwrap_or_default()
}
/// последняя проверка; то, что она нашла и что уже установлено, — не «доступно»
pub fn last() -> Option<Info> {
    let mut i: Info = std::fs::read_to_string(info_path()).ok().map(|t| yami_px::paths::lenient_json(&t))?;
    if i.target == installed().commit {
        i.available = false;
    }
    Some(i)
}
pub(crate) fn save_json<T: Serialize>(p: &Path, v: &T) {
    if let Ok(j) = serde_json::to_string_pretty(v) {
        let _ = crate::config::write_atomic(p, &(j + "\n"));
    }
}

// ── git ─────────────────────────────────────────────────────────────────────
/// git без вопросов: приватный репозиторий без ключей — ошибка сразу, а не ожидание пароля;
/// медленная сеть обрывается (20 с меньше 1 КБ/с)
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "http.lowSpeedLimit=1000", "-c", "http.lowSpeedTime=20", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| crate::tf!("нет git: {e}", "git not found: {e}", "git não encontrado: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&o.stderr).lines().rfind(|l| !l.trim().is_empty()).unwrap_or("git failed").trim().to_string())
    }
}

/// понятная причина вместо последней строки git
pub(crate) fn explain(e: String) -> String {
    let l = e.to_lowercase();
    if l.contains("could not read username") || l.contains("authentication") || l.contains("repository not found") || l.contains("403") {
        crate::tf!("нет доступа к репозиторию (приватный? gh auth login)", "no access to the repository (private? gh auth login)", "sem acesso ao repositório (privado? gh auth login)")
    } else if l.contains("could not resolve host") || l.contains("timed out") || l.contains("unable to access") || l.contains("operation too slow") {
        crate::tf!("нет сети или сервер недоступен", "no network or the server is unreachable", "sem rede ou servidor inacessível")
    } else {
        e
    }
}

/// свой клон: нет — клонируем (история без файлов, они придут при переходе на версию)
pub(crate) fn ensure_clone(set: &Settings, st: &Stamp) -> Result<PathBuf, String> {
    let dir = clone_dir();
    let url = [set.update_repo.as_str(), st.repo.as_str(), REPO].into_iter().find(|u| !u.is_empty()).unwrap_or(REPO).to_string();
    if !dir.join(".git").exists() {
        if let Some(p) = dir.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let d = dir.to_string_lossy().to_string();
        git(Path::new("/"), &["clone", "-q", "--filter=blob:none", "--", &url, &d]).map_err(explain)?;
    } else {
        if git(&dir, &["remote", "get-url", "origin"]).map(|u| u != url).unwrap_or(true) {
            git(&dir, &["remote", "set-url", "origin", &url])?;
        }
        // установщик клонировал с --depth 1: для сравнения версий нужна история
        if git(&dir, &["rev-parse", "--is-shallow-repository"]).map(|s| s == "true").unwrap_or(false) {
            git(&dir, &["fetch", "-q", "--unshallow", "--filter=blob:none", "origin"]).map_err(explain)?;
        }
    }
    Ok(dir)
}

pub(crate) fn has_commit(dir: &Path, c: &str) -> bool {
    !c.is_empty() && git(dir, &["cat-file", "-e", &format!("{c}^{{commit}}")]).is_ok()
}

/// «2.10.0» > «2.9.1»: по числам
pub(crate) fn ver(s: &str) -> Vec<u64> {
    s.trim_start_matches('v').split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).map(|x| x.parse().unwrap_or(0)).collect()
}

/// проверить, что нового (сеть, git): из нити или из терминала
pub fn check(set: &Settings) -> Info {
    if !ENABLED {
        return Info { err: managed_message(), ..Default::default() };
    }
    let mut i = Info { at: now(), ..Default::default() };
    if let Err(e) = check_into(set, &mut i) {
        i.err = e;
    }
    save_json(&info_path(), &i);
    i
}

pub(crate) fn check_into(set: &Settings, i: &mut Info) -> Result<(), String> {
    require_enabled()?;
    let st = installed();
    let dir = ensure_clone(set, &st)?;
    // теги без --force: тег, переписанный на сервере, не заменит уже виденный
    git(&dir, &["fetch", "-q", "--tags", "--prune", "origin", "+refs/heads/main:refs/remotes/origin/main"]).map_err(explain)?;
    let main = set.update_channel == "main";
    let reference = if main {
        "origin/main".to_string()
    } else {
        // выпусков ещё нет — не ошибка: ставить нечего (reference пустой)
        match git(&dir, &["tag", "-l", "v[0-9]*", "--sort=-v:refname"])?.lines().next() {
            Some(t) => t.to_string(),
            None => return Ok(()),
        }
    };
    i.target = git(&dir, &["rev-parse", &format!("{reference}^{{commit}}")])?;
    i.latest = if main { i.target.chars().take(8).collect() } else { reference.trim_start_matches('v').to_string() };
    i.reference = reference;
    let known = has_commit(&dir, &st.commit);
    i.available = if st.commit == i.target {
        false
    } else if known && git(&dir, &["merge-base", "--is-ancestor", &i.target, &st.commit]).is_ok() {
        // установлено то же или новее (сборка разработчика впереди выпуска)
        false
    } else if main {
        true
    } else {
        st.version.is_empty() || ver(&i.latest) > ver(&st.version)
    };
    i.notes = notes(&dir, &st, i, known, main);
    // по-русски — только CHANGELOG.ru.md: нет там выпуска — пусто, Info::notes() покажет английское
    let ru = if main { Err(String::new()) } else { git(&dir, &["show", &format!("{}:CHANGELOG.ru.md", i.target)]) };
    i.notes_ru = ru.map(|log| section(&log, &i.latest)).unwrap_or_default();
    i.sig = signature(&dir, &st, i, known, main, &set.update_verify);
    i.releases = releases(&dir, &st, known, &set.update_verify);
    Ok(())
}

/// последние восемь выпусков с подписями (их проверяют ключи установленной версии)
pub(crate) fn releases(dir: &Path, st: &Stamp, known: bool, policy: &str) -> Vec<Rel> {
    let Ok(tags) = git(dir, &["tag", "-l", "v[0-9]*", "--sort=-v:refname"]) else { return vec![] };
    tags.lines()
        .take(8)
        .map(|t| Rel {
            tag: t.to_string(),
            version: t.trim_start_matches('v').to_string(),
            sig: verify(dir, st, known, policy, "verify-tag", t),
            date: git(dir, &["log", "-1", "--format=%ct", &format!("{t}^{{commit}}")]).ok().and_then(|d| d.parse().ok()).unwrap_or(0),
        })
        .collect()
}

/// строка CHANGELOG как текст: [#5](ссылка) → #5, без ** и обратных кавычек
pub(crate) fn plain(l: &str) -> String {
    let mut out = String::with_capacity(l.len());
    let mut rest = l;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find("](").and_then(|j| tail[j + 2..].find(')').map(|k| (j, j + 2 + k))) {
            Some((j, end)) => {
                out.push_str(&tail[..j]);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('[');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "")
}

/// строки раздела «## ver» без заголовков «### …» и без разметки: ссылки, **жирное**, `код`
pub(crate) fn section(log: &str, ver: &str) -> Vec<String> {
    let head = format!("## {ver}");
    log.lines()
        .skip_while(|l| l.trim_end() != head)
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(|l| plain(l.trim().trim_start_matches(['-', '*']).trim()))
        .filter(|l| !l.is_empty())
        .take(12)
        .collect()
}

/// что нового: раздел CHANGELOG.md выпуска, иначе заголовки коммитов (русский раздел — notes_ru)
pub(crate) fn notes(dir: &Path, st: &Stamp, i: &Info, known: bool, main: bool) -> Vec<String> {
    if !main {
        if let Ok(log) = git(dir, &["show", &format!("{}:CHANGELOG.md", i.target)]) {
            let sec = section(&log, &i.latest);
            if !sec.is_empty() {
                return sec;
            }
        }
    }
    let range = if known { format!("{}..{}", st.commit, i.target) } else { i.target.clone() };
    git(dir, &["log", "--no-merges", "--format=%s", "-n", "12", &range]).map(|t| t.lines().map(String::from).collect()).unwrap_or_default()
}

/// подпись выпуска ключами из установленной версии (а если её коммита нет в клоне — из того,
/// что клон ставил последним)
pub(crate) fn signature(dir: &Path, st: &Stamp, i: &Info, known: bool, main: bool, policy: &str) -> String {
    let (what, obj) = if main { ("verify-commit", i.target.as_str()) } else { ("verify-tag", i.reference.as_str()) };
    verify(dir, st, known, policy, what, obj)
}

/// git verify-tag/verify-commit ключами установленной версии
pub(crate) fn verify(dir: &Path, st: &Stamp, known: bool, policy: &str, what: &str, obj: &str) -> String {
    if policy == "off" {
        return "off".into();
    }
    let base = if known { st.commit.clone() } else { "HEAD".into() };
    let keys = git(dir, &["show", &format!("{base}:{KEYS}")]).unwrap_or_default();
    if keys.lines().all(|l| l.trim().is_empty() || l.trim_start().starts_with('#')) {
        return "nokeys".into();
    }
    let kf = crate::config::state_dir().join("update-keys");
    if crate::config::write_atomic(&kf, &(keys + "\n")).is_err() {
        return "bad".into();
    }
    let kfs = kf.to_string_lossy().to_string();
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        // minTrustLevel=fully: подпись ключом не из allowed_signers («No principal matched») — не «good»
        .args(["-c", "gpg.format=ssh", "-c", &format!("gpg.ssh.allowedSignersFile={kfs}"), "-c", "gpg.minTrustLevel=fully", what, "--", obj])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output();
    let Ok(o) = o else { return "bad".into() };
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    // Good "git" signature for NAME with ED25519 key SHA256:… — без «for NAME» ключ не из списка
    let who = err.split("Good \"git\" signature for ").nth(1).and_then(|s| s.split(" with ").next()).map(|s| s.trim().to_string());
    if o.status.success() && !err.contains("No principal matched") {
        match who {
            Some(w) if !w.is_empty() => format!("good:{w}"),
            _ => "bad".into(),
        }
    } else if err.contains("no signature") || err.is_empty() || err.contains("cannot verify a non-tag") {
        "unsigned".into()
    } else {
        "bad".into()
    }
}

/// подпись для людей
pub fn sig_text(sig: &str) -> String {
    match sig {
        "off" => crate::tf!("не проверяется", "not checked", "não verificado"),
        "nokeys" => crate::tf!("ключей автора ещё нет (keys/allowed_signers)", "no author keys yet (keys/allowed_signers)", "ainda sem chaves do autor (keys/allowed_signers)"),
        "unsigned" => crate::tf!("не подписано", "not signed", "sem assinatura"),
        "bad" => crate::tf!("подпись НЕ сходится", "signature does NOT match", "a assinatura NÃO corresponde"),
        "" => "—".into(),
        s => {
            // 193045225+Trendorin@users.noreply.github.com → Trendorin
            let who = s.trim_start_matches("good:");
            let who = who.split('@').next().unwrap_or(who);
            let who = who.split_once('+').map(|(_, n)| n).unwrap_or(who);
            crate::tf!("подписано: {}", "signed: {}", "assinado: {}", who)
        }
    }
}

// ── шелл: проверка по расписанию, вкладка, команды от `yami update` ─────────
impl App {
    pub fn update_check(&mut self) {
        if !ENABLED || self.st.upd.checking {
            return;
        }
        self.st.upd.checking = true;
        let set = self.st.set.clone();
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("update".into())
            .spawn(move || {
                let i = check(&set);
                let _ = tx.send(Msg::Update(i));
            })
            .ok();
        self.dirty(&SKind::Modal);
    }

    /// раз в минуту: пора ли проверить (не раньше минуты после старта; ошибка — снова через час)
    pub fn update_tick(&mut self) {
        let s = &self.st.set;
        if !ENABLED || !s.update_check || s.update_every == 0 || self.st.upd.checking || self.st.upd.busy.is_some() || self.st.started.elapsed().as_secs() < 60 {
            return;
        }
        let every = match &self.st.upd.info {
            Some(i) if !i.err.is_empty() => 3600,
            _ => s.update_every as u64 * 3600,
        };
        if self.st.upd.info.as_ref().map(|i| now().saturating_sub(i.at) >= every).unwrap_or(true) {
            self.update_check();
        }
    }

    pub fn on_update(&mut self, i: Info) {
        let was = self.st.upd.info.as_ref().filter(|x| x.available).map(|x| x.target.clone()).unwrap_or_default();
        self.st.upd.checking = false;
        self.st.upd.stamp = installed();
        // новое — карточкой, один раз на версию
        if i.available && i.target != was && i.err.is_empty() {
            let body = i.notes().first().cloned().unwrap_or_default();
            let n = crate::dbus::Incoming {
                id: NOTE_ID,
                app: "yami-update".into(),
                icon: String::new(),
                summary: crate::tf!("доступно обновление yami {}", "yami {} is available", "yami {} está disponível", i.latest),
                body,
                actions: vec![("default".into(), crate::i18n::tr("подробнее").to_string())],
                urgency: 1,
                image: None,
                timeout: -1,
            };
            self.on_notif(n);
        }
        self.st.upd.info = Some(i);
        self.dirty(&SKind::Modal);
    }

    /// действие карточки «доступно обновление»: открыть вкладку
    pub fn update_note_action(&mut self, id: u32) -> bool {
        if id != NOTE_ID {
            return false;
        }
        self.open_settings_tab("updates");
        true
    }

    /// выбор версии в списке отката
    pub fn update_click(&mut self, a: &Act) -> bool {
        let Act::UpdSel(s) = a else { return false };
        self.st.upd.sel = if self.st.upd.sel == *s { String::new() } else { s.clone() };
        self.dirty(&SKind::Modal);
        true
    }

    /// пока открыта вкладка «обновления»: перечитать хвост лога, если файл вырос
    pub fn update_log_tick(&mut self) {
        if self.st.modal != Some(crate::app::Modal::Settings) || self.st.stab != "updates" {
            return;
        }
        let len = std::fs::metadata(log_path()).map(|m| m.len()).unwrap_or(0);
        if len != self.st.upd.log_len {
            (self.st.upd.log, self.st.upd.log_len) = log_tail();
            self.dirty(&SKind::Modal);
        }
    }

    /// команды шеллу от процесса обновления; true — команда наша
    pub fn update_ipc(&mut self, c: &str) -> bool {
        if let Some(s) = c.strip_prefix("update-progress ") {
            self.st.upd.busy = Some(s.to_string());
            self.st.upd.done = None;
        } else if let Some(s) = c.strip_prefix("update-done ") {
            let (ok, text) = s.split_once(' ').unwrap_or((s, ""));
            self.st.upd.busy = None;
            self.st.upd.done = Some((ok == "ok", text.to_string()));
            self.st.upd = Upd { busy: None, done: self.st.upd.done.take(), ..Upd::load() };
        } else if c == "update-reload" {
            // `yami update check` из терминала уже проверил: перечитать его итог
            let busy = self.st.upd.busy.take();
            self.st.upd = Upd { busy, ..Upd::load() };
        } else {
            return false;
        }
        self.dirty(&SKind::Modal);
        true
    }

    /// кнопки вкладки
    pub fn update_cycle(&mut self, k: &str) -> bool {
        if !ENABLED {
            return false;
        }
        let s = &mut self.st.set;
        match k {
            "update_channel" => {
                s.update_channel = if s.update_channel == "main" { "release".into() } else { "main".into() };
                self.st.upd.info = None;
                self.update_check();
            }
            "update_verify" => {
                s.update_verify = match s.update_verify.as_str() {
                    "auto" => "require".into(),
                    "require" => "off".into(),
                    _ => "auto".into(),
                };
            }
            "update_every" => {
                s.update_every = crate::settings::step_in(&[6, 12, 24, 72, 168], s.update_every as i32) as u32;
            }
            "update_now" => self.update_check(),
            "update_rollback" => {
                // только открыть выбор; список выпусков — из последней проверки, нет его — проверить
                let u = &mut self.st.upd;
                u.pick = !u.pick;
                u.sel.clear();
                if u.pick && u.info.as_ref().map(|i| i.releases.is_empty()).unwrap_or(true) {
                    self.update_check();
                }
            }
            "need_run" => {
                // команда в терминале; кнопка остаётся, пока файл на месте: закрыл терминал, не
                // поставив, — она никуда не делась (need_tick перечитывает файл)
                if let Some(n) = &self.st.upd.need {
                    crate::need::run_in_term(n);
                }
            }
            "update_pick_cancel" => {
                self.st.upd.pick = false;
                self.st.upd.sel.clear();
            }
            "update_apply" | "update_pick_go" => {
                if self.st.upd.busy.is_some() {
                    return true;
                }
                let sel = std::mem::take(&mut self.st.upd.sel);
                if k == "update_pick_go" && sel.is_empty() {
                    return true;
                }
                self.st.upd.pick = false;
                self.save_now();
                self.st.upd.busy = Some(crate::tf!("запускаю", "starting", "iniciando"));
                self.st.upd.done = None;
                if k == "update_apply" {
                    spawn_detached(&["apply"]);
                } else if sel == "prev" {
                    spawn_detached(&["rollback"]);
                } else {
                    spawn_detached(&["to", &sel]);
                }
            }
            _ => return false,
        }
        self.dirty(&SKind::Modal);
        true
    }
}

/// `yami-shell update apply --notify` отдельным процессом: живёт, даже когда шелл перезапустится
pub(crate) fn spawn_detached(what: &[&str]) {
    let log = log_path();
    if let Some(p) = log.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let Ok(f) = std::fs::File::create(&log) else { return };
    let Ok(f2) = f.try_clone() else { return };
    let _ = Command::new("setsid").arg("-f").arg(yami_px::paths::self_exe()).arg("update").args(what).arg("--notify").stdin(Stdio::null()).stdout(f).stderr(f2).status();
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "package-managed")]
    #[test]
    fn package_manager_owns_updates() {
        let set = crate::config::Settings::default();
        assert!(!super::ENABLED);
        assert!(!super::check(&set).err.is_empty());
        assert!(super::apply(&set).is_err());
        assert!(super::install_tag(&set, "2.5.0").is_err());
        assert!(super::rollback().is_err());
        assert!(super::build_install(&super::Info::default()).is_err());
    }

    #[test]
    fn notes_as_plain_text() {
        assert_eq!(super::plain("**Bar shape** ([#5](https://github.com/Trendorin/yami/issues/5)): `barFloat`"), "Bar shape (#5): barFloat");
        assert_eq!(super::plain("a [b] c"), "a [b] c");
        // «## 2.5.1» не должен находить «## 2.5.10», а раздел кончается на следующем «## »
        let log = "# x\n\n## 2.5.10\n\n- ten\n\n## 2.5.1\n\n### Fixes\n\n- `one`\n- two\n\n## 2.5.0\n\n- old\n";
        assert_eq!(super::section(log, "2.5.1"), vec!["one", "two"]);
        assert!(super::section(log, "2.6.0").is_empty());
    }
}
