//! Linux backend: X11 global shortcuts (the global-hotkey crate), a notification area icon on
//! a GTK thread, the XDG data folder, and a unix socket to reach the resident.
//!
//! X11 only: under Wayland a client cannot grab a key combination for itself, and the desktop
//! portal that replaces it is not something global-hotkey speaks.

use super::{HK_ADD, HK_ADD_FAILED, HK_LIST, HK_LIST_FAILED, HK_QUIT, Hotkey, HotkeyKey};
use crate::{dbg_log, load_settings, logo_rgba, path};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

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

const SOCK_FILE: &str = "prio.sock";
const CMD_RELOAD: usize = 1;
const CMD_SUSPEND: usize = 2;
const ID_LIST: &str = "prio-list";
const ID_ADD: &str = "prio-add";
const ID_QUIT: &str = "prio-quit";

/// Commands received on the socket, applied by `Runtime::tick`.
static CMD: AtomicUsize = AtomicUsize::new(0);
/// global-hotkey ids of the two current shortcuts, capture first, list second.
static IDS: Mutex<[u32; 2]> = Mutex::new([0, 0]);

pub fn data_dir() -> PathBuf {
    match std::env::var("XDG_DATA_HOME") {
        Ok(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share"),
    }
    .join("prio")
}

pub fn today() -> (i32, u32, u32) {
    super::unix::today()
}

/// The screen holding the mouse pointer, in egui points. X11 counts both the pointer and the
/// screens in physical pixels from the top left of the virtual screen, the corner a viewport
/// position also counts from, so only the scale has to come out.
pub fn pointer_screen(ctx: &egui::Context) -> egui::Rect {
    let ppp = ctx.pixels_per_point();
    match screen_at_pointer() {
        Some(r) => egui::Rect::from_min_size(r.min / ppp, r.size() / ppp),
        None => super::unix::pointer_screen(ctx),
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

/// The matching global-hotkey shortcut.
fn hotkey(h: Hotkey) -> Option<HotKey> {
    let mut m = Modifiers::empty();
    if h.ctrl {
        m |= Modifiers::CONTROL;
    }
    if h.alt {
        m |= Modifiers::ALT;
    }
    if h.shift {
        m |= Modifiers::SHIFT;
    }
    if h.meta {
        m |= Modifiers::SUPER;
    }
    let name = match h.key {
        HotkeyKey::Alnum(c) if c.is_ascii_alphabetic() => format!("Key{}", c as char),
        HotkeyKey::Alnum(c) => format!("Digit{}", c as char),
        HotkeyKey::Function(n) => format!("F{n}"),
        HotkeyKey::Space => "Space".into(),
    };
    Some(HotKey::new(Some(m), Code::from_str(&name).ok()?))
}

fn send(cmd: &str) -> bool {
    super::unix::send(&path(SOCK_FILE), cmd)
}

/// Asks the resident to read settings.json again and register its shortcuts anew.
pub fn notify_hotkeys_changed() {
    send("reload\n");
}

/// Suspends the global shortcuts for the time of a capture: otherwise the combination being
/// pressed (Ctrl+Alt+A, say) opens the window instead of staying with the panel.
pub fn suspend_hotkeys() {
    send("suspend\n");
}

/// Asks the running resident to show the list. false when no resident answers.
pub fn wake_resident() -> bool {
    send("list\n")
}

/// Opens the resident's socket, which is what makes the instance unique. false: another
/// resident already answers on it.
pub fn start_resident(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, _: Arc<AtomicUsize>) -> bool {
    super::unix::listen(path(SOCK_FILE), move |cmd| {
        match cmd {
            "list" => flag.store(HK_LIST, SeqCst),
            "reload" => {
                CMD.fetch_or(CMD_RELOAD, SeqCst);
            }
            "suspend" => {
                CMD.fetch_or(CMD_SUSPEND, SeqCst);
            }
            _ => return,
        }
        if let Some(c) = ctx.get() {
            c.request_repaint();
        }
    })
}

/// Global shortcuts and the notification area icon. The X11 shortcuts run on a thread of their
/// own inside global-hotkey; the icon needs a GTK loop, which gets its own thread too.
pub struct Runtime {
    manager: Option<GlobalHotKeyManager>,
    menu: Option<std::sync::mpsc::Sender<(String, String)>>, // list label, capture label
    registered: Vec<HotKey>,
    status: Arc<AtomicUsize>,
}

impl Runtime {
    pub fn start(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) -> Runtime {
        let manager = GlobalHotKeyManager::new()
            .inspect_err(|e| dbg_log(&format!("GlobalHotKeyManager: {e}")))
            .ok();
        let (tx, rx) = std::sync::mpsc::channel();
        let settings = load_settings();
        let labels = (
            format!("Prio\t{}", settings.hotkey_list),
            format!("Ajouter\t{}", settings.hotkey_add),
        );
        std::thread::spawn(move || tray_thread(labels, rx));
        std::thread::spawn(move || pump(ctx, flag));

        let mut rt = Runtime {
            manager,
            menu: Some(tx),
            registered: Vec::new(),
            status,
        };
        rt.reload();
        rt
    }

    /// Applies the commands that arrived on the socket.
    pub fn tick(&mut self) {
        let cmd = CMD.swap(0, SeqCst);
        if cmd & CMD_SUSPEND != 0 {
            self.unregister();
            dbg_log("shortcuts suspended (capture in progress)");
        }
        if cmd & CMD_RELOAD != 0 {
            self.reload();
        }
    }

    fn unregister(&mut self) {
        if let Some(m) = &self.manager {
            for hk in self.registered.drain(..) {
                let _ = m.unregister(hk);
            }
        }
        *IDS.lock().unwrap() = [0, 0];
    }

    /// (Re)registers both shortcuts from settings.json, and updates the menu.
    fn reload(&mut self) {
        self.unregister();
        let s = load_settings();
        let mut bits = 0;
        let mut ids = [0u32; 2];
        for (slot, spec, bit) in [(0, &s.hotkey_add, HK_ADD_FAILED), (1, &s.hotkey_list, HK_LIST_FAILED)] {
            let registered = super::parse_hotkey(spec)
                .and_then(hotkey)
                .filter(|hk| self.manager.as_ref().map(|m| m.register(*hk).is_ok()).unwrap_or(false));
            match registered {
                Some(hk) => {
                    ids[slot] = hk.id();
                    self.registered.push(hk);
                }
                None => bits |= bit,
            }
        }
        *IDS.lock().unwrap() = ids;
        self.status.store(bits, SeqCst);
        dbg_log(&format!(
            "register_hotkeys {:?}/{:?} -> failures={bits}",
            s.hotkey_add, s.hotkey_list
        ));
        if let Some(tx) = &self.menu {
            let _ = tx.send((format!("Prio\t{}", s.hotkey_list), format!("Ajouter\t{}", s.hotkey_add)));
        }
    }
}

/// The notification area icon lives here: GTK wants its own loop, and the menu items can only
/// be touched from the thread that built them, so new labels arrive through a channel.
fn tray_thread((list_label, add_label): (String, String), labels: std::sync::mpsc::Receiver<(String, String)>) {
    if gtk::init().is_err() {
        dbg_log("gtk::init failed: no notification area icon");
        return;
    }
    let m_list = MenuItem::with_id(ID_LIST, list_label, true, None);
    let m_add = MenuItem::with_id(ID_ADD, add_label, true, None);
    let m_quit = MenuItem::with_id(ID_QUIT, "Quitter", true, None);
    let menu = Menu::new();
    let _ = menu.append_items(&[&m_list, &m_add, &PredefinedMenuItem::separator(), &m_quit]);
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("Prio  ·  {}", load_settings().hotkey_list));
    if let Ok(i) = tray_icon::Icon::from_rgba(logo_rgba(32), 32, 32) {
        builder = builder.with_icon(i);
    }
    let tray = builder.build().inspect_err(|e| dbg_log(&format!("tray: {e}"))).ok();

    gtk::glib::timeout_add_local(Duration::from_millis(300), move || {
        while let Ok((list, add)) = labels.try_recv() {
            m_list.set_text(&list);
            m_add.set_text(&add);
            if let Some(t) = &tray {
                let _ = t.set_tooltip(Some(list.replace('\t', "  ·  ")));
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    gtk::main();
}

/// The waking thread: events arrive on global channels, the interface is repainted on demand.
fn pump(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>) {
    let fire = |n: usize| {
        flag.store(n, SeqCst);
        if let Some(c) = ctx.get() {
            c.request_repaint();
        }
    };
    loop {
        if let Ok(e) = GlobalHotKeyEvent::receiver().recv_timeout(Duration::from_millis(100))
            && e.state == HotKeyState::Pressed
        {
            let ids = *IDS.lock().unwrap();
            dbg_log(&format!("hotkey {}", e.id));
            if e.id == ids[0] {
                fire(HK_ADD);
            } else if e.id == ids[1] {
                fire(HK_LIST);
            }
        }
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            fire(match e.id().as_ref() {
                ID_LIST => HK_LIST,
                ID_ADD => HK_ADD,
                ID_QUIT => HK_QUIT,
                _ => 0,
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

/// A window opened by a shortcut does not always come to the front: ask, over the first few
/// passes, until the window manager has given it the keyboard.
pub fn activate(ctx: &egui::Context) {
    if ctx.cumulative_pass_nr() < 10 && !ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

/// Nothing to set on the window side: eframe is enough.
pub fn configure(_: &mut eframe::NativeOptions) {}
