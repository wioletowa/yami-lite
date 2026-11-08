// Рисование поверхностей шелла: каждая грязная поверхность рисуется заново в логическом холсте,
// при дробном масштабе увеличивается без сглаживания в буфер, размытие и ввод — регионами.
// Кадр, совпавший с уже отданным, композитору не отдаётся (frame_hash).

use crate::app::*;
use crate::gfx::{Canvas, R};
use crate::theme::CLEAR;
use crate::ui::Ui;
use smithay_client_toolkit::{compositor::Region, shm::slot::SlotPool};
use std::time::Instant;
use wayland_client::protocol::wl_shm;

impl App {
    // ── рисование ───────────────────────────────────────────────────────────
    pub fn flush(&mut self) {
        // окно на весь экран (игра): что под ним, не рисуем — останется грязным до выхода из него
        let covered = if self.st.game.fullscreen { Some(self.focus_out()) } else { None };
        let under = |s: &Surf| covered.as_deref() == Some(s.output.as_str()) && matches!(s.kind, SKind::Bar | SKind::Widget(_) | SKind::Dock | SKind::Wall);
        // всплывашка перерисовывается — значит, её данные поменялись, и размер (строки сетей, устройств,
        // программ трея, время трека) мог тоже; resize_popup ничего не делает, если он тот же
        if self.surfs.values().any(|s| s.kind == SKind::Popup && s.dirty && !s.closing) {
            self.resize_popup();
        }
        let mut ids: Vec<u32> = self.surfs.iter().filter(|(_, s)| s.dirty && s.configured && !s.frame_pending && !under(s)).map(|(i, _)| *i).collect();
        // обои отложены в копии (wall_park): поверхность обоев ждёт полную картинку
        if self.st.wall_parked && ids.iter().any(|i| self.surfs[i].kind == SKind::Wall) {
            self.wall_need();
            ids.retain(|i| self.surfs[i].kind != SKind::Wall);
        }
        for id in ids {
            self.draw(id);
        }
        for i in 0..self.lock_surfs.len() {
            if self.lock_surfs[i].dirty {
                self.draw_lock(i);
            }
        }
        self.save_settings_if_changed();
        let _ = self.conn.flush();
    }

    pub(crate) fn draw(&mut self, id: u32) {
        let Some(mut s) = self.surfs.remove(&id) else { return };
        s.dirty = false;
        if s.fresh {
            s.fresh = false;
            s.born = Instant::now();
        }
        // обои: пиксели нужны один раз
        let scaled = matches!(s.kind, SKind::Dim | SKind::Catcher);
        let (bw, bh) = if scaled { (1, 1) } else { (s.w, s.h) };
        if bw <= 0 || bh <= 0 {
            self.surfs.insert(id, s);
            return;
        }
        // HiDPI: буфер в физических пикселях, холст — логический, потом увеличение
        let (pw, ph) = if scaled || s.viewport.is_none() || s.scale == 1.0 { (bw, bh) } else { crate::gfx::phys(bw, bh, s.scale) };
        let up = (pw, ph) != (bw, bh);
        let need = (pw * ph * 4) as usize;
        if s.pool.as_ref().map(|p| p.len() < need).unwrap_or(true) {
            s.pool = SlotPool::new(need * 2, &self.shm).ok();
        }
        let Some(pool) = s.pool.as_mut() else {
            self.surfs.insert(id, s);
            return;
        };
        let Ok((buffer, bytes)) = pool.create_buffer(pw, ph, pw * 4, wl_shm::Format::Argb8888) else {
            self.surfs.insert(id, s);
            return;
        };
        let (out_ptr, out_len) = (bytes.as_mut_ptr() as *mut u32, bytes.len() / 4);
        let mut scratch = std::mem::take(&mut self.scratch);
        let buf: &mut [u32] = if up {
            scratch.clear();
            scratch.resize((bw * bh) as usize, 0);
            &mut scratch
        } else {
            unsafe { std::slice::from_raw_parts_mut(out_ptr, out_len) }
        };
        let ptr = self.ptr.filter(|p| p.0 == id).map(|p| (p.1, p.2));
        let down = self.press.as_ref().map(|p| p.sid == id && p.button == 0x110).unwrap_or(false);
        let mut done_closing = false;
        let blur: Vec<R>;
        {
            let mut ui = Ui {
                cv: Canvas::new(buf, bw, bh),
                // виджеты стола — своим шрифтом (fontWidgets)
                f: if matches!(s.kind, SKind::Widget(_)) { &mut self.res.wfonts } else { &mut self.res.fonts },
                ic: &mut self.res.icons,
                t: self.st.t,
                zones: vec![],
                ptr,
                down,
                anim: false,
                blink: self.st.blink,
                tr: crate::i18n::tr,
            };
            ui.cv.clear(CLEAR);
            let st = &self.st;
            // появление: пиксельное проявление по Байеру + плавное всплытие
            let (k, finished) = match s.kind {
                SKind::Modal | SKind::Popup | SKind::Menu | SKind::Dim | SKind::Catcher | SKind::Osd | SKind::EditBar | SKind::Widget(_) | SKind::Dock => {
                    let dur = match s.kind {
                        SKind::Dock => 200.0,
                        SKind::Widget(_) => 320.0,
                        SKind::Menu => 140.0,
                        SKind::Popup | SKind::Osd => 190.0,
                        _ => 230.0,
                    };
                    let (k, fin) = reveal_of(st, &s, dur);
                    (k, fin)
                }
                _ => (1.0, true),
            };
            if !finished {
                ui.anim = true;
            }
            if s.closing && finished {
                done_closing = true;
            }
            let rise = match s.kind {
                SKind::Modal | SKind::Popup => 12,
                SKind::Widget(_) => 16,
                SKind::Osd | SKind::EditBar | SKind::Dock => 8,
                SKind::Menu => 4,
                _ => 0,
            };
            ui.cv.opacity = (0.35 + 0.65 * k).min(1.0);
            ui.cv.oy = rise - ((1.0 - k) * rise as f32).round() as i32;
            let (w, h) = (s.w, s.h - rise);
            match &s.kind {
                SKind::Wall => crate::wall::draw(&mut ui, st, w, h),
                SKind::Bar => crate::bar::draw(&mut ui, st, &s.output, w),
                SKind::Dim => {
                    // затемнение цветом фона темы, а не чёрным: стол остаётся виден
                    let a = if st.modal == Some(crate::app::Modal::Walls) { st.set.dim * 0.7 } else { st.set.dim };
                    ui.cv.opacity = 1.0;
                    ui.cv.oy = 0;
                    ui.cv.clear(st.t.bg.alpha(a * k));
                }
                SKind::Catcher => {}
                SKind::Modal => match st.modal {
                    Some(Modal::Power) => crate::power::draw(&mut ui, st, w, h),
                    Some(Modal::Reply) => crate::reply::draw(&mut ui, st, w, h),
                    Some(Modal::Walls) => crate::walls::draw(&mut ui, st, w, h),
                    None => {}
                },
                SKind::Settings => crate::settings::draw(&mut ui, st, w, h),
                SKind::Tip => crate::privacy::draw_tip(&mut ui, st, w, h),
                SKind::Popup => crate::popups::draw(&mut ui, st, w, h),
                SKind::Menu => crate::menu::draw(&mut ui, st, w, h),
                SKind::Notifs => crate::notif::draw(&mut ui, st, w, h),
                SKind::Osd => crate::notif::draw_osd(&mut ui, st, w, h),
                SKind::Widget(wid) => crate::widgets::draw(&mut ui, st, *wid, w, h),
                SKind::EditBar => crate::widgets::draw_editbar(&mut ui, st, w, h),
                SKind::EditGrid => crate::widgets::draw_grid(&mut ui, st, w, h),
                SKind::Dock => crate::dock::draw(&mut ui, st, w, h),
                SKind::DockEdge => {}
            }
            // развёртка — только панели, не обои
            if st.set.scanlines && !matches!(s.kind, SKind::Dim | SKind::Catcher | SKind::Wall | SKind::EditGrid | SKind::DockEdge) {
                ui.cv.scanlines(if st.t.light { 0.06 } else { 0.22 });
            }
            if k < 1.0 && !matches!(s.kind, SKind::Dim | SKind::Catcher) {
                ui.cv.dissolve(k, 2);
            }
            // размытие — с первой трети проявления (#18: раньше ждало конца анимации и появлялось
            // рывком): под почти пустыми клетками ещё не нужно, дальше идёт вместе с панелью
            // (прямоугольники размытия сдвигаются с её всплытием); при закрытии гаснет так же
            blur = if !st.set.blur_on() || k < 0.35 {
                vec![]
            } else {
                std::mem::take(&mut ui.cv.blur)
            };
            s.zones = std::mem::take(&mut ui.zones);
            s.anim = ui.anim;
        }
        // тот же кадр, что уже у композитора (часы без секунд, датчики не сдвинулись): не отдавать —
        // композитору нечего пересобирать. Обои рисуются один раз и так
        let hash = if s.anim || done_closing || s.kind == SKind::Wall { 0 } else { frame_hash(buf, [s.w, s.h, pw, ph]) };
        if hash != 0 && hash == s.drawn && s.input == s.input_set && blur == s.blur_set {
            drop(buffer);
            self.scratch = if scratch.capacity() > 1 << 20 { Vec::new() } else { scratch };
            self.surfs.insert(id, s);
            return;
        }
        s.drawn = hash;
        if up {
            let dst = unsafe { std::slice::from_raw_parts_mut(out_ptr, out_len) };
            crate::gfx::upscale(&scratch, bw, bh, dst, pw, ph);
        }
        // большой холст (обои, скриншот) не держим
        self.scratch = if scratch.capacity() > 1 << 20 { Vec::new() } else { scratch };
        let surf = s.layer.wl_surface().clone();
        if let Some(vp) = &s.viewport {
            vp.set_destination(s.w.max(1), s.h.max(1));
        }
        surf.damage_buffer(0, 0, pw, ph);
        // поверхность ввода (заголовки окон: только видимые куски)
        if s.input != s.input_set {
            if let Ok(r) = Region::new(&self.compositor) {
                if let Some(rects) = &s.input {
                    for q in rects {
                        r.add(q.x, q.y, q.w, q.h);
                    }
                    surf.set_input_region(Some(r.wl_region()));
                } else {
                    surf.set_input_region(None);
                }
            }
            s.input_set = s.input.clone();
        }
        if blur != s.blur_set {
            if let Some(mgr) = &self.fx_mgr {
                let fx = s.fx.get_or_insert_with(|| mgr.get_background_effect(&surf, &self.qh, ()));
                if blur.is_empty() {
                    fx.set_blur_region(None);
                } else if let Ok(r) = Region::new(&self.compositor) {
                    for q in &blur {
                        r.add(q.x, q.y, q.w, q.h);
                    }
                    fx.set_blur_region(Some(r.wl_region()));
                }
            }
            s.blur_set = blur;
        }
        let wants_frame = s.anim || s.kind == SKind::Wall;
        if wants_frame {
            surf.frame(&self.qh, surf.clone());
            s.frame_pending = true;
        }
        let _ = buffer.attach_to(&surf);
        s.layer.commit();
        // обои рисуются один раз: буфер у композитора, свой не держим. Пока идёт смена обоев,
        // кадры нужны дальше — буфер остаётся до последнего
        if s.kind == SKind::Wall && !s.anim {
            s.release_pool = true;
        }
        if done_closing {
            return;
        }
        self.surfs.insert(id, s);
    }
}

/// отпечаток кадра: размер и пиксели (FNV по 64-битным словам — на бар ~30 мкс)
fn frame_hash(px: &[u32], dims: [i32; 4]) -> u64 {
    // FNV-1a по 8-байтным словам, а не по байтам: кадр — сотни килобайт, умножений в 8 раз меньше
    let mut h = yami_px::FNV0;
    let mut eat = |v: u64| h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
    for d in dims {
        eat(d as u32 as u64);
    }
    let mut it = px.chunks_exact(2);
    for c in &mut it {
        eat(c[0] as u64 | (c[1] as u64) << 32);
    }
    for c in it.remainder() {
        eat(*c as u64);
    }
    h | 1
}

pub fn reveal_of(st: &State, s: &Surf, dur_ms: f32) -> (f32, bool) {
    if !st.set.anim() {
        return (if s.closing { 0.0 } else { 1.0 }, true);
    }
    // закрытие быстрее открытия
    let dur = dur_ms * st.set.anim_speed.max(0.1) * if s.closing { 0.7 } else { 1.0 };
    let t = (s.born.elapsed().as_secs_f32() * 1000.0 / dur).min(1.0);
    // открытие: быстро и мягко садится (ease-out cubic); закрытие: разгоняется (ease-in)
    let k = if s.closing { 1.0 - t * t } else { 1.0 - (1.0 - t).powi(3) };
    (k, t >= 1.0)
}
