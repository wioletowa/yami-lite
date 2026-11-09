// Настройки шелла: ~/.config/yami-lite/settings.json. Меняются в панели настроек,
// перетаскиванием виджетов и руками — файл перечитывается на лету.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use yami_px::paths::{config_dir, data_dir, home, state_dir, write_atomic};

pub fn settings_path() -> PathBuf {
    config_dir().join("yami-lite/settings.json")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Widget {
    pub id: u32,
    /// clock | graph | net | media | cal | note
    pub kind: String,
    pub on: bool,
    pub x: i32,
    pub y: i32,
    /// graph: cpu | ram | temp | gpu | swap
    pub source: String,
    /// bars | line | blocks | wave | dots
    pub style: String,
    /// scroll | rise | flicker | none
    pub anim: String,
    /// s | m | l
    pub size: String,
    /// accent | bright | ok | warn | text
    pub color: String,
    /// подложка-панель или прямо по обоям
    pub frame: bool,
    pub label: bool,
    /// часы: секунды, дата; заметка: текст
    pub seconds: bool,
    pub date: bool,
    pub text: String,
}

impl Default for Widget {
    fn default() -> Self {
        Widget {
            id: 0,
            kind: "clock".into(),
            on: true,
            x: -1,
            y: -1,
            source: "cpu".into(),
            style: "bars".into(),
            anim: "scroll".into(),
            size: "m".into(),
            color: "accent".into(),
            frame: true,
            label: true,
            seconds: true,
            date: true,
            text: String::new(),
        }
    }
}

impl Widget {
    pub fn new(id: u32, kind: &str) -> Widget {
        let mut w = Widget { id, kind: kind.into(), ..Default::default() };
        if kind == "clock" {
            w.frame = false;
        }
        w
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// язык интерфейса: "" — по языку системы, "ru", "en"
    pub lang: String,
    // цвет: пресет или свой акцент (#rrggbb, пусто — пресет)
    pub palette: String,
    pub accent: String,
    pub tint: f32,
    /// светлая тема (бумага вместо графита) для шелла и всего, что красит theme.py
    pub light: bool,

    // вид
    pub glass: f32,
    /// яркость панелей: 0 — графит почти чёрный, 1 — заметно светлее
    pub lift: f32,
    /// плотность карточек, графиков и полей внутри панелей
    pub inner: f32,
    /// затемнение стола под меню и настройками
    pub dim: f32,
    pub blur_shell: bool,
    pub animations: bool,
    /// множитель длительности анимаций: 0.5 — быстрее, 2 — медленнее
    pub anim_speed: f32,
    /// узор проявления: dither | scan | noise | wipe | blocks (yami-px gfx::Reveal)
    pub reveal: String,
    /// крупность клеток проявления, 1–4
    pub reveal_grain: i32,
    /// закрывать панели с анимацией (иначе — сразу)
    pub anim_close: bool,
    /// строки развёртки поверх панелей шелла
    pub scanlines: bool,
    /// значки программ пиксельными: чистая сетка 16–30 клеток (yami-px pixelate)
    pub icon_pixel: bool,
    /// пиксельные значки — в 4 тонах палитры, а не в своих цветах
    pub icon_tint: bool,
    /// свои обои (путь); пусто — пиксельные обои из палитры
    pub wallpaper: String,
    /// папка выбора обоев (walls.rs); пусто — ~/Pictures/Wallpapers, папка текущих обоев или ~/Pictures
    pub wall_dir: String,
    /// где выбор обоев был открыт в последний раз: туда он и открывается (и после «из палитры»)
    pub wall_last_dir: String,
    /// порядок картинок в выборе обоев: new — сначала новые, name — по имени
    pub walls_sort: String,
    /// Wallhaven: ключ API (NSFW и больше запросов) и куда качать ("" — папка обоев)
    pub wallhaven_key: String,
    pub wallhaven_dir: String,
    /// смена обоев: узор (wall::FADES; none — сразу), сколько идёт, крупность клеток (точек экрана)
    pub wall_fade: String,
    pub wall_fade_time: f32,
    pub wall_fade_cell: i32,
    /// пикселизация своих обоев: 1 — как есть, дальше клетки крупнее (wall::cell)
    pub wall_pixel: i32,
    /// "fill" — заполнить экран (лишнее обрезать), "fit" — вписать целиком, поля цвета фона
    pub wall_fit: String,
    /// окраска своих обоев: "" — как есть, "tint" — оттенок акцента, "palette" — тона палитры с дизерингом
    pub wall_tone: String,
    /// затемнение обоев к фону темы: 0 — нет, 0.8 — почти фон
    pub wall_dim: f32,
    /// акцент (а с ним вся палитра) берётся из своих обоев
    pub wall_accent: bool,
    /// какой по весу цвет обоев брать акцентом (0 — главный); на новых обоях — тот же номер
    pub wall_pick: u32,
    /// второй акцент из обоев (#rrggbb); пусто — напротив основного
    pub accent2: String,

    // бар: модули по сторонам и их порядок
    pub bar_left: Vec<String>,
    pub bar_center: Vec<String>,
    pub bar_right: Vec<String>,
    /// модули, что стоят в бар-листах, но скрыты (глаз во вкладке «бар»)
    pub bar_hidden: Vec<String>,
    /// gauge | graph | number
    pub cpu_style: String,
    /// модуль fan: gauge | graph | number — как цп и память (процент работы), rpm — значок и обороты
    pub fan_style: String,
    /// столы в баре: boxes — квадраты с номером, dots — точки, активный — квадрат
    pub ws_style: String,
    /// модуль temp: "full" — как цп и память (подпись, шкала или график), "compact" — столбик и число
    pub temp_style: String,
    /// трей: "icons" — значки в баре, "arrow" — одна стрелка, значки во всплывашке
    pub tray_mode: String,
    /// значки трея клетками в тонах палитры, как пиксельные значки дока и лаунчера (#19)
    pub tray_pixel: bool,
    /// трей «смешанный»: эти программы (по Id или процессу) — значками в баре, остальные под стрелкой
    pub tray_pinned: Vec<String>,
    /// модуль bat прячется от сети без зарядки: полная или держит порог заряда (#21)
    pub bat_hide_full: bool,
    /// чужие программы в цветах палитры (recolor::APPS; настройки → цвет → программы); по умолчанию
    /// ни одной: выключенную шелл не трогает
    pub theme_apps: Vec<String>,
    pub clock_seconds: bool,
    pub clock_date: bool,
    pub task_width: i32,
    /// разделитель-модуль "sep": dots | line | blocks | gap
    pub bar_sep: String,
    /// ширина места под разделитель, px
    pub bar_sep_w: i32,
    /// border | accent | dim
    pub bar_sep_color: String,
    /// промежуток между модулями, px
    pub bar_gap: i32,
    /// от левого и правого края бара до крайних модулей, px
    pub bar_pad_l: i32,
    pub bar_pad_r: i32,
    /// от модулей по бокам до списка окон (и модулей середины), px
    pub bar_mid: i32,
    /// между кнопками окон, px
    pub bar_task_gap: i32,
    /// версия раскладки бара: 0 — разделители ещё ставились сами по группам (переносим)
    #[serde(default)]
    pub bar_v: u32,
    /// бар отдельной плашкой: отступ от краёв экрана, рамка со всех сторон, тень
    pub bar_float: bool,
    /// ширина бара в процентах ширины монитора (100 — во всю ширину; меньше — по центру)
    pub bar_width_pc: i32,
    /// отступ плавающего бара от краёв экрана, px
    pub bar_margin: i32,
    /// экраны с баром (имена niri: DP-2, eDP-1); пусто — все. Нет ни одного из них — тоже все (#8)
    pub bar_outputs: Vec<String>,
    /// модули середины: "center" — по центру бара, "after-left" — сразу за левыми
    pub bar_center_align: String,
    /// край экрана с баром: "top" или "bottom"
    pub bar_pos: String,


    // система
    pub lock_after: u32,
    pub screen_off_after: u32,
    pub dnd: bool,
    /// время карточки: программа может попросить своё (notifAppTimeout), срочные висят до закрытия
    /// (notifCriticalStay), под курсором таймер стоит (notifHoverPause)
    pub notif_app_timeout: bool,
    pub notif_critical_stay: bool,
    pub notif_hover_pause: bool,
    /// пока показывается экран (демонстрация, запись) — уведомления молча в историю
    pub cast_dnd: bool,
    pub notif_timeout: u32,
    /// где карточки уведомлений: top-right | top-left | top | bottom-right | bottom-left | bottom
    pub notif_pos: String,
    /// где OSD громкости и яркости: bottom | top | center
    pub osd_pos: String,
    /// на что OSD отвечает (OSD_KINDS): громкость, микрофон, яркость, раскладка, Caps, Num
    pub osd_kinds: Vec<String>,
    /// сколько OSD висит, секунд
    pub osd_time: f32,
    /// отступ карточек уведомлений от края экрана (от бара, если он у того края), px (#8)
    pub notif_offset: i32,
    /// карточки: от боковой кромки экрана (в углах)
    pub notif_offset_x: i32,
    /// отступ OSD от края экрана (сверху — от низа бара), px; по центру не нужен (#8)
    pub osd_offset: i32,



    // док
    pub dock: bool,
    /// bottom | top | left | right
    pub dock_side: String,
    /// never | smart | always
    pub dock_hide: String,
    pub dock_size: i32,
    /// анимация значков при наведении: grow | wave | lift | none
    pub dock_anim: String,
    /// прыжок значка при запуске
    pub dock_bounce: bool,
    pub dock_labels: bool,
    pub dock_running: bool,
    pub dock_launcher: bool,
    /// кнопка лаунчера в баре и доке запускает эту команду (fuzzel, rofi -show drun…); "" — кнопки нет
    pub launcher_cmd: String,
    /// плитки панели управления (control::TILES), от 1 до 6
    pub control_tiles: Vec<String>,
    pub dock_trash: bool,
    pub dock_pins: Vec<String>,

    // шрифты (вкладка «шрифты»): семейства fontconfig, "" — по умолчанию
    /// шелл: "" — Tiny5 (пиксельный)
    pub font_shell: String,
    /// виджеты рабочего стола: "" — как у шелла, иначе семейство
    pub font_widgets: String,

    // виджеты рабочего стола
    pub widgets: Vec<Widget>,
    /// виджеты без подложки — в тон обоям под ними: на светлом месте тёмный текст, на тёмном
    /// светлый, и жёсткая тень у букв (widgets.rs)
    pub widget_contrast: bool,


    pub game_dnd: bool,
    /// ночной свет (wlsunset): температура, координаты «lat, lon» или часы вкл/выкл
    pub night: bool,
    pub night_temp: i32,
    pub night_geo: String,
    /// погода во вкладке панели: город или «широта, долгота»; единицы c или f
    pub weather_city: String,
    /// bluetooth: устройства с автоподключением («dev_AA_BB_…», без адаптера) — шелл подключает их сам
    pub bt_auto: Vec<String>,
    /// стиль большого эквалайзера во вкладке музыки панели (ЛКМ по нему — следующий)
    pub ctl_viz: String,
    pub weather_units: String,
    pub night_on_h: i32,
    pub night_off_h: i32,
    /// свои app-id игр: через запятую, `name*` — по началу
    pub game_apps: String,

    // службы (agents.rs): что из D-Bus делает сам шелл; выключенное отдаётся другим программам
    /// сервер уведомлений org.freedesktop.Notifications
    pub agent_notif: bool,
    /// трей (StatusNotifierWatcher)
    pub agent_tray: bool,
    /// карточек на экране сразу и сколько хранит история
    pub notif_max: u32,
    pub notif_history: u32,
    /// картинки в уведомлениях (значки, превью от программ)
    pub notif_images: bool,
    /// обложки музыки по ссылкам из сети (иначе — только локальные файлы)
    pub media_art_net: bool,
    /// эквалайзер всплывашки музыки и модуля `media`: стиль виджета (bars, line…) или "off" (#13)
    pub media_viz: String,
    /// свои имена звуковых устройств: имя PipeWire (`pactl list sinks`) → как показывать
    pub audio_names: std::collections::BTreeMap<String, String>,

}

/// группа модуля — только для переноса старых настроек (migrate_seps)
fn group(m: &str) -> i32 {
    match m {
        "launcher" => 0,
        "workspaces" => 1,
        "tray" => 3,
        "net" | "netspeed" | "bt" | "kb" | "cpu" | "ram" | "temp" | "bat" | "night" | "walls" | "wallhaven" | "fan" | "privacy_mic" | "privacy_screen" | "privacy_cam" => 4,
        "vol" | "media" => 5,
        "clock" | "notif" | "settings" | "power" => 6,
        _ => 7,
    }
}

pub const ALL_MODULES: &[(&str, &str)] = &[
    ("launcher", "лаунчер"),
    ("workspaces", "столы"),
    ("tasks", "окна"),
    ("media", "музыка"),
    ("tray", "трей"),
    ("net", "сеть"),
    ("netspeed", "скорость сети"),
    ("bt", "bluetooth"),
    ("kb", "раскладка"),
    ("cpu", "процессор"),
    ("ram", "память"),
    ("temp", "температура"),
    ("gpu", "видеокарта"),
    ("disk", "диск"),
    ("fan", "вентилятор"),
    ("vol", "звук"),
    ("bat", "батарея"),
    ("night", "ночной свет"),
    ("walls", "обои"),
    ("wallhaven", "Wallhaven"),
    ("privacy_mic", "микрофон занят"),
    ("privacy_screen", "показ экрана"),
    ("privacy_cam", "камера занята"),
    ("clock", "часы"),
    ("notif", "уведомления"),
    ("settings", "настройки"),
    ("power", "питание"),
    ("sep", "разделитель"),
];

/// ключи прошлых версий: `config check` не зовёт их незнакомыми
const RETIRED: &[&str] = &[];


/// числа в пикселях, что меняются стрелками в настройках: ключ, от, до, шаг
pub const NUMS: &[(&str, i32, i32, i32)] = &[
    ("bar_pad_l", 0, 64, 2),
    ("bar_pad_r", 0, 64, 2),
    ("bar_gap", 0, 32, 2),
    ("bar_sep_w", 2, 32, 2),
    ("bar_mid", 0, 64, 2),
    ("bar_task_gap", 0, 16, 2),
    ("task_width", 60, 400, 10),
    ("bar_width_pc", 30, 100, 5),
    ("bar_margin", 0, 32, 2),
    ("reveal_grain", 1, 4, 1),
    ("night_temp", 2500, 6000, 250),
    ("night_on_h", 0, 23, 1),
    ("night_off_h", 0, 23, 1),
    ("notif_offset", 0, 400, 2),
    ("notif_offset_x", 0, 800, 2),
    ("osd_offset", 0, 400, 4),
    ("wall_fade_cell", 2, 48, 2),
];

impl Default for Settings {
    fn default() -> Self {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        Settings {
            lang: String::new(),
            palette: "blood".into(),
            accent: String::new(),
            tint: 1.0,
            light: false,
            glass: 0.78,
            lift: 0.0,
            inner: 1.0,
            dim: 0.5,
            blur_shell: true,
            animations: true,
            anim_speed: 1.0,
            reveal: "dither".into(),
            reveal_grain: 1,
            anim_close: true,
            scanlines: false,
            icon_pixel: true,
            icon_tint: false,
            wallpaper: String::new(),
            wall_dir: String::new(),
            wall_last_dir: String::new(),
            walls_sort: "new".into(),
            wallhaven_key: String::new(),
            wallhaven_dir: String::new(),
            wall_fade: "dither".into(),
            wall_fade_time: 1.2,
            wall_fade_cell: 8,
            wall_pixel: 1,
            wall_fit: "fill".into(),
            wall_tone: String::new(),
            wall_dim: 0.0,
            wall_accent: false,
            wall_pick: 0,
            accent2: String::new(),
            bar_left: v(&["launcher", "sep", "workspaces"]),
            bar_center: v(&["tasks"]),
            bar_hidden: vec![],
            bar_right: v(&["privacy_mic", "privacy_screen", "privacy_cam", "tray", "sep", "net", "kb", "cpu", "ram", "bat", "sep", "vol", "sep", "clock", "sep", "settings", "power"]),
            cpu_style: "gauge".into(),
            fan_style: "gauge".into(),
            ws_style: "boxes".into(),
            temp_style: "full".into(),
            tray_mode: "icons".into(),
            tray_pixel: false,
            tray_pinned: vec![],
            bat_hide_full: false,
            theme_apps: vec![],
            clock_seconds: false,
            clock_date: true,
            task_width: 190,
            bar_sep: "dots".into(),
            bar_sep_w: 10,
            bar_sep_color: "border".into(),
            bar_gap: 8,
            bar_pad_l: 6,
            bar_pad_r: 6,
            bar_mid: 12,
            bar_task_gap: 4,
            bar_v: 1,
            bar_float: false,
            bar_width_pc: 100,
            bar_outputs: vec![],
            bar_margin: 6,
            bar_center_align: "center".into(),
            bar_pos: "top".into(),
            lock_after: 10,
            screen_off_after: 15,
            dnd: false,
            notif_app_timeout: false,
            notif_critical_stay: true,
            notif_hover_pause: true,
            cast_dnd: true,
            notif_timeout: 7,
            notif_pos: "top-right".into(),
            osd_pos: "bottom".into(),
            osd_kinds: crate::notif::OSD_KINDS.iter().map(|k| k.0.to_string()).collect(),
            osd_time: 1.5,
            notif_offset: 10,
            notif_offset_x: 6,
            osd_offset: 64,
            dock: true,
            dock_side: "bottom".into(),
            dock_hide: "smart".into(),
            dock_size: 40,
            dock_anim: "grow".into(),
            dock_bounce: true,
            dock_labels: true,
            dock_running: true,
            dock_launcher: true,
            launcher_cmd: String::new(),
            control_tiles: ["wifi", "bt", "dnd", "night", "theme"].map(String::from).to_vec(),
            dock_trash: true,
            font_shell: String::new(),
            font_widgets: String::new(),
            dock_pins: ["yami-files", "kitty", "librewolf", "firefox", "chromium", "org.telegram.desktop", "steam", "yami-view"].iter().map(|s| s.to_string()).collect(),
            widgets: vec![
                Widget { id: 1, x: 48, y: 72, frame: false, ..Widget::new(1, "clock") },
            ],
            widget_contrast: true,
            game_dnd: true,
            night: false,
            night_temp: 4000,
            night_geo: String::new(),
            weather_city: String::new(),
            bt_auto: vec![],
            ctl_viz: "bars".into(),
            weather_units: "c".into(),
            night_on_h: 20,
            night_off_h: 7,
            game_apps: String::new(),
            agent_notif: true,
            agent_tray: true,
            notif_max: 4,
            notif_history: 50,
            notif_images: true,
            media_art_net: true,
            media_viz: "bars".into(),
            audio_names: Default::default(),
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let p = settings_path();
        match std::fs::read_to_string(&p) {
            Ok(t) => {
                let mut s = Settings::lenient(&t);
                s.migrate(&t);
                s.migrate_seps();
                // виджета агента больше нет
                s.widgets.retain(|w| w.kind != "agent");
                s.sanitize();
                s
            }
            Err(_) => Settings::default(),
        }
    }

    /// строки из файла (правка руками, чужая тема): имена — без управляющих символов, кавычек и
    /// разделителей, акцент — только #rrggbb
    pub fn sanitize(&mut self) {
        if !crate::widgets::STYLES.iter().any(|(k, _)| *k == self.ctl_viz) {
            self.ctl_viz = "bars".into();
        }
        // старые записи — полные пути BlueZ: только устройство
        self.bt_auto = self.bt_auto.iter().map(|p| crate::net::bt_key(p).to_string()).collect();
        let mut seen: Vec<String> = vec![];
        self.bt_auto.retain(|k| !seen.contains(k) && {
            seen.push(k.clone());
            true
        });
        if self.weather_units != "f" {
            self.weather_units = "c".into();
        }
        let name = |s: &mut String| {
            s.retain(|c| !c.is_control() && !"\"\\'`$;,{}<>|&#=".contains(c));
            if s.chars().count() > 64 {
                *s = s.chars().take(64).collect();
            }
        };
        name(&mut self.font_shell);
        name(&mut self.font_widgets);
        self.wallhaven_key.retain(|c| c.is_ascii_alphanumeric());
        self.lang = yami_px::i18n::normalize(&self.lang).into();
        if !self.accent.is_empty() && crate::theme::parse_hex(&self.accent).is_none() {
            self.accent.clear();
        }
        // app-id: то, что бывает в app-id, и разделители списка
        let ids = |s: &mut String| s.retain(|c| c.is_alphanumeric() || "._-*!+, ".contains(c));
        ids(&mut self.game_apps);
        for o in self.bar_outputs.iter_mut() {
            o.retain(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
            o.truncate(32);
        }
        self.bar_outputs.retain(|o| !o.is_empty());
        self.bar_outputs.dedup();
        self.bar_outputs.truncate(16);
        // как у ползунка: ниже 0.15 панели почти невидимы, а тень и размытие под ними — «битые рамки» (#8)
        self.glass = self.glass.clamp(0.15, 1.0);
        // модули, которых больше нет (лаунчер, «стол», профиль, погода, скрипты)
        for l in [&mut self.bar_left, &mut self.bar_center, &mut self.bar_right] {
            l.retain(|m| ALL_MODULES.iter().any(|(id, _)| id == m));
        }
        self.widgets.retain(|w| crate::widgets::KINDS.iter().any(|(k, _)| *k == w.kind));
        if self.bar_center_align != "after-left" {
            self.bar_center_align = "center".into();
        }
        if self.bar_pos != "bottom" {
            self.bar_pos = "top".into();
        }
        self.control_tiles.retain(|k| crate::control::TILES.iter().any(|t| t.0 == k));
        self.control_tiles.dedup();
        self.control_tiles.truncate(crate::control::MAX_TILES);
        if self.control_tiles.is_empty() {
            self.control_tiles.push("dnd".into());
        }
        if !crate::wall::FADES.iter().any(|f| f.0 == self.wall_fade) {
            self.wall_fade = "dither".into();
        }
        self.wall_fade_time = self.wall_fade_time.clamp(0.3, 4.0);
        if self.ws_style != "dots" {
            self.ws_style = "boxes".into();
        }
        if !matches!(self.fan_style.as_str(), "graph" | "number" | "rpm") {
            self.fan_style = "gauge".into();
        }
        // второй акцент бывает только из обоев
        if !self.wall_accent || crate::theme::parse_hex(&self.accent2).is_none() {
            self.accent2.clear();
        }
        if self.temp_style != "compact" {
            self.temp_style = "full".into();
        }
        self.theme_apps.retain(|a| crate::recolor::APPS.iter().any(|t| t.0 == a));
        self.theme_apps.sort_by_key(|a| crate::recolor::APPS.iter().position(|t| t.0 == a));
        self.theme_apps.dedup();
        if !matches!(self.tray_mode.as_str(), "arrow" | "hybrid") {
            self.tray_mode = "icons".into();
        }
        if self.media_viz != "off" && !crate::widgets::STYLES.iter().any(|(k, _)| *k == self.media_viz) {
            self.media_viz = "bars".into();
        }
        if !crate::notif::POS.iter().any(|(k, _)| *k == self.notif_pos) {
            self.notif_pos = "top-right".into();
        }
        self.osd_kinds.retain(|k| crate::notif::OSD_KINDS.iter().any(|x| x.0 == k));
        self.osd_time = self.osd_time.clamp(0.5, 5.0);
        if !crate::notif::OSD_POS.iter().any(|(k, _)| *k == self.osd_pos) {
            self.osd_pos = "bottom".into();
        }
    }

    /// разбор, прощающий ошибки: ключ с негодным значением берётся по умолчанию, а не весь
    /// файл (руками правят по одному ключу — одна опечатка не должна сбросить всё)
    pub fn lenient(text: &str) -> Settings {
        // всегда через Value: разбор Settings прямо из текста — ещё ~100 КБ кода в бинаре (второй
        // экземпляр serde-посетителя на ~200 полей) ради микросекунд на файл в несколько КБ
        let Some(user) = serde_json::from_str::<serde_json::Value>(text).ok().and_then(|v| if let serde_json::Value::Object(m) = v { Some(m) } else { None }) else {
            eprintln!("yami-lite: {} is not JSON, defaults used", settings_path().display());
            return Settings::default();
        };
        if let Ok(s) = serde_json::from_value::<Settings>(serde_json::Value::Object(user.clone())) {
            return s;
        }
        let (bad, _) = Settings::problems(&user);
        let mut v = serde_json::Map::new();
        for (k, x) in user {
            if !bad.contains(&k) {
                v.insert(k, x);
            }
        }
        for k in &bad {
            eprintln!("yami-lite: bad value for {k} in the settings, default used");
        }
        serde_json::from_value(serde_json::Value::Object(v)).unwrap_or_default()
    }

    /// ключи файла с негодным значением и неизвестные ключи
    pub fn problems(user: &serde_json::Map<String, serde_json::Value>) -> (Vec<String>, Vec<String>) {
        let base = serde_json::to_value(Settings::default()).unwrap_or_default();
        let known = base.as_object().cloned().unwrap_or_default();
        let (mut bad, mut unknown) = (vec![], vec![]);
        for (k, x) in user {
            if !known.contains_key(k) {
                if !RETIRED.contains(&k.as_str()) {
                    unknown.push(k.clone());
                }
                continue;
            }
            let mut one = base.clone();
            one[k] = x.clone();
            if serde_json::from_value::<Settings>(one).is_err() {
                bad.push(k.clone());
            }
        }
        (bad, unknown)
    }

    /// число в своих пределах (NUMS): в JSON руками можно вписать что угодно
    pub fn num(&self, k: &str) -> i32 {
        let v = match k {
            "bar_pad_l" => self.bar_pad_l,
            "bar_pad_r" => self.bar_pad_r,
            "bar_gap" => self.bar_gap,
            "bar_sep_w" => self.bar_sep_w,
            "bar_mid" => self.bar_mid,
            "bar_task_gap" => self.bar_task_gap,
            "task_width" => self.task_width,
            "reveal_grain" => self.reveal_grain,
            "night_temp" => self.night_temp,
            "night_on_h" => self.night_on_h,
            "night_off_h" => self.night_off_h,
            "bar_width_pc" => self.bar_width_pc,
            "bar_margin" => self.bar_margin,
            "notif_offset" => self.notif_offset,
            "notif_offset_x" => self.notif_offset_x,
            "osd_offset" => self.osd_offset,
            "wall_fade_cell" => self.wall_fade_cell,
            _ => 0,
        };
        NUMS.iter().find(|n| n.0 == k).map(|n| v.clamp(n.1, n.2)).unwrap_or(v)
    }

    /// шаг числа стрелкой или колесом: d = ±1 шаг
    pub fn step_num(&mut self, k: &str, d: i32) {
        let Some(&(_, lo, hi, step)) = NUMS.iter().find(|n| n.0 == k) else { return };
        let v = (self.num(k) + d * step).clamp(lo, hi);
        match k {
            "bar_pad_l" => self.bar_pad_l = v,
            "bar_pad_r" => self.bar_pad_r = v,
            "bar_gap" => self.bar_gap = v,
            "bar_sep_w" => self.bar_sep_w = v,
            "bar_mid" => self.bar_mid = v,
            "bar_task_gap" => self.bar_task_gap = v,
            "task_width" => self.task_width = v,
            "reveal_grain" => self.reveal_grain = v,
            "night_temp" => self.night_temp = v,
            "night_on_h" => self.night_on_h = v,
            "night_off_h" => self.night_off_h = v,
            "bar_width_pc" => self.bar_width_pc = v,
            "bar_margin" => self.bar_margin = v,
            "notif_offset" => self.notif_offset = v,
            "notif_offset_x" => self.notif_offset_x = v,
            "osd_offset" => self.osd_offset = v,
            "wall_fade_cell" => self.wall_fade_cell = v,
            _ => {}
        }
    }

    /// модуль m есть в баре (с любой стороны)
    pub fn on_bar(&self, m: &str) -> bool {
        self.bar_left.iter().chain(&self.bar_center).chain(&self.bar_right).any(|x| x == m)
    }

    /// плеер виден и без всплывашки звука: модуль `media` в баре или виджет музыки
    pub fn media_placed(&self) -> bool {
        self.on_bar("media") || self.widgets.iter().any(|w| w.on && w.kind == "media")
    }

    /// эквалайзер рисуется вне всплывашек: в модуле `media` (если не выключен) или в виджете музыки
    pub fn viz_placed(&self) -> bool {
        (self.on_bar("media") && self.media_viz != "off") || self.widgets.iter().any(|w| w.on && w.kind == "media")
    }

    /// модули стороны бара: 0 — слева, 1 — по центру, 2 — справа
    pub fn side_mut(&mut self, i: usize) -> &mut Vec<String> {
        match i {
            0 => &mut self.bar_left,
            1 => &mut self.bar_center,
            _ => &mut self.bar_right,
        }
    }

    /// раньше бар сам ставил разделитель между группами модулей; теперь это модуль "sep"
    fn migrate_seps(&mut self) {
        if self.bar_v >= 1 {
            return;
        }
        self.bar_v = 1;
        if self.bar_left.iter().chain(&self.bar_right).any(|m| m == "sep") {
            return;
        }
        for l in [&mut self.bar_left, &mut self.bar_right] {
            let mut out: Vec<String> = vec![];
            for m in l.drain(..) {
                if out.last().map(|p| group(p) != group(&m)).unwrap_or(false) {
                    out.push("sep".into());
                }
                out.push(m);
            }
            *l = out;
        }
    }


    fn migrate(&mut self, raw: &str) {
        const MODS: &[(&str, &str)] = &[("tray", "barTray"), ("net", "barNet"), ("kb", "barKb"), ("cpu", "barCpu"), ("ram", "barRam"), ("vol", "barVol"), ("clock", "barClock")];
        let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return };
        let old = ["clockOn", "clockX", "clockY", "barTasks"].iter().chain(MODS.iter().map(|(_, k)| k)).any(|k| v.get(*k).is_some());
        if v.get("widgets").is_some() || !old {
            return;
        }
        let num = |k: &str| v.get(k).and_then(|x| x.as_f64()).map(|x| x as i32);
        let flag = |k: &str| v.get(k).and_then(|x| x.as_bool());
        let mut ws = vec![];
        let mut c = Widget::new(1, "clock");
        c.on = flag("clockOn").unwrap_or(true);
        c.x = num("clockX").unwrap_or(48);
        c.y = num("clockY").unwrap_or(72);
        ws.push(c);
        self.widgets = ws;
        let mut right = vec![];
        for &(m, k) in MODS {
            if flag(k).unwrap_or(true) {
                right.push(m.to_string());
            }
        }
        right.push("settings".into());
        right.push("power".into());
        self.bar_right = right;
        if flag("barTasks") == Some(false) {
            self.bar_center.clear();
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default() + "\n"
    }

    /// второй акцент из обоев; None — напротив основного (Theme)
    pub fn accent2_rgb(&self) -> Option<u32> {
        crate::theme::parse_hex(&self.accent2)
    }

    pub fn accent_rgb(&self) -> u32 {
        if let Some(h) = crate::theme::parse_hex(&self.accent) {
            return h;
        }
        crate::theme::PRESETS.iter().find(|p| p.id == self.palette).unwrap_or(&crate::theme::PRESETS[0]).accent
    }

    pub fn next_widget_id(&self) -> u32 {
        self.widgets.iter().map(|w| w.id).max().unwrap_or(0) + 1
    }

    pub fn widget_mut(&mut self, id: u32) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|w| w.id == id)
    }


    pub fn anim(&self) -> bool {
        self.animations
    }
    /// размытие под панелями: регион отдаётся niri (ext-background-effect), размывает он сам
    pub fn blur_on(&self) -> bool {
        self.blur_shell
    }

}

#[cfg(test)]
mod tests {
    use super::Settings;

    fn load(raw: &str) -> Settings {
        let mut s = Settings::lenient(raw);
        s.migrate(raw);
        s.migrate_seps();
        s
    }

    #[test]
    fn migrate_only_old_files() {
        // файл установщика: бар по умолчанию, со всеми модулями и разделителями
        let s = load(r#"{"palette": "ash", "lang": "en", "fontUi": ""}"#);
        assert_eq!(s.bar_right, Settings::default().bar_right);
        // свой barRight без widgets — как написан
        assert_eq!(load(r#"{"barRight": ["vol", "settings"]}"#).bar_right, ["vol", "settings"]);
        // настоящий старый формат: флаги модулей и часы → список
        let s = load(r#"{"barKb": false, "barCpu": false, "clockX": 10}"#);
        assert_eq!(s.widgets.len(), 1);
        assert_eq!(s.widgets[0].x, 10);
        assert!(!s.bar_right.iter().any(|m| m == "kb" || m == "cpu"));
    }

}
