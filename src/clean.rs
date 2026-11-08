// `yami clean`: кеш сборки — исходники yami-niri и target шелла. Это гигабайты, которые нужны
// только следующей сборке: установленные бинарники и форк им не нужны, `yami update` соберёт заново.
// Размер показывает и `yami doctor`.

use crate::cmd::Sty;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// что считается кешем сборки: (подпись, путь, это дерево yami-niri)
fn targets() -> Vec<(String, PathBuf, bool)> {
    let base = yami_px::paths::cache_dir().join("yami");
    let src = std::env::var_os("YAMI_NIRI_SRC").map(PathBuf::from).unwrap_or_else(|| base.join("niri-src"));
    vec![(crate::tf!("исходники yami-niri", "yami-niri sources", "código-fonte do yami-niri"), src, true), (crate::tf!("сборка шелла", "shell build", "compilação do shell"), base.join("target"), false)]
}

/// занято на диске (блоки, как у du); ссылки не разворачиваются
pub fn size(p: &Path) -> u64 {
    let (mut total, mut stack) = (0u64, vec![p.to_path_buf()]);
    while let Some(d) = stack.pop() {
        let Ok(m) = std::fs::symlink_metadata(&d) else { continue };
        total += m.blocks() * 512;
        if m.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&d) {
                stack.extend(rd.flatten().map(|e| e.path()));
            }
        }
    }
    total
}

pub fn human(b: u64) -> String {
    if b >= 1 << 30 {
        crate::tf!("{:.1} ГБ", "{:.1} GB", "{:.1} GB", b as f64 / (1u64 << 30) as f64)
    } else {
        crate::tf!("{:.0} МБ", "{:.0} MB", "{:.0} MB", b as f64 / (1u64 << 20) as f64)
    }
}

/// весь кеш сборки: что есть и сколько занимает (для doctor)
pub fn total() -> u64 {
    targets().iter().filter(|t| t.1.exists()).map(|t| size(&t.1)).sum()
}

/// папка исходников форка: в ней только yami-niri и yami-smithay. YAMI_NIRI_SRC, указавший по ошибке
/// на дом или другую папку, не удаляется целиком
fn niri_src(p: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(p) else { return false };
    let names: Vec<_> = rd.flatten().map(|e| e.file_name()).collect();
    names.iter().all(|n| n == "yami-niri" || n == "yami-smithay")
}

/// дерево yami-niri с работой над патчами: правки или коммиты не из серии (их делает не yami@localhost)
fn dev_tree(src: &Path) -> bool {
    let git = |d: &Path, a: &[&str]| Command::new("git").arg("-C").arg(d).args(a).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
    ["yami-niri", "yami-smithay"].iter().map(|n| src.join(n)).filter(|d| d.join(".git").exists()).any(|d| {
        !git(&d, &["status", "--porcelain"]).trim().is_empty() || git(&d, &["log", "--format=%ce", "HEAD", "--not", "--remotes"]).lines().any(|l| l != "yami@localhost")
    })
}

pub fn run(y: &Sty, a: &[&str]) -> i32 {
    let (dry, force) = (a.contains(&"--dry-run") || a.contains(&"-n"), a.contains(&"--force"));
    if let Some(bad) = a.iter().find(|x| !["--dry-run", "-n", "--force"].contains(x)) {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("не понял: {} (yami clean [--dry-run] [--force])", "unknown option: {} (yami clean [--dry-run] [--force])", "opção desconhecida: {} (yami clean [--dry-run] [--force])", bad));
        return 2;
    }
    // обновление собирает прямо в этих папках
    let Some(_lock) = crate::update::lock() else {
        eprintln!(" {} {}", y.bad("✕"), crate::tf!("обновление уже идёт", "an update is already running", "uma atualização já está em andamento"));
        return 1;
    };
    y.section(&crate::tf!("кеш сборки", "build cache", "cache de compilação"));
    let (mut freed, mut found) = (0u64, false);
    for (what, p, niri) in targets() {
        if !p.exists() {
            continue;
        }
        found = true;
        let sz = size(&p);
        let line = |mark: String, note: &str| println!("   {mark} {:<22}{:>9}  {}", what, human(sz), y.dim(note));
        if niri && !niri_src(&p) {
            line(y.bad("✕"), &crate::tf!("это не папка исходников yami-niri (YAMI_NIRI_SRC?): не трогаю", "not a yami-niri source folder (YAMI_NIRI_SRC?): left alone", "não é uma pasta de código-fonte do yami-niri (YAMI_NIRI_SRC?): preservada"));
        } else if niri && !force && dev_tree(&p) {
            line(y.warn("·"), &crate::tf!("в дереве твоя работа над патчами: оставлено (--force — всё равно)", "your patch work is in the tree: kept (--force to remove anyway)", "seus patches estão na árvore: preservados (--force para remover mesmo assim)"));
        } else if dry {
            line(y.dim("·"), &crate::tf!("будет удалено", "would be removed", "seria removido"));
        } else {
            match std::fs::remove_dir_all(&p) {
                Ok(()) => {
                    freed += sz;
                    line(y.ok("✓"), &crate::tf!("удалено", "removed", "removido"));
                }
                Err(e) => line(y.bad("✕"), &e.to_string()),
            }
        }
    }
    if !found {
        println!("   {} {}", y.ok("✓"), crate::tf!("кеша нет, чистить нечего", "no cache, nothing to clean", "sem cache, nada para limpar"));
    } else if !dry && freed > 0 {
        println!("\n {} {}", y.ok("✓"), crate::tf!("освобождено {}; следующее обновление соберёт заново (yami-niri уже стоит и этим не затронут)", "freed {}; the next update rebuilds (the installed yami-niri is untouched)", "{} liberados; a próxima atualização recompila (o yami-niri instalado é preservado)", human(freed)));
    }
    println!();
    0
}
