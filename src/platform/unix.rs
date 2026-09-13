//! What the macOS and Linux backends share: the local date, the socket that carries a
//! command to the resident, the global-hotkey shortcuts and the thread that pumps their
//! events, and how a window opened by a shortcut is brought to the front.

use super::{HK_ADD, HK_ADD_FAILED, HK_LIST, HK_LIST_FAILED, HK_QUIT, Hotkey, HotkeyKey};
use crate::{Settings, dbg_log, load_settings, path};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

const SOCK_FILE: &str = "prio.sock";
const CMD_RELOAD: usize = 1;
const CMD_SUSPEND: usize = 2;
/// Menu item ids, the same on both systems so `pump` can tell them apart.
pub const ID_LIST: &str = "prio-list";
pub const ID_ADD: &str = "prio-add";
pub const ID_QUIT: &str = "prio-quit";

/// Commands received on the socket, applied by `Shortcuts::tick`: Carbon wants the main
/// thread to register or unregister a shortcut.
static CMD: AtomicUsize = AtomicUsize::new(0);
/// global-hotkey ids of the two current shortcuts, capture first, list second.
static IDS: Mutex<[u32; 2]> = Mutex::new([0, 0]);

pub fn today() -> (i32, u32, u32) {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    unsafe { libc::localtime_r(&now, &mut tm) };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

/// Menu labels and tooltip for these settings: list, capture, tooltip.
pub fn labels(s: &Settings) -> (String, String, String) {
    (
        format!("Prio\t{}", s.hotkey_list),
        format!("Ajouter\t{}", s.hotkey_add),
        format!("Prio  ·  {}", s.hotkey_list),
    )
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

/// Sends one command to the resident. false when no resident answers.
fn send(cmd: &str) -> bool {
    UnixStream::connect(path(SOCK_FILE))
        .and_then(|mut s| s.write_all(cmd.as_bytes()))
        .is_ok()
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

/// Opens the resident's socket and reads commands from it until the process ends. Holding
/// the socket is what makes the instance unique: false means another resident answers on it.
pub fn start_resident(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, _: Arc<AtomicUsize>) -> bool {
    let sock = path(SOCK_FILE);
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(_) => {
            if UnixStream::connect(&sock).is_ok() {
                return false;
            }
            // socket left behind by a dead resident
            let _ = std::fs::remove_file(&sock);
            match UnixListener::bind(&sock) {
                Ok(l) => l,
                Err(e) => {
                    dbg_log(&format!("socket {}: {e}", sock.display()));
                    return false;
                }
            }
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_err() {
                continue;
            }
            let cmd = line.trim();
            if cmd.is_empty() {
                continue; // the single-instance probe: it connects without writing anything
            }
            dbg_log(&format!("socket: {cmd}"));
            match cmd {
                "list" => flag.store(HK_LIST, SeqCst),
                "reload" => {
                    CMD.fetch_or(CMD_RELOAD, SeqCst);
                }
                "suspend" => {
                    CMD.fetch_or(CMD_SUSPEND, SeqCst);
                }
                _ => continue,
            }
            if let Some(c) = ctx.get() {
                c.request_repaint();
            }
        }
    });
    true
}

/// The global shortcuts: the manager and what is registered with it. Built on the main
/// thread (Carbon demands it) and driven by `tick`.
pub struct Shortcuts {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    status: Arc<AtomicUsize>,
}

impl Shortcuts {
    pub fn new(status: Arc<AtomicUsize>) -> Shortcuts {
        let manager = GlobalHotKeyManager::new()
            .inspect_err(|e| dbg_log(&format!("GlobalHotKeyManager: {e}")))
            .ok();
        Shortcuts {
            manager,
            registered: Vec::new(),
            status,
        }
    }

    /// Applies the commands that arrived on the socket. Some after a reload: the menu labels
    /// follow the new settings.
    pub fn tick(&mut self) -> Option<Settings> {
        let cmd = CMD.swap(0, SeqCst);
        if cmd & CMD_SUSPEND != 0 {
            self.unregister();
            dbg_log("shortcuts suspended (capture in progress)");
        }
        (cmd & CMD_RELOAD != 0).then(|| self.reload())
    }

    fn unregister(&mut self) {
        if let Some(m) = &self.manager {
            for hk in self.registered.drain(..) {
                let _ = m.unregister(hk);
            }
        }
        *IDS.lock().unwrap() = [0, 0];
    }

    /// (Re)registers both shortcuts from settings.json, and returns those settings.
    pub fn reload(&mut self) -> Settings {
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
        s
    }
}

/// The waking thread: shortcut, menu and icon events arrive on global channels, the
/// interface is repainted on demand.
pub fn pump(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>) {
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

/// A window opened by a shortcut does not always get the keyboard (no Dock icon on macOS,
/// a window manager taking its time on X11): keep asking over the first few passes.
pub fn activate(ctx: &egui::Context) {
    if ctx.cumulative_pass_nr() < 10 && !ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}
