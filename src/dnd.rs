// Перетаскивание и буфер обмена для рабочего стола (wl_data_device), как в плагине Noctalia-desktop.
// Наружу: тянут значки — источник text/uri-list, под курсором — картинка всей группы (значки на
// своих местах относительно друг друга); файловый менеджер или браузер берут пути сами. Внутрь:
// на стол бросили файлы — копия (с Shift — перенос, как выберет композитор) в клетку под курсором;
// на папку — внутрь неё, на ярлык — открыть файлы его программой. Свои значки, брошенные на стол,
// переставляются (на папку — переносятся). Буфер: x-special/gnome-copied-files (Nautilus, Nemo,
// Thunar), application/x-kde-cutselection (Dolphin), text/uri-list и пути текстом.

use crate::app::{App, Msg, SKind};
use crate::gfx::{Canvas, R};
use smithay_client_toolkit::data_device_manager::data_device::{DataDeviceData, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler, DragSource};
use smithay_client_toolkit::data_device_manager::{ReadPipe, WritePipe};
use smithay_client_toolkit::reexports::client::protocol::wl_data_device::WlDataDevice;
use smithay_client_toolkit::reexports::client::protocol::wl_data_device_manager::DndAction;
use smithay_client_toolkit::reexports::client::protocol::wl_data_source::WlDataSource;
use smithay_client_toolkit::reexports::client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use std::path::PathBuf;
use wayland_client::{Connection, Proxy, QueueHandle};

const URIS: &str = "text/uri-list";
const GNOME: &str = "x-special/gnome-copied-files";
const KDE_CUT: &str = "application/x-kde-cutselection";
const TEXT: &str = "text/plain;charset=utf-8";

/// что лежит в нашем буфере
pub enum Clip {
    Files(Vec<PathBuf>, bool),
    Text(String),
}

#[derive(Default)]
pub struct Dnd {
    /// своё перетаскивание: источник, пути, картинка под курсором (поверхность и её память)
    src: Option<DragSource>,
    paths: Vec<PathBuf>,
    icon: Option<(WlSurface, SlotPool, smithay_client_toolkit::shm::slot::Buffer)>,
    /// предложение над столом (своё или чужое)
    offer: Option<DragOffer>,
    /// наш буфер: источник и что в нём
    clip: Option<(CopyPasteSource, Clip)>,
}

/// куда легли чужие файлы
#[derive(Clone, Debug)]
pub enum DropAt {
    Launcher(String),
    Folder(PathBuf),
    Cell((i32, i32)),
}

/// ответ буфера: пути и «вырезано ли» из GNOME-формата или KDE-метки
fn parse_clip(data: &str, gnome: bool) -> (Vec<PathBuf>, bool) {
    let mut lines: Vec<&str> = data.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut cut = false;
    if gnome && matches!(lines.first(), Some(&"copy" | &"cut")) {
        cut = lines[0] == "cut";
        lines.remove(0);
    }
    let paths = lines.into_iter().filter(|l| !l.starts_with('#')).filter_map(|l| if l.starts_with('/') { Some(PathBuf::from(l)) } else { yami_px::icons::uri_path(l) }).collect();
    (paths, cut)
}

fn read_all(p: ReadPipe) -> String {
    use std::io::Read;
    let mut s = String::new();
    let _ = p.take(4 << 20).read_to_string(&mut s);
    s
}

impl App {
    /// начать перетаскивание выделенных значков; false — протокола нет (тогда — внутри стола)
    pub fn desk_dnd_start(&mut self, press: (f64, f64)) -> bool {
        if self.dd_mgr.is_none() || self.data_dev.is_none() {
            return false;
        }
        let Some(origin) = self.surfs.values().find(|s| s.kind == SKind::Desk).map(|s| s.layer.wl_surface().clone()) else { return false };
        let Some(dr) = self.st.desk.drag.clone() else { return false };
        let icon = self.desk_group_icon(&dr.names, press);
        let (Some(mgr), Some(dev)) = (self.dd_mgr.as_ref(), self.data_dev.as_ref()) else { return false };
        let src = mgr.create_drag_and_drop_source(&self.qh, [URIS, GNOME, TEXT], DndAction::Copy | DndAction::Move | DndAction::Ask);
        src.start_drag(dev, &origin, icon.as_ref().map(|i| &i.0.0), self.press_serial);
        // картинка — уже в роли значка перетаскивания: только теперь сдвиг (где за неё взялись)
        // применяется к ней; закоммиченный до start_drag он пропадал, и группа вставала углом
        // под курсор
        let icon = icon.and_then(|((surf, pool, buf), (hx, hy, w, h))| {
            if surf.version() >= 5 {
                surf.offset(-hx, -hy);
                buf.attach_to(&surf).ok()?;
            } else {
                surf.attach(Some(buf.wl_buffer()), -hx, -hy);
            }
            surf.damage_buffer(0, 0, w, h);
            surf.commit();
            Some((surf, pool, buf))
        });
        self.dnd.paths = dr.names.iter().filter_map(|n| self.st.desk.item(n).map(|i| i.path.clone())).collect();
        self.dnd.src = Some(src);
        self.dnd.icon = icon;
        // кнопка теперь у перетаскивания: отпускание придёт ему
        self.press = None;
        true
    }

    /// картинка группы: значки на своих местах относительно друг друга, курсор — где нажали
    #[allow(clippy::type_complexity)]
    fn desk_group_icon(&mut self, names: &[String], press: (f64, f64)) -> Option<((WlSurface, SlotPool, smithay_client_toolkit::shm::slot::Buffer), (i32, i32, i32, i32))> {
        let d = &self.st.desk;
        let g = d.grid;
        let tiles: Vec<(crate::desk::Item, R)> = names
            .iter()
            .filter_map(|n| {
                let it = d.item(n)?.clone();
                let (x, y) = g.origin(*d.placement.get(n)?);
                Some((it, R::new(x + 4, y + 2, g.cw - 8, g.ch - 4)))
            })
            .take(24)
            .collect();
        if tiles.is_empty() {
            return None;
        }
        let x0 = tiles.iter().map(|t| t.1.x).min()?;
        let y0 = tiles.iter().map(|t| t.1.y).min()?;
        let w = (tiles.iter().map(|t| t.1.right()).max()? - x0).min(1600);
        let h = (tiles.iter().map(|t| t.1.bottom()).max()? - y0).min(1200);
        let mut px = vec![0u32; (w * h) as usize];
        {
            let st = &self.st;
            let mut ui = crate::ui::Ui { cv: Canvas::new(&mut px, w, h), f: &mut self.res.fonts, ic: &mut self.res.icons, t: st.t, zones: vec![], ptr: None, down: false, anim: false, blink: false, tr: crate::i18n::tr };
            for (it, r) in &tiles {
                crate::desk::draw_tile(&mut ui, st, it, R::new(r.x - x0, r.y - y0, r.w, r.h), 1.0, false, false, false);
            }
        }
        let mut pool = SlotPool::new((w * h * 4) as usize, &self.shm).ok()?;
        let (buf, mem) = pool.create_buffer(w, h, w * 4, wayland_client::protocol::wl_shm::Format::Argb8888).ok()?;
        for (dst, s) in mem.chunks_exact_mut(4).zip(px.iter()) {
            dst.copy_from_slice(&s.to_le_bytes());
        }
        let surf = self.compositor.create_surface(&self.qh);
        // горячая точка: где за картинку взялись (сдвиг ставится после start_drag)
        let (hx, hy) = (press.0 as i32 - x0, press.1 as i32 - y0);
        Some(((surf, pool, buf), (hx, hy, w, h)))
    }

    fn dnd_over_desk(&self, s: &WlSurface) -> bool {
        self.surfs.values().any(|x| x.kind == SKind::Desk && x.layer.wl_surface() == s)
    }

    /// своё перетаскивание кончилось (брошено или отменено)
    fn dnd_out_done(&mut self) {
        self.dnd.src = None;
        self.dnd.icon = None;
        self.dnd.paths.clear();
        self.st.desk.drag = None;
        self.dirty(&SKind::Desk);
    }

    /// чужие файлы прочитаны: копия/перенос, или открыть ими программу ярлыка
    pub fn desk_dropped(&mut self, paths: Vec<PathBuf>, at: DropAt, mv: bool) {
        if let Some(o) = self.dnd.offer.take() {
            o.finish();
            o.destroy();
        }
        let dir = self.st.desk.dir.clone();
        match at {
            DropAt::Launcher(n) => {
                if let Some(it) = self.st.desk.item(&n).cloned() {
                    self.desk_launch_with(&it, &paths);
                }
            }
            DropAt::Folder(p) => self.desk_transfer(paths, p, mv, None),
            DropAt::Cell(c) => self.desk_transfer(paths, dir, mv, Some(c)),
        }
    }

    // ── буфер обмена ────────────────────────────────────────────────────────
    fn clip_set(&mut self, mimes: &[&str], c: Clip) {
        let (Some(mgr), Some(dev)) = (self.dd_mgr.as_ref(), self.data_dev.as_ref()) else { return };
        let src = mgr.create_copy_paste_source(&self.qh, mimes.iter().copied());
        src.set_selection(dev, self.press_serial);
        self.dnd.clip = Some((src, c));
    }

    /// файлы в буфер: вырезанные бледнеют до вставки
    pub fn clip_set_files(&mut self, paths: Vec<PathBuf>, cut: bool) {
        if paths.is_empty() {
            return;
        }
        let mut mimes = vec![GNOME, URIS, TEXT];
        if cut {
            mimes.push(KDE_CUT);
        }
        self.st.desk.cut = if cut { paths.clone() } else { vec![] };
        self.clip_set(&mimes, Clip::Files(paths, cut));
        self.dirty(&SKind::Desk);
    }

    pub fn clip_set_text(&mut self, s: String) {
        self.clip_set(&[TEXT, "text/plain", "UTF8_STRING"], Clip::Text(s));
    }

    /// в буфере файлы (наши или чужие)
    pub fn clip_has_files(&self) -> bool {
        if let Some((_, Clip::Files(..))) = &self.dnd.clip {
            return true;
        }
        let Some(o) = self.data_dev.as_ref().and_then(|d| d.inner().data::<DataDeviceData>()).and_then(|d| d.selection_offer()) else { return false };
        o.with_mime_types(|m| m.iter().any(|t| t == GNOME || t == URIS))
    }

    /// вставить в папку dest (на стол — поближе к cell): копия, вырезанное — переносом
    pub fn clip_paste(&mut self, dest: PathBuf, cell: Option<(i32, i32)>) {
        if let Some((_, Clip::Files(paths, cut))) = &self.dnd.clip {
            let (paths, cut) = (paths.clone(), *cut);
            if cut {
                self.dnd.clip = None;
                self.st.desk.cut.clear();
                if let Some(d) = &self.data_dev {
                    d.unset_selection(self.press_serial);
                }
            }
            self.desk_transfer(paths, dest, cut, cell);
            return;
        }
        let Some(o) = self.data_dev.as_ref().and_then(|d| d.inner().data::<DataDeviceData>()).and_then(|d| d.selection_offer()) else { return };
        let (gnome, uris, kde) = o.with_mime_types(|m| (m.iter().any(|t| t == GNOME), m.iter().any(|t| t == URIS), m.iter().any(|t| t == KDE_CUT)));
        let Ok(main) = o.receive(if gnome { GNOME } else if uris { URIS } else { return }.to_string()) else { return };
        let flag = if kde { o.receive(KDE_CUT.to_string()).ok() } else { None };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (paths, mut cut) = parse_clip(&read_all(main), gnome);
            if let Some(f) = flag {
                cut |= read_all(f).trim() == "1";
            }
            let _ = tx.send(Msg::DeskPasted(paths, dest, cut, cell));
        });
    }

    pub fn desk_pasted(&mut self, paths: Vec<PathBuf>, dest: PathBuf, cut: bool, cell: Option<(i32, i32)>) {
        if cut {
            if let Some(d) = &self.data_dev {
                d.unset_selection(self.press_serial);
            }
        }
        self.desk_transfer(paths, dest, cut, cell);
    }
}

impl DataDeviceHandler for App {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, dev: &WlDataDevice, x: f64, y: f64, s: &WlSurface) {
        let Some(offer) = dev.data::<DataDeviceData>().and_then(|d| d.drag_offer()) else { return };
        let files = offer.with_mime_types(|m| m.iter().any(|t| t == URIS));
        if !self.dnd_over_desk(s) || self.st.editing || !files {
            offer.accept_mime_type(offer.serial, None);
            return;
        }
        offer.accept_mime_type(offer.serial, Some(URIS.into()));
        if self.dnd.src.is_some() {
            offer.set_actions(DndAction::Move | DndAction::Copy, DndAction::Move);
        } else {
            // чужие — копией, с Shift (композитор) — переносом
            offer.set_actions(DndAction::Copy | DndAction::Move, DndAction::Copy);
            self.st.desk.drag = Some(crate::desk::Drag::default());
        }
        self.dnd.offer = Some(offer);
        self.desk_drag_over(x as i32, y as i32);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if self.dnd.offer.as_ref().is_some_and(|o| !o.dropped) {
            self.dnd.offer = None;
        }
        match self.st.desk.drag.as_mut() {
            Some(dr) if !dr.names.is_empty() => {
                dr.drop_cell = None;
                dr.drop_tile = None;
            }
            _ => self.st.desk.drag = None,
        }
        self.dirty(&SKind::Desk);
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64) {
        if self.dnd.offer.is_some() {
            self.desk_drag_over(x as i32, y as i32);
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, dev: &WlDataDevice) {
        let Some(offer) = dev.data::<DataDeviceData>().and_then(|d| d.drag_offer()) else { return };
        if self.dnd.offer.is_none() {
            return;
        }
        // свои значки: перестановка, перенос в папку, открыть ярлыком
        if self.dnd.src.is_some() {
            self.desk_drag_end();
            offer.finish();
            offer.destroy();
            self.dnd.offer = None;
            return;
        }
        let Some(dr) = self.st.desk.drag.take() else { return };
        let d = &self.st.desk;
        let at = match (&dr.drop_tile, dr.drop_cell) {
            (Some(n), _) if d.item(n).is_some_and(|i| i.launcher()) => DropAt::Launcher(n.clone()),
            (Some(n), _) => DropAt::Folder(d.dir.join(n)),
            (None, Some(c)) => DropAt::Cell(c),
            (None, None) => DropAt::Cell((0, 0)),
        };
        let mv = offer.selected_action == DndAction::Move;
        self.dnd.offer = Some(offer.clone());
        let Ok(pipe) = offer.receive(URIS.into()) else {
            self.dnd.offer = None;
            return;
        };
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let paths = crate::fileops::parse_uri_list(&read_all(pipe));
            let _ = tx.send(Msg::DeskDropped(paths, at, mv));
        });
        self.dirty(&SKind::Desk);
    }
}

impl DataOfferHandler for App {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for App {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    /// окно просит данные: путей перетаскивания или буфера, в нужном виде
    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, src: &WlDataSource, mime: String, mut fd: WritePipe) {
        let uri = |ps: &[PathBuf]| ps.iter().map(|p| yami_px::icons::file_uri(p)).collect::<Vec<_>>();
        let data: Option<String> = if self.dnd.src.as_ref().is_some_and(|s| s.inner() == src) {
            let ps = &self.dnd.paths;
            match mime.as_str() {
                URIS => Some(crate::fileops::uri_list(ps)),
                GNOME => Some(format!("copy\n{}", uri(ps).join("\n"))),
                _ => Some(ps.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")),
            }
        } else {
            match self.dnd.clip.as_ref().filter(|c| c.0.inner() == src).map(|c| &c.1) {
                Some(Clip::Files(ps, cut)) => match mime.as_str() {
                    GNOME => Some(format!("{}\n{}", if *cut { "cut" } else { "copy" }, uri(ps).join("\n"))),
                    URIS => Some(crate::fileops::uri_list(ps)),
                    KDE_CUT => Some("1".into()),
                    _ => Some(ps.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")),
                },
                Some(Clip::Text(t)) => Some(t.clone()),
                None => None,
            }
        };
        if let Some(d) = data {
            std::thread::spawn(move || {
                use std::io::Write;
                let _ = fd.write_all(d.as_bytes());
            });
        }
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, src: &WlDataSource) {
        if self.dnd.src.as_ref().is_some_and(|s| s.inner() == src) {
            self.dnd_out_done();
        } else if self.dnd.clip.as_ref().is_some_and(|c| c.0.inner() == src) {
            // буфер занял кто-то другой: вырезанное больше не бледное
            self.dnd.clip = None;
            self.st.desk.cut.clear();
            self.dirty(&SKind::Desk);
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, src: &WlDataSource) {
        if self.dnd.src.as_ref().is_some_and(|s| s.inner() == src) {
            self.dnd_out_done();
        }
    }

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

smithay_client_toolkit::delegate_data_device!(App);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_formats_parse() {
        let (p, cut) = parse_clip("cut\nfile:///home/u/Desktop/a%20b.txt\nfile:///tmp/c", true);
        assert!(cut);
        assert_eq!(p, vec![PathBuf::from("/home/u/Desktop/a b.txt"), PathBuf::from("/tmp/c")]);
        let (p, cut) = parse_clip("file:///tmp/x\r\n# comment\r\n/tmp/y\r\n", false);
        assert!(!cut);
        assert_eq!(p, vec![PathBuf::from("/tmp/x"), PathBuf::from("/tmp/y")]);
    }
}
