//! macOS backend: Carbon global shortcuts (the global-hotkey crate), the menu bar,
//! ~/Library/Application Support, and a unix socket to reach the resident.

use super::unix::{ID_ADD, ID_LIST, ID_QUIT, Shortcuts, labels};
use crate::{Settings, dbg_log, load_settings, logo_rgba};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};
use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

pub use super::unix::{activate, notify_hotkeys_changed, start_resident, suspend_hotkeys, today, wake_resident};

pub const DEFAULT_HOTKEY_ADD: &str = "Cmd+Alt+A";
pub const DEFAULT_HOTKEY_LIST: &str = "Cmd+Alt+P";
pub const MODS_HINT: &str = "Cmd, Ctrl et/ou Alt (Shift en plus si besoin)";
pub const CMD_LABEL: &str = "Cmd";
pub const FONT_REGULAR: &[(&str, u32)] = &[
    ("/System/Library/Fonts/SFNS.ttf", 0),
    ("/System/Library/Fonts/HelveticaNeue.ttc", 0),
];
pub const FONT_SEMIBOLD: &[(&str, u32)] = &[("/System/Library/Fonts/HelveticaNeue.ttc", 10)];

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Library/Application Support/prio")
}

/// The screen holding the mouse pointer, in egui points. Quartz measures both the pointer
/// and the screens in the same space as a viewport position, origin at the top left of the
/// main screen, so nothing has to be flipped.
pub fn pointer_screen(ctx: &egui::Context) -> egui::Rect {
    use core_graphics::display::CGDisplay;
    use core_graphics::event::CGEvent;
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let at = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .and_then(CGEvent::new)
        .map(|e| e.location());
    if let Ok(p) = at
        && let Ok(ids) = CGDisplay::active_displays()
    {
        let screens: Vec<(f32, f32, f32, f32)> = ids
            .into_iter()
            .map(|id| CGDisplay::new(id).bounds())
            .map(|b| (b.origin.x as f32, b.origin.y as f32, b.size.width as f32, b.size.height as f32))
            .collect();
        if let Some(r) = super::screen_at(p.x as f32, p.y as f32, &screens) {
            return r;
        }
    }
    super::origin_screen(ctx)
}

/// Global shortcuts and the menu bar icon. Carbon and AppKit demand the main thread: both
/// are built in eframe's creation closure and driven by `tick`.
pub struct Runtime {
    shortcuts: Shortcuts,
    tray: Option<TrayIcon>,
    menu: (MenuItem, MenuItem), // list, capture
}

impl Runtime {
    pub fn start(ctx: Arc<OnceLock<egui::Context>>, flag: Arc<AtomicUsize>, status: Arc<AtomicUsize>) -> Runtime {
        let (list, add, tooltip) = labels(&load_settings());
        let m_list = MenuItem::with_id(ID_LIST, list, true, None);
        let m_add = MenuItem::with_id(ID_ADD, add, true, None);
        let m_quit = MenuItem::with_id(ID_QUIT, "Quitter", true, None);
        let menu = Menu::new();
        let _ = menu.append_items(&[&m_list, &m_add, &PredefinedMenuItem::separator(), &m_quit]);
        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip)
            .with_icon_as_template(true)
            .with_menu_on_left_click(false);
        if let Ok(i) = tray_icon::Icon::from_rgba(template_rgba(), 32, 32) {
            builder = builder.with_icon(i);
        }
        let tray = builder.build().inspect_err(|e| dbg_log(&format!("tray: {e}"))).ok();
        std::thread::spawn(move || super::unix::pump(ctx, flag));

        let mut rt = Runtime {
            shortcuts: Shortcuts::new(status),
            tray,
            menu: (m_list, m_add),
        };
        let s = rt.shortcuts.reload();
        rt.relabel(&s);
        rt
    }

    /// Applies the commands that arrived on the socket.
    pub fn tick(&mut self) {
        if let Some(s) = self.shortcuts.tick() {
            self.relabel(&s);
        }
    }

    fn relabel(&self, s: &Settings) {
        let (list, add, tooltip) = labels(s);
        self.menu.0.set_text(list);
        self.menu.1.set_text(add);
        if let Some(t) = &self.tray {
            let _ = t.set_tooltip(Some(tooltip));
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

/// A menu bar application: no Dock icon, no Cmd+Tab entry.
pub fn configure(opts: &mut eframe::NativeOptions) {
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    opts.event_loop_builder = Some(Box::new(|b| {
        b.with_activation_policy(ActivationPolicy::Accessory);
    }));
}
