//! Backend macOS: raccourcis globaux Carbon (crate global-hotkey), barre de menus,
//! ~/Library/Application Support, socket unix pour joindre le resident.

use super::{HK_ADD, HK_ADD_FAILED, HK_LIST, HK_LIST_FAILED, HK_QUIT, Hotkey, HotkeyKey};
use crate::{dbg_log, load_settings, logo_rgba, path};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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

/// Commandes recues sur la socket, appliquees par `Runtime::tick`: Carbon veut le thread
/// principal pour (des)enregistrer un raccourci.
static CMD: AtomicUsize = AtomicUsize::new(0);
/// Identifiants global-hotkey des deux raccourcis courants, pour l'ajout et pour la liste.
static IDS: Mutex<[u32; 2]> = Mutex::new([0, 0]);

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/prio")
}

pub fn today() -> (i32, u32, u32) {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    unsafe { libc::localtime_r(&now, &mut tm) };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

/// Taille de l'ecran principal, en points egui.
pub fn screen_points(ctx: &egui::Context) -> (f32, f32) {
    let size = ctx.input(|i| i.viewport().monitor_size);
    size.map(|s| (s.x, s.y)).unwrap_or((1440.0, 900.0))
}

/// Raccourci global-hotkey correspondant.
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
    UnixStream::connect(path(SOCK_FILE))
        .and_then(|mut s| s.write_all(cmd.as_bytes()))
        .is_ok()
}

/// Demande au resident de relire settings.json et de re-enregistrer ses raccourcis.
pub fn notify_hotkeys_changed() {
    send("reload\n");
}

/// Suspend les raccourcis globaux le temps d'une saisie: sinon la combinaison en cours
/// (ex. Cmd+Alt+A) ouvre la fenetre au lieu de rester au panneau.
pub fn suspend_hotkeys() {
    send("suspend\n");
}

/// Demande au resident existant d'afficher la liste. false si aucun resident joignable.
pub fn wake_resident() -> bool {
    send("list\n")
}

/// Ouvre la socket du resident: c'est elle qui fait l'instance unique. false: un autre
/// resident repond deja dessus.
pub fn start_resident(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, _: Arc<AtomicUsize>) -> bool {
    let sock = path(SOCK_FILE);
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(_) => {
            if UnixStream::connect(&sock).is_ok() {
                return false;
            }
            // socket laissee par un resident mort
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
                continue; // sonde d'instance unique: elle se connecte sans rien ecrire
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

/// Raccourcis globaux et icone de barre de menus. Carbon et AppKit exigent le thread
/// principal: construits dans la closure de creation d'eframe, pilotes par `tick`.
pub struct Runtime {
    manager: Option<GlobalHotKeyManager>,
    tray: Option<TrayIcon>,
    menu: Option<(MenuItem, MenuItem)>, // liste, ajout
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

    /// Applique les commandes arrivees sur la socket.
    pub fn tick(&mut self) {
        let cmd = CMD.swap(0, SeqCst);
        if cmd & CMD_SUSPEND != 0 {
            self.unregister();
            dbg_log("raccourcis suspendus (saisie en cours)");
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

    /// (Re)enregistre les deux raccourcis d'apres settings.json, et met a jour le menu.
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
        dbg_log(&format!("register_hotkeys {:?}/{:?} -> echecs={bits}", s.hotkey_add, s.hotkey_list));
        if let Some((m_list, m_add)) = &self.menu {
            m_list.set_text(format!("Prio\t{}", s.hotkey_list));
            m_add.set_text(format!("Ajouter\t{}", s.hotkey_add));
        }
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(format!("Prio  ·  {}", s.hotkey_list)));
        }
    }
}

/// Thread de reveil: les evenements arrivent sur des canaux globaux, l'interface est
/// repeinte a la demande.
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

/// Icone de barre de menus: une image template ne garde que l'alpha, donc les barres du
/// logo deviennent des trous et la teinte suit le theme du systeme.
fn template_rgba() -> Vec<u8> {
    let mut px = logo_rgba(32);
    for p in px.chunks_exact_mut(4) {
        if p[0] < 0x40 {
            p[3] = 0;
        }
        p[..3].copy_from_slice(&[0, 0, 0]);
    }
    px
}

/// Sans icone dans le Dock, une fenetre ouverte par un raccourci n'a pas le focus, et NSApp
/// ignore la demande tant qu'elle n'est pas affichee: on insiste sur les premieres passes.
pub fn activate(ctx: &egui::Context) {
    if ctx.cumulative_pass_nr() < 10 && !ctx.input(|i| i.viewport().focused.unwrap_or(false)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }
}

/// Application de barre de menus: pas d'icone dans le Dock, pas de bascule Cmd+Tab.
pub fn configure(opts: &mut eframe::NativeOptions) {
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    opts.event_loop_builder = Some(Box::new(|b| {
        b.with_activation_policy(ActivationPolicy::Accessory);
    }));
}
