//! Windows backend: Win32 (global shortcuts and a message loop), %APPDATA%, Segoe UI.

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
const WM_RELOAD_HOTKEYS: u32 = 0x8001; // WM_APP + 1, posted to the shortcut thread
const WM_SUSPEND_HOTKEYS: u32 = 0x8002; // WM_APP + 2: unregisters for the time of a capture

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("prio")
}

pub fn today() -> (i32, u32, u32) {
    use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    (st.wYear as i32, st.wMonth as u32, st.wDay as u32)
}

// ponytail: always the primary monitor, so a window still opens away from the pointer on a
// multi-screen desktop; GetCursorPos plus MonitorFromPoint is the way up.
pub fn pointer_screen(ctx: &egui::Context) -> egui::Rect {
    let (w, h) = screen_points(ctx);
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h))
}

/// Size of the main screen, in egui points.
fn screen_points(ctx: &egui::Context) -> (f32, f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let ppp = ctx.pixels_per_point();
    unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN) as f32 / ppp,
            GetSystemMetrics(SM_CYSCREEN) as f32 / ppp,
        )
    }
}

/// RegisterHotKey modifiers and virtual key code.
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

/// Posts a message to the resident's shortcut thread, ours or another process's.
fn post_hotkey_thread(msg: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW;
    if let Some(tid) = std::fs::read_to_string(path(TID_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        unsafe { PostThreadMessageW(tid, msg, 0, 0) };
    }
}

/// Asks the resident to read settings.json again and register its shortcuts anew.
pub fn notify_hotkeys_changed() {
    post_hotkey_thread(WM_RELOAD_HOTKEYS);
}

/// Suspends the global shortcuts for the time of a capture: otherwise Windows catches the
/// combination being pressed (Ctrl+Alt+A, say) and opens the window instead of leaving it
/// to the panel.
pub fn suspend_hotkeys() {
    post_hotkey_thread(WM_SUSPEND_HOTKEYS);
}

/// Asks the running resident to show the list: a message posted to its shortcut thread,
/// whose id it publishes in %APPDATA%\prio\resident.tid. false when no resident answers.
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

/// (Re)registers both shortcuts from settings.json. Returns false when NEITHER could be taken
/// (typically: another resident is running). Individual failures are published in `status`.
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
    dbg_log(&format!(
        "register_hotkeys {:?}/{:?} -> failures={bits}",
        s.hotkey_add, s.hotkey_list
    ));
    bits != HK_ADD_FAILED | HK_LIST_FAILED
}

/// Takes the global shortcuts. false: another resident already holds them.
pub fn start_resident(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || hotkey_loop(tx, ctx, flag, status));
    rx.recv().unwrap_or(false)
}

/// Shortcuts and the notification area icon live in the message loop thread (see
/// `hotkey_loop`): nothing for the interface to do.
pub struct Runtime;

impl Runtime {
    pub fn start(_: Arc<OnceLock<egui::Context>>, _: Arc<AtomicUsize>, _: Arc<AtomicUsize>) -> Runtime {
        Runtime
    }

    pub fn tick(&mut self) {}
}

/// A window that has just appeared already has focus on Windows.
pub fn activate(_: &egui::Context) {}

/// A thread of its own: RegisterHotKey binds the shortcuts to the calling thread, and winit
/// does not expose WM_HOTKEY. It also hosts the notification area icon (same message loop).
/// Wakes the interface with request_repaint whenever an event lands.
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
    let tray = builder.build(); // keeps the icon alive until the thread ends

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
            dbg_log(&format!("hotkey_loop: GetMessageW={r} -> thread ends"));
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
            dbg_log("shortcuts suspended (capture in progress)");
        }
        if msg.message == WM_RELOAD_HOTKEYS {
            register_hotkeys(&status);
            let s = load_settings();
            m_list.set_text(format!("Prio\t{}", s.hotkey_list));
            m_add.set_text(format!("Ajouter\t{}", s.hotkey_add));
            if let Ok(t) = &tray {
                let _ = t.set_tooltip(Some(format!("Prio  ·  {}", s.hotkey_list)));
            }
            fire(0); // refreshes the settings panel (shortcut state)
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

/// Nothing to set on the window side: eframe is enough.
pub fn configure(_: &mut eframe::NativeOptions) {}
