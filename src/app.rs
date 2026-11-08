// Ядро: Wayland (layer-shell, ввод, кадры), поверхности шелла, цикл событий, маршрутизация
// кликов в действия. Состояние, которое рисуется, — в State; ресурсы (шрифты, иконки) — в Res.

use crate::config::{self, Settings};
use crate::gfx::R;
use crate::icons::{self, Icons};
use crate::niri::{self, Niri};
use crate::sys::Sys;
use crate::theme::Theme;
use crate::ui::{Act, Field, Zone};
use crate::{audio, dbus, font::Fonts, media, menu, net, notif};

use calloop::{
    channel::{self, Sender},
    timer::{TimeoutAction, Timer},
    EventLoop, LoopHandle,
};
use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
    compositor::{CompositorState, Region},
    output::OutputState,
    registry::RegistryState,
    seat::{
        keyboard::Modifiers,
        pointer::cursor_shape::CursorShapeManager, SeatState,
    },
    session_lock::{SessionLock, SessionLockState, SessionLockSurface},
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm},
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer},
    Connection, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::{ext_background_effect_manager_v1, ext_background_effect_surface_v1};
use wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape, WpCursorShapeDeviceV1};
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1 as frac_mgr, wp_fractional_scale_v1 as frac};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};

pub const BAR_H: i32 = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modal {
    Power,
    /// ответ из уведомления (reply.rs)
    Reply,
    /// выбор обоев (walls.rs)
    Walls,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SKind {
    Wall,
    Bar,
    Dim,
    Catcher,
    Modal,
    Popup,
    Menu,
    Notifs,
    Osd,
    Widget(u32),
    /// окно настроек (xdg-shell, не слой)
    Settings,
    EditBar,
    /// подсказка у индикатора приватности (privacy.rs)
    Tip,
    Dock,
    DockEdge,
}

pub struct Surf {
    pub kind: SKind,
    pub layer: crate::shellsurf::Shell,
    pub output: String,
    pub w: i32,
    pub h: i32,
    pub configured: bool,
    pub pool: Option<SlotPool>,
    pub dirty: bool,
    pub frame_pending: bool,
    pub anim: bool,
    pub zones: Vec<Zone>,
    pub closing: bool,
    pub born: Instant,
    pub viewport: Option<wp_viewport::WpViewport>,
    pub margin: (i32, i32),
    /// где поверхность сейчас у композитора (подтверждено wl_display.sync): события
    /// курсора приходят относительно именно этого места, а не только что запрошенного
    pub applied: (i32, i32),
    pub input: Option<Vec<R>>,
    pub input_set: Option<Vec<R>>,
    /// обои: буфер отдан и больше не нужен
    pub release_pool: bool,
    /// ещё ни разу не рисовалась: анимация появления считается от первого кадра
    pub fresh: bool,
    /// размытие фона через ext-background-effect: объект и отданный композитору регион
    pub fx: Option<ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1>,
    pub blur_set: Vec<R>,
    /// док: у какого края стоит
    pub anchor_side: String,
    /// физических пикселей на логический (wp_fractional_scale): буфер рисуется в нём,
    /// логический холст увеличивается ближайшим соседом — на HiDPI пиксели чёткие
    pub scale: f64,
    pub frac: Option<frac::WpFractionalScaleV1>,
    /// отпечаток последнего отданного кадра (0 — не было): тот же кадр второй раз не отдаётся —
    /// бар перерисовывается раз в секунду, а меняется обычно раз в минуту
    pub drawn: u64,
}

pub struct LockSurf {
    pub s: SessionLockSurface,
    /// монитор: по нему видно, у какого вывода экрана блокировки ещё нет
    pub wl: wl_output::WlOutput,
    pub output: String,
    pub w: i32,
    pub h: i32,
    pub pool: Option<SlotPool>,
    pub zones: Vec<Zone>,
    pub dirty: bool,
    pub scale: f64,
    pub frac: Option<frac::WpFractionalScaleV1>,
    pub vp: Option<wp_viewport::WpViewport>,
}

pub enum Msg {
    Niri(serde_json::Value),
    Ipc(String),
    Audio(audio::AudioState),
    Viz(Vec<f32>),
    Media(media::MediaState),
    /// живые плееры MPRIS (имена без «org.mpris.MediaPlayer2.»)
    Players(Vec<String>),
    Notif(notif::Incoming),
    NotifClose(u32),
    Tray(dbus::TrayEv),
    TrayMenu(String, Vec<menu::MItem>),
    Net(net::NetState),
    Bt(net::BtState),
    Pam(bool),
    /// яркость и лампочки клавиатуры (leds.rs)
    Leds(crate::leds::Leds),
    /// кто держит камеру (camera.rs)
    Camera(Vec<String>),
    /// Wallhaven: страница найденного (номер запроса), миниатюра, скачанная и поставленная картинка
    WhResults(u64, Result<(Vec<crate::wallhaven::Item>, u32), String>),
    WhThumb(u64, String, Option<crate::gfx::Img>),
    WhApplied(Result<std::path::PathBuf, String>),
    Art(String, Option<crate::gfx::Img>),
    /// текст песни нашёлся (или нет) для трека Lyrics.key
    Lyrics(crate::lyrics::Lyrics),
    /// свои темы (themetab.rs): список, что сделано, применена ли тема (перечитать настройки)
    Themes(Vec<crate::bundle::Card>, String, bool),
    /// обои готовы (wall.rs): номер задания, картинка, акцент из неё
    Wall(u64, Option<crate::gfx::Img>, Option<u32>),
    /// папка выбора обоев прочитана (walls.rs): номер открытия, папка, картинки, сколько сверх MAX
    WallsListed(u64, std::path::PathBuf, Vec<std::path::PathBuf>, Vec<std::path::PathBuf>, usize),
    /// миниатюра для выбора обоев (walls.rs): номер открытия, клетка, картинка
    WallThumb(u64, usize, Option<crate::gfx::Img>),
}

pub struct Res {
    pub fonts: Fonts,
    pub icons: Icons,
    /// семейство, из которого собраны fonts (fontShell)
    pub font: String,
    /// шрифты виджетов стола (fontWidgets; "" — как у шелла) и их семейство
    pub wfonts: Fonts,
    pub wfont: String,
    /// набор значков системы, из которого собраны icons
    pub iset: String,
}

pub struct Popup {
    pub kind: &'static str,
}

pub struct State {
    pub set: Settings,
    pub t: Theme,
    pub niri: Niri,
    pub sys: Sys,
    pub apps: Vec<icons::App>,
    pub modal: Option<Modal>,
    pub popup: Option<Popup>,
    pub menu: Option<menu::MenuState>,
    pub bar_hidden: bool,
    pub power_cur: usize,
    pub stab: &'static str,
    /// прокрутка содержимого настроек: сдвиг и вкладка, к которой он относится
    pub sscroll: (i32, &'static str),
    /// высота содержимого открытой вкладки и списка вкладок с прошлого кадра
    pub sscroll_h: std::cell::Cell<i32>,
    pub stab_scroll: i32,
    pub stab_h: std::cell::Cell<i32>,
    pub hex: Field,
    pub field_focus: Option<&'static str>,
    pub bar_ed: crate::settings::BarEd,
    pub sel_widget: u32,
    pub editing: bool,
    /// окно настроек открыто (бар подсвечивает шестерёнку)
    pub settings_shown: bool,
    /// выбор обоев открыт из модуля бара: под баром у курсора, без затемнения (закрывается ловцом)
    pub walls_anchored: bool,
    pub notifs: notif::Notifs,
    pub osd: Option<(&'static str, Instant)>,
    pub leds: crate::leds::Leds,
    pub camera: Vec<String>,
    /// ползунок панели управления тянут: до этого времени на экране — его значение (ответ системы
    /// приходит позже и со старым), и OSD громкости и яркости молчит
    pub ctl_hold: Option<Instant>,
    pub wh: crate::wallhaven::Wh,
    /// поля ключа Wallhaven и его папки во вкладке «обои»
    pub wh_key_field: Field,
    pub wh_dir_field: Field,
    /// подсказка индикатора приватности: что и над какой зоной
    pub tip: Option<(&'static str, R)>,
    pub audio: audio::AudioState,
    /// показанный плеер (media_pick_sync), все живые и выбранный в шапке всплывашки (#24)
    pub media: media::MediaState,
    pub players: Vec<media::MediaState>,
    pub media_pick: String,
    pub art: HashMap<String, Option<std::rc::Rc<crate::gfx::Img>>>,
    /// текст песни текущего трека (#13): ищется, только когда всплывашка его показывает
    pub lyrics: crate::lyrics::Lyrics,
    /// вкладка «цвет»: страница «темы», свои темы и последнее действие с ними (themetab.rs)
    /// раздел вкладки «цвет»: 0 — палитра, 1 — свои темы, 2 — программы (#17)
    pub color_page: u8,
    pub themes: Vec<crate::bundle::Card>,
    pub theme_note: String,
    pub viz: Vec<f32>,
    /// куда тянут полосу позиции (доля трека); перемотка — когда отпустят
    pub seek: Option<f32>,
    pub net: net::NetState,
    pub bt: net::BtState,
    pub wifi_pass: Field,
    pub wifi_target: String,
    /// о низком заряде уже сказали: 1 — на 10%, 2 — на 5%; сбрасывается на зарядке
    pub bat_warn: u8,
    pub tray: Vec<dbus::TrayItem>,
    pub locked: bool,
    pub lock_field: Field,
    pub lock_status: String,
    pub lock_fails: u32,
    pub lock_shake: Option<Instant>,
    pub cal_month: i32,
    pub blink: bool,
    pub wall: Option<std::rc::Rc<crate::gfx::Img>>,
    /// в wall лежит уменьшенная копия (wall_park): полная уже у niri
    pub wall_parked: bool,
    pub wall_stamp: u64,
    /// смена обоев: прошлая картинка, когда началась, и кадр смеси (шаг, картинка)
    pub wall_prev: Option<std::rc::Rc<crate::gfx::Img>>,
    pub wall_fade: Option<Instant>,
    pub wall_mix: std::cell::RefCell<Option<(i32, std::rc::Rc<crate::gfx::Img>)>>,
    /// последний показанный шаг смены: за кадр — не больше одного (иначе медленный кадр
    /// проглатывает шаги и смена «перескакивает»)
    pub wall_step: std::cell::Cell<i32>,
    pub started: Instant,
    pub rss: f32,
    pub wall_field: Field,
    pub game: crate::games::GameMode,
    pub night: crate::night::Night,
    /// виджет без подложки стоит на светлых обоях (widgets.rs, widget_contrast)
    pub widget_bg: HashMap<u32, bool>,
    /// окно ответа из уведомления
    pub reply: crate::reply::Reply,
    /// координаты для ночного света
    pub geo_field: Field,
    /// своя команда лаунчера (настройки → бар)
    pub launcher_field: Field,
    /// список шрифтов во вкладке «вид»: открыт ли, поиск, первая видимая строка
    /// какой список открыт: font_shell или font_widgets
    pub font_open: Option<&'static str>,
    pub font_search: Field,
    pub font_scroll: usize,
    pub game_field: Field,
    pub dock_hover: bool,
    pub dock_leave: Option<Instant>,
    pub dock_bounce: Vec<(String, Instant)>,
    /// наведение в доке для анимации: (сейчас, до этого, когда сменилось)
    pub dock_hov: std::cell::Cell<(Option<usize>, Option<usize>, Instant)>,
    pub trash_full: bool,
    /// столы-точки: активный стол сейчас, прошлый и когда сменился (анимация в баре)
    pub ws_anim: std::cell::Cell<(u64, u64, Instant)>,
    /// выбор обоев: папка, клетки, миниатюры видимых строк
    pub walls: crate::walls::Walls,
}

pub struct Press {
    pub sid: u32,
    pub act: Act,
    pub button: u32,
    pub x: f64,
    pub y: f64,
    pub zone: R,
    /// абсолютная позиция на экране в момент нажатия (для перетаскивания)
    pub ax: f64,
    pub ay: f64,
    pub moved: bool,
    pub start: (i32, i32),
    /// где стояла поверхность при нажатии: пока кнопка зажата, композитор (smithay
    /// ClickGrab) считает координаты курсора от этого места, даже если поверхность уехала
    pub origin: (i32, i32),
}

pub struct App {
    pub conn: Connection,
    pub qh: QueueHandle<App>,
    pub handle: LoopHandle<'static, App>,
    pub tx: Sender<Msg>,
    pub(crate) registry: RegistryState,
    pub(crate) seat: SeatState,
    pub outputs: OutputState,
    pub(crate) compositor: CompositorState,
    pub(crate) layer_shell: LayerShell,
    /// обычные окна (настройки); нет xdg-shell — настройки не открываются
    pub(crate) xdg: Option<smithay_client_toolkit::shell::xdg::XdgShell>,
    pub(crate) shm: Shm,
    pub(crate) cursor_mgr: Option<CursorShapeManager>,
    pub(crate) cursor_dev: Option<WpCursorShapeDeviceV1>,
    pub(crate) cursor_now: Option<Shape>,
    pub(crate) viewporter: Option<wp_viewporter::WpViewporter>,
    pub(crate) fx_mgr: Option<ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1>,
    pub(crate) frac_mgr: Option<frac_mgr::WpFractionalScaleManagerV1>,
    /// логический холст перед увеличением (HiDPI); большой — освобождается сразу
    pub(crate) scratch: Vec<u32>,
    /// когда отдать ядру код, который больше не исполняется (yami_px::shed_code): через 30 с после
    /// старта и после закрытия настроек или выбора обоев (их код велик, а нужен изредка)
    pub(crate) shed_at: Option<Instant>,
    pub(crate) lock_state: SessionLockState,
    pub lock: Option<SessionLock>,
    pub lock_surfs: Vec<LockSurf>,
    pub(crate) idle: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    pub(crate) idle_notes: Vec<ext_idle_notification_v1::ExtIdleNotificationV1>,
    pub(crate) idle_cfg: (u32, u32),
    pub(crate) pointer: Option<wl_pointer::WlPointer>,
    pub(crate) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(crate) enter_serial: u32,
    pub surfs: HashMap<u32, Surf>,
    pub(crate) next_id: u32,
    /// курсор: (поверхность, x, y); для экрана блокировки sid = u32::MAX - n
    pub ptr: Option<(u32, f64, f64)>,
    pub press: Option<Press>,
    pub kb_surf: Option<u32>,
    pub mods: Modifiers,
    pub res: Res,
    pub st: State,
    pub set_saved: String,
    /// настройки на момент последней сверки с файлом: пока они те же, JSON не собирается
    /// (flush идёт на каждом проходе цикла — при эквалайзере 30 раз в секунду)
    pub(crate) set_seen: Settings,
    pub(crate) set_mtime: Option<std::time::SystemTime>,
    pub(crate) last_sample: Instant,
    pub(crate) last_min: u32,
    pub(crate) palette_written: String,
    /// обои: ключ того, из чего собрана текущая картинка, и номер последнего задания
    pub wall_key: String,
    pub wall_job: u64,
    /// когда пришла последняя картинка обоев; идёт пересчёт отложенной (wall_need)
    pub wall_at: Instant,
    pub wall_reload: bool,
    /// иероглиф рисуется поверх готовой картинки: его переключение — только перерисовка
    pub wall_kanji: bool,
    pub quit: bool,
    /// выйти и запустить себя заново (тот же путь: после обновления — новый бинарь)
    pub restart: bool,
    /// где открыть меню программы трея: монитор, x, y
    /// где открыть меню программы трея: монитор, x, y и вверх ли от y (из списка трея — над курсором)
    pub tray_anchor: Option<(String, i32, i32, bool)>,
    /// окно настроек сдвинуто за заголовок: его поверхность и левый верхний угол; mdrag —
    /// угол в начале текущего перетаскивания
    pub modal_at: Option<(u32, (i32, i32))>,
    pub mdrag: Option<(i32, i32)>,
}

pub(crate) const LOCK_BASE: u32 = u32::MAX - 64;

impl State {
    /// «не беспокоить» на деле: включено руками, игра в фокусе или идёт показ экрана — после игры и
    /// демонстрации уведомления снова приходят сами
    pub fn quiet(&self) -> bool {
        self.set.dnd || (self.set.game_dnd && (self.game.game || self.game.fullscreen)) || (self.set.cast_dnd && self.niri.casting)
    }
}

impl State {
    pub fn new(set: Settings, niri: Niri) -> State {
        yami_px::gfx::set_reveal(yami_px::gfx::Reveal::from_id(&set.reveal), set.reveal_grain);
        State {
            t: Theme::new(set.accent_rgb(), set.tint, set.glass, set.lift, set.inner, set.light).apply(),
            niri,
            sys: Sys::new(),
            apps: icons::load_apps(),
            modal: None,
            popup: None,
            menu: None,
            bar_hidden: false,
            power_cur: 0,
            stab: "color",
            sscroll: (0, ""),
            sscroll_h: Default::default(),
            stab_scroll: 0,
            stab_h: Default::default(),
            hex: Field::default(),
            field_focus: None,
            bar_ed: Default::default(),
            sel_widget: 0,
            editing: false,
            settings_shown: false,
            walls_anchored: false,
            notifs: notif::Notifs::default(),
            osd: None,
            leds: Default::default(),
            camera: vec![],
            ctl_hold: None,
            wh: Default::default(),
            wh_key_field: Field::default(),
            wh_dir_field: Field::default(),
            tip: None,
            audio: audio::AudioState::default(),
            media: media::MediaState::default(),
            players: Vec::new(),
            media_pick: String::new(),
            art: HashMap::new(),
            lyrics: Default::default(),
            color_page: 0,
            themes: vec![],
            theme_note: String::new(),
            viz: vec![],
            seek: None,
            net: net::NetState::default(),
            bt: net::BtState::default(),
            wifi_pass: Field::default(),
            wifi_target: String::new(),
            bat_warn: 0,
            tray: vec![],
            locked: false,
            lock_field: Field::default(),
            lock_status: String::new(),
            lock_fails: 0,
            lock_shake: None,
            cal_month: 0,
            blink: true,
            wall: None,
            wall_parked: false,
            wall_stamp: 0,
            wall_prev: None,
            wall_fade: None,
            wall_mix: std::cell::RefCell::new(None),
            wall_step: std::cell::Cell::new(0),
            started: Instant::now(),
            rss: 0.0,
            wall_field: Field::default(),
            game: Default::default(),
            night: Default::default(),
            widget_bg: HashMap::new(),
            reply: Default::default(),
            geo_field: Field::default(),
            launcher_field: Field::default(),
            font_open: None,
            font_search: Field::default(),
            font_scroll: 0,
            game_field: Field::default(),
            dock_hover: false,
            dock_leave: None,
            dock_bounce: vec![],
            dock_hov: std::cell::Cell::new((None, None, Instant::now())),
            trash_full: false,
            ws_anim: std::cell::Cell::new((0, 0, Instant::now())),
            walls: Default::default(),
            set,
        }
    }
}


pub fn run() {
    crate::proc::note_inherited();
    // из TTY или по ssh: понятная строка вместо паники
    let Ok(conn) = Connection::connect_to_env() else {
        eprintln!("yami-lite: no Wayland display (start it inside niri, or set WAYLAND_DISPLAY)");
        std::process::exit(1);
    };
    let (globals, mut queue) = registry_queue_init::<App>(&conn).expect("wayland registry");
    let qh = queue.handle();
    let mut ev: EventLoop<'static, App> = EventLoop::try_new().expect("event loop");
    let handle = ev.handle();

    let (tx, rx) = channel::channel::<Msg>();
    handle
        .insert_source(rx, |e, _, app: &mut App| {
            if let channel::Event::Msg(m) = e {
                app.on_msg(m);
            }
        })
        .expect("message channel");

    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("layer-shell");
    let xdg = smithay_client_toolkit::shell::xdg::XdgShell::bind(&globals, &qh).ok();
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let cursor_mgr = CursorShapeManager::bind(&globals, &qh).ok();
    let viewporter: Option<wp_viewporter::WpViewporter> = globals.bind(&qh, 1..=1, ()).ok();
    let fx_mgr: Option<ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1> = globals.bind(&qh, 1..=1, ()).ok();
    let idle: Option<ext_idle_notifier_v1::ExtIdleNotifierV1> = globals.bind(&qh, 1..=1, ()).ok();
    let frac_mgr: Option<frac_mgr::WpFractionalScaleManagerV1> = globals.bind(&qh, 1..=1, ()).ok();

    let set = Settings::load();
    crate::i18n::set(&set.lang);
    let iset = icons::system_theme();
    let mut icons = Icons::new(&iset);
    icons.set_style(Theme::new(set.accent_rgb(), set.tint, set.glass, set.lift, set.inner, set.light).icon_ramp(), set.icon_pixel, set.icon_tint);
    let set_saved = set.to_json();
    let set_font = set.font_shell.clone();
    let set_wfont = if set.font_widgets.is_empty() { set.font_shell.clone() } else { set.font_widgets.clone() };

    let niri = Niri::new();

    let st = State::new(set, niri);

    let mut app = App {
        conn: conn.clone(),
        qh: qh.clone(),
        handle: handle.clone(),
        tx: tx.clone(),
        registry: RegistryState::new(&globals),
        seat: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        xdg,
        shm,
        cursor_mgr,
        cursor_dev: None,
        cursor_now: None,
        viewporter,
        fx_mgr,
        frac_mgr,
        scratch: vec![],
        shed_at: Some(Instant::now() + Duration::from_secs(30)),
        lock_state: SessionLockState::new(&globals, &qh),
        lock: None,
        lock_surfs: vec![],
        idle,
        idle_notes: vec![],
        idle_cfg: (0, 0),
        pointer: None,
        keyboard: None,
        enter_serial: 0,
        surfs: HashMap::new(),
        next_id: 1,
        ptr: None,
        press: None,
        kb_surf: None,
        mods: Modifiers::default(),
        res: Res { fonts: Fonts::with(&set_font), wfonts: Fonts::with(&set_wfont), wfont: set_wfont, icons, font: set_font, iset },
        st,
        set_saved,
        set_seen: Settings::default(),
        set_mtime: std::fs::metadata(config::settings_path()).and_then(|m| m.modified()).ok(),
        last_sample: Instant::now(),
        last_min: 99,
        palette_written: String::new(),
        wall_key: String::new(),
        wall_job: 0,
        wall_at: Instant::now(),
        wall_reload: false,
        wall_kanji: true,
        quit: false,
        restart: false,
        tray_anchor: None,
        modal_at: None,
        mdrag: None,
    };

    // нити: niri, IPC, D-Bus (уведомления, трей), звук, музыка
    {
        let tx = tx.clone();
        niri::spawn_stream(move |v| {
            let _ = tx.send(Msg::Niri(v));
        });
    }
    crate::ipc::serve(tx.clone());
    dbus::spawn(tx.clone(), app.st.set.agent_notif, app.st.set.agent_tray);
    audio::spawn(tx.clone());
    media::spawn(tx.clone());
    net::spawn_bt(tx.clone());
    crate::leds::spawn(tx.clone());
    crate::camera::spawn(tx.clone());
    app.write_palette();
    // обои: при старте палитра обычно та же, theme.py не запускается — готовим картинку сами
    app.sync_wall();
    app.setup_idle();
    app.side_sync();

    handle
        .insert_source(Timer::from_duration(Duration::from_millis(300)), |_, _, app: &mut App| {
            app.tick();
            TimeoutAction::ToDuration(app.tick_interval())
        })
        .expect("timer");

    // мониторы, сиденье: первые события — сразу, до цикла
    let _ = queue.roundtrip(&mut app);
    let _ = queue.roundtrip(&mut app);
    WaylandSource::new(conn.clone(), queue).insert(handle.clone()).expect("wayland source");
    app.flush();
    loop {
        if ev.dispatch(Some(Duration::from_secs(5)), &mut app).is_err() {
            break;
        }
        app.flush();
        if app.quit || app.restart {
            break;
        }
    }
    if app.restart {
        app.save_now();
        // exec вместо выхода: петля автозапуска niri после чистого выхода не перезапускает
        use std::os::unix::process::CommandExt;
        let args: Vec<String> = std::env::args().skip(1).collect();
        drop(app);
        let e = std::process::Command::new(yami_px::paths::self_exe()).args(args).env("YAMI_RESTARTED", "1").exec();
        eprintln!("restart: {e}");
        std::process::exit(1);
    }
}

impl App {
    // ── поверхности ─────────────────────────────────────────────────────────
    pub fn output_obj(&self, name: &str) -> Option<wl_output::WlOutput> {
        self.outputs.outputs().find(|o| self.outputs.info(o).and_then(|i| i.name.clone()).as_deref() == Some(name))
    }
    pub fn output_names(&self) -> Vec<String> {
        self.outputs.outputs().filter_map(|o| self.outputs.info(&o).and_then(|i| i.name.clone())).collect()
    }
    pub fn output_size(&self, name: &str) -> (i32, i32) {
        self.outputs
            .outputs()
            .filter_map(|o| self.outputs.info(&o))
            .find(|i| i.name.as_deref() == Some(name))
            .and_then(|i| i.logical_size)
            .unwrap_or((1920, 1080))
    }
    /// монитор, на котором сейчас работает человек
    pub fn focus_out(&self) -> String {
        let f = self.st.niri.focused_output();
        if !f.is_empty() && self.output_names().contains(&f) {
            return f;
        }
        self.output_names().into_iter().next().unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mk(&mut self, kind: SKind, output: &str, layer: Layer, ns: &str, anchor: Anchor, size: (i32, i32), margin: (i32, i32, i32, i32), excl: i32, kb: KeyboardInteractivity) -> u32 {
        let surface = self.compositor.create_surface(&self.qh);
        let out = self.output_obj(output);
        let ls = self.layer_shell.create_layer_surface(&self.qh, surface, layer, Some(ns), out.as_ref());
        ls.set_anchor(anchor);
        ls.set_size(size.0.max(0) as u32, size.1.max(0) as u32);
        ls.set_margin(margin.0, margin.1, margin.2, margin.3);
        ls.set_exclusive_zone(excl);
        ls.set_keyboard_interactivity(kb);
        // viewport у всех: Dim/Catcher растягивают буфер 1×1, остальные — буфер в физических пикселях
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(ls.wl_surface(), &self.qh, ()));
        let id = self.next_id;
        self.next_id += 1;
        let frac = self.frac_mgr.as_ref().map(|m| m.get_fractional_scale(ls.wl_surface(), &self.qh, id));
        ls.commit();
        self.add_surf(id, kind, crate::shellsurf::Shell::Layer(ls), output, size, margin, viewport, frac);
        id
    }

    /// окно xdg-shell (настройки): ставит его niri, размер приходит в configure. Без xdg-shell — None
    pub fn mk_window(&mut self, kind: SKind, app_id: &str, title: &str, min: (i32, i32)) -> Option<u32> {
        let xdg = self.xdg.as_ref()?;
        let surface = self.compositor.create_surface(&self.qh);
        let win = xdg.create_window(surface, smithay_client_toolkit::shell::xdg::window::WindowDecorations::RequestServer, &self.qh);
        win.set_app_id(app_id);
        win.set_title(crate::i18n::tr(title));
        win.set_min_size(Some((min.0 as u32, min.1 as u32)));
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(win.wl_surface(), &self.qh, ()));
        let id = self.next_id;
        self.next_id += 1;
        let frac = self.frac_mgr.as_ref().map(|m| m.get_fractional_scale(win.wl_surface(), &self.qh, id));
        win.commit();
        let out = self.focus_out();
        self.add_surf(id, kind, crate::shellsurf::Shell::Window(win), &out, min, (0, 0, 0, 0), viewport, frac);
        Some(id)
    }

    #[allow(clippy::too_many_arguments)]
    fn add_surf(&mut self, id: u32, kind: SKind, layer: crate::shellsurf::Shell, output: &str, size: (i32, i32), margin: (i32, i32, i32, i32), viewport: Option<wp_viewport::WpViewport>, frac: Option<frac::WpFractionalScaleV1>) {
        self.surfs.insert(
            id,
            Surf {
                kind,
                layer,
                output: output.to_string(),
                w: size.0,
                h: size.1,
                configured: false,
                pool: None,
                dirty: true,
                frame_pending: false,
                anim: false,
                zones: vec![],
                closing: false,
                born: Instant::now(),
                viewport,
                margin: (margin.3, margin.0),
                applied: (margin.3, margin.0),
                input: None,
                input_set: None,
                release_pool: false,
                fresh: true,
                fx: None,
                blur_set: vec![],
                anchor_side: String::new(),
                scale: 1.0,
                frac,
                drawn: 0,
            },
        );
    }

    pub fn find(&self, k: &SKind) -> Vec<u32> {
        self.surfs.iter().filter(|(_, s)| &s.kind == k && !s.closing).map(|(i, _)| *i).collect()
    }
    pub fn find_on(&self, k: &SKind, out: &str) -> Option<u32> {
        self.surfs.iter().find(|(_, s)| &s.kind == k && s.output == out && !s.closing).map(|(i, _)| *i)
    }
    pub fn dirty(&mut self, k: &SKind) {
        for s in self.surfs.values_mut() {
            if &s.kind == k {
                s.dirty = true;
            }
        }
    }
    /// содержимое панелей сменилось: модальное окно и окно настроек
    pub fn dirty_panels(&mut self) {
        self.dirty_where(|k| matches!(k, SKind::Modal | SKind::Settings));
    }
    pub fn dirty_where(&mut self, f: impl Fn(&SKind) -> bool) {
        for s in self.surfs.values_mut() {
            if f(&s.kind) {
                s.dirty = true;
            }
        }
    }
    /// всё, что рисует состояние (кроме обоев)
    pub fn dirty_all(&mut self) {
        self.dirty_where(|k| *k != SKind::Wall);
        for l in self.lock_surfs.iter_mut() {
            l.dirty = true;
        }
    }
    pub fn destroy(&mut self, id: u32) {
        if let Some(s) = self.surfs.remove(&id) {
            if let Some(fx) = s.fx {
                fx.destroy();
            }
            if let Some(f) = s.frac {
                f.destroy();
            }
            if let Some(v) = s.viewport {
                v.destroy();
            }
            if self.ptr.map(|p| p.0) == Some(id) {
                self.ptr = None;
            }
            if self.kb_surf == Some(id) {
                self.kb_surf = None;
            }
            self.drop_press(id);
        }
    }

    /// поверхность ушла: нажатие на ней больше ни к чему, и перемотка, которую там тянули, не залипнет
    fn drop_press(&mut self, id: u32) {
        if self.press.as_ref().is_some_and(|p| p.sid == id) {
            self.press = None;
            self.st.seek = None;
        }
    }
    /// закрыть с анимацией (или сразу, если анимации выключены)
    pub fn close(&mut self, id: u32) {
        let anim = self.st.set.anim() && self.st.set.anim_close;
        if let Some(s) = self.surfs.get_mut(&id) {
            if !anim || !s.configured || s.layer.window().is_some() {
                self.destroy(id);
                return;
            }
            if !s.closing {
                s.closing = true;
                s.born = Instant::now();
                s.dirty = true;
                s.layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                // клики сквозь закрывающееся
                let r = Region::new(&self.compositor).ok();
                if let Some(r) = r {
                    s.layer.wl_surface().set_input_region(Some(r.wl_region()));
                }
                s.layer.commit();
            }
        }
        // закрывается посреди перемотки (Esc): бегунок — назад к игре; само нажатие живёт до
        // отпускания (тянуть заголовок окна можно и через пересоздание его поверхности)
        if self.press.as_ref().is_some_and(|p| p.sid == id) {
            self.st.seek = None;
        }
    }
    pub fn close_kind(&mut self, k: &SKind) {
        for id in self.find(k) {
            self.close(id);
        }
    }

    pub fn ensure_output_surfaces(&mut self) {
        let names = self.output_names();
        // мониторы, которых больше нет
        let gone: Vec<u32> = self.surfs.iter().filter(|(_, s)| s.layer.window().is_none() && !names.contains(&s.output)).map(|(i, _)| *i).collect();
        for id in gone {
            self.destroy(id);
        }
        for n in names {
            if self.find_on(&SKind::Wall, &n).is_none() {
                let id = self.mk(SKind::Wall, &n, Layer::Background, "yami-wall", Anchor::all(), (0, 0), (0, 0, 0, 0), -1, KeyboardInteractivity::None);
                // мышь сквозь обои: щёлкать на них нечего, а вход и выход курсора перерисовывали
                // бы всю картинку
                if let Some(s) = self.surfs.get_mut(&id) {
                    s.input = Some(vec![]);
                }
                // монитор появился: теперь известен его размер (обои режутся под него)
                self.sync_wall();
            }
        }
        self.sync_bar_outputs();
        // монитор сменил размер: ширина бара в процентах — от новой
        self.sync_bar();
        self.sync_widgets();
        self.sync_dock();
        self.sync_lock_surfs();
    }

    pub fn toggle_bar(&mut self) {
        self.st.bar_hidden = !self.st.bar_hidden;
        if self.st.bar_hidden {
            for id in self.find(&SKind::Bar) {
                self.destroy(id);
            }
        }
        self.ensure_output_surfaces();
    }

    pub fn close_all(&mut self) {
        self.close_modal();
        self.close_popup();
        self.close_menu();
    }
}
