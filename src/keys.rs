// Клавиши: имя клавиши по событию и её латинская буква — для сочетаний внутри шелла (поля,
// меню питания). Сочетания niri шелл не пишет: они в твоём config.kdl.

use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};

pub fn key_name(e: &KeyEvent) -> Option<String> {
    let code = e.raw_code;
    const ROW: [(u32, &str); 47] = [
        (2, "1"), (3, "2"), (4, "3"), (5, "4"), (6, "5"), (7, "6"), (8, "7"), (9, "8"), (10, "9"), (11, "0"),
        (12, "Minus"), (13, "Equal"), (16, "Q"), (17, "W"), (18, "E"), (19, "R"), (20, "T"), (21, "Y"), (22, "U"),
        (23, "I"), (24, "O"), (25, "P"), (26, "BracketLeft"), (27, "BracketRight"), (30, "A"), (31, "S"), (32, "D"),
        (33, "F"), (34, "G"), (35, "H"), (36, "J"), (37, "K"), (38, "L"), (39, "Semicolon"), (40, "Apostrophe"),
        (41, "Grave"), (43, "Backslash"), (44, "Z"), (45, "X"), (46, "C"), (47, "V"), (48, "B"), (49, "N"), (50, "M"),
        (51, "Comma"), (52, "Period"), (53, "Slash"),
    ];
    if let Some((_, n)) = ROW.iter().find(|(c, _)| *c == code) {
        return Some(n.to_string());
    }
    // Shift+Tab xkb отдаёт как ISO_Left_Tab, а niri ждёт Tab с модификатором Shift
    if e.keysym == Keysym::ISO_Left_Tab {
        return Some("Tab".into());
    }
    let n = e.keysym.name()?;
    let n = n.strip_prefix("XK_").map(String::from).or_else(|| n.strip_prefix("XF86XK_").map(|x| format!("XF86{x}")))?;
    Some(n)
}

/// буква клавиши для сочетаний шелла (Ctrl+A/C/V/U/P в полях, буквы меню питания): в латинской
/// раскладке — та, что подписана на клавише (AZERTY, Dvorak, Colemak), в другой (русская) — по
/// положению, как у QWERTY
pub fn latin_letter(e: &KeyEvent) -> Option<char> {
    let k = e.keysym.raw();
    if (0x41..=0x5a).contains(&k) || (0x61..=0x7a).contains(&k) {
        return Some((k as u8 as char).to_ascii_uppercase());
    }
    key_name(e).filter(|n| n.len() == 1).and_then(|n| n.chars().next()).filter(char::is_ascii_uppercase)
}
