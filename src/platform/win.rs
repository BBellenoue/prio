//! Backend Windows: Win32 (raccourcis globaux et boucle de messages), %APPDATA%, Segoe UI.

use super::{HK_ADD, HK_ADD_FAILED, HK_LIST, HK_LIST_FAILED, HK_QUIT, Hotkey, HotkeyKey};
use crate::{dbg_log, load_settings, logo_rgba, path};
use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

pub const DEFAULT_HOTKEY_ADD: &str = "Ctrl+Alt+A";
pub const DEFAULT_HOTKEY_LIST: &str = "Ctrl+Alt+P";
pub const MODS_HINT: &str = "Ctrl et/ou Alt (Shift en plus si besoin)";
pub const CMD_LABEL: &str = "Ctrl";
pub const FONT_REGULAR: &[(&str, u32)] = &[(r"C:\Windows\Fonts\segoeui.ttf", 0)];
pub const FONT_SEMIBOLD: &[(&str, u32)] = &[(r"C:\Windows\Fonts\seguisb.ttf", 0)];

const TID_FILE: &str = "resident.tid";
const WM_RELOAD_HOTKEYS: u32 = 0x8001; // WM_APP + 1, poste au thread des raccourcis
const WM_SUSPEND_HOTKEYS: u32 = 0x8002; // WM_APP + 2: desenregistre le temps d'une saisie

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("prio")
}

pub fn today() -> (i32, u32, u32) {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    (st.wYear as i32, st.wMonth as u32, st.wDay as u32)
}

/// Taille de l'ecran principal, en points egui.
pub fn screen_points(ctx: &egui::Context) -> (f32, f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let ppp = ctx.pixels_per_point();
    unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN) as f32 / ppp,
            GetSystemMetrics(SM_CYSCREEN) as f32 / ppp,
        )
    }
}

/// Modificateurs RegisterHotKey et code de touche virtuelle.
fn vk(h: Hotkey) -> (u32, u32) {
    let mut m = 0u32;
    if h.alt {
        m |= 0x0001;
    }
    if h.ctrl {
        m |= 0x0002;
    }
    if h.shift {
        m |= 0x0004;
    }
    if h.meta {
        m |= 0x0008;
    }
    let code = match h.key {
        HotkeyKey::Alnum(c) => c as u32,
        HotkeyKey::Function(n) => 0x6F + n,
        HotkeyKey::Space => 0x20,
    };
    (m, code)
}

/// Poste un message au thread des raccourcis du resident (le notre ou celui d'un autre process).
fn post_hotkey_thread(msg: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
    if let Some(tid) = std::fs::read_to_string(path(TID_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        unsafe { PostThreadMessageW(tid, msg, 0, 0) };
    }
}

/// Demande au resident de relire settings.json et de re-enregistrer ses raccourcis.
pub fn notify_hotkeys_changed() {
    post_hotkey_thread(WM_RELOAD_HOTKEYS);
}

/// Suspend les raccourcis globaux le temps d'une saisie: sinon Windows intercepte la
/// combinaison en cours (ex. Ctrl+Alt+A) et ouvre la fenetre au lieu de la laisser au panneau.
pub fn suspend_hotkeys() {
    post_hotkey_thread(WM_SUSPEND_HOTKEYS);
}

/// Demande au resident existant d'afficher la liste: message poste a son thread de raccourcis,
/// dont l'id est publie dans %APPDATA%\prio\resident.tid. false si aucun resident joignable.
pub fn wake_resident() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_HOTKEY};
    let Some(tid) = std::fs::read_to_string(path(TID_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return false;
    };
    unsafe { PostThreadMessageW(tid, WM_HOTKEY, HK_LIST, 0) != 0 }
}

/// (Re)enregistre les deux raccourcis d'apres settings.json. Renvoie false si AUCUN n'a pu l'etre
/// (typiquement: un autre resident tourne). Les echecs individuels sont publies dans `status`.
fn register_hotkeys(status: &AtomicUsize) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
    let s = load_settings();
    let mut bits = 0;
    for (id, spec, bit) in [(HK_ADD, &s.hotkey_add, HK_ADD_FAILED), (HK_LIST, &s.hotkey_list, HK_LIST_FAILED)] {
        unsafe { UnregisterHotKey(std::ptr::null_mut(), id as i32) };
        let ok = super::parse_hotkey(spec)
            .map(vk)
            .map(|(m, code)| unsafe { RegisterHotKey(std::ptr::null_mut(), id as i32, m | MOD_NOREPEAT, code) != 0 })
            .unwrap_or(false);
        if !ok {
            bits |= bit;
        }
    }
    status.store(bits, SeqCst);
    dbg_log(&format!("register_hotkeys {:?}/{:?} -> echecs={bits}", s.hotkey_add, s.hotkey_list));
    bits != HK_ADD_FAILED | HK_LIST_FAILED
}

/// Prend les raccourcis globaux. false: un autre resident les tient deja.
pub fn start_resident(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || hotkey_loop(tx, ctx, flag, status));
    rx.recv().unwrap_or(false)
}

/// Raccourcis et icone de zone de notification vivent dans le thread de la boucle de messages
/// (cf. `hotkey_loop`): rien a faire depuis l'interface.
pub struct Runtime;

impl Runtime {
    pub fn start(_: Arc<OnceLock<egui::Context>>, _: Arc<AtomicUsize>, _: Arc<AtomicUsize>) -> Runtime {
        Runtime
    }

    pub fn tick(&mut self) {}
}

/// La fenetre qui vient d'apparaitre a deja le focus sous Windows.
pub fn activate(_: &egui::Context) {}

/// Thread dedie: RegisterHotKey lie les raccourcis au thread appelant, et winit
/// n'expose pas WM_HOTKEY. Heberge aussi l'icone de zone de notification (meme boucle
/// de messages). Reveille l'UI via request_repaint quand un evenement tombe.
fn hotkey_loop(tx: std::sync::mpsc::Sender<bool>, ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage, WM_HOTKEY};
    let ok = register_hotkeys(&status);
    let _ = tx.send(ok);
    if !ok {
        return;
    }
    let _ = std::fs::write(path(TID_FILE), unsafe { GetCurrentThreadId() }.to_string());

    let settings = load_settings();
    let menu = Menu::new();
    let m_list = MenuItem::new(format!("Prio\t{}", settings.hotkey_list), true, None);
    let m_add = MenuItem::new(format!("Ajouter\t{}", settings.hotkey_add), true, None);
    let m_quit = MenuItem::new("Quitter", true, None);
    let _ = menu.append_items(&[&m_list, &m_add, &PredefinedMenuItem::separator(), &m_quit]);
    let icon = tray_icon::Icon::from_rgba(logo_rgba(32), 32, 32).ok();
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("Prio  ·  {}", settings.hotkey_list))
        .with_menu_on_left_click(false);
    if let Some(i) = icon {
        builder = builder.with_icon(i);
    }
    let tray = builder.build(); // garde l'icone vivante jusqu'a la fin du thread

    let fire = |n: usize| {
        flag.store(n, SeqCst);
        if let Some(c) = ctx.get() {
            c.request_repaint();
        }
    };
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let r = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
        if r <= 0 {
            dbg_log(&format!("hotkey_loop: GetMessageW={r} -> fin du thread"));
            break;
        }
        if msg.message == WM_HOTKEY {
            dbg_log(&format!("WM_HOTKEY {}", msg.wParam));
            fire(msg.wParam);
        }
        if msg.message == WM_SUSPEND_HOTKEYS {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey;
            unsafe {
                UnregisterHotKey(std::ptr::null_mut(), HK_ADD as i32);
                UnregisterHotKey(std::ptr::null_mut(), HK_LIST as i32);
            }
            dbg_log("raccourcis suspendus (saisie en cours)");
        }
        if msg.message == WM_RELOAD_HOTKEYS {
            register_hotkeys(&status);
            let s = load_settings();
            m_list.set_text(format!("Prio\t{}", s.hotkey_list));
            m_add.set_text(format!("Ajouter\t{}", s.hotkey_add));
            if let Ok(t) = &tray {
                let _ = t.set_tooltip(Some(format!("Prio  ·  {}", s.hotkey_list)));
            }
            fire(0); // rafraichit le panneau Reglages (etat des raccourcis)
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            let id = e.id();
            fire(if *id == *m_list.id() {
                HK_LIST
            } else if *id == *m_add.id() {
                HK_ADD
            } else if *id == *m_quit.id() {
                HK_QUIT
            } else {
                0
            });
        }
        while let Ok(e) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                fire(HK_LIST);
            }
        }
    }
}

/// Rien a regler cote fenetre: eframe suffit.
pub fn configure(_: &mut eframe::NativeOptions) {}
