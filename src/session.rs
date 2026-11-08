// Восстановление сессии: какие программы были открыты, на каком столе и мониторе. Шелл пишет
// снимок (~/.local/state/yami/session.json) через несколько секунд после изменений окон; при
// следующем входе запускает каждую программу один раз и возвращает её первое окно на место.
// Только в свежем сеансе (niri только что запущен), никогда после `yami restart`. Игры и окна
// самого yami не входят; пустой снимок не заменяет полный (при выходе окна закрываются по одному).
// i18n: inline pairs

use crate::app::App;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub app_id: String,
    pub output: String,
    /// номер стола на этом мониторе (с 1, как показывает niri)
    pub ws: u32,
    pub floating: bool,
}

#[derive(Default)]
pub struct Session {
    /// окна изменились: записать снимок на следующем тике
    pub dirty: bool,
    last: Vec<Entry>,
    /// запущенные восстановлением программы ждут своего первого окна
    pending: Vec<(Entry, Instant)>,
    tried: bool,
}

fn path() -> std::path::PathBuf {
    crate::config::state_dir().join("session.json")
}

fn skip(app_id: &str) -> bool {
    app_id.is_empty() || app_id.starts_with("steam_app_") || app_id == "gamescope" || app_id.starts_with("yami-")
}

/// niri запущен меньше двух минут назад: это вход, а не перезапуск шелла. Метка в runtime-папке
/// (на этот niri) — второй раз не восстанавливаем, даже если шелл упал и поднялся в эти две минуты
fn fresh() -> bool {
    let Some(sock) = std::env::var_os("NIRI_SOCKET") else { return false };
    let young = std::fs::metadata(&sock).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|e| e < Duration::from_secs(120));
    let Some(name) = std::path::Path::new(&sock).file_name() else { return false };
    let mark = crate::ipc::path().with_file_name(format!("yami-restored-{}", name.to_string_lossy()));
    young && std::fs::OpenOptions::new().write(true).create_new(true).open(mark).is_ok()
}

impl App {
    fn session_now(&self) -> Vec<Entry> {
        let n = &self.st.niri;
        let mut v: Vec<Entry> = n
            .wins
            .iter()
            .filter(|w| !skip(&w.app_id))
            .filter_map(|w| {
                let ws = n.ws.iter().find(|x| Some(x.id) == w.ws)?;
                Some(Entry { app_id: w.app_id.clone(), output: ws.output.clone(), ws: ws.idx, floating: w.floating })
            })
            .collect();
        v.sort_by(|a, b| (&a.output, a.ws, &a.app_id).cmp(&(&b.output, b.ws, &b.app_id)));
        v
    }

    /// раз в несколько секунд, из tick
    pub fn session_tick(&mut self) {
        if !self.st.set.session_restore {
            return;
        }
        // окна, которые так и не пришли
        self.st.session.pending.retain(|p| p.1.elapsed() < Duration::from_secs(90));
        if !std::mem::take(&mut self.st.session.dirty) {
            return;
        }
        let now = self.session_now();
        if now.is_empty() || now == self.st.session.last {
            return;
        }
        let _ = crate::config::write_atomic(&path(), &serde_json::to_string_pretty(&now).unwrap_or_default());
        self.st.session.last = now;
    }

    /// первый список окон этого шелла: запустить то, что было открыто в прошлый раз
    pub fn session_restore(&mut self) {
        if std::mem::replace(&mut self.st.session.tried, true) || !self.st.set.session_restore || !fresh() {
            return;
        }
        let Some(saved) = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str::<Vec<Entry>>(&t).ok()) else { return };
        self.st.session.last = saved.clone();
        let open: Vec<String> = self.st.niri.wins.iter().map(|w| w.app_id.clone()).collect();
        let mut started: Vec<String> = vec![];
        for e in saved {
            // каждая программа — один раз: браузер сам вернёт свои окна
            if skip(&e.app_id) || open.contains(&e.app_id) || started.contains(&e.app_id) {
                continue;
            }
            let Some(a) = crate::dock::app_for(&self.st.apps, &e.app_id) else { continue };
            App::spawn(&crate::icons::exec_argv(&a.exec));
            started.push(e.app_id.clone());
            self.st.session.pending.push((e, Instant::now()));
        }
    }

    /// новое окно: если его программу запустило восстановление — вернуть на место
    pub fn session_place(&mut self, id: u64) {
        let Some(w) = self.st.niri.win(id) else { return };
        let Some(i) = self.st.session.pending.iter().position(|p| p.0.app_id == w.app_id) else { return };
        let (e, _) = self.st.session.pending.remove(i);
        let n = &self.st.niri;
        if n.ws.iter().any(|x| x.output == e.output) {
            n.act(serde_json::json!({"MoveWindowToMonitor": {"id": id, "output": e.output}}));
        }
        n.act(serde_json::json!({"MoveWindowToWorkspace": {"window_id": id, "reference": {"Index": e.ws}, "focus": false}}));
        if e.floating && !w.floating {
            n.act(serde_json::json!({"ToggleWindowFloating": {"id": id}}));
        }
    }
}
