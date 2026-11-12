// Модули, что собираются по выбору (cargo features, install.sh --fan --wallhaven …). Без фичи модуль
// выключен во всех входах: в баре, всплывашках, настройках, командах, нитях — его код недостижим, и
// компилятор (LTO) его выбрасывает из бинарника.

/// вентилятор ThinkPad (thfn-gtk): модуль бара и всплывашка
pub const FAN: bool = cfg!(feature = "fan");
/// Wallhaven: модуль бара, поиск и загрузка обоев
pub const WALLHAVEN: bool = cfg!(feature = "wallhaven");
/// рабочий стол: значки ~/Desktop, перетаскивание, буфер обмена файлов
pub const DESKTOP: bool = cfg!(feature = "desktop");
/// перекраска значков папок в акцент
pub const RECOLOR: bool = cfg!(feature = "recolor");
/// приватность окон: скрытие из трансляции экрана
pub const PRIVACY: bool = cfg!(feature = "privacy");
/// визуализатор звука по краям экрана (edge.rs)
pub const VISUALIZER: bool = cfg!(feature = "visualizer");
/// буфер обмена: история копий, панель, приват (clip.rs)
pub const CLIPBOARD: bool = cfg!(feature = "clipboard");

/// модуль бара собран
pub fn module_on(id: &str) -> bool {
    match id {
        "fan" => FAN,
        "wallhaven" => WALLHAVEN,
        "privacy_hidden" => PRIVACY,
        _ => true,
    }
}

/// вид всплывашки собран
pub fn popup_on(kind: &str) -> bool {
    match kind {
        "fan" => FAN,
        "wallhaven" => WALLHAVEN,
        "hidden" => PRIVACY,
        _ => true,
    }
}

/// собранные модули — для «о шелле» и `yami-lite version`
pub fn list() -> Vec<&'static str> {
    [("fan", FAN), ("wallhaven", WALLHAVEN), ("desktop", DESKTOP), ("recolor", RECOLOR), ("privacy", PRIVACY), ("visualizer", VISUALIZER), ("clipboard", CLIPBOARD)].into_iter().filter(|m| m.1).map(|m| m.0).collect()
}
