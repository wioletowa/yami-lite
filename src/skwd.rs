// skwd-wall (github.com/liixini/skwd-wall): выбор обоев и движок к ним (картинки, видео, сцены
// Wallpaper Engine) со своей палитрой. Шелл ничего не меняет в его настройках, а читает то, что
// skwd-wall пишет сам: ~/.cache/skwd-wall-v2/last-wallpaper.json — что выбрано (тип, путь,
// кадр-превью), colors.json — текущая палитра (primary — акцент шелла, фон — светлая или тёмная
// тема). Рисует ли обои skwd-paper, видно по его слою у niri: на таком мониторе свои обои шелл
// прячет. Слоя нет (режим «только выбор», skwd-paper не запущен) — картинку рисует шелл, в своих
// клетках и тонах (у видео и сцен — кадр-превью). Та же картинка — на экране блокировки и входа.
// i18n: inline pairs

use crate::app::{App, Msg, SKind, State};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

#[derive(Default)]
pub struct Skwd {
    /// время изменения last-wallpaper.json и colors.json при последнем чтении
    stamp: (Option<SystemTime>, Option<SystemTime>),
    /// что выбрано: картинка для шелла (сама картинка или кадр-превью) и тип: static | video | we
    pub pick: Option<(PathBuf, String)>,
    /// мониторы, на которых обои рисует skwd-paper (слои niri)
    pub drawn: Vec<String>,
    /// палитра skwd-wall: акцент и светлая ли тема
    pub colors: Option<(u32, bool)>,
    /// где лежит colors.json (paths.cache в его config.json) — ищется раз в полминуты
    colors_at: Option<(PathBuf, Instant)>,
    /// слои niri спрашиваются в нити; не чаще раза в 3 с
    layers_at: Option<Instant>,
}

/// папка, куда skwd-walld пишет last-wallpaper.json (всегда $XDG_CACHE_HOME/skwd-wall-v2)
fn cache() -> PathBuf {
    yami_px::paths::cache_dir().join("skwd-wall-v2")
}

fn config_dir() -> PathBuf {
    std::env::var_os("SKWD_WALL_V2_CONFIG").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| crate::config::config_dir().join("skwd-wall-v2"))
}

/// папка палитры: SKWD_WALL_V2_CACHE, иначе paths.cache из его config.json, иначе как у last-wallpaper
fn colors_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SKWD_WALL_V2_CACHE").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    let set: Option<String> = std::fs::read_to_string(config_dir().join("config.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.pointer("/paths/cache").and_then(Value::as_str).map(str::to_string))
        .filter(|s| !s.trim().is_empty());
    match set {
        Some(s) => {
            let s = s.trim();
            let p = yami_px::paths::expand(s);
            if p.is_absolute() {
                p
            } else {
                cache()
            }
        }
        None => cache(),
    }
}

/// skwd-wall есть на этой машине: его программы или его папки
pub fn installed() -> bool {
    let w = crate::doctor::which;
    w("skwd-wall-v2") || w("skwd-walld") || w("skwd-paper-v2") || cache().is_dir() || config_dir().is_dir()
}

impl State {
    /// обои сейчас ведёт skwd-wall (его выбор есть и он включён)
    pub fn skwd_drives(&self) -> bool {
        self.set.wall_skwd && self.skwd.pick.is_some()
    }
}

fn decodable(p: &str) -> bool {
    crate::wall::decodable(Path::new(p))
}

/// last-wallpaper.json → картинка для шелла и тип. Картинка — сама; видео, сцена и форматы, которых
/// шелл не читает, — кадр-превью, что skwd-wall сохраняет рядом
pub fn parse_pick(text: &str) -> Option<(PathBuf, String)> {
    let v: Value = serde_json::from_str(text).ok()?;
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("static").to_string();
    let path = v.get("path").and_then(Value::as_str).unwrap_or("");
    let thumb = v.get("thumb").and_then(Value::as_str).unwrap_or("");
    let img = if kind == "static" && decodable(path) {
        path
    } else if decodable(thumb) {
        thumb
    } else {
        return None;
    };
    let p = PathBuf::from(img);
    (p.is_absolute() && p.is_file()).then_some((p, kind))
}

/// "#rrggbb" или "#rrggbbaa" → 0xrrggbb
fn hex(v: &Value, k: &str) -> Option<u32> {
    let s = v.get(k)?.as_str()?.trim();
    let s = if s.len() == 9 { &s[..7] } else { s };
    crate::theme::parse_hex(s)
}

/// colors.json → акцент (primary) и светлая ли тема (по яркости фона)
pub fn parse_colors(text: &str) -> Option<(u32, bool)> {
    let v: Value = serde_json::from_str(text).ok()?;
    let accent = hex(&v, "primary")?;
    let light = hex(&v, "background").or_else(|| hex(&v, "surface")).map(|c| {
        let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
        (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0 > 0.5
    });
    Some((accent, light.unwrap_or(false)))
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// слои niri → мониторы, где обои рисует skwd-wall: картинки — skwd-paper (skwd-wall-still,
/// «skwd-paper-stationary»), видео и сцены Wallpaper Engine — skwd-wall-vk (демон не задаёт ему
/// SKWD_PAPER_NAMESPACE), или awww, если skwd-wall настроен на него. Фон в обзоре
/// (skwd-paper-backdrop) и окно самого выбора (skwd-wall-v2.<uid>) — не обои стола
pub fn drawn_on(layers: &Value) -> Vec<String> {
    let mut v: Vec<String> = layers
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|l| {
                    let ns = l.get("namespace").and_then(Value::as_str).unwrap_or("");
                    (ns.starts_with("skwd-paper") || ns.starts_with("skwd-wall-vk") || ns.starts_with("awww")) && !ns.ends_with("-backdrop")
                })
                .filter_map(|l| l.get("output").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v.dedup();
    v
}

/// часть yami-fx.kdl: фон skwd-paper в обзоре niri — под столами, как советует skwd-wall
pub fn kdl(set: &crate::config::Settings, l: &mut Vec<String>) {
    if !set.wall_skwd {
        return;
    }
    l.push("// skwd-wall draws the wallpaper (settings → wallpaper): its overview backdrop".into());
    l.push("layer-rule {".into());
    l.push("    match namespace=\"^skwd-paper-backdrop$\"".into());
    l.push("    place-within-backdrop true".into());
    l.push("}".into());
    l.push(String::new());
}

impl App {
    /// раз в секунду из tick: выбрали другие обои, сменилась палитра, skwd-paper запустился или
    /// закрылся. Только чтение двух файлов (stat) — дёшево; слои niri — в нити
    pub fn skwd_tick(&mut self) {
        let s = &self.st.set;
        if !s.wall_skwd {
            if self.st.skwd.pick.is_some() || !self.st.skwd.drawn.is_empty() {
                self.st.skwd = Skwd::default();
                self.ensure_output_surfaces();
                self.sync_wall();
            }
            return;
        }
        let k = &mut self.st.skwd;
        let fresh = |at: &Option<(PathBuf, Instant)>| at.as_ref().is_some_and(|(p, t)| t.elapsed().as_secs() < 30 && p.is_file());
        if !fresh(&k.colors_at) {
            k.colors_at = Some((colors_dir().join("colors.json"), Instant::now()));
        }
        let wp = cache().join("last-wallpaper.json");
        let cp = k.colors_at.as_ref().map(|c| c.0.clone()).unwrap_or_default();
        let stamp = (mtime(&wp), mtime(&cp));
        if stamp.0 != k.stamp.0 {
            k.stamp.0 = stamp.0;
            let pick = std::fs::read_to_string(&wp).ok().and_then(|t| parse_pick(&t));
            if pick != k.pick {
                k.pick = pick;
                self.sync_wall();
                self.dirty(&SKind::Modal);
            }
        }
        let k = &mut self.st.skwd;
        if stamp.1 != k.stamp.1 {
            k.stamp.1 = stamp.1;
            k.colors = std::fs::read_to_string(&cp).ok().and_then(|t| parse_colors(&t));
            self.skwd_colors();
        }
        // кто рисует обои: спросить niri в нити, ответ — Msg::SkwdLayers
        let k = &mut self.st.skwd;
        if k.layers_at.is_none_or(|t| t.elapsed().as_secs() >= 3) {
            k.layers_at = Some(Instant::now());
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                if let Some(v) = crate::niri::request(&serde_json::json!("Layers")) {
                    let _ = tx.send(Msg::SkwdLayers(drawn_on(&v["Ok"]["Layers"])));
                }
            });
        }
    }

    /// ответ niri: на каких мониторах обои рисует skwd-paper — там свои обои убрать, на остальных
    /// вернуть
    pub fn skwd_layers(&mut self, drawn: Vec<String>) {
        if !self.st.set.wall_skwd || drawn == self.st.skwd.drawn {
            return;
        }
        self.st.skwd.drawn = drawn;
        self.ensure_output_surfaces();
    }

    /// палитра skwd-wall → акцент и тема шелла (всё остальное перекрашивается само)
    pub fn skwd_colors(&mut self) {
        let s = &mut self.st.set;
        let Some((a, light)) = self.st.skwd.colors.filter(|_| s.wall_skwd && s.wall_skwd_colors) else { return };
        let hex = format!("#{a:06x}");
        if hex != s.accent || light != s.light {
            s.accent = hex.clone();
            s.light = light;
            self.st.hex.set(&hex);
            self.settings_changed();
        }
    }

    /// свои обои на этом мониторе не нужны: их рисует skwd-paper
    pub fn skwd_draws(&self, out: &str) -> bool {
        self.st.set.wall_skwd && self.st.skwd.drawn.iter().any(|o| o == out)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn picks_an_image_or_its_frame() {
        let d = std::env::temp_dir().join(format!("yami-skwd-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (img, th) = (d.join("a.png"), d.join("t.jpg"));
        std::fs::write(&img, b"x").unwrap();
        std::fs::write(&th, b"x").unwrap();
        let p = super::parse_pick(&json!({"type": "static", "path": img, "thumb": th}).to_string()).unwrap();
        assert_eq!(p.0, img);
        let p = super::parse_pick(&json!({"type": "video", "path": d.join("v.mp4"), "thumb": th}).to_string()).unwrap();
        assert_eq!((p.0, p.1.as_str()), (th.clone(), "video"));
        // gif шелл не читает — кадр-превью; ничего годного — None
        assert_eq!(super::parse_pick(&json!({"type": "static", "path": d.join("a.gif"), "thumb": th}).to_string()).unwrap().0, th);
        assert!(super::parse_pick(&json!({"type": "we", "path": "", "thumb": ""}).to_string()).is_none());
        assert!(super::parse_pick(&json!({"type": "static", "path": "rel.png"}).to_string()).is_none());
        assert!(super::parse_pick("not json").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn colors_and_mode() {
        assert_eq!(super::parse_colors(r##"{"primary":"#ffb4a8","background":"#1a1110"}"##), Some((0xffb4a8, false)));
        assert_eq!(super::parse_colors(r##"{"primary":"#8f4a3f","surface":"#fff8f6"}"##), Some((0x8f4a3f, true)));
        assert_eq!(super::parse_colors(r##"{"primary":"#8f4a3fff"}"##), Some((0x8f4a3f, false)));
        assert!(super::parse_colors(r##"{"primary":"red"}"##).is_none());
    }

    #[test]
    fn who_draws() {
        let l = json!([
            {"namespace": "yami-wall", "output": "DP-1", "layer": "Background"},
            {"namespace": "skwd-paper", "output": "DP-2", "layer": "Background"},
            {"namespace": "skwd-paper-backdrop", "output": "DP-1", "layer": "Background"},
            {"namespace": "skwd-paper", "output": "DP-2", "layer": "Bottom"},
            // видео и сцены: свой рендер, своё имя слоя
            {"namespace": "skwd-wall-vk", "output": "HDMI-A-1", "layer": "Background"},
            // окно выбора skwd-wall — не обои
            {"namespace": "skwd-wall-v2.1000", "output": "DP-3", "layer": "Overlay"},
        ]);
        assert_eq!(super::drawn_on(&l), vec!["DP-2".to_string(), "HDMI-A-1".to_string()]);
    }
}
