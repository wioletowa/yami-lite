// Звуки шелла без файлов в комплекте: сигнал таймера синтезируется здесь же — чиптюн квадратной
// волной со скважностью 25% (как у старых приставок): арпеджио до-ми-соль-до вверх и долгая верхняя
// до с лёгким вибрато и затуханием. WAV пишется один раз в кеш, играет pw-play (или paplay, aplay).

use std::path::PathBuf;

const RATE: u32 = 44_100;

/// ноты: частота (0 — пауза) и длительность, мс
const TUNE: &[(f32, u32)] = &[(1046.5, 70), (1318.5, 70), (1568.0, 70), (2093.0, 70), (0.0, 40), (1568.0, 60), (2093.0, 320)];

/// отсчёты сигнала (16 бит, моно)
pub fn chime_pcm() -> Vec<i16> {
    let mut out = vec![];
    let total = TUNE.len();
    for (k, &(f, ms)) in TUNE.iter().enumerate() {
        let n = (RATE * ms / 1000) as usize;
        let last = k + 1 == total;
        for i in 0..n {
            if f == 0.0 {
                out.push(0);
                continue;
            }
            let t = i as f32 / RATE as f32;
            // долгая нота — с вибрато ±0.6%
            let freq = if last { f * (1.0 + 0.006 * (t * 2.0 * std::f32::consts::PI * 6.0).sin()) } else { f };
            let phase = (t * freq).fract();
            let sq = if phase < 0.25 { 1.0 } else { -1.0 };
            // огибающая: короткая атака, у долгой — затухание до нуля
            let attack = (i as f32 / (RATE as f32 * 0.004)).min(1.0);
            let decay = if last { 1.0 - i as f32 / n as f32 } else { 1.0 - 0.3 * i as f32 / n as f32 };
            out.push((sq * attack * decay * 0.22 * i16::MAX as f32) as i16);
        }
    }
    out
}

/// WAV: заголовок RIFF и отсчёты
pub fn wav(pcm: &[i16]) -> Vec<u8> {
    let data = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(RATE.to_le_bytes());
    b.extend((RATE * 2).to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    for s in pcm {
        b.extend(s.to_le_bytes());
    }
    b
}

fn chime_file() -> Option<PathBuf> {
    let p = yami_px::paths::cache_dir().join("yami-lite/timer-chime.wav");
    if !p.exists() {
        std::fs::create_dir_all(p.parent()?).ok()?;
        std::fs::write(&p, wav(&chime_pcm())).ok()?;
    }
    Some(p)
}

/// сыграть сигнал таймера с громкостью 0..1 (в своей нити: плеер ждать не нужно)
pub fn chime(volume: f32) {
    let v = volume.clamp(0.0, 1.0);
    if v <= 0.0 {
        return;
    }
    std::thread::spawn(move || {
        let Some(p) = chime_file() else { return };
        let players: [(&str, Vec<String>); 3] = [("pw-play", vec![format!("--volume={v:.2}")]), ("paplay", vec![format!("--volume={}", (v * 65536.0) as u32)]), ("aplay", vec![])];
        for (player, args) in players {
            let ok = std::process::Command::new(player).args(&args).arg(&p).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
            if ok {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn chime_is_short_and_wav_is_well_formed() {
        let pcm = super::chime_pcm();
        let secs = pcm.len() as f32 / super::RATE as f32;
        assert!(secs > 0.5 && secs < 1.0, "{secs}");
        let w = super::wav(&pcm);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(w.len(), 44 + pcm.len() * 2);
        // последний отсчёт затух
        assert!(pcm.last().unwrap().abs() < 200);
    }
}
