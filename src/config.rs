// Настройки шелла: ~/.config/yami-shell.json. Меняются в панели настроек (Mod+S),
// перетаскиванием виджетов и руками — файл перечитывается на лету.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use yami_px::paths::{config_dir, data_dir, home, state_dir, write_atomic};

pub fn settings_path() -> PathBuf {
    config_dir().join("yami-shell.json")
}

/// переезд со старого имени (DARK-4RMY): настройки, состояние и закреплённое в буфере
pub fn migrate() {
    let state = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(".local/state"));
    for (old, new) in [
        (config_dir().join("dark4rmy-shell.json"), settings_path()),
        (state.join("dark4rmy"), state.join("yami")),
        (data_dir().join("dark4rmy-clip"), data_dir().join("yami-clip")),
    ] {
        if old.exists() && !new.exists() {
            let _ = std::fs::rename(&old, &new);
        }
    }
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
    /// язык интерфейса: "" — по языку системы, "ru", "en", "pt-BR"
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
    pub blur_term: bool,
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
    /// лёгкий режим для слабых и виртуальных машин (lite.rs): без размытия, без анимаций шелла
    /// и niri, неактивные окна непрозрачные; свои настройки эффектов при этом не меняются
    pub lite: bool,
    /// режим «стол»: все окна плавают, у каждого заголовок как в DE
    pub stacking: bool,
    pub titlebars: bool,
    pub icon_theme: String,
    /// значки программ пиксельными: чистая сетка 16–30 клеток (yami-px pixelate)
    pub icon_pixel: bool,
    /// пиксельные значки — в 4 тонах палитры, а не в своих цветах
    pub icon_tint: bool,
    /// свои обои (путь); пусто — пиксельные обои из палитры
    pub wallpaper: String,
    /// папка выбора обоев (walls.rs); пусто — ~/Pictures/Wallpapers, папка текущих обоев или ~/Pictures
    pub wall_dir: String,
    /// пикселизация своих обоев: 1 — как есть, дальше клетки крупнее (wall::cell)
    pub wall_pixel: i32,
    pub wall_kanji: bool,
    /// "fill" — заполнить экран (лишнее обрезать), "fit" — вписать целиком, поля цвета фона
    pub wall_fit: String,
    /// окраска своих обоев: "" — как есть, "tint" — оттенок акцента, "palette" — тона палитры с дизерингом
    pub wall_tone: String,
    /// затемнение обоев к фону темы: 0 — нет, 0.8 — почти фон
    pub wall_dim: f32,
    /// акцент (а с ним вся палитра) берётся из своих обоев
    pub wall_accent: bool,
    /// обои выбирает skwd-wall (skwd.rs): рисует skwd-paper — свои прячутся, нет — картинку рисует шелл
    pub wall_skwd: bool,
    /// акцент и светлая / тёмная тема — из палитры skwd-wall
    pub wall_skwd_colors: bool,

    // бар: модули по сторонам и их порядок
    pub bar_left: Vec<String>,
    pub bar_center: Vec<String>,
    pub bar_right: Vec<String>,
    /// gauge | graph | number
    pub cpu_style: String,
    /// модуль temp: "full" — как цп и память (подпись, шкала или график), "compact" — столбик и число
    pub temp_style: String,
    /// трей: "icons" — значки в баре, "arrow" — одна стрелка, значки во всплывашке
    pub tray_mode: String,
    /// значки трея клетками в тонах палитры, как пиксельные значки дока и лаунчера (#19)
    pub tray_pixel: bool,
    /// модуль bat прячется от сети без зарядки: полная или держит порог заряда (#21)
    pub bat_hide_full: bool,
    /// чужие программы, которые theme.py красит по палитре (THEME_APPS; настройки → цвет → программы,
    /// установщик); выключенную yami не трогает. До 2.7 — appThemes: foot, cava, starship сверх остальных
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

    // экран входа (yami-greet: login.rs выкладывает их в look.json)
    /// "list" — пользователи списком, "name" — имя вводят сами (список не показывается)
    pub greet_users: String,
    /// обои экрана входа: "desktop" — как на столе, "pixel" — пиксельные из палитры, "plain" — цвет темы
    pub greet_wall: String,
    /// плотность вуали над обоями экрана входа (0.2–0.9)
    pub greet_dim: f32,
    pub greet_clock: bool,
    pub greet_host: bool,
    /// кнопки сна, перезагрузки и выключения (там, где их пускает logind)
    pub greet_power: bool,
    /// погасить мониторы экрана входа через столько минут простоя (0 — никогда)
    pub greet_off: u32,
    pub greet_numlock: bool,

    // система
    pub lock_after: u32,
    pub screen_off_after: u32,
    pub dnd: bool,
    pub notif_timeout: u32,
    /// где карточки уведомлений: top-right | top-left | top | bottom-right | bottom-left | bottom
    pub notif_pos: String,
    /// где OSD громкости и яркости: bottom | top | center
    pub osd_pos: String,
    /// отступ карточек уведомлений от края экрана (от бара, если он у того края), px (#8)
    pub notif_offset: i32,
    /// отступ OSD от края экрана (сверху — от низа бара), px; по центру не нужен (#8)
    pub osd_offset: i32,

    // скриншоты: папка ("" — Изображения/Screenshots), куда класть, лупа, таймер
    pub shot_dir: String,
    pub shot_copy: bool,
    pub shot_save: bool,
    pub shot_loupe: bool,
    pub shot_notify: bool,
    pub shot_delay: u32,
    /// подтверждение снимка (#13): "space" — выделил, потом пробел или enter; "instant" — снимок,
    /// как только отпущена мышь (область, окно), а снимок экрана — сразу, без оверлея
    pub shot_confirm: String,
    /// шторка для трансляций (curtain.rs): "blur", "pixels", "wall"; клетка пикселей в px
    pub curtain_style: String,
    pub curtain_cell: i32,

    // плавающие окна
    pub float_media: bool,
    pub float_pip: bool,
    /// свои app-id через запятую
    pub float_apps: String,
    pub float_w: f32,
    pub float_h: f32,
    /// отступы окон (niri gaps/struts, пишутся в yami-fx.kdl): между окнами и от краёв экрана
    /// (сверху — от бара), px
    pub win_gap: i32,
    pub win_left: i32,
    pub win_right: i32,
    pub win_top: i32,
    pub win_bottom: i32,
    /// скругление углов окон, px (0 — прямые); игры и окна на весь экран — всегда прямые
    pub win_radius: i32,
    /// рамка окна, px (0 — без рамки)
    pub win_border: i32,

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
    pub dock_trash: bool,
    pub dock_pins: Vec<String>,

    // курсор: yami (пиксельный в палитре) | system
    pub cursor: String,
    pub cursor_size: i32,
    /// UI-шрифт программ (GTK, Qt, kitty, браузер), px по сетке Tiny5: 0 — по экрану, 16 | 24 | 32
    pub font_size: i32,
    // шрифты (вкладка «шрифты»): семейства fontconfig, "" — по умолчанию
    /// шелл: "" — Tiny5 (пиксельный)
    pub font_shell: String,
    /// yami-files и yami-view: "" — как у шелла
    pub font_files: String,
    pub font_view: String,
    /// программы (GTK, Qt): "" — Tiny5, "sans" — системный, иначе семейство
    pub font_ui: String,
    /// терминал: "" — Monocraft / Terminess, иначе семейство
    pub font_mono: String,
    /// кегль для не пиксельных шрифтов программ, pt: 0 — 11
    pub font_pt: i32,
    /// браузеры Firefox-семейства: меню ПКМ, списки и подсказки в палитре
    pub browser_menus: bool,
    /// и пиксельный шрифт в интерфейсе браузера (не на страницах)
    pub browser_font: bool,

    // виджеты рабочего стола
    pub widgets: Vec<Widget>,
    /// виджеты без подложки — в тон обоям под ними: на светлом месте тёмный текст, на тёмном
    /// светлый, и жёсткая тень у букв (widgets.rs)
    pub widget_contrast: bool,

    /// клавиши: id из keys::BINDS → сочетание ("" — выключено); чего нет — по умолчанию
    pub binds: std::collections::BTreeMap<String, String>,
    /// свои команды на сочетания (spawn-sh)
    pub custom_binds: Vec<crate::keys::Custom>,
    /// fastfetch с железом (процессор, видеокарта, память, диск, экран); иначе — короткий
    pub fetch_full: bool,
    /// раскладки xkb через запятую («us,ru»); пусто — по языкам системы (keys::auto_layout)
    pub kb_layout: String,
    /// переключение раскладок: опция xkb (grp:alt_shift_toggle) или пусто — без сочетания
    pub kb_switch: String,

    // игры (games.rs): правила игровых окон и игровой режим
    pub game_fullscreen: bool,
    pub game_vrr: bool,
    /// разрывы ради задержки: только на yami-niri
    pub game_tearing: bool,
    pub game_cache: bool,
    pub game_dnd: bool,
    pub game_profile: bool,
    /// PROTON_USE_NTSYNC=1, если в ядре есть /dev/ntsync
    pub game_ntsync: bool,
    /// регистрировать игру в фокусе у gamemoded
    pub game_gamemode: bool,
    /// открыть при входе программы, что были открыты (session.rs)
    pub session_restore: bool,
    /// ночной свет (wlsunset): температура, координаты «lat, lon» или часы вкл/выкл
    pub night: bool,
    pub night_temp: i32,
    pub night_geo: String,
    /// погода (#6): город или «широта, долгота» для Open-Meteo; пусто — погоды нет
    pub weather_city: String,
    /// "c" — °C и км/ч, "f" — °F и mph
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
    /// окно пароля polkit
    pub agent_polkit: bool,
    /// карточек на экране сразу и сколько хранит история
    pub notif_max: u32,
    pub notif_history: u32,
    /// картинки в уведомлениях (значки, превью от программ)
    pub notif_images: bool,
    /// уведомления видны в записи и трансляциях (иначе их не видят зрители)
    pub notif_cast: bool,
    /// обложки музыки по ссылкам из сети (иначе — только локальные файлы)
    pub media_art_net: bool,
    /// тексты песен с lrclib.net (иначе — только от плеера и файлы .lrc рядом с треком)
    pub lyrics_net: bool,
    /// флешки, диски, телефоны и камеры подключаются сами, как только их вставили (automount.rs)
    pub automount: bool,
    /// подключённое само сразу открывается в файлах (иначе — карточка «открыть»)
    pub automount_open: bool,
    /// эквалайзер всплывашки музыки и модуля `media`: стиль виджета (bars, line…) или "off" (#13)
    pub media_viz: String,
    /// что показывает всплывашка музыки: cover | lyrics | both | minimal (#13)
    pub media_view: String,
    /// свои имена звуковых устройств: имя PipeWire (`pactl list sinks`) → как показывать
    pub audio_names: std::collections::BTreeMap<String, String>,
    /// порталы: диалог файлов yami-files и выбор экрана для трансляций (иначе GTK / GNOME)
    pub portal_files: bool,
    pub portal_cast: bool,

    // обновления (update.rs)
    /// проверять сама раз в updateEvery часов (0 — только по кнопке)
    pub update_check: bool,
    pub update_every: u32,
    /// "release" — выпуски (теги vX.Y.Z), "main" — каждый коммит ветки
    pub update_channel: String,
    /// подписи выпусков: "auto" — проверять, если в установленной версии есть ключи,
    /// "require" — неподписанное не ставить, "off" — не проверять
    pub update_verify: String,
    /// свой адрес репозитория ("" — откуда ставили; публичный и приватный — один и тот же)
    pub update_repo: String,
}

/// группа модуля — только для переноса старых настроек (migrate_seps)
fn group(m: &str) -> i32 {
    match m {
        "launcher" => 0,
        "workspaces" => 1,
        "mode" => 2,
        "tray" => 3,
        "net" | "netspeed" | "bt" | "kb" | "cpu" | "ram" | "temp" | "bat" | "profile" | "night" => 4,
        "vol" | "media" => 5,
        "clock" | "notif" | "settings" | "power" => 6,
        _ => 7,
    }
}

pub const ALL_MODULES: &[(&str, &str)] = &[
    ("launcher", "меню программ"),
    ("workspaces", "столы"),
    ("mode", "стол / лента"),
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
    ("weather", "погода"),
    ("vol", "звук"),
    ("bat", "батарея"),
    ("profile", "профиль питания"),
    ("night", "ночной свет"),
    ("clock", "часы"),
    ("notif", "уведомления"),
    ("settings", "настройки"),
    ("power", "питание"),
    ("sep", "разделитель"),
];

/// виды всплывашки музыки (#13): обложка с эквалайзером, обложка и текст, текст, коротко
/// чужие программы в цветах палитры (themeApps): id, имя, что пишется. Шелл, экран входа, рамки niri и
/// свои шаблоны — всегда; курсор — отдельный ключ cursor
pub const THEME_APPS: &[(&str, &str, &str)] = &[
    ("gtk", "GTK", "тема, шрифт, тёмная или светлая"),
    ("qt", "Qt и KDE", "qt6ct, kdeglobals"),
    ("kitty", "kitty", "строка include в kitty.conf"),
    ("fastfetch", "fastfetch", "свой конфиг с логотипом"),
    ("btop", "btop", "тема yami"),
    ("firefox", "Firefox", "меню: userChrome.css"),
    ("mangohud", "MangoHud", "только наш конфиг"),
    ("foot", "foot", "include в foot.ini"),
    ("cava", "cava", "градиент полос"),
    ("starship", "starship", "палитра приглашения"),
];
/// ключи прошлых версий: их читает миграция (load), `config check` не зовёт их незнакомыми
const RETIRED: &[&str] = &["appThemes"];

pub const MEDIA_VIEWS: &[(&str, &str)] = &[("cover", "обложка"), ("both", "обложка и текст"), ("lyrics", "текст"), ("minimal", "коротко")];

/// числа в пикселях, что меняются стрелками в настройках: ключ, от, до, шаг
pub const NUMS: &[(&str, i32, i32, i32)] = &[
    ("win_gap", 0, 64, 2),
    ("win_left", 0, 128, 2),
    ("win_right", 0, 128, 2),
    ("win_top", 0, 128, 2),
    ("win_bottom", 0, 128, 2),
    ("bar_pad_l", 0, 64, 2),
    ("bar_pad_r", 0, 64, 2),
    ("bar_gap", 0, 32, 2),
    ("bar_sep_w", 2, 32, 2),
    ("bar_mid", 0, 64, 2),
    ("bar_task_gap", 0, 16, 2),
    ("task_width", 60, 400, 10),
    ("bar_width_pc", 30, 100, 5),
    ("bar_margin", 0, 32, 2),
    ("win_radius", 0, 24, 2),
    ("win_border", 0, 8, 1),
    ("reveal_grain", 1, 4, 1),
    ("night_temp", 2500, 6000, 250),
    ("night_on_h", 0, 23, 1),
    ("night_off_h", 0, 23, 1),
    ("notif_offset", 0, 400, 2),
    ("osd_offset", 0, 400, 4),
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
            blur_term: true,
            animations: true,
            anim_speed: 1.0,
            reveal: "dither".into(),
            reveal_grain: 1,
            anim_close: true,
            scanlines: false,
            lite: false,
            stacking: false,
            titlebars: true,
            icon_theme: String::new(),
            icon_pixel: true,
            icon_tint: false,
            wallpaper: String::new(),
            wall_dir: String::new(),
            wall_pixel: 1,
            wall_kanji: true,
            wall_fit: "fill".into(),
            wall_tone: String::new(),
            wall_dim: 0.0,
            wall_accent: false,
            wall_skwd: false,
            wall_skwd_colors: true,
            bar_left: v(&["launcher", "sep", "workspaces", "sep", "mode"]),
            bar_center: v(&["tasks"]),
            bar_right: v(&["tray", "sep", "net", "kb", "cpu", "ram", "bat", "profile", "sep", "vol", "sep", "clock", "sep", "settings", "power"]),
            cpu_style: "gauge".into(),
            temp_style: "full".into(),
            tray_mode: "icons".into(),
            tray_pixel: false,
            bat_hide_full: false,
            theme_apps: THEME_APPS.iter().map(|a| a.0.to_string()).collect(),
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
            greet_users: "list".into(),
            greet_wall: "desktop".into(),
            greet_dim: 0.55,
            greet_clock: true,
            greet_host: true,
            greet_power: true,
            greet_off: 2,
            greet_numlock: true,
            lock_after: 10,
            screen_off_after: 15,
            dnd: false,
            notif_timeout: 7,
            notif_pos: "top-right".into(),
            osd_pos: "bottom".into(),
            notif_offset: 10,
            osd_offset: 64,
            shot_dir: String::new(),
            shot_copy: true,
            shot_save: true,
            shot_loupe: true,
            shot_notify: true,
            shot_delay: 3,
            shot_confirm: "space".into(),
            curtain_style: "blur".into(),
            curtain_cell: 24,
            float_media: true,
            float_pip: true,
            float_apps: String::new(),
            float_w: 0.62,
            float_h: 0.78,
            win_gap: 8,
            win_left: 8,
            win_right: 8,
            win_top: 8,
            win_bottom: 8,
            win_radius: 0,
            win_border: 2,
            dock: true,
            dock_side: "bottom".into(),
            dock_hide: "smart".into(),
            dock_size: 40,
            dock_anim: "grow".into(),
            dock_bounce: true,
            dock_labels: true,
            dock_running: true,
            dock_launcher: true,
            dock_trash: true,
            cursor: "yami".into(),
            cursor_size: 24,
            font_size: 0,
            font_shell: String::new(),
            font_files: String::new(),
            font_view: String::new(),
            font_ui: String::new(),
            font_mono: String::new(),
            font_pt: 0,
            browser_menus: true,
            browser_font: false,
            dock_pins: ["yami-files", "kitty", "librewolf", "firefox", "chromium", "org.telegram.desktop", "steam", "yami-view"].iter().map(|s| s.to_string()).collect(),
            widgets: vec![
                Widget { id: 1, x: 48, y: 72, frame: false, ..Widget::new(1, "clock") },
            ],
            widget_contrast: true,
            binds: Default::default(),
            custom_binds: vec![],
            fetch_full: false,
            kb_layout: String::new(),
            kb_switch: "grp:alt_shift_toggle".into(),
            game_fullscreen: true,
            game_vrr: true,
            game_tearing: false,
            game_cache: true,
            game_dnd: true,
            game_profile: true,
            game_ntsync: true,
            game_gamemode: true,
            session_restore: false,
            night: false,
            night_temp: 4000,
            night_geo: String::new(),
            weather_city: String::new(),
            weather_units: "c".into(),
            night_on_h: 20,
            night_off_h: 7,
            game_apps: String::new(),
            agent_notif: true,
            agent_tray: true,
            agent_polkit: true,
            notif_max: 4,
            notif_history: 50,
            notif_images: true,
            notif_cast: false,
            media_art_net: true,
            lyrics_net: true,
            automount: true,
            automount_open: false,
            media_viz: "bars".into(),
            media_view: "cover".into(),
            audio_names: Default::default(),
            portal_files: true,
            portal_cast: true,
            update_check: true,
            update_every: 24,
            update_channel: "release".into(),
            update_verify: "auto".into(),
            update_repo: String::new(),
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
                s.migrate_theme_apps(&t);
                s.migrate_seps();
                // Bibata больше не ставится: свой пиксельный курсор
                if s.cursor == "bibata" {
                    s.cursor = "yami".into();
                }
                // виджета агента больше нет
                s.widgets.retain(|w| w.kind != "agent");
                s.sanitize();
                s
            }
            Err(_) => Settings::default(),
        }
    }

    /// строки из файла (правка руками, `yami import`, чужая тема) уходят в конфиги niri, kitty,
    /// GTK и Qt: имена — без управляющих символов, кавычек и разделителей (иначе перевод строки
    /// в имени шрифта дописал бы в kitty.conf свою команду), акцент — только #rrggbb
    pub fn sanitize(&mut self) {
        let name = |s: &mut String| {
            s.retain(|c| !c.is_control() && !"\"\\'`$;,{}<>|&#=".contains(c));
            if s.chars().count() > 64 {
                *s = s.chars().take(64).collect();
            }
        };
        for f in [&mut self.font_shell, &mut self.font_files, &mut self.font_view, &mut self.font_ui, &mut self.font_mono, &mut self.icon_theme, &mut self.cursor] {
            name(f);
        }
        self.lang = yami_px::i18n::normalize(&self.lang).into();
        if !self.accent.is_empty() && crate::theme::parse_hex(&self.accent).is_none() {
            self.accent.clear();
        }
        // app-id: то, что бывает в app-id, и разделители списка
        let ids = |s: &mut String| s.retain(|c| c.is_alphanumeric() || "._-*!+, ".contains(c));
        ids(&mut self.float_apps);
        ids(&mut self.game_apps);
        self.kb_switch.retain(|c| c.is_ascii_alphanumeric() || "_:,()-".contains(c));
        for o in self.bar_outputs.iter_mut() {
            o.retain(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
            o.truncate(32);
        }
        self.bar_outputs.retain(|o| !o.is_empty());
        self.weather_city.retain(|c| !c.is_control());
        self.weather_city = self.weather_city.chars().take(64).collect::<String>().trim().to_string();
        if self.weather_units != "f" {
            self.weather_units = "c".into();
        }
        // виджет-скрипт: только имя файла в папке модулей (как модули бара `x:`)
        for w in self.widgets.iter_mut().filter(|w| w.kind == "script") {
            if !crate::plugins::name_ok(&w.source) {
                w.source.clear();
            }
        }
        self.bar_outputs.dedup();
        self.bar_outputs.truncate(16);
        self.cursor_size = self.cursor_size.clamp(16, 96);
        // как у ползунка: ниже 0.15 панели почти невидимы, а тень и размытие под ними — «битые рамки» (#8)
        self.glass = self.glass.clamp(0.15, 1.0);
        self.font_pt = self.font_pt.clamp(0, 48);
        self.binds.retain(|_, k| k.is_empty() || crate::keys::key_ok(k));
        self.custom_binds.retain(|c| c.key.is_empty() || crate::keys::key_ok(&c.key));
        // модули-скрипты: только имя файла в ~/.config/yami/modules; «rec» (повтор) убран
        for l in [&mut self.bar_left, &mut self.bar_center, &mut self.bar_right] {
            l.retain(|m| m != "rec" && m.strip_prefix("x:").map_or(true, crate::plugins::name_ok));
        }
        if self.bar_center_align != "after-left" {
            self.bar_center_align = "center".into();
        }
        if self.bar_pos != "bottom" {
            self.bar_pos = "top".into();
        }
        if self.temp_style != "compact" {
            self.temp_style = "full".into();
        }
        self.theme_apps.retain(|a| THEME_APPS.iter().any(|t| t.0 == a));
        self.theme_apps.sort_by_key(|a| THEME_APPS.iter().position(|t| t.0 == a));
        self.theme_apps.dedup();
        if self.tray_mode != "arrow" {
            self.tray_mode = "icons".into();
        }
        if self.media_viz != "off" && !crate::widgets::STYLES.iter().any(|(k, _)| *k == self.media_viz) {
            self.media_viz = "bars".into();
        }
        if !MEDIA_VIEWS.iter().any(|(k, _)| *k == self.media_view) {
            self.media_view = "cover".into();
        }
        if !crate::notif::POS.iter().any(|(k, _)| *k == self.notif_pos) {
            self.notif_pos = "top-right".into();
        }
        if !crate::notif::OSD_POS.iter().any(|(k, _)| *k == self.osd_pos) {
            self.osd_pos = "bottom".into();
        }
        if self.shot_confirm != "instant" {
            self.shot_confirm = "space".into();
        }
    }

    /// разбор, прощающий ошибки: ключ с негодным значением берётся по умолчанию, а не весь
    /// файл (руками правят по одному ключу — одна опечатка не должна сбросить всё)
    pub fn lenient(text: &str) -> Settings {
        // всегда через Value: разбор Settings прямо из текста — ещё ~100 КБ кода в бинаре (второй
        // экземпляр serde-посетителя на ~200 полей) ради микросекунд на файл в несколько КБ
        let Some(user) = serde_json::from_str::<serde_json::Value>(text).ok().and_then(|v| if let serde_json::Value::Object(m) = v { Some(m) } else { None }) else {
            eprintln!("yami-shell: {} is not JSON, defaults used", settings_path().display());
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
            eprintln!("yami-shell: bad value for {k} in the settings, default used");
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
            "win_gap" => self.win_gap,
            "win_left" => self.win_left,
            "win_right" => self.win_right,
            "win_top" => self.win_top,
            "win_bottom" => self.win_bottom,
            "bar_width_pc" => self.bar_width_pc,
            "bar_margin" => self.bar_margin,
            "win_radius" => self.win_radius,
            "win_border" => self.win_border,
            "notif_offset" => self.notif_offset,
            "osd_offset" => self.osd_offset,
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
            "win_gap" => self.win_gap = v,
            "win_left" => self.win_left = v,
            "win_right" => self.win_right = v,
            "win_top" => self.win_top = v,
            "win_bottom" => self.win_bottom = v,
            "bar_width_pc" => self.bar_width_pc = v,
            "bar_margin" => self.bar_margin = v,
            "win_radius" => self.win_radius = v,
            "win_border" => self.win_border = v,
            "notif_offset" => self.notif_offset = v,
            "osd_offset" => self.osd_offset = v,
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

    /// старый формат настроек: clockOn/clockX… → список виджетов. Только когда старые ключи есть:
    /// файл установщика (палитра, язык) и написанный руками тоже без `widgets`, но они новые —
    /// иначе barRight заменялся старым набором (без батареи, профиля и разделителей)
    /// appThemes (≤ 2.6: foot, cava, starship сверх постоянных) → themeApps (все, кого красить)
    fn migrate_theme_apps(&mut self, raw: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else { return };
        if v.get("themeApps").is_some() {
            return;
        }
        let Some(old) = v.get("appThemes").and_then(|x| x.as_array()) else { return };
        let had = |id: &str| old.iter().any(|x| x.as_str() == Some(id));
        self.theme_apps = THEME_APPS.iter().map(|a| a.0).filter(|id| !matches!(*id, "foot" | "cava" | "starship") || had(id)).map(String::from).collect();
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

    /// плавающие окна для просмотра фото и видео (settings → окна)
    fn float_kdl(&self, l: &mut Vec<String>) {
        let size = |l: &mut Vec<String>, w: f32, h: f32| {
            l.push(format!("    default-column-width {{ proportion {:.2}; }}", w));
            l.push(format!("    default-window-height {{ proportion {:.2}; }}", h));
        };
        if self.float_media {
            l.push("// photo and video viewers open as a floating window, not as a new column".into());
            l.push("window-rule {".into());
            l.push("    match app-id=r#\"(?i)(telegram|ayugram|64gram|kotatogram)\"# title=r#\"^(Media viewer|Просмотр медиа|Медиапросмотрщик)$\"#".into()); // i18n-ok
            for id in ["yami-view", "yami-play", "imv", "feh", "nsxiv", "sxiv", "org\\.gnome\\.Loupe", "org\\.gnome\\.eog", "org\\.kde\\.gwenview", "qimgv", "swayimg", "mpv", "vlc", "org\\.gnome\\.Showtime"] {
                l.push(format!("    match app-id=r#\"^{id}$\"#"));
            }
            l.push("    open-floating true".into());
            l.push("    open-fullscreen false".into());
            l.push("    open-focused true".into());
            size(l, self.float_w, self.float_h);
            l.push("}".into());
            l.push(String::new());
        }
        if self.float_pip {
            l.push("window-rule {".into());
            l.push("    match title=r#\"^(Picture-in-Picture|Картинка в картинке)$\"#".into()); // i18n-ok
            l.push("    open-floating true".into());
            l.push("    default-floating-position x=24 y=56 relative-to=\"bottom-right\"".into());
            l.push("    default-column-width { fixed 480; }".into());
            l.push("    default-window-height { fixed 270; }".into());
            l.push("}".into());
            l.push(String::new());
        }
        let extra: Vec<&str> = self.float_apps.split([',', ' ']).map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        if !extra.is_empty() {
            l.push("// your own floating apps (settings → windows)".into());
            l.push("window-rule {".into());
            for id in extra {
                l.push(format!("    match app-id=r#\"^{}$\"#", id.replace('"', "").replace('.', "\\.")));
            }
            l.push("    open-floating true".into());
            size(l, self.float_w, self.float_h);
            l.push("}".into());
            l.push(String::new());
        }
    }

    /// отступы и рамка окон: gaps — между окнами и у краёв; struts добирают край до заданного
    /// (бывают и отрицательными, если край уже промежутка). struts в niri не сливаются между
    /// файлами: свои в user.kdl заменят эти целиком. Рамка — толщина (цвета — yami-colors.kdl)
    fn layout_kdl(&self, l: &mut Vec<String>) {
        let g = self.num("win_gap");
        l.push("// window gaps and border (settings → windows): between windows, from the screen edges".into());
        l.push("layout {".into());
        l.push(format!("    gaps {g}"));
        l.push("    struts {".into());
        for (side, k) in [("left", "win_left"), ("right", "win_right"), ("top", "win_top"), ("bottom", "win_bottom")] {
            l.push(format!("        {side} {}", self.num(k) - g));
        }
        l.push("    }".into());
        match self.num("win_border") {
            0 => l.push("    border { off; }".into()),
            b => l.push(format!("    border {{ on; width {b}; }}")),
        }
        l.push("}".into());
        l.push(String::new());
    }

    /// углы окон: скругление по геометрии окна (рамка niri идёт по нему же). Окна на весь экран и
    /// развёрнутые niri и так рисует прямыми, игры — прямые правилом games::kdl. В режиме «стол»
    /// над плавающим окном — пиксельный заголовок шелла: у таких окон скруглены только нижние углы,
    /// и заголовок прилегает без щели
    fn corners_kdl(&self, l: &mut Vec<String>) {
        let r = self.num("win_radius");
        if r == 0 {
            return;
        }
        l.push("// window corners (settings → windows)".into());
        l.push("window-rule {".into());
        l.push(format!("    geometry-corner-radius {r}"));
        l.push("    clip-to-geometry true".into());
        l.push("}".into());
        if self.stacking && self.titlebars {
            l.push("window-rule {".into());
            l.push("    match is-floating=true".into());
            l.push(format!("    geometry-corner-radius 0 0 {r} {r}"));
            l.push("}".into());
        }
        l.push(String::new());
    }

    /// анимации на деле: выключены и сами, и в лёгком режиме
    pub fn anim(&self) -> bool {
        self.animations && !self.lite
    }
    /// размытие под панелями шелла и под терминалами на деле (лёгкий режим их снимает)
    pub fn blur_on(&self) -> bool {
        self.blur_shell && !self.lite
    }
    pub fn blur_term(&self) -> bool {
        self.blur_term && !self.lite
    }

    /// yami-fx.kdl для niri уровня `lvl` (0 — сток, иначе наш форк: allow-tearing; ≥ 4 —
    /// `show-only-in`; ≥ 5 — `hide-from`); curtain — шторка опущена (без `show-only-in` тогда
    /// прячутся окна). Матовость — размытие под терминалами (панели шелла — своим регионом)
    pub fn fx_kdl(&self, lvl: u32, curtain: bool) -> String {
        let mut l: Vec<String> = vec!["// yami: written by the shell (settings → look, windows, games, services). Do not edit by hand: it is rewritten.".into(), String::new()];
        self.float_kdl(&mut l);
        self.layout_kdl(&mut l);
        self.corners_kdl(&mut l);
        crate::games::kdl(self, lvl > 0, &mut l);
        crate::skwd::kdl(self, &mut l);
        crate::curtain::kdl(lvl, curtain, self.notif_cast, &mut l);
        if self.lite {
            crate::lite::kdl(&mut l);
        }
        if !self.blur_term() && !self.blur_on() {
            l.push("blur { off; }".into());
            return l.join("\n") + "\n";
        }
        for s in ["blur {", "    passes 3", "    offset 3", "    noise 0.03", "    saturation 1.2", "}", ""] {
            l.push(s.into());
        }
        if self.blur_term() {
            for s in ["window-rule {", "    match app-id=r#\"^(kitty|foot|Alacritty|org\\.wezfurlong\\.wezterm|com\\.mitchellh\\.ghostty)$\"#", "    background-effect { blur true; }", "}", ""] {
                l.push(s.into());
            }
        }
        // панели шелла niri размывает по региону, который шелл отдаёт через ext-background-effect:
        // правило на всю поверхность захватило бы и прозрачные поля под тенью
        l.join("\n") + "\n"
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

    #[test]
    fn brazilian_portuguese_survives_sanitizing_and_saving() {
        for tag in ["pt-BR", "pt_BR", "pt_BR.UTF-8", "pt"] {
            let mut s = Settings { lang: tag.into(), ..Settings::default() };
            s.sanitize();
            assert_eq!(s.lang, "pt-BR");
            let saved = serde_json::to_string(&s).unwrap();
            let mut restored = Settings::lenient(&saved);
            restored.sanitize();
            assert_eq!(restored.lang, "pt-BR");
        }
    }
}
