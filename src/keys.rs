// Клавиши: все сочетания niri и шелла. Шелл — их хозяин: таблица по умолчанию здесь,
// переопределения — в настройках (`binds`: id → "Mod+X", "" — выключено) и свои команды
// (`customBinds`), итог пишется в ~/.config/niri/yami-binds.kdl. Руками поверх — niri/user.kdl:
// он подключён позже, его сочетания перекрывают наши.
//
// i18n: inline pairs — у каждого бинда русское и английское название рядом.
//
// Вкладка «клавиши»: щелчок по сочетанию — запись (niri на это время отдаёт все клавиши
// шеллу через keyboard-shortcuts-inhibit), Backspace — снять, Esc — отмена.

use crate::app::{App, SKind, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Tone, Ui};
use serde::{Deserialize, Serialize};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, Modifiers};

pub struct Bind {
    pub id: &'static str,
    pub cat: &'static str,
    pub key: &'static str,
    /// флаги niri перед действием: repeat=false, allow-when-locked=true…
    pub opts: &'static str,
    pub act: &'static str,
    pub ru: &'static str,
    pub en: &'static str,
    /// показывать в подсказке niri (Mod+Shift+/)
    pub overlay: bool,
}

const fn b(id: &'static str, cat: &'static str, key: &'static str, opts: &'static str, act: &'static str, ru: &'static str, en: &'static str, overlay: bool) -> Bind {
    Bind { id, cat, key, opts, act, ru, en, overlay }
}

const NR: &str = "repeat=false";
const LK: &str = "allow-when-locked=true";

pub const CATS: &[(&str, &str, &str)] = &[
    ("shell", "оболочка", "shell"),
    ("windows", "окна", "windows"),
    ("focus", "фокус", "focus"),
    ("move", "перемещение", "moving"),
    ("ws", "столы", "workspaces"),
    ("shots", "скриншоты", "screenshots"),
    ("media", "система", "media, system"),
    ("custom", "свои", "custom"),
    ("layout", "раскладка", "layout"),
];

/// сочетания переключения раскладки (опции xkb); Super+Space занят меню программ
pub const SWITCHES: &[(&str, &str)] = &[
    ("grp:alt_shift_toggle", "Alt+Shift"),
    ("grp:ctrl_shift_toggle", "Ctrl+Shift"),
    ("grp:caps_toggle", "Caps Lock"),
    ("grp:alt_space_toggle", "Alt+Space"),
    ("", "нет"),
];

/// раскладки по языкам системы: LANG и все LC_* (из окружения и /etc/locale.conf). Английский —
/// us (en_GB — gb), любой другой язык добавляет свою; нелатинские идут после us, чтобы сочетания
/// и пароли набирались. Американец получит «us», русский — «us,ru», немец — «de»
pub fn auto_layout() -> String {
    let mut locs: Vec<String> = ["LC_ALL", "LANG", "LC_MESSAGES", "LC_TIME", "LC_ADDRESS", "LC_NAME", "LC_TELEPHONE", "LC_MONETARY", "LC_PAPER", "LC_MEASUREMENT", "LC_NUMERIC"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .collect();
    if let Ok(t) = std::fs::read_to_string("/etc/locale.conf") {
        locs.extend(t.lines().filter_map(|l| l.split_once('=')).map(|(_, v)| v.trim().trim_matches('"').to_string()));
    }
    let mut out: Vec<&str> = vec![];
    for l in &locs {
        let base = l.split(['.', '@']).next().unwrap_or("");
        let (lang, cc) = base.split_once('_').unwrap_or((base, ""));
        let lay = match (lang, cc) {
            ("" | "C" | "POSIX", _) => continue,
            ("en", "GB") => "gb",
            ("en", _) | ("nl", _) | ("zh", _) | ("vi", _) | ("id", _) | ("ms", _) => "us",
            ("de", "CH") | ("fr", "CH") => "ch",
            ("fr", "CA") => "ca",
            ("fr", "BE") => "be",
            ("pt", "BR") => "br",
            ("es", "ES") | ("es", "") => "es",
            ("es", _) => "latam",
            ("uk", _) => "ua",
            ("be", _) => "by",
            ("kk", _) => "kz",
            ("sr", _) => "rs",
            ("el", _) => "gr",
            ("he", _) => "il",
            ("ar", _) => "ara",
            ("fa", _) => "ir",
            ("hi", _) => "in",
            ("ka", _) => "ge",
            ("hy", _) => "am",
            ("cs", _) => "cz",
            ("sl", _) => "si",
            ("sv", _) => "se",
            ("da", _) => "dk",
            ("nb" | "nn" | "no", _) => "no",
            ("et", _) => "ee",
            ("ja", _) => "jp",
            ("ko", _) => "kr",
            ("ru" | "de" | "fr" | "it" | "pt" | "pl" | "sk" | "hr" | "hu" | "ro" | "fi" | "lv" | "lt" | "tr" | "bg" | "th", _) => lang,
            _ => continue,
        };
        if !out.contains(&lay) {
            out.push(lay);
        }
    }
    const NON_LATIN: &[&str] = &["ru", "ua", "by", "kz", "bg", "rs", "gr", "il", "ara", "ir", "in", "th", "ge", "am", "jp", "kr"];
    if out.iter().any(|l| NON_LATIN.contains(l)) && !out.iter().any(|l| !NON_LATIN.contains(l)) {
        out.insert(0, "us");
    }
    // латинская раскладка — первой (по ней набираются пароли и сочетания)
    out.sort_by_key(|l| NON_LATIN.contains(l));
    if out.is_empty() {
        "us".into()
    } else {
        out.join(",")
    }
}

/// раскладки, которые уйдут в niri: своё значение (только буквы, цифры, _ - ( ) и запятые) или авто
pub fn layouts(set: &crate::config::Settings) -> String {
    let own: String = set.kb_layout.chars().filter(|c| c.is_ascii_alphanumeric() || "_-(),".contains(*c)).collect();
    let own = own.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(",");
    if own.is_empty() {
        auto_layout()
    } else {
        own
    }
}

#[rustfmt::skip]
pub const BINDS: &[Bind] = &[
    b("terminal", "shell", "Mod+T", "", r#"spawn "kitty";"#, "терминал", "Terminal", true),
    b("launcher", "shell", "Mod+Space", NR, r#"spawn "yami-shell" "msg" "launcher";"#, "меню программ", "App launcher", true),
    b("launcher-2", "shell", "Mod+D", NR, r#"spawn "yami-shell" "msg" "launcher";"#, "меню программ (второе)", "App launcher (second)", false),
    // без yami-files — файловый менеджер по умолчанию (kdl подменяет действие)
    b("files", "shell", "Mod+E", "", r#"spawn "yami-files";"#, "файлы", "Files", true),
    b("clipboard", "shell", "Mod+V", NR, r#"spawn "yami-shell" "msg" "clipboard";"#, "буфер обмена", "Clipboard", true),
    b("settings", "shell", "Mod+S", NR, r#"spawn "yami-shell" "msg" "settings";"#, "настройки шелла", "Shell settings", true),
    // без клавиши по умолчанию: назначается в настройках → клавиши, ничьё сочетание не занимает
    b("walls", "shell", "", NR, r#"spawn "yami-shell" "msg" "walls";"#, "выбор обоев", "Wallpaper picker", true),
    b("lock", "shell", "Mod+Escape", "repeat=false allow-when-locked=false", r#"spawn "yami-shell" "msg" "lock";"#, "заблокировать", "Lock screen", true),
    b("power", "shell", "Mod+Shift+E", NR, r#"spawn "yami-shell" "msg" "power";"#, "меню питания", "Power menu", true),
    b("power-2", "shell", "Ctrl+Alt+Delete", NR, r#"spawn "yami-shell" "msg" "power";"#, "меню питания (второе)", "Power menu (second)", false),
    b("bar", "shell", "Mod+B", NR, r#"spawn "yami-shell" "msg" "bar";"#, "спрятать бар", "Toggle bar", true),
    b("stacking", "shell", "Mod+Shift+Space", NR, r#"spawn "yami-shell" "msg" "stacking";"#, "режим: стол / лента", "Mode: desk / tiling", true),
    b("minimize", "shell", "Mod+N", NR, r#"spawn "yami-shell" "msg" "minimize";"#, "свернуть окно", "Minimize window", true),
    b("restore", "shell", "Mod+Shift+N", NR, r#"spawn "yami-shell" "msg" "restore";"#, "вернуть свёрнутое", "Restore last minimized", true),
    b("overview", "shell", "Mod+Tab", NR, "toggle-overview;", "обзор столов", "Overview", true),
    b("hotkeys", "shell", "Mod+Shift+Slash", "", "show-hotkey-overlay;", "подсказка клавиш", "Show hotkeys", false),

    b("close", "windows", "Mod+Q", NR, "close-window;", "закрыть окно", "Close window", true),
    b("close-2", "windows", "Alt+F4", NR, "close-window;", "закрыть окно (второе)", "Close window (second)", false),
    b("maximize", "windows", "Mod+F", "", "maximize-column;", "развернуть колонку", "Maximize column", true),
    b("fullscreen", "windows", "Mod+Shift+F", "", "fullscreen-window;", "на весь экран", "Fullscreen", true),
    b("windowed-fs", "windows", "Mod+Ctrl+F", "", "toggle-windowed-fullscreen;", "полноэкранное в окне", "Windowed fullscreen", false),
    b("float", "windows", "Mod+G", "", "toggle-window-floating;", "плавающее / в ленте", "Toggle floating", true),
    b("float-focus", "windows", "Mod+Shift+T", "", "switch-focus-between-floating-and-tiling;", "фокус: плавающие / лента", "Focus floating / tiling", false),
    b("tabbed", "windows", "Mod+W", "", "toggle-column-tabbed-display;", "колонка вкладками", "Tabbed column", false),
    b("center", "windows", "Mod+C", "", "center-column;", "колонку в центр", "Center column", false),
    b("width-preset", "windows", "Mod+R", "", "switch-preset-column-width;", "ширина: следующая", "Next column width", false),
    b("height-preset", "windows", "Mod+Shift+R", "", "switch-preset-window-height;", "высота: следующая", "Next window height", false),
    b("height-reset", "windows", "Mod+Ctrl+R", "", "reset-window-height;", "высота: сбросить", "Reset window height", false),
    b("width-less", "windows", "Mod+Minus", "", r#"set-column-width "-10%";"#, "уже на 10%", "Narrower by 10%", false),
    b("width-more", "windows", "Mod+Equal", "", r#"set-column-width "+10%";"#, "шире на 10%", "Wider by 10%", false),
    b("height-less", "windows", "Mod+Shift+Minus", "", r#"set-window-height "-10%";"#, "ниже на 10%", "Shorter by 10%", false),
    b("height-more", "windows", "Mod+Shift+Equal", "", r#"set-window-height "+10%";"#, "выше на 10%", "Taller by 10%", false),
    b("consume-left", "windows", "Mod+BracketLeft", "", "consume-or-expel-window-left;", "в колонку слева / из неё", "Into / out of the left column", false),
    b("consume-right", "windows", "Mod+BracketRight", "", "consume-or-expel-window-right;", "в колонку справа / из неё", "Into / out of the right column", false),
    b("consume", "windows", "Mod+Comma", "", "consume-window-into-column;", "забрать окно в колонку", "Consume window into column", false),
    b("expel", "windows", "Mod+Period", "", "expel-window-from-column;", "вынуть окно из колонки", "Expel window from column", false),

    b("focus-left", "focus", "Mod+Left", "", "focus-column-left;", "колонка слева", "Column on the left", false),
    b("focus-right", "focus", "Mod+Right", "", "focus-column-right;", "колонка справа", "Column on the right", false),
    b("focus-up", "focus", "Mod+Up", "", "focus-window-or-workspace-up;", "окно / стол выше", "Window / workspace above", false),
    b("focus-down", "focus", "Mod+Down", "", "focus-window-or-workspace-down;", "окно / стол ниже", "Window / workspace below", false),
    b("focus-left-2", "focus", "Mod+H", "", "focus-column-left;", "колонка слева (vim)", "Column on the left (vim)", false),
    b("focus-right-2", "focus", "Mod+L", "", "focus-column-right;", "колонка справа (vim)", "Column on the right (vim)", false),
    b("focus-up-2", "focus", "Mod+K", "", "focus-window-or-workspace-up;", "окно / стол выше (vim)", "Window / workspace above (vim)", false),
    b("focus-down-2", "focus", "Mod+J", "", "focus-window-or-workspace-down;", "окно / стол ниже (vim)", "Window / workspace below (vim)", false),
    b("focus-first", "focus", "Mod+Home", "", "focus-column-first;", "первая колонка", "First column", false),
    b("focus-last", "focus", "Mod+End", "", "focus-column-last;", "последняя колонка", "Last column", false),
    b("wheel-right", "focus", "Mod+Shift+WheelScrollDown", "", "focus-column-right;", "колёсико: колонка справа", "Wheel: column on the right", false),
    b("wheel-left", "focus", "Mod+Shift+WheelScrollUp", "", "focus-column-left;", "колёсико: колонка слева", "Wheel: column on the left", false),

    b("move-left", "move", "Mod+Ctrl+Left", "", "move-column-left;", "колонку влево", "Move column left", false),
    b("move-right", "move", "Mod+Ctrl+Right", "", "move-column-right;", "колонку вправо", "Move column right", false),
    b("move-up", "move", "Mod+Ctrl+Up", "", "move-window-up-or-to-workspace-up;", "окно выше / на стол выше", "Window up / to workspace above", false),
    b("move-down", "move", "Mod+Ctrl+Down", "", "move-window-down-or-to-workspace-down;", "окно ниже / на стол ниже", "Window down / to workspace below", false),
    b("move-left-2", "move", "Mod+Ctrl+H", "", "move-column-left;", "колонку влево (vim)", "Move column left (vim)", false),
    b("move-right-2", "move", "Mod+Ctrl+L", "", "move-column-right;", "колонку вправо (vim)", "Move column right (vim)", false),
    b("move-up-2", "move", "Mod+Ctrl+K", "", "move-window-up-or-to-workspace-up;", "окно выше (vim)", "Window up (vim)", false),
    b("move-down-2", "move", "Mod+Ctrl+J", "", "move-window-down-or-to-workspace-down;", "окно ниже (vim)", "Window down (vim)", false),

    b("ws-1", "ws", "Mod+1", "", "focus-workspace 1;", "стол 1", "Workspace 1", false),
    b("ws-2", "ws", "Mod+2", "", "focus-workspace 2;", "стол 2", "Workspace 2", false),
    b("ws-3", "ws", "Mod+3", "", "focus-workspace 3;", "стол 3", "Workspace 3", false),
    b("ws-4", "ws", "Mod+4", "", "focus-workspace 4;", "стол 4", "Workspace 4", false),
    b("ws-5", "ws", "Mod+5", "", "focus-workspace 5;", "стол 5", "Workspace 5", false),
    b("ws-6", "ws", "Mod+6", "", "focus-workspace 6;", "стол 6", "Workspace 6", false),
    b("ws-7", "ws", "Mod+7", "", "focus-workspace 7;", "стол 7", "Workspace 7", false),
    b("ws-8", "ws", "Mod+8", "", "focus-workspace 8;", "стол 8", "Workspace 8", false),
    b("ws-9", "ws", "Mod+9", "", "focus-workspace 9;", "стол 9", "Workspace 9", false),
    b("to-ws-1", "ws", "Mod+Shift+1", "", "move-column-to-workspace 1;", "колонку на стол 1", "Column to workspace 1", false),
    b("to-ws-2", "ws", "Mod+Shift+2", "", "move-column-to-workspace 2;", "колонку на стол 2", "Column to workspace 2", false),
    b("to-ws-3", "ws", "Mod+Shift+3", "", "move-column-to-workspace 3;", "колонку на стол 3", "Column to workspace 3", false),
    b("to-ws-4", "ws", "Mod+Shift+4", "", "move-column-to-workspace 4;", "колонку на стол 4", "Column to workspace 4", false),
    b("to-ws-5", "ws", "Mod+Shift+5", "", "move-column-to-workspace 5;", "колонку на стол 5", "Column to workspace 5", false),
    b("to-ws-6", "ws", "Mod+Shift+6", "", "move-column-to-workspace 6;", "колонку на стол 6", "Column to workspace 6", false),
    b("to-ws-7", "ws", "Mod+Shift+7", "", "move-column-to-workspace 7;", "колонку на стол 7", "Column to workspace 7", false),
    b("to-ws-8", "ws", "Mod+Shift+8", "", "move-column-to-workspace 8;", "колонку на стол 8", "Column to workspace 8", false),
    b("to-ws-9", "ws", "Mod+Shift+9", "", "move-column-to-workspace 9;", "колонку на стол 9", "Column to workspace 9", false),
    b("ws-up", "ws", "Mod+Page_Up", "", "focus-workspace-up;", "стол выше", "Workspace above", false),
    b("ws-down", "ws", "Mod+Page_Down", "", "focus-workspace-down;", "стол ниже", "Workspace below", false),
    b("to-ws-up", "ws", "Mod+Shift+Page_Up", "", "move-column-to-workspace-up;", "колонку на стол выше", "Column to workspace above", false),
    b("to-ws-down", "ws", "Mod+Shift+Page_Down", "", "move-column-to-workspace-down;", "колонку на стол ниже", "Column to workspace below", false),
    b("wheel-ws-down", "ws", "Mod+WheelScrollDown", "cooldown-ms=150", "focus-workspace-down;", "колёсико: стол ниже", "Wheel: workspace below", false),
    b("wheel-ws-up", "ws", "Mod+WheelScrollUp", "cooldown-ms=150", "focus-workspace-up;", "колёсико: стол выше", "Wheel: workspace above", false),

    b("curtain", "shots", "Mod+Shift+H", NR, r#"spawn "yami-shell" "msg" "curtain";"#, "скрыть экран в трансляции", "Hide the screen from streams", true),
    b("shot", "shots", "Print", NR, r#"spawn "yami-shell" "msg" "screenshot";"#, "скриншот", "Screenshot", true),
    b("shot-2", "shots", "Mod+Shift+S", NR, r#"spawn "yami-shell" "msg" "screenshot";"#, "скриншот (второе)", "Screenshot (second)", false),
    b("shot-screen", "shots", "Ctrl+Print", NR, r#"spawn "yami-shell" "msg" "screenshot" "screen";"#, "скриншот монитора", "Screenshot: monitor", true),
    b("shot-window", "shots", "Alt+Print", NR, r#"spawn "yami-shell" "msg" "screenshot" "window";"#, "скриншот окна", "Screenshot: window", true),

    b("vol-up", "media", "XF86AudioRaiseVolume", LK, r#"spawn-sh "wpctl set-volume -l 1.5 @DEFAULT_AUDIO_SINK@ 5%+";"#, "громче", "Volume up", false),
    b("vol-down", "media", "XF86AudioLowerVolume", LK, r#"spawn-sh "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-";"#, "тише", "Volume down", false),
    b("mute", "media", "XF86AudioMute", LK, r#"spawn-sh "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle";"#, "без звука", "Mute", false),
    b("mic-mute", "media", "XF86AudioMicMute", LK, r#"spawn-sh "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle";"#, "микрофон выкл / вкл", "Mute microphone", false),
    b("play", "media", "XF86AudioPlay", LK, r#"spawn-sh "yami-shell msg media play-pause || playerctl play-pause";"#, "пауза / играть", "Play / pause", false),
    b("next", "media", "XF86AudioNext", LK, r#"spawn-sh "yami-shell msg media next || playerctl next";"#, "следующий трек", "Next track", false),
    b("prev", "media", "XF86AudioPrev", LK, r#"spawn-sh "yami-shell msg media previous || playerctl previous";"#, "предыдущий трек", "Previous track", false),
    b("bright-up", "media", "XF86MonBrightnessUp", LK, r#"spawn "brightnessctl" "set" "5%+";"#, "ярче", "Brightness up", false),
    b("bright-down", "media", "XF86MonBrightnessDown", LK, r#"spawn "brightnessctl" "set" "5%-";"#, "темнее", "Brightness down", false),
    b("monitors-off", "media", "Mod+Shift+P", "", "power-off-monitors;", "погасить мониторы", "Power off monitors", false),
    b("inhibit", "media", "Mod+Ctrl+Escape", "allow-inhibiting=false", "toggle-keyboard-shortcuts-inhibit;", "отдать все клавиши окну", "Pass all keys to the window", false),
    b("quit", "media", "Mod+Shift+Ctrl+Q", "", "quit;", "выйти из niri", "Quit niri", false),
];

/// своя команда на сочетание: spawn-sh
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Custom {
    pub key: String,
    pub cmd: String,
}

/// состояние вкладки: категория, прокрутка, запись сочетания, поле новой команды
#[derive(Default)]
pub struct KeysUi {
    pub cat: &'static str,
    pub scroll: i32,
    /// что сейчас записываем: id бинда, "custom:N" или "new" (новая своя команда)
    pub capture: Option<String>,
    /// снятое из-за конфликта — показать, с чего
    pub note: String,
    pub new_key: String,
    pub cmd: crate::ui::Field,
    /// поле раскладок (раздел «раскладка»)
    pub lay: crate::ui::Field,
}

pub fn title_of(b: &Bind) -> &'static str {
    crate::i18n::select(b.ru, b.en)
}

/// текущее сочетание бинда: переопределение или по умолчанию
pub fn key_of(set: &crate::config::Settings, b: &Bind) -> String {
    set.binds.get(b.id).cloned().unwrap_or_else(|| b.key.to_string())
}

/// сочетание годится в KDL как есть: только буквы, цифры, «+» и «_» (Mod+Shift+Return,
/// XF86AudioRaiseVolume). Из файла настроек или чужого импорта может прийти что угодно — с
/// кавычками и скобками строка дописала бы в binds свои команды
pub fn key_ok(k: &str) -> bool {
    !k.is_empty() && k.len() <= 64 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '_') && !k.starts_with('+') && !k.ends_with('+')
}

/// строка KDL: кавычки и обратная косая экранируются, переводы строк — \n (в строке KDL 2 их
/// быть не может: команда со строкой из правленого руками JSON сломала бы весь конфиг niri)
pub fn kdl_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if c.is_control() => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// yami-binds.kdl целиком
pub fn kdl(set: &crate::config::Settings) -> String {
    let mut l = vec![
        "// yami: keybindings and keyboard layouts, written by the shell (settings → keys). Rewritten on every change:".to_string(),
        "// add your own in niri/user.kdl (included later, its keys win).".to_string(),
        "binds {".to_string(),
    ];
    let mut used: Vec<String> = vec![];
    for b in BINDS {
        let key = key_of(set, b);
        if !key_ok(&key) || used.iter().any(|u| same_key(u, &key)) {
            continue;
        }
        used.push(key.clone());
        let mut line = format!("    {key}");
        if !b.opts.is_empty() {
            line += &format!(" {}", b.opts);
        }
        // на «столе» все окна плавают, а maximize-column их не трогает: там Mod+F разворачивает
        // само окно до краёв (niri ≥ 25.11; ниже 26.04 yami не ставится)
        let desk_max = set.stacking && b.id == "maximize";
        if b.overlay {
            let title = if desk_max { crate::tf!("развернуть окно", "Maximize window", "Maximizar janela") } else { title_of(b).to_string() };
            line += &format!(" hotkey-overlay-title={}", kdl_str(&title));
        }
        // yami-files не выбран при установке: домашняя папка в файловом менеджере по умолчанию
        let act = match b.id {
            _ if desk_max => "maximize-window-to-edges;",
            "files" if !crate::proc::have("yami-files") => r#"spawn-sh "xdg-open ~";"#,
            _ => b.act,
        };
        line += &format!(" {{ {act} }}");
        l.push(line);
    }
    for c in &set.custom_binds {
        if !key_ok(&c.key) || c.cmd.trim().is_empty() || used.iter().any(|u| same_key(u, &c.key)) {
            continue;
        }
        used.push(c.key.clone());
        l.push(format!("    {} hotkey-overlay-title={} {{ spawn-sh {}; }}", c.key, kdl_str(c.cmd.trim()), kdl_str(c.cmd.trim())));
    }
    l.push("}".into());
    // раскладки (settings → keys → layout); input сливается с config.kdl, user.kdl — позже и главнее
    l.push(String::new());
    l.push("input {".into());
    l.push("    keyboard {".into());
    l.push("        xkb {".into());
    l.push(format!("            layout {}", kdl_str(&layouts(set))));
    if !set.kb_switch.is_empty() {
        l.push(format!("            options {}", kdl_str(&set.kb_switch)));
    }
    l.push("        }".into());
    l.push("    }".into());
    // фокус за мышью — только в ленте. На «столе» niri по наведению делает окно активным, но
    // не поднимает: клавиатура уходит окну, которое под другим, а заголовки (они по порядку
    // фокуса) лезли поверх переднего окна. На столе — как в DE: щелчок фокусирует и поднимает
    if !set.stacking {
        l.push("    focus-follows-mouse max-scroll-amount=\"0%\"".into());
    }
    l.push("}".into());
    l.join("\n") + "\n"
}

pub fn write(set: &crate::config::Settings) {
    let path = crate::config::config_dir().join("niri/yami-binds.kdl");
    let text = kdl(set);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        let _ = crate::config::write_atomic(&path, &text);
    }
}

// ── запись сочетания ────────────────────────────────────────────────────────

/// имя клавиши для niri: по положению (evdev) для букв, цифр и знаков — так сочетание
/// одинаково в русской и английской раскладке; остальное — по keysym.
/// raw_code в SCTK — уже код evdev (xkb-код — это он + 8)
pub fn key_name(e: &KeyEvent) -> Option<String> {
    let code = e.raw_code;
    const ROW: [(u32, &str); 47] = [
        (2, "1"), (3, "2"), (4, "3"), (5, "4"), (6, "5"), (7, "6"), (8, "7"), (9, "8"), (10, "9"), (11, "0"),
        (12, "Minus"), (13, "Equal"), (16, "Q"), (17, "W"), (18, "E"), (19, "R"), (20, "T"), (21, "Y"), (22, "U"),
        (23, "I"), (24, "O"), (25, "P"), (26, "BracketLeft"), (27, "BracketRight"), (30, "A"), (31, "S"), (32, "D"),
        (33, "F"), (34, "G"), (35, "H"), (36, "J"), (37, "K"), (38, "L"), (39, "Semicolon"), (40, "Apostrophe"),
        (41, "Grave"), (43, "Backslash"), (44, "Z"), (45, "X"), (46, "C"), (47, "V"), (48, "B"), (49, "N"), (50, "M"),
        (51, "Comma"), (52, "Period"), (53, "Slash"),
    ];
    if let Some((_, n)) = ROW.iter().find(|(c, _)| *c == code) {
        return Some(n.to_string());
    }
    // Shift+Tab xkb отдаёт как ISO_Left_Tab, а niri ждёт Tab с модификатором Shift
    if e.keysym == Keysym::ISO_Left_Tab {
        return Some("Tab".into());
    }
    let n = e.keysym.name()?;
    let n = n.strip_prefix("XK_").map(String::from).or_else(|| n.strip_prefix("XF86XK_").map(|x| format!("XF86{x}")))?;
    Some(n)
}

/// буква клавиши для сочетаний шелла (Ctrl+A/C/V/U/P в полях, буквы меню питания): в латинской
/// раскладке — та, что подписана на клавише (AZERTY, Dvorak, Colemak), в другой (русская) — по
/// положению, как у QWERTY
pub fn latin_letter(e: &KeyEvent) -> Option<char> {
    let k = e.keysym.raw();
    if (0x41..=0x5a).contains(&k) || (0x61..=0x7a).contains(&k) {
        return Some((k as u8 as char).to_ascii_uppercase());
    }
    key_name(e).filter(|n| n.len() == 1).and_then(|n| n.chars().next()).filter(char::is_ascii_uppercase)
}

fn is_modifier(k: Keysym) -> bool {
    matches!(
        k,
        Keysym::Shift_L | Keysym::Shift_R | Keysym::Control_L | Keysym::Control_R | Keysym::Alt_L | Keysym::Alt_R | Keysym::Super_L | Keysym::Super_R | Keysym::Meta_L | Keysym::Meta_R | Keysym::ISO_Level3_Shift | Keysym::Caps_Lock | Keysym::Num_Lock
    )
}

/// одно сочетание в разных записях («Mod+Shift+Ctrl+Q» и «Super+Control+Shift+q») — одно и то же:
/// niri не различает порядок модификаторов и регистр, а два одинаковых бинда в конфиге — ошибка
pub fn same_key(a: &str, b: &str) -> bool {
    !a.is_empty() && canon(a) == canon(b)
}

fn canon(k: &str) -> (u8, String) {
    let parts: Vec<&str> = k.split('+').collect();
    let (key, mods) = parts.split_last().unwrap_or((&"", &[]));
    let mut m = 0u8;
    for x in mods {
        m |= match x.to_ascii_lowercase().as_str() {
            "mod" | "super" | "win" | "mod4" => 1,
            "ctrl" | "control" => 2,
            "alt" | "mod1" => 4,
            "shift" => 8,
            "iso_level3_shift" | "mod5" => 16,
            _ => 0,
        };
    }
    (m, key.to_ascii_lowercase())
}

pub fn combo(m: &Modifiers, key: &str) -> String {
    let mut s = String::new();
    for (on, n) in [(m.logo, "Mod+"), (m.ctrl, "Ctrl+"), (m.alt, "Alt+"), (m.shift, "Shift+")] {
        if on {
            s += n;
        }
    }
    s + key
}

// ── вкладка ─────────────────────────────────────────────────────────────────

const ROW_H: i32 = 30;

/// раздел «раскладка»: коды xkb и сочетание переключения
fn layout_rows(ui: &mut Ui, st: &State, list: R, mut ry: i32) -> i32 {
    let t = ui.t;
    let s = &st.set;
    let focused = st.field_focus == Some("kb_layout");
    let ph = if s.kb_layout.trim().is_empty() {
        crate::tf!("авто по языкам системы: {}", "automatic from the system languages: {}", "automático pelos idiomas do sistema: {}", auto_layout())
    } else {
        crate::tf!("свои: {}", "your own: {}", "personalizado: {}", layouts(s))
    };
    ui.text_in(PX, R::new(list.x, ry, list.w, 20), "раскладки — коды xkb через запятую, первая по умолчанию", t.text_dim);
    ry += 24;
    ui.field(R::new(list.x, ry, list.w - 130, 26), &st.keys.lay, &ph, focused, false, Act::SField("kb_layout"));
    ui.button(R::new(list.right() - 120, ry, 120, 26), "применить", "check", Tone::Plain, false, true, Act::SCycle("kb_apply"));
    ry += 36;
    let sw = SWITCHES.iter().find(|x| x.0 == s.kb_switch).map(|x| x.1).unwrap_or("свои");
    crate::settings::opt_btn(ui, R::new(list.x, ry, list.w, 26), "переключать раскладку", sw, Act::SCycle("kb_switch"));
    ry += 36;
    ui.wrap(PX, R::new(list.x, ry, list.w, 60), "us, gb, de, fr, es, it, pl, ru, ua… Пустое поле — авто. Свой блок input в niri/user.kdl главнее.", t.text_faint, 3);
    ry + 60
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let k = &st.keys;
    let cat = if k.cat.is_empty() { "shell" } else { k.cat };
    // категории: две строки кнопок
    // португальские названия длиннее: три кнопки в ряд вместо пяти
    let per = if crate::i18n::pt_br() { 3 } else { 5 };
    let cw = (b.w - (per - 1) * 6) / per;
    for (i, (id, ru, en)) in CATS.iter().enumerate() {
        let r = R::new(b.x + (i as i32 % per) * (cw + 6), b.y + (i as i32 / per) * 32, cw, 26);
        let name = crate::i18n::select(ru, en);
        ui.button(r, name, "", Tone::Plain, cat == *id, true, Act::KeyCat(id));
    }
    let mut y = b.y + ((CATS.len() as i32 + per - 1) / per) * 32 + 8;
    let hint = if k.capture.is_some() {
        "нажми сочетание… Esc — отмена, Backspace — снять"
    } else if !k.note.is_empty() {
        k.note.as_str()
    } else {
        "щелчок по сочетанию — записать новое, стрелка справа — как было"
    };
    if cat != "layout" {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), hint, if k.capture.is_some() { t.red } else { t.text_faint });
        y += 28;
    }
    let list = R::new(b.x, y, b.w, b.bottom() - y);
    // колёсико над списком; строки — поверх (зоны рисуются позже)
    ui.zone(list, Act::KeyScroll, Cur::Default);
    let mut ry = list.y - k.scroll;
    if cat == "custom" {
        ry = custom_rows(ui, st, list, ry);
    } else if cat == "layout" {
        ry = layout_rows(ui, st, list, ry);
    } else {
        for bind in BINDS.iter().filter(|x| x.cat == cat) {
            let r = R::new(list.x, ry, list.w, ROW_H - 4);
            ry += ROW_H;
            if r.y < list.y || r.bottom() > list.bottom() {
                continue;
            }
            let key = key_of(&st.set, bind);
            let changed = key != bind.key;
            row(ui, st, r, title_of(bind), &key, bind.id, changed);
        }
    }
    let total = ry + k.scroll - list.y;
    if total > list.h {
        // полоса прокрутки
        let th = (list.h * list.h / total).max(16);
        let ty = list.y + (list.h - th) * k.scroll / (total - list.h).max(1);
        ui.cv.fill(R::new(list.right() - 2, ty, 2, th), t.border);
    }
}

fn key_chip(ui: &mut Ui, st: &State, r: R, key: &str, id: &str) {
    let t = ui.t;
    let rec = st.keys.capture.as_deref() == Some(id);
    let hov = ui.hover(r);
    let fill = if rec { t.red.alpha(0.25) } else if hov { t.surface_hi } else { t.surface };
    ui.cv.pxbox(r, &BoxStyle::new(fill, if rec || hov { t.red } else { t.border }).bevel(true));
    let s = if rec {
        "…".to_string()
    } else if key.is_empty() {
        crate::i18n::tr("нет").to_string()
    } else {
        key.to_string()
    };
    // сочетание — не перевод: рисуем как есть
    ui.cv.text_center(ui.f, PX, r, &s, if key.is_empty() && !rec { t.text_faint } else { t.text });
    ui.zone(r, Act::KeyRec(id.to_string()), Cur::Pointer);
}

fn row(ui: &mut Ui, st: &State, r: R, name: &str, key: &str, id: &str, changed: bool) {
    let t = ui.t;
    if ui.hover(r) {
        ui.cv.fill(r, t.surface.alpha(0.5));
    }
    let kw = 210;
    ui.text_in(PX, R::new(r.x + 6, r.y, r.w - kw - 40, r.h), name, if changed { t.text } else { t.text_dim });
    key_chip(ui, st, R::new(r.right() - kw - 34, r.y, kw, r.h), key, id);
    if changed {
        ui.ghost(R::new(r.right() - 28, r.y + 1, 24, r.h - 2), "reboot", Act::KeyReset(id.to_string()));
    }
}

fn custom_rows(ui: &mut Ui, st: &State, list: R, mut ry: i32) -> i32 {
    let t = ui.t;
    let inside = |r: R| r.y >= list.y && r.bottom() <= list.bottom();
    for (i, c) in st.set.custom_binds.iter().enumerate() {
        let r = R::new(list.x, ry, list.w, ROW_H - 4);
        ry += ROW_H;
        if !inside(r) {
            continue;
        }
        let kw = 210;
        // команда — текст пользователя, не перевод
        let cmd = ui.f.elide(PX, &c.cmd, r.w - kw - 50);
        ui.cv.text_in(ui.f, PX, R::new(r.x + 6, r.y, r.w - kw - 50, r.h), &cmd, t.text);
        key_chip(ui, st, R::new(r.right() - kw - 34, r.y, kw, r.h), &c.key, &format!("custom:{i}"));
        ui.ghost(R::new(r.right() - 28, r.y + 1, 24, r.h - 2), "close", Act::KeyDel(i));
    }
    ry += 8;
    let k = &st.keys;
    let fr = R::new(list.x, ry, list.w - 210 - 40, 26);
    if inside(fr) {
        let focused = st.field_focus == Some("key_cmd");
        ui.field(fr, &k.cmd, "команда, например: kitty -e btop", focused, false, Act::SField("key_cmd"));
        key_chip(ui, st, R::new(list.right() - 210 - 34, ry, 210, 26), &k.new_key, "new");
    }
    ry += 32;
    let br = R::new(list.x, ry, 160, 26);
    if inside(br) {
        ui.button(br, "добавить", "plus", Tone::Plain, false, !k.new_key.is_empty() && !k.cmd.text.trim().is_empty(), Act::KeyAdd);
    }
    ry + 32
}

impl App {
    pub fn keys_click(&mut self, a: &Act) -> bool {
        match a {
            Act::KeyCat(c) => {
                self.st.keys.cat = c;
                self.st.keys.scroll = 0;
                self.st.keys.note.clear();
                self.keys_capture(None);
            }
            Act::KeyRec(id) => {
                let same = self.st.keys.capture.as_deref() == Some(id.as_str());
                self.st.field_focus = None;
                self.keys_capture(if same { None } else { Some(id.clone()) });
            }
            Act::KeyReset(id) => {
                self.st.set.binds.remove(id.as_str());
                self.binds_changed();
            }
            Act::KeyDel(i) => {
                if *i < self.st.set.custom_binds.len() {
                    self.st.set.custom_binds.remove(*i);
                    self.binds_changed();
                }
            }
            Act::KeyAdd => {
                let cmd = self.st.keys.cmd.text.trim().to_string();
                let key = std::mem::take(&mut self.st.keys.new_key);
                if !cmd.is_empty() && !key.is_empty() {
                    self.free_key(&key, "");
                    self.st.set.custom_binds.push(Custom { key, cmd });
                    self.st.keys.cmd.clear();
                    self.st.field_focus = None;
                    self.binds_changed();
                }
            }
            Act::KeyScroll => {}
            _ => return false,
        }
        self.dirty(&SKind::Modal);
        true
    }

    pub fn keys_wheel(&mut self, d: i32) {
        self.st.keys.scroll = (self.st.keys.scroll + d * ROW_H).clamp(0, 2000);
        let cat = if self.st.keys.cat.is_empty() { "shell" } else { self.st.keys.cat };
        let n = if cat == "custom" { self.st.set.custom_binds.len() + 3 } else { BINDS.iter().filter(|b| b.cat == cat).count() };
        // видно строк: высота окна минус шапка, категории и подсказка
        let seen = (crate::settings::SIZE.1 - 64 - 100) / ROW_H;
        let max = (n as i32 - seen).max(0) * ROW_H;
        self.st.keys.scroll = self.st.keys.scroll.min(max);
        self.dirty(&SKind::Modal);
    }

    /// запись: niri отдаёт шеллу все клавиши, пока она идёт
    pub fn keys_capture(&mut self, id: Option<String>) {
        self.st.keys.capture = id;
        let on = self.st.keys.capture.is_some();
        self.inhibit_shortcuts(on);
    }

    /// клавиша во время записи; true — съедена
    pub fn keys_key(&mut self, e: &KeyEvent) -> bool {
        let Some(id) = self.st.keys.capture.clone() else { return false };
        if is_modifier(e.keysym) {
            return true;
        }
        let m = self.mods;
        let bare = !(m.ctrl || m.alt || m.logo || m.shift);
        let key = if bare && e.keysym == Keysym::Escape {
            None
        } else if bare && e.keysym == Keysym::BackSpace {
            Some(String::new())
        } else {
            key_name(e).map(|k| combo(&m, &k))
        };
        self.keys_capture(None);
        if let Some(key) = key {
            self.assign(&id, key);
        }
        self.dirty(&SKind::Modal);
        true
    }

    fn assign(&mut self, id: &str, key: String) {
        if id == "new" {
            self.st.keys.new_key = key;
            return;
        }
        if !key.is_empty() {
            self.free_key(&key, id);
        }
        if let Some(i) = id.strip_prefix("custom:").and_then(|n| n.parse::<usize>().ok()) {
            if let Some(c) = self.st.set.custom_binds.get_mut(i) {
                c.key = key;
            }
        } else if let Some(b) = BINDS.iter().find(|b| b.id == id) {
            if same_key(&key, b.key) {
                self.st.set.binds.remove(id);
            } else {
                self.st.set.binds.insert(id.to_string(), key);
            }
        }
        self.binds_changed();
    }

    /// сочетание занято другим — снять его там и сказать, откуда
    fn free_key(&mut self, key: &str, except: &str) {
        let mut freed = vec![];
        for b in BINDS {
            if b.id != except && same_key(&key_of(&self.st.set, b), key) {
                self.st.set.binds.insert(b.id.to_string(), String::new());
                freed.push(title_of(b).to_string());
            }
        }
        for (i, c) in self.st.set.custom_binds.iter_mut().enumerate() {
            if format!("custom:{i}") != except && same_key(&c.key, key) {
                c.key.clear();
                freed.push(c.cmd.clone());
            }
        }
        self.st.keys.note = if freed.is_empty() { String::new() } else { crate::tf!("{key} снято с: {}", "{key} taken from: {}", "{key} removido de: {}", freed.join(", ")) };
    }

    /// settings_changed перепишет yami-binds.kdl
    pub fn binds_changed(&mut self) {
        self.settings_changed();
    }
}

#[cfg(test)]
mod tests {
    /// Ctrl+буква: QWERTY и русская — по положению, AZERTY и Dvorak — по подписи на клавише
    #[test]
    fn latin_letters_in_any_layout() {
        use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
        let ev = |code: u32, sym: Keysym| KeyEvent { time: 0, raw_code: code, keysym: sym, utf8: None };
        // evdev 30 — место «A» в QWERTY; 16 — место «Q»; 47 — «V»; 52 — «.»
        assert_eq!(super::latin_letter(&ev(30, Keysym::a)), Some('A'));
        assert_eq!(super::latin_letter(&ev(30, Keysym::Cyrillic_ef)), Some('A'));
        assert_eq!(super::latin_letter(&ev(47, Keysym::Cyrillic_em)), Some('V'));
        assert_eq!(super::latin_letter(&ev(16, Keysym::a)), Some('A'), "AZERTY: A where QWERTY has Q");
        assert_eq!(super::latin_letter(&ev(30, Keysym::q)), Some('Q'), "AZERTY: Q where QWERTY has A");
        assert_eq!(super::latin_letter(&ev(52, Keysym::v)), Some('V'), "Dvorak: V where QWERTY has the period");
        assert_eq!(super::latin_letter(&ev(30, Keysym::A)), Some('A'), "Caps Lock");
        assert_eq!(super::latin_letter(&ev(2, Keysym::_1)), None);
    }

    #[test]
    fn keys_cannot_inject() {
        assert!(super::key_ok("Mod+Shift+Return") && super::key_ok("XF86AudioRaiseVolume"));
        assert!(!super::key_ok("Mod+T { spawn \"sh\"; }\nMod+Y") && !super::key_ok("") && !super::key_ok("Mod+"));
        let mut s = crate::config::Settings::default();
        s.binds.insert(super::BINDS[0].id.into(), "Mod+X { spawn \"evil\"; }".into());
        s.custom_binds.push(super::Custom { key: "Mod+Q\"".into(), cmd: "evil".into() });
        let k = super::kdl(&s);
        assert!(!k.contains("evil"));
    }
}
