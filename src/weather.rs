// Погода: вкладка панели управления. Open-Meteo, без ключа: город из настроек → координаты
// (geocoding-api.open-meteo.com; «широта, долгота» — сразу), потом прогноз (api.open-meteo.com)
// через curl. Уходят только город (или координаты) и единицы. Спрашивается раз в 20 минут (после
// ошибки — через 5), пока город задан; ответ — в ~/.cache/yami-lite/weather.json: после перезапуска
// шелла сначала он. Нить — на один запрос. Во вкладке города нет — он только в настройках.

use crate::app::{App, Msg, SKind, State};
use crate::config::Settings;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Ui};
use serde::{Deserialize, Serialize};

/// как часто спрашивать, с
pub const EVERY: u64 = 20 * 60;
/// после ошибки — раньше
const RETRY: u64 = 5 * 60;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Day {
    /// ГГГГ-ММ-ДД по времени того места
    pub date: String,
    pub code: i32,
    pub hi: f32,
    pub lo: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Weather {
    /// город и единицы, для которых ответ (key)
    pub key: String,
    pub place: String,
    pub lat: f64,
    pub lon: f64,
    pub temp: f32,
    pub feels: f32,
    pub hum: f32,
    pub wind: f32,
    pub code: i32,
    pub day: bool,
    pub days: Vec<Day>,
    /// ближайшие сутки по часам: час (0–23), температура, вероятность осадков %
    pub hours: Vec<(u8, f32, f32)>,
    /// когда получен ответ, unix с; 0 — ответа ещё не было
    pub at: u64,
    /// последняя ошибка (данные тогда — прежние)
    pub err: String,
}

impl Weather {
    pub fn has(&self) -> bool {
        self.at > 0
    }
}

/// для чего спрашивать: город и единицы; пустой город — погоды нет
pub fn key(s: &Settings) -> String {
    let c = s.weather_city.trim();
    if c.is_empty() {
        String::new()
    } else {
        format!("{c}\t{}", s.weather_units)
    }
}

fn cache() -> std::path::PathBuf {
    yami_px::paths::cache_dir().join("yami-lite/weather.json")
}

/// прошлый ответ с диска (маленький файл: читается при старте)
pub fn cached() -> Weather {
    std::fs::read_to_string(cache()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// градусы без «-0°»
pub fn deg(v: f32) -> String {
    format!("{}°", v.round() as i32)
}

/// есть ответ для нынешних настроек
pub fn ready<'a>(st: &'a crate::app::State) -> Option<&'a Weather> {
    let w = &st.weather;
    (w.has() && w.key == key(&st.set) && !w.key.is_empty()).then_some(w)
}

/// значок 7×7 по коду WMO
pub fn icon(code: i32, day: bool) -> &'static str {
    match code {
        0 | 1 => {
            if day {
                "sun"
            } else {
                "moon"
            }
        }
        2 if day => "cloudsun",
        45 | 48 => "fog",
        51..=67 | 80..=82 => "rain",
        71..=77 | 85 | 86 => "snow",
        95..=99 => "storm",
        _ => "cloud",
    }
}

/// словами (переводится через PAIRS)
pub fn text(code: i32) -> &'static str {
    match code {
        0 => "ясно",
        1 => "почти ясно",
        2 => "переменная облачность",
        3 => "пасмурно",
        45 | 48 => "туман",
        51..=57 => "морось",
        61..=67 => "дождь",
        71..=77 => "снег",
        80..=82 => "ливень",
        85 | 86 => "снегопад",
        95..=99 => "гроза",
        _ => "облачно",
    }
}

/// день недели (0 — воскресенье) из ГГГГ-ММ-ДД; 1 января 1970-го — четверг
pub fn wday(date: &str) -> Option<usize> {
    let mut it = date.split('-').map(|x| x.parse::<u32>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((crate::widgets::days_from_civil(y as i32, m, d) + 4).rem_euclid(7) as usize)
}

/// подпись дня прогноза: первый — «сегодня», дальше «пн», «вт»…
pub fn day_name(i: usize, d: &Day) -> &'static str {
    if i == 0 {
        return "сегодня";
    }
    wday(&d.date).map(|k| crate::clock::RU_DAY[k]).unwrap_or("?")
}

/// погоды ещё нет: города нет — no_city, ошибка — она, иначе ответ ещё идёт
pub fn status(st: &crate::app::State, no_city: &str) -> String {
    if key(&st.set).is_empty() {
        crate::i18n::tr(no_city).to_string()
    } else if !st.weather.err.is_empty() {
        st.weather.err.clone()
    } else {
        crate::i18n::tr("спрашиваю open-meteo.com…").to_string()
    }
}

/// запрос к Open-Meteo: https и после переадресаций, 10 с, ответ ≤ 1 МБ
fn get(url: &str, q: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let mut c = crate::proc::curl_get(10, 1 << 20, url, q);
    let o = crate::proc::owned(c.arg("-f")).output().map_err(|_| crate::tf!("нет curl", "no curl"))?;
    if !o.status.success() {
        return Err(crate::tf!("open-meteo.com не ответил", "open-meteo.com did not answer"));
    }
    serde_json::from_slice(&o.stdout).map_err(|_| crate::tf!("непонятный ответ open-meteo.com", "odd answer from open-meteo.com"))
}

/// город → координаты и имя места; «широта, долгота» — как есть
fn locate(city: &str) -> Result<(f64, f64, String), String> {
    if let Some((la, lo)) = crate::night::geo(city) {
        return Ok((la, lo, format!("{la:.2}, {lo:.2}")));
    }
    let lang = if crate::i18n::en() { "en" } else { "ru" };
    let v = get("https://geocoding-api.open-meteo.com/v1/search", &[("name", city.to_string()), ("count", "1".into()), ("language", lang.into()), ("format", "json".into())])?;
    let r = &v["results"][0];
    let (Some(la), Some(lo)) = (r["latitude"].as_f64(), r["longitude"].as_f64()) else {
        return Err(crate::tf!("город не найден: {}", "city not found: {}", city));
    };
    Ok((la, lo, r["name"].as_str().unwrap_or(city).chars().take(40).collect()))
}

/// прогноз на месте: сейчас и неделя
fn forecast(old: &Weather, key: &str, city: &str, fahrenheit: bool) -> Result<Weather, String> {
    // координаты того же города — из прошлого ответа, без второго запроса
    let (lat, lon, place) = if old.key == key && old.has() { (old.lat, old.lon, old.place.clone()) } else { locate(city)? };
    let mut q = vec![
        ("latitude", format!("{lat:.4}")),
        ("longitude", format!("{lon:.4}")),
        ("current", "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,is_day,wind_speed_10m".into()),
        ("daily", "weather_code,temperature_2m_max,temperature_2m_min".into()),
        ("hourly", "temperature_2m,precipitation_probability".into()),
        ("forecast_hours", "25".into()),
        ("timezone", "auto".into()),
        ("forecast_days", "7".into()),
    ];
    if fahrenheit {
        q.push(("temperature_unit", "fahrenheit".into()));
        q.push(("wind_speed_unit", "mph".into()));
    }
    let v = get("https://api.open-meteo.com/v1/forecast", &q)?;
    let c = &v["current"];
    let f = |x: &serde_json::Value| x.as_f64().unwrap_or(0.0) as f32;
    let Some(temp) = c["temperature_2m"].as_f64() else { return Err(crate::tf!("непонятный ответ open-meteo.com", "odd answer from open-meteo.com")) };
    let d = &v["daily"];
    let days = (0..7)
        .filter_map(|i| Some(Day { date: d["time"][i].as_str()?.to_string(), code: d["weather_code"][i].as_i64()? as i32, hi: f(&d["temperature_2m_max"][i]), lo: f(&d["temperature_2m_min"][i]) }))
        .collect();
    let h = &v["hourly"];
    let hours = (0..25)
        .filter_map(|i| {
            // «2026-11-09T19:00» → 19
            let hour = h["time"][i].as_str()?.get(11..13)?.parse::<u8>().ok()?;
            Some((hour, h["temperature_2m"][i].as_f64()? as f32, h["precipitation_probability"][i].as_f64().unwrap_or(0.0) as f32))
        })
        .collect();
    Ok(Weather {
        key: key.to_string(),
        place,
        lat,
        lon,
        temp: temp as f32,
        feels: f(&c["apparent_temperature"]),
        hum: f(&c["relative_humidity_2m"]),
        wind: f(&c["wind_speed_10m"]),
        code: c["weather_code"].as_i64().unwrap_or(3) as i32,
        day: c["is_day"].as_i64().unwrap_or(1) == 1,
        days,
        hours,
        at: yami_px::time::unix_now(),
        err: String::new(),
    })
}

/// вкладка панели: сейчас крупно, справа подробности, снизу неделя карточками

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let Some(w) = ready(st) else {
        // города нет — подсказка и кнопка в настройки; иначе ошибка или «спрашиваю»
        let msg = status(st, "укажите город в настройках");
        ui.cv.icon("cloud", (b.x + b.w / 2) - 21, b.y + 56, 6, t.text_faint);
        ui.text_center(PX, R::new(b.x, b.y + 112, b.w, 20), &msg, t.text_dim);
        if key(&st.set).is_empty() {
            ui.button(R::new((b.x + b.w / 2) - 80, b.y + 142, 160, 28), "настройки", "gear", crate::ui::Tone::Plain, false, true, Act::CtlWeatherSet);
        }
        return;
    };
    let fahr = st.set.weather_units == "f";
    // сейчас: значок, градусы, словами
    ui.cv.icon(icon(w.code, w.day), b.x + 4, b.y + 10, 6, t.red);
    let big = font::px(48);
    ui.text(big, b.x + 62, b.y + 2, &deg(w.temp), t.text_hi);
    ui.text_in(PX, R::new(b.x + 4, b.y + 68, b.w / 2, 20), text(w.code), t.text);
    // справа: ощущается, влажность, ветер
    let rx = b.x + b.w / 2 + 20;
    let wind = if fahr { crate::tf!("{} mph", "{} mph", w.wind.round() as i32) } else { crate::tf!("{} м/с", "{} m/s", (w.wind / 3.6).round() as i32) };
    for (i, (ic, k, v)) in [("temp", "ощущается", deg(w.feels)), ("drop", "влажность", format!("{}%", w.hum.round() as i32)), ("fan", "ветер", wind)].into_iter().enumerate() {
        let y = b.y + 8 + i as i32 * 26;
        ui.cv.icon(ic, rx, y + 3, 2, t.text_dim);
        let kw = ui.text_in(PX, R::new(rx + 22, y, b.w / 2 - 40, 20), k, t.text_dim);
        ui.text_right(PX, R::new(rx + 30 + kw, y, b.right() - rx - 30 - kw, 20), &v, t.text);
    }
    // неделя
    let top = b.y + 96;
    let n = w.days.len().max(1) as i32;
    let cw = (b.w - (n - 1) * 6) / n;
    let ch = 96;
    for (i, d) in w.days.iter().enumerate() {
        let r = R::new(b.x + i as i32 * (cw + 6), top, cw, ch);
        let today = i == 0;
        ui.cv.pxbox(r, &BoxStyle::new(if today { t.surface_hi } else { t.surface }, if today { t.red } else { t.border }));
        ui.text_center(PX, R::new(r.x, r.y + 6, r.w, 18), day_name(i, d), if today { t.text_hi } else { t.text_dim });
        ui.cv.icon(icon(d.code, true), r.x + (r.w - 21) / 2, r.y + 30, 3, if today { t.red } else { t.text });
        let half = r.w / 2;
        ui.text_center(PX, R::new(r.x + 2, r.bottom() - 26, half, 18), &deg(d.hi), t.text_hi);
        ui.text_center(PX, R::new(r.x + half - 2, r.bottom() - 26, r.w - half, 18), &deg(d.lo), t.text_faint);
    }
    hourly(ui, w, R::new(b.x, top + ch + 10, b.w, b.bottom() - 22 - top - ch - 14));
    // когда обновлено; ошибка — поверх прежних данных
    let foot = if !w.err.is_empty() {
        w.err.clone()
    } else {
        let mins = yami_px::time::unix_now().saturating_sub(w.at) / 60;
        if mins == 0 { crate::i18n::tr("обновлено только что").to_string() } else { crate::tf!("обновлено {} мин назад", "updated {} min ago", mins) }
    };
    ui.text_in(PX, R::new(b.x, b.bottom() - 18, b.w, 18), &foot, t.text_faint);
}

/// сутки по часам: температура линией, вероятность осадков столбиками снизу, часы через 3
fn hourly(ui: &mut Ui, w: &Weather, r: R) {
    let t = ui.t;
    if w.hours.len() < 2 || r.h < 70 {
        return;
    }
    ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.border));
    let inr = R::new(r.x + 24, r.y + 24, r.w - 48, r.h - 46);
    let lo = w.hours.iter().map(|h| h.1).fold(f32::MAX, f32::min);
    let hi = w.hours.iter().map(|h| h.1).fold(f32::MIN, f32::max);
    let span = (hi - lo).max(4.0);
    // температура — общим графиком (widgets::graph, линия): он кладёт точки столбцами по 3 px
    // справа налево, поэтому сутки растягиваются на всю ширину — по точке на столбец
    const COL: i32 = 3;
    let cols = (inr.w / COL) as usize + 2;
    let last = w.hours.len() - 1;
    let temp_at = |f: f32| {
        let i = (f.floor() as usize).min(last);
        let (a, b) = (w.hours[i].1, w.hours[(i + 1).min(last)].1);
        a + (b - a) * (f - i as f32)
    };
    let data: Vec<f32> = (0..cols).map(|j| (temp_at(j as f32 / (cols - 1) as f32 * last as f32) - lo) / span).collect();
    let at = |i: usize| {
        let j = (i as f32 / last as f32 * (cols - 1) as f32).round() as i32;
        inr.right() - (cols as i32 - j) * COL + COL
    };
    let yof = |v: f32| inr.bottom() - (((v - lo) / span).clamp(0.0, 1.0) * inr.h as f32).round().max(1.0) as i32;
    // осадки — столбики у дна
    for (i, h) in w.hours.iter().enumerate() {
        if h.2 >= 10.0 {
            let ph = ((h.2 / 100.0) * 18.0).round() as i32;
            ui.cv.fill(R::new(at(i) - 2, r.bottom() - 22 - ph, 4, ph), t.accent2.alpha(0.6));
        }
    }
    crate::widgets::graph(ui, inr, &data, "line", "none", 0.0, t.red, None);
    // подписи: градусы над точкой и час под ней, через 3 часа
    for (i, h) in w.hours.iter().enumerate().step_by(3) {
        let x = at(i);
        ui.cv.fill(R::new(x - 2, yof(h.1) - 2, 4, 4), t.ember);
        ui.text_center(PX, R::new(x - 20, yof(h.1) - 20, 40, 16), &deg(h.1), t.text);
        ui.text_center(PX, R::new(x - 20, r.bottom() - 20, 40, 16), &format!("{:02}", h.0), t.text_faint);
    }
}

impl App {
    /// из tick: пора спросить — одна нить на запрос
    pub fn weather_tick(&mut self) {
        if self.st.weather_busy {
            return;
        }
        let key = key(&self.st.set);
        if key.is_empty() {
            return;
        }
        let w = &self.st.weather;
        let same = w.key == key;
        let age = yami_px::time::unix_now().saturating_sub(w.at);
        // свежий ответ для этого города или недавняя попытка — ждём
        // ответ из кеша без почасового — от прошлой версии: спросить заново
        if same && w.has() && age < EVERY && w.err.is_empty() && !w.hours.is_empty() {
            return;
        }
        let wait = if w.err.is_empty() { EVERY } else { RETRY };
        if same && self.st.weather_try.is_some_and(|t| t.elapsed().as_secs() < wait) {
            return;
        }
        self.st.weather_busy = true;
        self.st.weather_try = Some(std::time::Instant::now());
        let (old, city, fahr, tx) = (self.st.weather.clone(), self.st.set.weather_city.trim().to_string(), self.st.set.weather_units == "f", self.tx.clone());
        std::thread::spawn(move || {
            let w = match forecast(&old, &key, &city, fahr) {
                Ok(w) => {
                    if let Ok(t) = serde_json::to_string(&w) {
                        let p = cache();
                        let _ = yami_px::paths::write_atomic(&p, &t);
                    }
                    w
                }
                // ошибка: прежние данные того же города остаются, другой город — без данных
                Err(e) => Weather { err: e, key: key.clone(), ..if old.key == key { old } else { Weather::default() } },
            };
            let _ = tx.send(Msg::Weather(w));
        });
    }

    pub fn weather_got(&mut self, w: Weather) {
        self.st.weather_busy = false;
        if w.key == key(&self.st.set) {
            self.st.weather = w;
        }
        self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Popup | SKind::Widget(_)));
    }

    /// город или единицы сменились: старое не показывать, спросить сразу
    pub fn weather_reset(&mut self) {
        if self.st.weather.key != key(&self.st.set) {
            self.st.weather = Weather::default();
            self.st.weather_try = None;
        }
        self.weather_tick();
        self.dirty_where(|k| matches!(k, SKind::Bar | SKind::Popup | SKind::Widget(_)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_and_codes() {
        // 2026-11-06 — пятница, 2000-01-01 — суббота, 2024-02-29 — четверг
        assert_eq!(wday("2026-11-06"), Some(5));
        assert_eq!(wday("2000-01-01"), Some(6));
        assert_eq!(wday("2024-02-29"), Some(4));
        assert_eq!(wday("bad"), None);
        assert_eq!(icon(0, false), "moon");
        assert_eq!(icon(63, true), "rain");
        assert_eq!(icon(95, true), "storm");
        assert_eq!(text(75), "снег");
    }

    /// живой запрос: `cargo test weather -- --ignored`
    #[test]
    #[ignore]
    fn open_meteo_live() {
        let w = forecast(&Weather::default(), "Berlin\tc", "Berlin", false).expect("open-meteo");
        // имя места — на языке шелла
        assert!(w.has() && w.days.len() == 7 && (w.place.contains("Berlin") || w.place.contains("Берлин")));
        let w = forecast(&Weather::default(), "52.52, 13.41\tf", "52.52, 13.41", true).expect("open-meteo coords");
        assert!(w.temp > -60.0 && w.temp < 140.0);
    }
}
