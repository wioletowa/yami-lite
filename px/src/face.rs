// Лицо экрана блокировки и экрана входа: обои под вуалью со строками развёртки, большие часы с
// тенью, дата, 闇 в углу. Одно на двоих (шелл lock.rs и yami-greet), чтобы вход и блокировка
// были одним экраном.

use crate::font;
use crate::gfx::{over, Img, C, R};
use crate::theme::Theme;
use crate::time::{local_now, RU_DAY_FULL, RU_MON};
use crate::ui::{Action, Ui};

/// картинка на весь экран с обрезкой краёв (без сглаживания: клетки остаются клетками)
pub fn cover<A: Action>(ui: &mut Ui<A>, img: &Img, w: i32, h: i32) {
    let k = (w as f32 / img.w.max(1) as f32).max(h as f32 / img.h.max(1) as f32);
    let (dw, dh) = ((img.w as f32 * k).ceil() as i32, (img.h as f32 * k).ceil() as i32);
    ui.cv.image_fast(img, R::new((w - dw) / 2, (h - dh) / 2, dw, dh));
}

/// вуаль над обоями: в тёмной теме чёрная, в светлой — цветом фона (высветлить). dim — её
/// плотность (None — как у экрана блокировки)
pub fn veil(t: &Theme, dim: Option<f32>) -> C {
    let d = dim.unwrap_or(0.55).clamp(0.0, 0.95);
    if t.light {
        t.bg.alpha((d + 0.07).min(0.95))
    } else {
        C(0xff000000).alpha(d)
    }
}

/// впечь вуаль в картинку один раз (экран входа): дальше каждый кадр — простое копирование
pub fn bake(img: &mut Img, veil: C) {
    let v = veil.premul();
    for p in img.px.iter_mut() {
        *p = over(*p, v);
    }
}

/// обои, вуаль и строки развёртки (экран блокировки: вуаль кладётся каждый кадр)
pub fn backdrop<A: Action>(ui: &mut Ui<A>, wall: Option<&Img>, w: i32, h: i32) {
    let v = veil(&ui.t, None);
    backdrop_with(ui, wall, w, h, v, false);
}

/// то же; baked — вуаль уже впечена в картинку (bake). Без картинки — фон темы под вуалью
pub fn backdrop_with<A: Action>(ui: &mut Ui<A>, wall: Option<&Img>, w: i32, h: i32, veil: C, baked: bool) {
    let t = ui.t;
    match wall {
        Some(img) => {
            cover(ui, img, w, h);
            if !baked {
                ui.cv.fill(R::new(0, 0, w, h), veil);
            }
        }
        None => ui.cv.clear(C(over(t.bg.premul(), veil.premul()))),
    }
    let line = if t.light { t.text.alpha(0.07) } else { C(0xff000000).alpha(0.18) };
    for y in (0..h).step_by(4) {
        ui.cv.fill(R::new(0, y, w, 1), line);
    }
}

/// большие часы и дата по центру чуть выше середины; возвращает низ даты
pub fn clock<A: Action>(ui: &mut Ui<A>, w: i32, h: i32) -> i32 {
    let t = ui.t;
    let tm = local_now();
    let big = font::px(128);
    let s = format!("{:02}:{:02}", tm.hour, tm.min);
    let cw = ui.w(big, &s);
    let cy = h / 2 - 150;
    // тень-сдвиг на 4 px, как у панелей
    ui.text(big, (w - cw) / 2 + 4, cy + 4, &s, t.blood.fade(0.8));
    ui.text(big, (w - cw) / 2, cy, &s, t.text);
    let (day, mon) = ((ui.tr)(RU_DAY_FULL[tm.wday as usize % 7]), (ui.tr)(RU_MON[(tm.mon as usize).clamp(1, 12) - 1]));
    // «sábado, 7 de novembro» по-португальски
    let date = if crate::i18n::pt_br() { format!("{day}, {} de {mon}", tm.day) } else { format!("{day}, {} {mon}", tm.day) };
    let dy = cy + ui.f.line(big) + 6;
    ui.text_center(font::PX24, R::new(0, dy, w, 30), &date, t.text_dim);
    dy + 30
}

/// 闇 в правом нижнем углу
pub fn kanji<A: Action>(ui: &mut Ui<A>, w: i32, h: i32) {
    let t = ui.t;
    ui.text(font::jp(32), w - 72, h - 64, "闇", t.red.fade(0.5));
}
