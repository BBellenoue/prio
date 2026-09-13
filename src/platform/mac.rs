//! macOS backend: Carbon global shortcuts (the global-hotkey crate), the menu bar,
//! ~/Library/Application Support, and a unix socket to reach the resident.

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
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const DEFAULT_HOTKEY_ADD: &str = "Cmd+Alt+A";
pub const DEFAULT_HOTKEY_LIST: &str = "Cmd+Alt+P";
pub const MODS_HINT: &str = "Cmd, Ctrl et/ou Alt (Shift en plus si besoin)";
pub const CMD_LABEL: &str = "Cmd";
pub const FONT_REGULAR: &[(&str, u32)] = &[
    ("/System/Library/Fonts/SFNS.ttf", 0),
    ("/System/Library/Fonts/HelveticaNeue.ttc", 0),
];
pub const FONT_SEMIBOLD: &[(&str, u32)] = &[("/System/Library/Fonts/HelveticaNeue.ttc", 10)];

const SOCK_FILE: &str = "prio.sock";
const CMD_RELOAD: usize = 1;
const CMD_SUSPEND: usize = 2;

/// Commands received on the socket, applied by `Runtime::tick`: Carbon wants the main
/// thread to register or unregister a shortcut.
static CMD: AtomicUsize = AtomicUsize::new(0);
/// global-hotkey ids of the two current shortcuts, capture first, list second.
static IDS: Mutex<[u32; 2]> = Mutex::new([0, 0]);

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/prio")
}

pub fn today() -> (i32, u32, u32) {
    super::unix::today()
}

/// Size of the main screen, in egui points.
pub fn screen_points(ctx: &egui::Context) -> (f32, f32) {
    super::unix::screen_points(ctx)
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
/// pressed (Cmd+Alt+A, say) opens the window instead of staying with the panel.
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

/// Global shortcuts and the menu bar icon. Carbon and AppKit demand the main thread: both
/// are built in eframe's creation closure and driven by `tick`.
pub struct Runtime {
    manager: Option<GlobalHotKeyManager>,
    tray: Option<TrayIcon>,
    menu: Option<(MenuItem, MenuItem)>, // list, capture
    registered: Vec<HotKey>,
    status: Arc<AtomicUsize>,
}

impl Runtime {
    pub fn start(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) -> Runtime {
        let manager = GlobalHotKeyManager::new()
            .inspect_err(|e| dbg_log(&format!("GlobalHotKeyManager: {e}")))
            .ok();
        let settings = load_settings();
        let m_list = MenuItem::new(format!("Prio\t{}", settings.hotkey_list), true, None);
        let m_add = MenuItem::new(format!("Ajouter\t{}", settings.hotkey_add), true, None);
        let m_quit = MenuItem::new("Quitter", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&m_list, &m_add, &PredefinedMenuItem::separator(), &m_quit]);
        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(format!("Prio  ·  {}", settings.hotkey_list))
            .with_icon_as_template(true)
            .with_menu_on_left_click(false);
        if let Ok(i) = tray_icon::Icon::from_rgba(template_rgba(), 32, 32) {
            builder = builder.with_icon(i);
        }
        let tray = builder.build().inspect_err(|e| dbg_log(&format!("tray: {e}"))).ok();

        let ids = (m_list.id().clone(), m_add.id().clone(), m_quit.id().clone());
        std::thread::spawn(move || pump(ids, ctx, flag));

        let mut rt = Runtime {
            manager,
            tray,
            menu: Some((m_list, m_add)),
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
        if let Some((m_list, m_add)) = &self.menu {
            m_list.set_text(format!("Prio\t{}", s.hotkey_list));
            m_add.set_text(format!("Ajouter\t{}", s.hotkey_add));
        }
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(format!("Prio  ·  {}", s.hotkey_list)));
        }
    }
}

/// The waking thread: events arrive on global channels, the interface is repainted on
/// demand.
fn pump(
    (id_list, id_add, id_quit): (tray_icon::menu::MenuId, tray_icon::menu::MenuId, tray_icon::menu::MenuId),
    ctx: Arc<OnceLock<egui::Context>>,
    flag: Arc<AtomicUsize>,
) {
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
            let id = e.id();
            fire(if *id == id_list {
                HK_LIST
            } else if *id == id_add {
                HK_ADD
            } else if *id == id_quit {
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

/// Menu bar icon: a template image keeps only the alpha, so the bars of the logo become
/// holes and the tint follows the system appearance.
fn template_rgba() -> Vec<u8> {
    let mut px = logo_rgba(32);
    for p in px.as_chunks_mut::<4>().0 {
        if p[0] < 0x40 {
            p[3] = 0;
        }
        p[..3].copy_from_slice(&[0, 0, 0]);
    }
    px
}

/// Without a Dock icon, a window opened by a shortcut does not get the keyboard, and NSApp
/// ignores the request until it is on screen: keep asking over the first few passes.
pub fn activate(ctx: &egui::Context) {
    if ctx.cumulative_pass_nr() < 10 && !ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

/// A menu bar application: no Dock icon, no Cmd+Tab entry.
pub fn configure(opts: &mut eframe::NativeOptions) {
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    opts.event_loop_builder = Some(Box::new(|b| {
        b.with_activation_policy(ActivationPolicy::Accessory);
    }));
}
