// Погода (#6): модуль бара `weather`, всплывашка с прогнозом и виджет на столе. Open-Meteo, без
// ключа: город → координаты (geocoding-api.open-meteo.com; «широта, долгота» — сразу), потом
// прогноз (api.open-meteo.com) через curl. Уходят только город (или координаты) и единицы.
// Спрашивается раз в 20 минут (после ошибки — через 5) и только пока погода в баре или на столе;
// ответ — в ~/.cache/yami/weather.json: после перезапуска шелла сначала он. Нить — на один запрос.

use crate::app::{App, Msg, SKind};
use crate::config::Settings;
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
    yami_px::paths::cache_dir().join("yami/weather.json")
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
    let o = crate::proc::owned(c.arg("-f")).output().map_err(|_| crate::tf!("нет curl", "no curl", "curl não encontrado"))?;
    if !o.status.success() {
        return Err(crate::tf!("open-meteo.com не ответил", "open-meteo.com did not answer", "open-meteo.com não respondeu"));
    }
    serde_json::from_slice(&o.stdout).map_err(|_| crate::tf!("непонятный ответ open-meteo.com", "odd answer from open-meteo.com", "resposta inesperada do open-meteo.com"))
}

/// город → координаты и имя места; «широта, долгота» — как есть
fn locate(city: &str) -> Result<(f64, f64, String), String> {
    if let Some((la, lo)) = crate::night::geo(city) {
        return Ok((la, lo, format!("{la:.2}, {lo:.2}")));
    }
    let lang = if crate::i18n::pt_br() { "pt" } else if crate::i18n::en() { "en" } else { "ru" };
    let v = get("https://geocoding-api.open-meteo.com/v1/search", &[("name", city.to_string()), ("count", "1".into()), ("language", lang.into()), ("format", "json".into())])?;
    let r = &v["results"][0];
    let (Some(la), Some(lo)) = (r["latitude"].as_f64(), r["longitude"].as_f64()) else {
        return Err(crate::tf!("город не найден: {}", "city not found: {}", "cidade não encontrada: {}", city));
    };
    Ok((la, lo, r["name"].as_str().unwrap_or(city).chars().take(40).collect()))
}

/// прогноз на месте: сейчас и пять дней
fn forecast(old: &Weather, key: &str, city: &str, fahrenheit: bool) -> Result<Weather, String> {
    // координаты того же города — из прошлого ответа, без второго запроса
    let (lat, lon, place) = if old.key == key && old.has() { (old.lat, old.lon, old.place.clone()) } else { locate(city)? };
    let mut q = vec![
        ("latitude", format!("{lat:.4}")),
        ("longitude", format!("{lon:.4}")),
        ("current", "temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,is_day,wind_speed_10m".into()),
        ("daily", "weather_code,temperature_2m_max,temperature_2m_min".into()),
        ("timezone", "auto".into()),
        ("forecast_days", "5".into()),
    ];
    if fahrenheit {
        q.push(("temperature_unit", "fahrenheit".into()));
        q.push(("wind_speed_unit", "mph".into()));
    }
    let v = get("https://api.open-meteo.com/v1/forecast", &q)?;
    let c = &v["current"];
    let f = |x: &serde_json::Value| x.as_f64().unwrap_or(0.0) as f32;
    let Some(temp) = c["temperature_2m"].as_f64() else { return Err(crate::tf!("непонятный ответ open-meteo.com", "odd answer from open-meteo.com", "resposta inesperada do open-meteo.com")) };
    let d = &v["daily"];
    let days = (0..5)
        .filter_map(|i| Some(Day { date: d["time"][i].as_str()?.to_string(), code: d["weather_code"][i].as_i64()? as i32, hi: f(&d["temperature_2m_max"][i]), lo: f(&d["temperature_2m_min"][i]) }))
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
        at: crate::clock::unix_now(),
        err: String::new(),
    })
}

impl App {
    /// погода нужна: модуль в баре или виджет на столе
    pub fn weather_wanted(&self) -> bool {
        let s = &self.st.set;
        s.on_bar("weather") || s.widgets.iter().any(|w| w.on && w.kind == "weather")
    }

    /// из tick: пора спросить — одна нить на запрос
    pub fn weather_tick(&mut self) {
        if self.st.weather_busy || !self.weather_wanted() {
            return;
        }
        let key = key(&self.st.set);
        if key.is_empty() {
            return;
        }
        let w = &self.st.weather;
        let same = w.key == key;
        let age = crate::clock::unix_now().saturating_sub(w.at);
        // свежий ответ для этого города или недавняя попытка — ждём
        if same && w.has() && age < EVERY && w.err.is_empty() {
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
                        let _ = crate::config::write_atomic(&p, &t);
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

    /// живой запрос: `cargo test -p yami-shell weather -- --ignored`
    #[test]
    #[ignore]
    fn open_meteo_live() {
        let w = forecast(&Weather::default(), "Berlin\tc", "Berlin", false).expect("open-meteo");
        // имя места — на языке шелла
        assert!(w.has() && w.days.len() == 5 && (w.place.contains("Berlin") || w.place.contains("Берлин"))); // i18n-ok
        let w = forecast(&Weather::default(), "52.52, 13.41\tf", "52.52, 13.41", true).expect("open-meteo coords");
        assert!(w.temp > -60.0 && w.temp < 140.0);
    }
}
