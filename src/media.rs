// Музыка: `playerctl -a -F metadata` сообщает о смене трека и паузе каждого плеера, список
// живых плееров — сигналы шины (NameOwnerChanged имён org.mpris.MediaPlayer2.*), без опроса.
// Показан и управляется один (`App::media_pick_sync`): выбранный в шапке всплывашки, пока жив, иначе
// заигравший последним (#24: вкладка YouTube на паузе не заслоняет Spotify). Обложка скачивается
// один раз и пикселизуется под стиль. Кнопки — playerctl -p <плеер> play-pause / next / previous.

use crate::app::{App, Msg, SKind};
use calloop::channel::Sender;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct MediaState {
    /// имя на шине без «org.mpris.MediaPlayer2.» (firefox.instance_1_61): для `playerctl -p`
    pub inst: String,
    pub player: String,
    pub status: String,
    pub artist: String,
    pub title: String,
    pub album: String,
    pub art: String,
    pub length: f64,
    pub pos: f64,
    pub at: Instant,
    /// с каких пор играет: из играющих показан заигравший последним
    pub played: Option<Instant>,
    /// повтор: None, Playlist, Track (пусто — плеер не умеет)
    pub repeat: String,
    pub shuffle: bool,
}

impl Default for MediaState {
    fn default() -> Self {
        MediaState { inst: String::new(), player: String::new(), status: String::new(), artist: String::new(), title: String::new(), album: String::new(), art: String::new(), length: 0.0, pos: 0.0, at: Instant::now(), played: None, repeat: String::new(), shuffle: false }
    }
}

impl MediaState {
    pub fn playing(&self) -> bool {
        self.status == "Playing"
    }
    /// позиция сейчас, с; пока играет — идёт сама
    pub fn now_pos(&self) -> f64 {
        let p = if self.playing() { self.pos + self.at.elapsed().as_secs_f64() } else { self.pos };
        if self.length > 0.0 {
            p.min(self.length)
        } else {
            p
        }
    }
}

pub fn viz_wanted(on: bool) {
    crate::audio::viz_wanted(on);
}

/// команда плееру inst (play-pause, next, position 12.5…) в своей нити; пустой — тому, кого выберет playerctl
fn playerctl(inst: &str, args: Vec<String>) {
    let mut c = Command::new("playerctl");
    if !inst.is_empty() {
        c.args(["-p", inst]);
    }
    c.args(args).stdout(Stdio::null()).stderr(Stdio::null());
    std::thread::spawn(move || {
        let _ = c.status();
    });
}

pub fn ctl(inst: &str, cmd: &str) {
    playerctl(inst, vec![cmd.into()]);
}

/// повтор по кругу: нет → весь список → трек → нет
pub fn repeat_next(inst: &str, cur: &str) -> &'static str {
    let next = match cur {
        "Playlist" => "Track",
        "Track" => "None",
        _ => "Playlist",
    };
    playerctl(inst, vec!["loop".into(), next.into()]);
    next
}

pub fn shuffle_toggle(inst: &str) {
    playerctl(inst, vec!["shuffle".into(), "Toggle".into()]);
}

/// перемотать на позицию, с (плеер без CanSeek просто не послушается)
pub fn seek(inst: &str, secs: f64) {
    playerctl(inst, vec!["position".into(), format!("{:.2}", secs.max(0.0))]);
}

const MPRIS: &str = "org.mpris.MediaPlayer2.";

/// живые плееры по шине: сразу и при каждом приходе или уходе (NameOwnerChanged их имён); соединение
/// своё, нить спит на сокете — простоя не стоит. Шины нет — список ведёт сам playerctl (пустая строка)
fn watch_players(tx: Sender<Msg>) {
    use zbus::blocking::{fdo::DBusProxy, Connection, MessageIterator};
    let Ok(c) = Connection::session() else { return };
    let list = |c: &Connection| -> Vec<String> {
        DBusProxy::new(c).ok().and_then(|p| p.list_names().ok()).unwrap_or_default().iter().filter_map(|n| n.as_str().strip_prefix(MPRIS).map(String::from)).collect()
    };
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")
        .and_then(|b| b.interface("org.freedesktop.DBus"))
        .and_then(|b| b.member("NameOwnerChanged"))
        .and_then(|b| b.arg0ns(MPRIS.trim_end_matches('.')));
    let Ok(rule) = rule.map(|b| b.build()) else { return };
    let Ok(it) = MessageIterator::for_match_rule(rule, &c, Some(16)) else { return };
    let _ = tx.send(Msg::Players(list(&c)));
    for m in it {
        if m.is_err() {
            continue;
        }
        if tx.send(Msg::Players(list(&c))).is_err() {
            return;
        }
    }
}

pub fn spawn(tx: Sender<Msg>) {
    let t = tx.clone();
    std::thread::Builder::new().name("mpris".into()).spawn(move || watch_players(t)).ok();
    std::thread::Builder::new()
        .name("media".into())
        .spawn(move || loop {
            let fmt = "{{playerInstance}}\t{{playerName}}\t{{status}}\t{{artist}}\t{{title}}\t{{mpris:artUrl}}\t{{mpris:length}}\t{{position}}\t{{album}}\t{{loop}}\t{{shuffle}}";
            // -a: строки всех плееров, не только «главного» по мнению playerctl; пустая — плееров не осталось
            let Ok(mut c) = crate::proc::owned(Command::new("playerctl").args(["-a", "-F", "metadata", "--format", fmt]).stdout(Stdio::piped()).stderr(Stdio::null())).spawn() else {
                std::thread::sleep(std::time::Duration::from_secs(30));
                continue;
            };
            let Some(out) = c.stdout.take() else {
                let _ = c.kill();
                let _ = c.wait();
                continue;
            };
            for l in BufReader::new(out).lines() {
                let Ok(l) = l else { break };
                let f: Vec<&str> = l.split('\t').collect();
                let g = |i: usize| f.get(i).copied().unwrap_or("").to_string();
                let m = MediaState {
                    inst: g(0),
                    player: g(1),
                    status: g(2),
                    artist: g(3),
                    title: g(4),
                    album: g(8),
                    art: g(5),
                    length: g(6).parse::<f64>().unwrap_or(0.0) / 1e6,
                    pos: g(7).parse::<f64>().unwrap_or(0.0) / 1e6,
                    at: Instant::now(),
                    played: None,
                    repeat: g(9),
                    shuffle: g(10) == "true",
                };
                let _ = tx.send(Msg::Media(m));
            }
            let _ = c.wait();
            let _ = tx.send(Msg::Media(MediaState::default()));
            std::thread::sleep(std::time::Duration::from_secs(3));
        })
        .ok();
}

/// обложка: файл или ссылка → 64×64 клетками по 2 px
/// net: обложки по сети разрешены (агенты → «обложки из сети»)
pub fn fetch_art(url: String, net: bool, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let data = if let Some(p) = url.strip_prefix("file://") {
            // путь даёт плеер: только обычный файл и не больше потолка (не /dev/zero)
            crate::icons::uri_path(&url).or_else(|| Some(p.into())).and_then(|p| yami_px::paths::read_capped(&p, crate::icons::MAX_FILE))
        } else if net && (url.starts_with("https://") || url.starts_with("http://")) {
            // только http(s) и при переадресации, размер с потолком, без чужих конфигов curl (-q)
            let max = crate::icons::MAX_FILE.to_string();
            Command::new("curl")
                .args(["-q", "-sfL", "--max-time", "8", "--max-redirs", "3", "--proto", "=http,https", "--proto-redir", "=https", "--max-filesize", &max, "--", &url])
                .output()
                .ok()
                .map(|o| o.stdout)
                .filter(|d| !d.is_empty() && d.len() as u64 <= crate::icons::MAX_FILE)
        } else {
            None
        };
        let img = data.and_then(|d| crate::icons::decode(&d, crate::icons::MAX_DECODE)).map(|i| {
            let small = image::imageops::resize(&i.to_rgba8(), 32, 32, image::imageops::FilterType::Triangle);
            let big = image::imageops::resize(&small, 64, 64, image::imageops::FilterType::Nearest);
            crate::icons::from_rgba(&big)
        });
        let _ = tx.send(Msg::Art(url, img));
    });
}

/// новая строка плеера: «заиграл» — переход в Playing (не каждая смена трека); пауза метку не
/// стирает, иначе бар перепрыгнул бы на другой играющий и следующая play/pause ушла бы ему. true — заиграл
fn mark_played(old: Option<&MediaState>, m: &mut MediaState, now: Instant) -> bool {
    let started = m.playing() && !old.is_some_and(|o| o.playing());
    m.played = if started { Some(now) } else { old.and_then(|o| o.played) };
    started
}

/// кого показывать: выбранного руками, пока он жив; иначе того, кто заиграл последним; иначе
/// обновлённого последним
fn pick<'a>(ps: &'a [MediaState], picked: &str) -> Option<&'a MediaState> {
    ps.iter()
        .find(|p| !picked.is_empty() && p.inst == picked)
        .or_else(|| ps.iter().filter(|p| p.played.is_some()).max_by_key(|p| p.played))
        .or_else(|| ps.iter().max_by_key(|p| p.at))
}

impl App {
    /// строка плеера из playerctl: пустой inst — плееров не осталось
    pub fn media_got(&mut self, mut m: MediaState) {
        let ps = &mut self.st.players;
        if m.inst.is_empty() {
            ps.clear();
        } else {
            let old = ps.iter().position(|p| p.inst == m.inst);
            // заиграл другой плеер — ручной выбор отпускаем: показан тот, кого только что включили
            if mark_played(old.map(|i| &ps[i]), &mut m, Instant::now()) && m.inst != self.st.media_pick {
                self.st.media_pick.clear();
            }
            match old {
                Some(i) => ps[i] = m,
                None => ps.push(m),
            }
        }
        self.media_pick_sync();
    }

    /// список живых плееров с шины: ушедшие забываются (playerctl об уходе не всегда скажет)
    pub fn media_players(&mut self, live: Vec<String>) {
        self.st.players.retain(|p| live.contains(&p.inst));
        self.media_pick_sync();
    }

    /// следующий (d = 1) или прошлый (-1) плеер — выбран, пока жив (шапка всплывашки, `msg media player`)
    pub fn media_step(&mut self, d: i32) {
        let ps = &self.st.players;
        if ps.len() < 2 {
            return;
        }
        let n = ps.len() as i32;
        let i = ps.iter().position(|p| p.inst == self.st.media.inst).unwrap_or(0) as i32;
        self.st.media_pick = ps[(i + d).rem_euclid(n) as usize].inst.clone();
        self.media_pick_sync();
    }

    /// команда показанному плееру: кнопки, бар, медиаклавиши (`msg media …`)
    pub fn media_ctl(&self, cmd: &str) {
        ctl(&self.st.media.inst, cmd);
    }

    /// показать того, кого выбирает `pick` (ушедший выбранный забывается)
    pub fn media_pick_sync(&mut self) {
        let ps = &self.st.players;
        if !ps.iter().any(|p| p.inst == self.st.media_pick) {
            self.st.media_pick.clear();
        }
        let cur = pick(ps, &self.st.media_pick).cloned().unwrap_or_default();
        if !cur.art.is_empty() && !self.st.art.contains_key(&cur.art) {
            // обложки прошлых треков не копим: за долгий вечер с плеером их были бы тысячи
            if self.st.art.len() >= 8 {
                let keep = self.st.media.art.clone();
                self.st.art.retain(|k, _| *k == keep);
            }
            self.st.art.insert(cur.art.clone(), None);
            fetch_art(cur.art.clone(), self.st.set.media_art_net, self.tx.clone());
        }
        self.st.media = cur;
        self.viz_sync();
        self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Popup | SKind::Widget(_)));
    }

    /// эквалайзер где-то виден: захвату звука (parec) есть для кого работать
    pub fn media_visible(&self) -> bool {
        self.st.set.viz_placed() || crate::popups::shows_viz(&self.st)
    }

    /// захват звука для эквалайзера — пока тот где-то виден и музыка играет
    pub fn viz_sync(&self) {
        viz_wanted(self.media_visible() && self.st.media.playing());
    }

    /// эквалайзер поменялся: захват звука — если он виден
    pub fn media_sync(&mut self) {
        self.viz_sync();
        self.dirty_where(|k| matches!(k, SKind::Popup | SKind::Bar));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pl(inst: &str, status: &str) -> MediaState {
        MediaState { inst: inst.into(), status: status.into(), ..Default::default() }
    }
    // строка playerctl: как media_got, без App
    fn got(ps: &mut Vec<MediaState>, picked: &mut String, mut m: MediaState, now: Instant) {
        let old = ps.iter().position(|p| p.inst == m.inst);
        if mark_played(old.map(|i| &ps[i]), &mut m, now) && m.inst != *picked {
            picked.clear();
        }
        match old {
            Some(i) => ps[i] = m,
            None => ps.push(m),
        }
    }
    fn shown(ps: &[MediaState], picked: &str) -> String {
        pick(ps, picked).map(|p| p.inst.clone()).unwrap_or_default()
    }

    #[test]
    fn follows_the_last_to_start_and_keeps_it_paused() {
        let t = Instant::now();
        let (mut ps, mut picked) = (vec![], String::new());
        got(&mut ps, &mut picked, pl("spotify", "Playing"), t);
        got(&mut ps, &mut picked, pl("firefox", "Paused"), t + Duration::from_secs(1));
        assert_eq!(shown(&ps, &picked), "spotify", "a paused tab does not take over");
        got(&mut ps, &mut picked, pl("firefox", "Playing"), t + Duration::from_secs(2));
        assert_eq!(shown(&ps, &picked), "firefox", "the one started last");
        got(&mut ps, &mut picked, pl("firefox", "Paused"), t + Duration::from_secs(3));
        assert_eq!(shown(&ps, &picked), "firefox", "paused, it stays: play-pause resumes it");
        got(&mut ps, &mut picked, pl("spotify", "Playing"), t + Duration::from_secs(4));
        assert_eq!(shown(&ps, &picked), "firefox", "a new track of a player still playing is no start");
    }

    #[test]
    fn a_pick_lasts_until_another_starts() {
        let t = Instant::now();
        let (mut ps, mut picked) = (vec![], String::new());
        got(&mut ps, &mut picked, pl("spotify", "Playing"), t);
        got(&mut ps, &mut picked, pl("firefox", "Playing"), t + Duration::from_secs(1));
        picked = "spotify".into();
        assert_eq!(shown(&ps, &picked), "spotify");
        got(&mut ps, &mut picked, pl("firefox", "Paused"), t + Duration::from_secs(2));
        got(&mut ps, &mut picked, pl("firefox", "Playing"), t + Duration::from_secs(3));
        assert_eq!(shown(&ps, &picked), "firefox", "started again: shown");
        assert!(picked.is_empty());
        assert_eq!(shown(&[], ""), "");
    }
}
