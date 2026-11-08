// Лёгкий режим (настройки → вид, `lite`): для слабых и виртуальных машин. Размытие, анимации
// niri и шелла, полупрозрачность неактивных окон стоят видеокарте каждый кадр; на старом
// встроенном Intel, в виртуалке или при программной отрисовке (llvmpipe) именно они делают стол
// вязким. Сами настройки не трогаются: выключил лёгкий режим — всё как было.
// Установщик на чистой установке включает его сам, если машина такая (`config lite-auto`).

/// почему эта машина слабая для эффектов; None — обычная
pub fn reason() -> Option<String> {
    vm().or_else(gpu).or_else(small)
}

fn read(p: &str) -> String {
    std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// виртуальная машина: флаг hypervisor у процессора; имя — из DMI
fn vm() -> Option<String> {
    let cpu = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let flags = cpu.lines().find(|l| l.starts_with("flags"))?;
    if !flags.split_whitespace().any(|f| f == "hypervisor") {
        return None;
    }
    let who = format!("{} {}", read("/sys/class/dmi/id/sys_vendor"), read("/sys/class/dmi/id/product_name"));
    let name = [("VirtualBox", "VirtualBox"), ("innotek", "VirtualBox"), ("VMware", "VMware"), ("QEMU", "QEMU"), ("KVM", "KVM"), ("Parallels", "Parallels"), ("Xen", "Xen"), ("Virtual Machine", "Hyper-V")]
        .iter()
        .find(|(k, _)| who.contains(k))
        .map(|(_, n)| *n)
        .unwrap_or("?");
    Some(crate::tf!("виртуальная машина ({})", "a virtual machine ({})", "uma máquina virtual ({})", name))
}

/// драйверы без 3D: картинку считает процессор
const SOFT: &[&str] = &["simpledrm", "simple-framebuffer", "vboxvideo", "vmwgfx", "qxl", "bochs-drm", "bochs", "cirrus", "cirrus-qemu", "hyperv_drm", "udl", "mgag200", "ast"];
/// Intel до Sandy Bridge (Gen 3–5: GMA 950/3100/X4500, HD Graphics первых Core i): GL 2.1
const OLD_INTEL: &[u32] = &[
    0x2582, 0x258a, 0x2592, 0x2772, 0x27a2, 0x27ae, 0x29b2, 0x29c2, 0x29d2, 0xa001, 0xa011, 0x2972, 0x2982, 0x2992, 0x29a2, 0x2a02, 0x2a12, 0x2a42, 0x2e02, 0x2e12, 0x2e22, 0x2e32,
    0x2e42, 0x2e92, 0x0042, 0x0046,
];

/// видеокарта: нет рендер-узла (программная отрисовка), драйвер без 3D или очень старый Intel
fn gpu() -> Option<String> {
    let dri: Vec<String> = std::fs::read_dir("/dev/dri").map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect()).unwrap_or_default();
    if !dri.iter().any(|n| n.starts_with("renderD")) {
        return Some(crate::tf!("нет ускорения графики (отрисовка процессором)", "no graphics acceleration (the CPU draws)", "sem aceleração gráfica (a CPU renderiza)"));
    }
    let mut drivers = vec![];
    for dev in crate::sys::drm_cards() {
        let drv = std::fs::read_link(dev.join("driver")).ok().and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_string())).unwrap_or_default();
        let id = u32::from_str_radix(read(&dev.join("device").to_string_lossy()).trim_start_matches("0x"), 16).unwrap_or(0);
        if drv == "i915" && OLD_INTEL.contains(&id) {
            return Some(crate::tf!("старая встроенная графика Intel", "old integrated Intel graphics", "gráficos Intel integrados antigos"));
        }
        drivers.push(drv);
    }
    // все карты без 3D (у ноутбука может быть и simpledrm до загрузки драйвера, и настоящая)
    (!drivers.is_empty() && drivers.iter().all(|d| SOFT.contains(&d.as_str()))).then(|| crate::tf!("видеокарта без 3D ({})", "a display adapter without 3D ({})", "um adaptador de vídeo sem 3D ({})", drivers.join(", ")))
}

/// мало памяти и ядер: 4 ГБ и два-четыре потока — старый ноутбук
fn small() -> Option<String> {
    let mem = crate::sys::kv_num(&read("/proc/meminfo"), "MemTotal:").map(|v| v as u64).unwrap_or(u64::MAX);
    let gb = (mem as f64 / 1048576.0).round() as u64;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8);
    (gb <= 4 && threads <= 4).then(|| crate::tf!("{} ГБ памяти, {} потока", "{} GB of memory, {} threads", "{} GB de memória, {} threads", gb, threads))
}

/// строки для yami-fx.kdl: что лёгкий режим выключает у niri (размытие пишет fx_kdl сам)
pub fn kdl(l: &mut Vec<String>) {
    for s in ["// lite mode (settings → look): no animations, opaque inactive windows", "animations { off; }", "window-rule {", "    match is-active=false", "    opacity 1.0", "}", ""] {
        l.push(s.into());
    }
}
