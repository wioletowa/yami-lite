// Кто держит камеру: inotify на /dev/video* будит нить при каждом открытии и закрытии (опроса нет),
// тогда она смотрит, чьи процессы держат эти файлы. Камеру, подключённую позже, нить замечает при
// пересмотре /dev раз в 30 с.

use crate::app::Msg;
use calloop::channel::Sender;
use std::ffi::CString;

fn devices() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir("/dev").into_iter().flatten().flatten().map(|e| e.path().to_string_lossy().to_string()).filter(|p| p.starts_with("/dev/video")).collect();
    v.sort();
    v
}

/// имена процессов (comm), у которых открыта камера; свои процессы — все, чужие /proc не покажет
fn users() -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else { continue };
        let holds = fds.flatten().any(|f| std::fs::read_link(f.path()).is_ok_and(|t| t.to_string_lossy().starts_with("/dev/video")));
        if holds {
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string();
            if !comm.is_empty() && !out.contains(&comm) {
                out.push(comm);
            }
        }
    }
    out
}

pub fn spawn(tx: Sender<Msg>) {
    let _ = std::thread::Builder::new().name("camera".into()).spawn(move || {
        // SAFETY: inotify — обычный fd этой нити, читается только здесь
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if fd < 0 {
            return;
        }
        let mut watched: Vec<String> = vec![];
        let mut last: Option<Vec<String>> = None;
        loop {
            let now = devices();
            if now != watched {
                for d in &now {
                    if let Ok(c) = CString::new(d.as_str()) {
                        unsafe { libc::inotify_add_watch(fd, c.as_ptr(), libc::IN_OPEN | libc::IN_CLOSE_WRITE | libc::IN_CLOSE_NOWRITE) };
                    }
                }
                watched = now;
            }
            let u = if watched.is_empty() { vec![] } else { users() };
            if last.as_ref() != Some(&u) {
                if tx.send(Msg::Camera(u.clone())).is_err() {
                    return;
                }
                last = Some(u);
            }
            // ждать события камеры не дольше 30 с (потом пересмотреть /dev)
            let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut p, 1, 30_000) } > 0 {
                let mut buf = [0u8; 4096];
                unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                // открытие и закрытие идут пачкой: дать программе договорить
                std::thread::sleep(std::time::Duration::from_millis(150));
                let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
                while unsafe { libc::poll(&mut p, 1, 0) } > 0 {
                    unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
                }
            }
        }
    });
}
