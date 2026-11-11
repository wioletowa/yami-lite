// YAMI-LITE // личный шелл для niri на Rust, один бинарник: бар, док, питание, настройки,
// уведомления, трей, всплывашки (календарь, звук, сеть, bluetooth, система), виджеты рабочего
// стола, блокировка. Рисует сам, пикселями, в shm — без тулкитов.
//
//   yami-lite                 запустить (так зовёт niri)
//   yami-lite <команда>       справка — `yami-lite help` (cmd.rs)

pub use yami_px::{font, gfx, icons, theme};

mod app;
mod audio;
mod bar;
mod bartab;
mod bundle;
mod camera;
mod cli;
mod click;
mod clock;
mod cmd;
mod dbus;
mod dnd;
mod desk;
mod dock;
mod dropdown;
mod fonttab;
mod games;
mod i18n;
mod config;
mod control;
mod ipc;
mod keys;
mod leds;
mod lock;
mod media;
mod menu;
mod msgs;
mod net;
mod notes;
mod niri;
mod notif;
mod night;
mod pam;
mod panels;
mod input;
mod prefs;
mod privacy;
mod proc;
mod render;
mod wayland;
mod recolor;
mod reply;
mod popups;
mod power;
mod setclick;
mod settabs;
mod shellsurf;
mod settings;
mod sleep;
mod sound;
mod sys;
mod tick;
mod timer;
mod ui;
mod wall;
mod wallhaven;
mod fan;
mod features;
mod fileops;
mod hide;
mod walls;
mod weather;
mod themetab;
mod widgets;

fn main() {
    // большие буферы (обои, скриншоты, обложки) — сразу через mmap и обратно системе при
    // освобождении: иначе glibc поднимает порог и держит их в куче нитей после каждого декода
    // (musl так не делает, и настроек этих у него нет)
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 256 << 10);
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        // один экземпляр: шелл уже работает — открыть его настройки (ярлык в лаунчере), иначе стартовать
        if ipc::send("settings") {
            return;
        }
        app::run();
        return;
    }
    std::process::exit(cmd::run(&args[1..]))
}
