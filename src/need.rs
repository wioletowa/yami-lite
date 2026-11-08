// Пакеты, которых не хватает выбранным частям установки (ночной свет, текст со скриншота,
// GameMode…). Установщик (installer/packages.sh, do_need_pkgs) пишет
// ~/.local/state/yami/packages-needed: «pm pacman» и «pkgs a b c» — только то, что есть в
// репозиториях и не стоит; «login ПУТЬ» — экран входа (greet-install.sh того дерева, из которого
// шло обновление): нового нет или он старее сборки. Обновление идёт без sudo и само их не поставит: `yami update need`
// печатает одну команду, `--run` выполняет её здесь же, вкладка «обновления» открывает её в
// терминале (пароль sudo спрашивает там). Команду собирает шелл по своей таблице: из файла
// берутся только имена пакетов, и только безопасные.
// i18n: inline pairs

use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Need {
    pub pm: String,
    pub pkgs: Vec<String>,
    /// tools/greet-install.sh, который поставит экран входа (вместо SDDM или новее)
    pub login: Option<PathBuf>,
}

pub fn path() -> PathBuf {
    crate::config::state_dir().join("packages-needed")
}

/// имя пакета: буквы, цифры и «+._-/:» (gentoo-категории, :arch у debian), не с «-»
fn name_ok(p: &str) -> bool {
    !p.is_empty() && p.len() <= 96 && !p.starts_with('-') && p.chars().all(|c| c.is_ascii_alphanumeric() || "+._-/:".contains(c))
}

/// установка пакетов у этого менеджера
fn base(pm: &str) -> Option<&'static [&'static str]> {
    Some(match pm {
        "pacman" => &["pacman", "-S", "--needed"],
        "dnf" => &["dnf", "install"],
        "rpm-ostree" => &["rpm-ostree", "install"],
        "apt" => &["apt", "install"],
        "zypper" => &["zypper", "install"],
        "xbps" => &["xbps-install"],
        "apk" => &["apk", "add"],
        _ => return None,
    })
}

pub fn parse(t: &str) -> Option<Need> {
    let mut n = Need::default();
    for l in t.lines() {
        if let Some(v) = l.strip_prefix("pm ") {
            n.pm = v.trim().to_string();
        } else if let Some(v) = l.strip_prefix("pkgs ") {
            n.pkgs = v.split_whitespace().filter(|p| name_ok(p)).map(String::from).collect();
        } else if let Some(v) = l.strip_prefix("login ") {
            // только настоящий greet-install.sh по абсолютному пути: другое имя файл не подсунет
            let p = PathBuf::from(v.trim());
            if p.is_absolute() && p.ends_with("tools/greet-install.sh") && p.is_file() && !v.chars().any(char::is_control) {
                n.login = Some(p);
            }
        }
    }
    if base(&n.pm).is_none() {
        n.pkgs.clear();
    }
    (!n.pkgs.is_empty() || n.login.is_some()).then_some(n)
}

pub fn load() -> Option<Need> {
    parse(&std::fs::read_to_string(path()).ok()?)
}

impl Need {
    /// установка пакетов (пусто — пакетов не нужно)
    pub fn argv(&self) -> Vec<String> {
        if self.pkgs.is_empty() {
            return vec![];
        }
        let su = if crate::doctor::which("sudo") || !crate::doctor::which("doas") { "sudo" } else { "doas" };
        let mut v = vec![su.to_string()];
        v.extend(base(&self.pm).unwrap_or(&[]).iter().map(|s| s.to_string()));
        v.extend(self.pkgs.iter().cloned());
        v
    }
    /// вся команда одной строкой: пакеты, потом экран входа (он сам спросит sudo)
    pub fn line(&self) -> String {
        let mut parts = vec![];
        if !self.pkgs.is_empty() {
            parts.push(self.argv().join(" "));
        }
        if let Some(p) = &self.login {
            parts.push(sq(&p.to_string_lossy()));
        }
        parts.join(" && ")
    }
    /// что не поставлено, одной строкой — yami doctor и `yami update need`
    pub fn what(&self) -> String {
        self.items().join("  ·  ")
    }
    /// по пункту на строку — вкладка «обновления»
    pub fn items(&self) -> Vec<String> {
        let mut v = vec![];
        if !self.pkgs.is_empty() {
            v.push(crate::tf!("не хватает пакетов: {}", "missing packages: {}", "pacotes ausentes: {}", self.pkgs.join(" ")));
        }
        if self.login.is_some() {
            v.push(if std::path::Path::new("/usr/share/sddm/themes/yami").exists() {
                crate::tf!("новый экран входа вместо SDDM (SDDM выключится, не удалится)", "the new login screen instead of SDDM (SDDM is turned off, not removed)", "a nova tela de login no lugar do SDDM (o SDDM é desativado, não removido)")
            } else {
                crate::tf!("экран входа обновился", "the login screen changed", "a tela de login mudou")
            });
        }
        v
    }
}

fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// открыть команду в терминале; после успеха файл убирается
pub fn run_in_term(n: &Need) {
    let script = format!("{} && rm -f {}; printf '\\n[Enter]'; read _", n.line(), sq(&path().to_string_lossy()));
    crate::app::App::spawn(&crate::launcher::in_term(vec!["sh".into(), "-c".into(), script]));
}

impl crate::app::App {
    /// раз в секунду, пока открыта вкладка «обновления»: пакеты поставили (терминал удалил файл)
    /// — кнопка уходит; закрыли терминал раньше — остаётся
    pub fn need_tick(&mut self) {
        if !matches!(self.st.modal, Some(crate::app::Modal::Settings)) || self.st.stab != "updates" {
            return;
        }
        let n = load();
        if n != self.st.upd.need {
            self.st.upd.need = n;
            self.dirty(&crate::app::SKind::Modal);
        }
    }
}

/// `yami update need [--print | --run]`
pub fn cli(y: &crate::cmd::Sty, args: &[&str]) -> i32 {
    let Some(n) = load() else {
        if !args.contains(&"--print") {
            println!(" {} {}", y.ok("✓"), crate::tf!("всех пакетов хватает", "no packages missing", "nenhum pacote ausente"));
        }
        return 0;
    };
    if args.contains(&"--print") {
        println!("{}", n.line());
        return 0;
    }
    if args.contains(&"--run") {
        let a = n.argv();
        let mut ok = a.is_empty() || std::process::Command::new(&a[0]).args(&a[1..]).status().map(|s| s.success()).unwrap_or(false);
        if let (true, Some(p)) = (ok, &n.login) {
            ok = std::process::Command::new(p).status().map(|s| s.success()).unwrap_or(false);
        }
        if ok {
            let _ = std::fs::remove_file(path());
            println!(" {} {}", y.ok("✓"), crate::tf!("поставлено", "installed", "instalado"));
            return 0;
        }
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("не вышло — команда выше", "failed, see the command above", "falhou; veja o comando acima"));
        return 1;
    }
    y.section(&crate::tf!("не поставлено", "not installed", "não instalado"));
    println!("   {}", n.what());
    println!("\n   {}  {}", y.bold(&n.line()), y.dim(&crate::tf!("(или: yami update need --run)", "(or: yami update need --run)", "(ou: yami update need --run)")));
    println!();
    0
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_safe_names() {
        let n = super::parse("pm pacman\npkgs wlsunset zbar ;rm -rf --no-preserve-root $(id)\n").unwrap();
        assert_eq!(n.pkgs, vec!["wlsunset", "zbar"]);
        assert!(super::parse("pm sh -c\npkgs a\n").is_none());
        assert!(super::parse("pm apt\npkgs\n").is_none());
    }
    #[test]
    fn login_line() {
        let d = std::env::temp_dir().join(format!("yami-need-{}", std::process::id()));
        let _ = std::fs::create_dir_all(d.join("tools"));
        let g = d.join("tools/greet-install.sh");
        let _ = std::fs::write(&g, "#!/bin/sh\n");
        // только экран входа: менеджер пакетов не нужен
        let n = super::parse(&format!("login {}\n", g.display())).unwrap();
        assert!(n.pkgs.is_empty() && n.login.as_deref() == Some(g.as_path()));
        assert_eq!(n.line(), format!("'{}'", g.display()));
        let n = super::parse(&format!("pm pacman\npkgs zbar\nlogin {}\n", g.display())).unwrap();
        assert!(n.line().ends_with(&format!("pacman -S --needed zbar && '{}'", g.display())));
        // чужой файл, относительный путь, нет файла — мимо
        assert!(super::parse("login /bin/sh\n").is_none());
        assert!(super::parse("login tools/greet-install.sh\n").is_none());
        assert!(super::parse("login /nowhere/tools/greet-install.sh\n").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
