//! Linux backend: X11 global shortcuts (the global-hotkey crate), a notification area icon on
//! a GTK thread, the XDG data folder, and a unix socket to reach the resident.
//!
//! X11 only: under Wayland a client cannot grab a key combination for itself, and the desktop
//! portal that replaces it is not something global-hotkey speaks.

use super::unix::{ID_ADD, ID_LIST, ID_QUIT, Shortcuts, labels};
use crate::{Settings, dbg_log, load_settings, logo_rgba};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tray_icon::TrayIconBuilder;
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

pub use super::unix::{activate, notify_hotkeys_changed, start_resident, suspend_hotkeys, today, wake_resident};

pub const DEFAULT_HOTKEY_ADD: &str = "Ctrl+Alt+A";
pub const DEFAULT_HOTKEY_LIST: &str = "Ctrl+Alt+P";
pub const MODS_HINT: &str = "Ctrl et/ou Alt (Shift en plus si besoin)";
pub const CMD_LABEL: &str = "Ctrl";
pub const FONT_REGULAR: &[(&str, u32)] = &[
    ("/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
    ("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf", 0),
];
pub const FONT_SEMIBOLD: &[(&str, u32)] = &[
    ("/usr/share/fonts/truetype/ubuntu/Ubuntu-M.ttf", 0),
    ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 0),
    ("/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf", 0),
];

pub fn data_dir() -> PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share"),
    }
    .join("prio")
}

/// The screen holding the mouse pointer, in egui points. X11 counts both the pointer and the
/// screens in physical pixels from the top left of the virtual screen, the corner a viewport
/// position also counts from, so only the scale has to come out.
pub fn pointer_screen(ctx: &egui::Context) -> egui::Rect {
    let ppp = ctx.pixels_per_point();
    match screen_at_pointer() {
        Some(r) => r / ppp,
        None => super::origin_screen(ctx),
    }
}

/// XQueryPointer on the root window, then the Xinerama screen holding it, in pixels. A
/// connection of its own: GTK runs on the tray thread and is not shared.
fn screen_at_pointer() -> Option<egui::Rect> {
    let xlib = x11_dl::xlib::Xlib::open().ok()?;
    let xinerama = x11_dl::xinerama::Xlib::open().ok()?;
    unsafe {
        let dpy = (xlib.XOpenDisplay)(std::ptr::null());
        if dpy.is_null() {
            return None;
        }
        let root = (xlib.XDefaultRootWindow)(dpy);
        let (mut win, mut child): (x11_dl::xlib::Window, x11_dl::xlib::Window) = (0, 0);
        let (mut px, mut py, mut wx, mut wy) = (0, 0, 0, 0);
        let mut mask = 0;
        let on_screen = (xlib.XQueryPointer)(dpy, root, &mut win, &mut child, &mut px, &mut py, &mut wx, &mut wy, &mut mask) != 0;
        let mut count = 0;
        let screens = (xinerama.XineramaQueryScreens)(dpy, &mut count);
        let mut found = None;
        if on_screen && !screens.is_null() {
            let heads: Vec<(f32, f32, f32, f32)> = std::slice::from_raw_parts(screens, count.max(0) as usize)
                .iter()
                .map(|s| (f32::from(s.x_org), f32::from(s.y_org), f32::from(s.width), f32::from(s.height)))
                .collect();
            found = super::screen_at(px as f32, py as f32, &heads);
            (xlib.XFree)(screens.cast());
        }
        (xlib.XCloseDisplay)(dpy);
        found
    }
}

/// Global shortcuts and the notification area icon. The X11 shortcuts run on a thread of their
/// own inside global-hotkey; the icon needs a GTK loop, which gets its own thread too, and
/// new labels reach it through a channel.
pub struct Runtime {
    shortcuts: Shortcuts,
    menu: Sender<Settings>,
}

impl Runtime {
    pub fn start(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) -> Runtime {
        let (tx, rx) = channel();
        std::thread::spawn(move || tray_thread(rx));
        std::thread::spawn(move || super::unix::pump(ctx, flag));

        let mut rt = Runtime {
            shortcuts: Shortcuts::new(status),
            menu: tx,
        };
        let s = rt.shortcuts.reload();
        let _ = rt.menu.send(s);
        rt
    }

    /// Applies the commands that arrived on the socket.
    pub fn tick(&mut self) {
        if let Some(s) = self.shortcuts.tick() {
            let _ = self.menu.send(s);
        }
    }
}

/// The notification area icon lives here: GTK wants its own loop, and the menu items can only
/// be touched from the thread that built them, so new settings arrive through a channel.
fn tray_thread(settings: Receiver<Settings>) {
    if gtk::init().is_err() {
        dbg_log("gtk::init failed: no notification area icon");
        return;
    }
    let (list, add, tooltip) = labels(&load_settings());
    let m_list = MenuItem::with_id(ID_LIST, list, true, None);
    let m_add = MenuItem::with_id(ID_ADD, add, true, None);
    let m_quit = MenuItem::with_id(ID_QUIT, "Quitter", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&m_list, &m_add, &PredefinedMenuItem::separator(), &m_quit]);
    let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(tooltip);
    if let Ok(i) = tray_icon::Icon::from_rgba(logo_rgba(32), 32, 32) {
        builder = builder.with_icon(i);
    }
    let tray = builder.build().inspect_err(|e| dbg_log(&format!("tray: {e}"))).ok();

    gtk::glib::timeout_add_local(Duration::from_millis(300), move || {
        while let Ok(s) = settings.try_recv() {
            let (list, add, tooltip) = labels(&s);
            m_list.set_text(list);
            m_add.set_text(add);
            if let Some(t) = &tray {
                let _ = t.set_tooltip(Some(tooltip));
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

/// Nothing to set on the window side: eframe is enough.
pub fn configure(_: &mut eframe::NativeOptions) {}
