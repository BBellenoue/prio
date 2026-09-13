//! What depends on the system: data folder, local date, fonts, screen size, global
//! shortcuts, notification area or menu bar icon, and how the resident is woken.

#[cfg(unix)]
mod unix;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

pub const HK_ADD: usize = 1;
pub const HK_LIST: usize = 2;
pub const HK_QUIT: usize = 3;
pub const HK_ADD_FAILED: usize = 1; // bits of hk_status
pub const HK_LIST_FAILED: usize = 2;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HotkeyKey {
    Alnum(u8), // 'A' to 'Z' or '0' to '9'
    Function(u32),
    Space,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool, // the Windows or Command key
    pub key: HotkeyKey,
}

/// "Ctrl+Alt+A" -> a neutral combination, which each backend then translates into its own
/// codes. At least one strong modifier, then a letter, a digit, F1 to F24 or Space.
pub fn parse_hotkey(spec: &str) -> Option<Hotkey> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (key, mods) = parts.split_last()?;
    let mut h = Hotkey {
        ctrl: false,
        alt: false,
        shift: false,
        meta: false,
        key: HotkeyKey::Space,
    };
    for md in mods {
        match md.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => h.ctrl = true,
            "alt" | "option" => h.alt = true,
            "shift" | "maj" => h.shift = true,
            "cmd" | "command" | "win" | "super" => h.meta = true,
            _ => return None,
        }
    }
    if !strong_mods(h) {
        return None; // Shift alone: too easy to trigger by accident
    }
    let k = key.to_ascii_uppercase();
    h.key = match k.as_str() {
        "SPACE" | "ESPACE" => HotkeyKey::Space,
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => HotkeyKey::Alnum(k.as_bytes()[0]),
        k if k.starts_with('F') => {
            let n: u32 = k[1..].parse().ok()?;
            (1..=24).contains(&n).then_some(HotkeyKey::Function(n))?
        }
        _ => return None,
    };
    Some(h)
}

/// Windows keeps Win+letter for its shell; macOS leaves Command to applications.
fn strong_mods(h: Hotkey) -> bool {
    if cfg!(target_os = "macos") {
        h.ctrl || h.alt || h.meta
    } else {
        h.ctrl || h.alt
    }
}

/// The "Ctrl+Alt+A" label for an egui key and its modifiers, when the combination is valid.
/// `mac_cmd` is always false off macOS.
pub fn hotkey_label(key: eframe::egui::Key, mods: eframe::egui::Modifiers) -> Option<String> {
    let mut parts = Vec::new();
    if mods.mac_cmd {
        parts.push("Cmd");
    }
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    let name = key.name().to_string();
    parts.push(&name);
    let label = parts.join("+");
    parse_hotkey(&label).map(|_| label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn parsing() {
        let a = parse_hotkey("Ctrl+Alt+A").unwrap();
        assert_eq!(
            (a.ctrl, a.alt, a.shift, a.meta, a.key),
            (true, true, false, false, HotkeyKey::Alnum(b'A'))
        );
        let f = parse_hotkey("ctrl + shift + f5").unwrap();
        assert_eq!((f.ctrl, f.shift, f.key), (true, true, HotkeyKey::Function(5)));
        assert_eq!(parse_hotkey("Alt+Espace").unwrap().key, HotkeyKey::Space);
        assert_eq!(parse_hotkey("Shift+A"), None);
        assert_eq!(parse_hotkey("Ctrl+Alt+Entrée"), None);
        assert_eq!(parse_hotkey("Ctrl+Alt+F25"), None);
        assert_eq!(parse_hotkey("Cmd+A").is_some(), cfg!(target_os = "macos"));
    }

    #[test]
    fn label() {
        let m = egui::Modifiers {
            ctrl: true,
            alt: true,
            ..Default::default()
        };
        assert_eq!(hotkey_label(egui::Key::A, m), Some("Ctrl+Alt+A".into()));
        assert_eq!(hotkey_label(egui::Key::A, egui::Modifiers::default()), None);
    }
}
