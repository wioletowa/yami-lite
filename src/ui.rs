// Действия панелей шелла. Сам немедленный UI (кнопки, поля, зоны) — общий, в yami-px:
// им же рисуют файловый менеджер и диалоги.

pub use yami_px::ui::{Cur, Field, Tone};

pub type Ui<'a, 'b> = yami_px::ui::Ui<'a, 'b, Act>;
pub type Zone = yami_px::ui::Zone<Act>;

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// глушит клики (фон панели)
    Block,
    CloseModal,
    // бар
    Launcher,
    Power,
    Settings,
    Ws(u64),
    Task(u64),
    Tray(String),
    Kb,
    Popup(&'static str),
    // питание
    PowerDo(usize),
    // настройки
    STab(&'static str),
    /// колесо над содержимым вкладки и над списком вкладок
    SScroll,
    SSideScroll,
    SPreset(usize),
    SBool(&'static str),
    /// настройки → цвет → программы: красить ли эту программу (recolor::APPS)
    SThemeApp(&'static str),
    SSlider(&'static str, f32, f32, i32),
    SCycle(&'static str),
    /// число в пикселях (config::NUMS): на шаг вниз (-1) или вверх (+1); колесо — тоже
    SNum(&'static str, i32),
    /// вкладка «обои» (wall.rs)
    SWall(&'static str),
    /// выбор обоев (walls.rs): клетка, колесо над сеткой, назад
    WallPick(usize),
    WallsScroll,
    WallsBack,
    /// модуль бара во вкладке «бар»: тащить (сторона, индекс), новый, убрать
    SModDrag(usize, usize),
    SModNew(String),
    NightToggle,
    ReplySend,
    SModDel(usize, usize),
    /// свои темы: применить, удалить (themetab.rs)
    STheme(usize),
    SThemeDel(usize),
    SWidgetAdd(&'static str),
    SWidgetDel(u32),
    SWidgetOpt(u32, &'static str),
    SWidgetSel(u32),
    SEdit,
    SResetPos,
    SField(&'static str),
    // меню
    Menu(usize),
    MenuBack,
    // всплывашки
    CalMonth(i32),
    VolSlider,
    VolMute,
    Sink(String),
    /// микрофон по умолчанию (всплывашка звука, #10)
    Source(String),
    MicSlider,
    MicMute,
    AppVol(u32),
    AppMute(u32),
    MediaPlay,
    MediaNext,
    MediaPrev,
    /// перемотка по полосе позиции (тянуть; сама перемотка — при отпускании)
    MediaSeek,
    /// вид всплывашки музыки — следующий (#13)
    MediaView,
    /// шапка всплывашки музыки: другой плеер (#24)
    MediaPlayer,
    Wifi(bool),
    WifiNet(String),
    WifiRescan,
    NetEditor,
    BtPower(bool),
    BtDev(String),
    BtScan,
    /// полный менеджер bluetooth (blueman или bluetoothctl в терминале)
    BtManager,
    Dnd,
    NotifClear,
    NotifDel(u32),
    NotifAct(u32, String),
    NotifClick(u32),
    Pavu,
    SysMon,
    /// заголовок окна настроек: тянуть — окно едет
    MDrag,
    // виджеты
    WDrag(u32),
    WMenu(u32),
    EditDone,
    DockItem(usize),
    DockLauncher,
    DockTrash,
    DockPin(String, i32),
    LockField,
}

impl Act {
    pub fn is_drag(&self) -> bool {
        matches!(self, Act::SSlider(..) | Act::VolSlider | Act::MicSlider | Act::AppVol(_) | Act::MediaSeek | Act::MDrag | Act::WDrag(_) | Act::SModDrag(..) | Act::SModNew(_))
    }
}

impl yami_px::ui::Action for Act {
    fn block() -> Self {
        Act::Block
    }
}
