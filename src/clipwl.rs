// Буфер обмена (модуль clipboard): своё Wayland-соединение в своей нити, протокол
// ext-data-control-v1 (или старый zwlr-data-control-v1). Новый выбор — читаем лучший тип через pipe
// (тоже в нити), готовую запись — в главный цикл (Msg::Clip). Когда шелл сам ставит выбор, он
// добавляет свой тип OWNED: такой выбор не записывается заново.
//
// Нить не ждёт ни чужую программу, ни главный цикл: чтение и отдача — в коротких нитях с тайм-аутом,
// команды приходят через канал и будят poll по eventfd.

use crate::app::Msg;
use crate::clipstore::{self, Payload, Prepared};
use calloop::channel::Sender;
use std::collections::HashMap;
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{event_created_child, Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1 as ext_device, ext_data_control_manager_v1 as ext_manager, ext_data_control_offer_v1 as ext_offer,
    ext_data_control_source_v1 as ext_source,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1 as wlr_device, zwlr_data_control_manager_v1 as wlr_manager, zwlr_data_control_offer_v1 as wlr_offer,
    zwlr_data_control_source_v1 as wlr_source,
};

/// свой тип у выбора, который поставил шелл: его не записывать
pub const OWNED: &str = "application/x-yami-owned";
/// менеджеры паролей (KeePassXC и другие) так помечают секреты
const SECRET: &str = "x-kde-passwordManagerHint";
const MAX_SIZE: usize = 64 * 1024 * 1024;
/// передача без движения столько — брошена
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const TEXT: &[&str] = &["text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "TEXT", "STRING"];

pub enum Ev {
    /// скопировали: уже прочитано, разобрано, картинка сохранена
    Captured(Prepared),
    /// выбор опустел (обычно владелец закрылся)
    Cleared,
    /// нить буфера умерла: модуль дальше не работает
    Died,
}

enum Cmd {
    Set { payload: Payload, only_if_empty: bool },
}

/// общее у главной нити и нити буфера
#[derive(Clone, Default)]
pub struct Shared {
    /// приват: содержимое даже не запрашивается
    pub private: Arc<AtomicBool>,
    /// выбора сейчас нет ни у кого
    empty: Arc<AtomicBool>,
}

impl Shared {
    pub fn is_empty(&self) -> bool {
        self.empty.load(Ordering::SeqCst)
    }
}

pub struct Handle {
    cmds: mpsc::Sender<Cmd>,
    wake: OwnedFd,
}

impl Handle {
    /// шелл — владелец выбора: отдаёт каждый тип payload
    pub fn set(&self, payload: Payload) {
        self.send(Cmd::Set { payload, only_if_empty: false });
    }

    /// как set, но только если выбор всё ещё пуст, когда нить до него дойдёт: свежее копирование
    /// программы не затирается
    pub fn restore(&self, payload: Payload) {
        self.send(Cmd::Set { payload, only_if_empty: true });
    }

    fn send(&self, c: Cmd) {
        if self.cmds.send(c).is_err() {
            return;
        }
        let one = 1u64.to_ne_bytes();
        // SAFETY: eventfd — 8 байт
        unsafe { libc::write(self.wake.as_raw_fd(), one.as_ptr().cast(), 8) };
    }
}

/// какой тип записывать: картинки, потом списки файлов, потом текст
pub fn pick_mime(mimes: &[String]) -> Option<&str> {
    let has = |m: &str| mimes.iter().any(|x| x == m);
    if has("image/png") {
        return Some("image/png");
    }
    if let Some(m) = mimes.iter().find(|m| m.starts_with("image/") && !m.contains("svg")) {
        return Some(m);
    }
    if has("text/uri-list") {
        return Some("text/uri-list");
    }
    TEXT.iter().copied().find(|m| has(m))
}

pub fn spawn(tx: Sender<Msg>, shared: Shared, blobs: PathBuf) -> Option<Handle> {
    let conn = Connection::connect_to_env().ok()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut st = State { tx: tx.clone(), shared, blobs, seat: None, manager: None, device: None, mimes: HashMap::new(), source: None };
    queue.roundtrip(&mut st).ok()?;
    let Some(seat) = st.seat.clone() else { return None };
    st.device = Some(match st.manager.as_ref() {
        Some(Manager::Ext(m)) => Device::Ext(m.get_data_device(&seat, &qh, ())),
        Some(Manager::Wlr(m)) => Device::Wlr(m.get_data_device(&seat, &qh, ())),
        None => {
            eprintln!("clipboard: {}", crate::tf!("композитор не умеет data-control", "the compositor has no data-control"));
            return None;
        }
    });
    let (ctx, crx) = mpsc::channel();
    // SAFETY: просто создаём eventfd
    let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    if fd < 0 {
        return None;
    }
    // SAFETY: fd только что создан и больше ничей
    let wake = unsafe { OwnedFd::from_raw_fd(fd) };
    let twake = wake.try_clone().ok()?;
    std::thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || {
            if let Err(e) = run(conn, queue, st, crx, twake) {
                eprintln!("clipboard: {e}");
            }
            let _ = tx.send(Msg::Clip(Ev::Died));
        })
        .ok()?;
    Some(Handle { cmds: ctx, wake })
}

fn run(conn: Connection, mut queue: EventQueue<State>, mut st: State, cmds: mpsc::Receiver<Cmd>, wake: OwnedFd) -> Result<(), Box<dyn std::error::Error>> {
    let qh = queue.handle();
    loop {
        queue.dispatch_pending(&mut st)?;
        conn.flush()?;
        let Some(guard) = queue.prepare_read() else { continue };
        let mut fds = [
            libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: wake.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: два pollfd, длина верная
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, -1) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e.into());
        }
        if fds[0].revents != 0 {
            guard.read()?;
        } else {
            drop(guard);
        }
        if fds[1].revents != 0 {
            let mut b = [0u8; 8];
            // SAFETY: читаем 8 байт счётчика eventfd
            unsafe { libc::read(wake.as_raw_fd(), b.as_mut_ptr().cast(), 8) };
            // сперва события этого прохода: empty — свежий
            queue.dispatch_pending(&mut st)?;
            while let Ok(c) = cmds.try_recv() {
                match c {
                    Cmd::Set { only_if_empty: true, .. } if !st.shared.is_empty() => {}
                    Cmd::Set { payload, .. } => st.set_selection(&qh, payload),
                }
            }
        }
    }
}

enum Manager {
    Ext(ext_manager::ExtDataControlManagerV1),
    Wlr(wlr_manager::ZwlrDataControlManagerV1),
}

enum Device {
    Ext(ext_device::ExtDataControlDeviceV1),
    Wlr(wlr_device::ZwlrDataControlDeviceV1),
}

enum Source {
    Ext(ext_source::ExtDataControlSourceV1),
    Wlr(wlr_source::ZwlrDataControlSourceV1),
}

impl Source {
    fn destroy(&self) {
        match self {
            Source::Ext(s) => s.destroy(),
            Source::Wlr(s) => s.destroy(),
        }
    }
    fn id(&self) -> ObjectId {
        match self {
            Source::Ext(s) => s.id(),
            Source::Wlr(s) => s.id(),
        }
    }
}

struct State {
    tx: Sender<Msg>,
    shared: Shared,
    blobs: PathBuf,
    seat: Option<WlSeat>,
    manager: Option<Manager>,
    device: Option<Device>,
    /// типы каждого живого предложения
    mimes: HashMap<ObjectId, Vec<String>>,
    /// выбор, которым владеет шелл, и что он отдаёт по типам
    source: Option<(Source, Payload)>,
}

impl State {
    fn set_selection(&mut self, qh: &QueueHandle<State>, payload: Payload) {
        let mut mimes: Vec<String> = payload.iter().map(|(m, _)| m.clone()).collect();
        mimes.push(OWNED.to_owned());
        let src = match (self.manager.as_ref(), self.device.as_ref()) {
            (Some(Manager::Ext(m)), Some(Device::Ext(d))) => {
                let s = m.create_data_source(qh, ());
                mimes.iter().for_each(|x| s.offer(x.clone()));
                d.set_selection(Some(&s));
                Source::Ext(s)
            }
            (Some(Manager::Wlr(m)), Some(Device::Wlr(d))) => {
                let s = m.create_data_source(qh, ());
                mimes.iter().for_each(|x| s.offer(x.clone()));
                d.set_selection(Some(&s));
                Source::Wlr(s)
            }
            _ => return,
        };
        if let Some((old, _)) = self.source.replace((src, payload)) {
            old.destroy();
        }
    }

    /// отдать свой выбор в fd, не держа нить Wayland
    fn send(&self, id: ObjectId, mime: &str, fd: OwnedFd) {
        let Some((src, payload)) = &self.source else { return };
        if src.id() != id {
            return;
        }
        let Some(data) = payload.iter().find(|(m, _)| m == mime).or(payload.first()).map(|(_, d)| d.clone()) else { return };
        std::thread::spawn(move || {
            let _ = write_all(fd, &data);
        });
    }

    fn cancelled(&mut self, id: ObjectId) {
        if self.source.as_ref().is_some_and(|(s, _)| s.id() == id) {
            if let Some((s, _)) = self.source.take() {
                s.destroy();
            }
        }
    }

    /// новый выбор: лучший тип читается в своей нити
    fn selection(&mut self, id: ObjectId, receive: impl FnOnce(&str, std::os::fd::BorrowedFd)) {
        let mimes = self.mimes.remove(&id).unwrap_or_default();
        // приват: у источника даже не спрашиваем
        if self.shared.private.load(Ordering::SeqCst) || mimes.iter().any(|m| m == OWNED) {
            return;
        }
        let Some(mime) = pick_mime(&mimes).map(str::to_owned) else { return };
        let secret = mimes.iter().any(|m| m == SECRET);
        let at = clipstore::now();
        let Ok((r, w)) = pipe() else { return };
        receive(&mime, w.as_fd());
        drop(w);
        let (tx, blobs) = (self.tx.clone(), self.blobs.clone());
        std::thread::spawn(move || {
            if let Ok(data) = read_all(r) {
                if let Some(p) = clipstore::prepare(&blobs, &mime, &data, secret, at) {
                    let _ = tx.send(Msg::Clip(Ev::Captured(p)));
                }
            }
        });
    }
}

fn pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    // SAFETY: два int под концы трубы
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: оба fd только что созданы
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// ждать готовности fd; без движения IO_TIMEOUT — ошибка
fn wait(fd: &impl AsRawFd, events: libc::c_short) -> std::io::Result<()> {
    loop {
        let mut p = libc::pollfd { fd: fd.as_raw_fd(), events, revents: 0 };
        // SAFETY: один pollfd
        match unsafe { libc::poll(&mut p, 1, IO_TIMEOUT.as_millis() as _) } {
            0 => return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "peer stopped responding")),
            n if n < 0 => {
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
            _ => return Ok(()),
        }
    }
}

/// до конца; источник завис или шлёт слишком много — бросить
fn read_all(fd: OwnedFd) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::from(fd);
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        wait(&f, libc::POLLIN)?;
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > MAX_SIZE {
            return Err(std::io::Error::other("too large"));
        }
    }
}

/// всё; читатель перестал читать — бросить
fn write_all(fd: OwnedFd, mut data: &[u8]) -> std::io::Result<()> {
    // SAFETY: флаги своего fd
    unsafe {
        let fl = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, fl | libc::O_NONBLOCK);
    }
    while !data.is_empty() {
        wait(&fd, libc::POLLOUT)?;
        // SAFETY: пишем не больше длины среза
        let n = unsafe { libc::write(fd.as_raw_fd(), data.as_ptr().cast(), data.len()) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            match e.kind() {
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted => continue,
                // читатель взял, что хотел, и закрыл трубу
                std::io::ErrorKind::BrokenPipe => return Ok(()),
                _ => return Err(e),
            }
        }
        data = &data[n as usize..];
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(st: &mut Self, reg: &wl_registry::WlRegistry, ev: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let wl_registry::Event::Global { name, interface, version } = ev else { return };
        match interface.as_str() {
            "wl_seat" if st.seat.is_none() => st.seat = Some(reg.bind(name, version.min(7), qh, ())),
            "ext_data_control_manager_v1" => st.manager = Some(Manager::Ext(reg.bind(name, 1, qh, ()))),
            "zwlr_data_control_manager_v1" if st.manager.is_none() => st.manager = Some(Manager::Wlr(reg.bind(name, version.min(2), qh, ()))),
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

/// ext и wlr отличаются только именами — один макрос на оба
macro_rules! data_control {
    ($manager:ident, $device:ident, $offer:ident, $source:ident, $M:ty, $D:ty, $O:ty, $S:ty) => {
        impl Dispatch<$M, ()> for State {
            fn event(_: &mut Self, _: &$M, _: $manager::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }

        impl Dispatch<$D, ()> for State {
            fn event(st: &mut Self, _: &$D, ev: $device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                match ev {
                    $device::Event::DataOffer { id } => {
                        st.mimes.insert(id.id(), Vec::new());
                    }
                    $device::Event::Selection { id: Some(offer) } => {
                        st.shared.empty.store(false, Ordering::SeqCst);
                        st.selection(offer.id(), |mime, fd| offer.receive(mime.to_owned(), fd));
                        offer.destroy();
                    }
                    $device::Event::Selection { id: None } => {
                        st.shared.empty.store(true, Ordering::SeqCst);
                        let _ = st.tx.send(Msg::Clip(Ev::Cleared));
                    }
                    $device::Event::PrimarySelection { id: Some(offer) } => {
                        st.mimes.remove(&offer.id());
                        offer.destroy();
                    }
                    _ => {}
                }
            }

            event_created_child!(State, $D, [
                $device::EVT_DATA_OFFER_OPCODE => ($O, ()),
            ]);
        }

        impl Dispatch<$O, ()> for State {
            fn event(st: &mut Self, offer: &$O, ev: $offer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                if let $offer::Event::Offer { mime_type } = ev {
                    st.mimes.entry(offer.id()).or_default().push(mime_type);
                }
            }
        }

        impl Dispatch<$S, ()> for State {
            fn event(st: &mut Self, src: &$S, ev: $source::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
                match ev {
                    $source::Event::Send { mime_type, fd } => st.send(src.id(), &mime_type, fd),
                    $source::Event::Cancelled => st.cancelled(src.id()),
                    _ => {}
                }
            }
        }
    };
}

data_control!(
    ext_manager,
    ext_device,
    ext_offer,
    ext_source,
    ext_manager::ExtDataControlManagerV1,
    ext_device::ExtDataControlDeviceV1,
    ext_offer::ExtDataControlOfferV1,
    ext_source::ExtDataControlSourceV1
);
data_control!(
    wlr_manager,
    wlr_device,
    wlr_offer,
    wlr_source,
    wlr_manager::ZwlrDataControlManagerV1,
    wlr_device::ZwlrDataControlDeviceV1,
    wlr_offer::ZwlrDataControlOfferV1,
    wlr_source::ZwlrDataControlSourceV1
);
