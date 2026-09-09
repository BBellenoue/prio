#![windows_subsystem = "windows"]

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Id, Layout, Rect, RichText, Sense, Stroke, StrokeKind, TextEdit,
    ViewportCommand, pos2, vec2,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

const BG: Color32 = Color32::from_rgb(0x0F, 0x11, 0x18);
const CARD: Color32 = Color32::from_rgb(0x19, 0x1D, 0x29);
const CARD_HOVER: Color32 = Color32::from_rgb(0x23, 0x28, 0x38);
const BORDER: Color32 = Color32::from_rgb(0x2B, 0x31, 0x44);
const FIELD: Color32 = Color32::from_rgb(0x15, 0x19, 0x24);
const ACCENT: Color32 = Color32::from_rgb(0x8A, 0xB4, 0xFF);
const TEXT: Color32 = Color32::from_rgb(0xF4, 0xF6, 0xFA);
const MUTED: Color32 = Color32::from_rgb(0x8E, 0x97, 0xAB);
const DIM: Color32 = Color32::from_rgb(0x3C, 0x44, 0x5A);
const RED: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x6B);
const AMBER: Color32 = Color32::from_rgb(0xFF, 0xC5, 0x4D);
const GREEN: Color32 = Color32::from_rgb(0x5E, 0xD6, 0x9A);
const AVATARS: [Color32; 6] = [
    Color32::from_rgb(0x7C, 0x6C, 0xF5),
    Color32::from_rgb(0xF5, 0x7C, 0xA4),
    Color32::from_rgb(0x3F, 0xB9, 0xC9),
    Color32::from_rgb(0xF0, 0x9A, 0x3E),
    Color32::from_rgb(0x5C, 0xB8, 0x5C),
    Color32::from_rgb(0xD9, 0x6C, 0x4F),
];
const RADIUS: f32 = 12.0;
const STALE_DAYS: i64 = 14;

/// Texte d'aide des champs: plus pale que la saisie (override_text_color imposerait sinon la meme couleur).
fn hint(s: &str) -> RichText {
    RichText::new(s).color(DIM.lerp_to_gamma(MUTED, 0.55))
}

fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

// ---------- dates (stdlib + GetLocalTime, pas de crate calendrier) ----------

#[derive(Clone, Copy, PartialEq, Debug)]
struct Date(i32, u32, u32);

impl Date {
    fn today() -> Date {
        use windows_sys::Win32::{Foundation::SYSTEMTIME, System::SystemInformation::GetLocalTime};
        let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
        unsafe { GetLocalTime(&mut st) };
        Date(st.wYear as i32, st.wMonth as u32, st.wDay as u32)
    }

    /// Jours depuis 1970-01-01 (algorithme de Howard Hinnant).
    fn days(self) -> i64 {
        let Date(y, m, d) = self;
        let y = (if m <= 2 { y - 1 } else { y }) as i64;
        let era = (if y >= 0 { y } else { y - 399 }) / 400;
        let yoe = y - era * 400;
        let doy = (153 * ((m as i64 + 9) % 12) + 2) / 5 + d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    /// Accepte AAAA-MM-JJ, JJ/MM/AAAA, JJ/MM/AA, JJ/MM (annee courante).
    fn parse(s: &str, today: Date) -> Option<Date> {
        let p: Option<Vec<u32>> = s.split(['/', '-', '.']).map(|x| x.trim().parse().ok()).collect();
        let (y, m, d) = match p?[..] {
            [y, m, d] if y > 31 => (y as i32, m, d),
            [d, m, y] => ((if y < 100 { 2000 + y } else { y }) as i32, m, d),
            [d, m] => (today.0, m, d),
            _ => return None,
        };
        ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some(Date(y, m, d))
    }

    fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.0, self.1, self.2)
    }

    fn fr(self) -> String {
        format!("{:02}/{:02}/{}", self.2, self.1, self.0)
    }

    fn from_days(days: i64) -> Date {
        // Inverse de days() (Hinnant, civil_from_days).
        let z = days + 719468;
        let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let y = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32;
        Date(y, m, d)
    }

    /// 0 = lundi ... 6 = dimanche.
    fn weekday(self) -> u32 {
        ((self.days() + 3).rem_euclid(7)) as u32
    }

    fn plus(self, n: i64) -> Date {
        Date::from_days(self.days() + n)
    }
}

const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

const CELL: f32 = 36.0;

/// Bouton rond discret (fleches du calendrier).
fn nav_button(ui: &mut egui::Ui, glyph: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(r.center(), 13.0, CARD_HOVER);
    }
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(18.0),
        if resp.hovered() { TEXT } else { MUTED },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Chip borde (raccourcis de date).
fn chip(ui: &mut egui::Ui, label: &str) -> bool {
    let g = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.0), TEXT);
    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(20.0, 10.0), Sense::click());
    let h = resp.hovered();
    ui.painter().rect(
        r,
        11.0,
        if h { ACCENT.linear_multiply(0.18) } else { CARD },
        Stroke::new(1.0_f32, if h { ACCENT } else { BORDER }),
        StrokeKind::Inside,
    );
    ui.painter().galley(r.center() - g.size() / 2.0, g, if h { TEXT } else { MUTED });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Mini calendrier: un mois, clic sur un jour. `view` = (annee, mois) affiche.
fn calendar(ui: &mut egui::Ui, view: &mut (i32, u32), selected: Option<Date>, today: Date) -> Option<Date> {
    let mut picked = None;
    let width = 7.0 * CELL;
    ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
    ui.allocate_ui_with_layout(vec2(width, 30.0), Layout::left_to_right(Align::Center), |ui| {
        if nav_button(ui, "‹") {
            *view = if view.1 == 1 { (view.0 - 1, 12) } else { (view.0, view.1 - 1) };
        }
        ui.allocate_ui_with_layout(
            vec2(width - 56.0, 28.0),
            Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                let month = MONTHS[view.1 as usize - 1];
                let cap = month[..1].to_uppercase() + &month[1..];
                ui.label(
                    RichText::new(format!("{cap} {}", view.0))
                        .font(FontId::new(14.0, semibold()))
                        .color(TEXT),
                );
            },
        );
        if nav_button(ui, "›") {
            *view = if view.1 == 12 { (view.0 + 1, 1) } else { (view.0, view.1 + 1) };
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for d in ["LUN", "MAR", "MER", "JEU", "VEN", "SAM", "DIM"] {
            let (r, _) = ui.allocate_exact_size(vec2(CELL, 16.0), Sense::hover());
            ui.painter()
                .text(r.center(), Align2::CENTER_CENTER, d, FontId::new(9.5, semibold()), MUTED);
        }
    });
    ui.add_space(2.0);
    let first = Date(view.0, view.1, 1);
    let next = if view.1 == 12 {
        Date(view.0 + 1, 1, 1)
    } else {
        Date(view.0, view.1 + 1, 1)
    };
    let n_days = (next.days() - first.days()) as u32;
    let offset = first.weekday();
    let mut cell = 0;
    while cell < offset + n_days {
        ui.horizontal(|ui| {
            for _ in 0..7 {
                let (r, resp) = ui.allocate_exact_size(vec2(CELL, 32.0), Sense::click());
                if cell >= offset && cell < offset + n_days {
                    let d = Date(view.0, view.1, cell - offset + 1);
                    let is_sel = selected == Some(d);
                    let is_today = d == today;
                    let past = d.days() < today.days();
                    let hovered = resp.hovered();
                    if is_sel {
                        ui.painter().circle_filled(r.center(), 14.0, ACCENT);
                    } else if hovered {
                        ui.painter().circle_filled(r.center(), 14.0, CARD_HOVER);
                    }
                    if is_today && !is_sel {
                        ui.painter().circle_stroke(r.center(), 14.0, Stroke::new(1.5_f32, ACCENT));
                    }
                    let col = if is_sel {
                        BG
                    } else if is_today {
                        ACCENT
                    } else if past {
                        DIM
                    } else if d.weekday() >= 5 {
                        MUTED
                    } else {
                        TEXT
                    };
                    let font = if is_sel || is_today {
                        FontId::new(13.0, semibold())
                    } else {
                        FontId::proportional(13.0)
                    };
                    ui.painter()
                        .text(r.center(), Align2::CENTER_CENTER, (cell - offset + 1).to_string(), font, col);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        picked = Some(d);
                    }
                }
                cell += 1;
            }
        });
    }
    ui.add_space(10.0);
    ui.allocate_ui_with_layout(vec2(width, 30.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let to_friday = (4 + 7 - today.weekday() as i64) % 7;
        let friday = today.plus(if to_friday == 0 { 7 } else { to_friday });
        for (label, d) in [
            ("Demain", today.plus(1)),
            ("Vendredi", friday),
            ("+1 sem.", today.plus(7)),
            ("+1 mois", today.plus(30)),
        ] {
            if chip(ui, label) {
                picked = Some(d);
            }
        }
    });
    picked
}

/// Icone calendrier peinte (pas d'emoji).
fn calendar_icon(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    let r = Rect::from_center_size(c, vec2(14.0, 13.0));
    p.rect_stroke(r, 2.5, Stroke::new(1.4_f32, color), StrokeKind::Inside);
    p.rect_filled(
        Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + 4.0)),
        CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        color,
    );
    for (x, y) in [(-3.5, 1.0), (0.0, 1.0), (3.5, 1.0), (-3.5, 4.0), (0.0, 4.0)] {
        p.circle_filled(c + vec2(x, y), 0.9, color);
    }
}

// ---------- stockage: un JSON, backup quotidien ----------

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Task {
    title: String,
    from: String,
    added: String,
    deadline: String,
    archived: String,
    notes: String,
    waiting: String, // "en attente de <qui>" ; vide = actif
    tags: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    active: Vec<Task>,
    done: Vec<Task>,
}

const FILE: &str = "tasks.json";

fn path(name: &str) -> PathBuf {
    let dir = PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("prio");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(name)
}

fn load() -> Store {
    if let Ok(s) = std::fs::read_to_string(path(FILE)) {
        return serde_json::from_str(&s).unwrap_or_default();
    }
    // Migration depuis l'ancien format TSV (archived\tadded\tdeadline\tfrom\ttitle).
    let tsv = |name: &str| -> Vec<Task> {
        std::fs::read_to_string(path(name))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let f: Vec<&str> = l.splitn(5, '\t').collect();
                let g = |i: usize| f.get(i).map(|s| s.to_string()).unwrap_or_default();
                Task {
                    archived: g(0),
                    added: g(1),
                    deadline: g(2),
                    from: g(3),
                    title: g(4),
                    ..Default::default()
                }
            })
            .collect()
    };
    let store = Store {
        active: tsv("list.tsv"),
        done: tsv("done.tsv"),
    };
    if !store.active.is_empty() || !store.done.is_empty() {
        save(&store);
    }
    store
}

/// Sauvegarde quotidienne: avant la premiere ecriture du jour, copie l'etat
/// precedent dans backup/tasks.<AAAA-MM-JJ>.json. Garde les 30 derniers.
fn backup() {
    let src = path(FILE);
    if !src.exists() {
        return;
    }
    let dir = path("backup");
    let dst = dir.join(format!("tasks.{}.json", Date::today().iso()));
    if dst.exists() {
        return;
    }
    let _ = std::fs::copy(&src, &dst);
    let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    old.sort(); // noms ISO => ordre chronologique
    for p in old.iter().rev().skip(30) {
        let _ = std::fs::remove_file(p);
    }
}

fn save(store: &Store) {
    backup();
    if let Ok(s) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(path(FILE), s);
    }
}

fn urls(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_whitespace() || c == '<' || c == '>' || c == '"')
        .filter(|w| w.starts_with("http://") || w.starts_with("https://"))
        .collect()
}

/// Ordre d'affichage: actives d'abord, "en attente" en bas (tri stable).
fn sort_waiting(items: &mut [Task]) {
    items.sort_by_key(|t| !t.waiting.is_empty());
}

// ---------- app ----------

fn main() -> eframe::Result {
    // Sans argument: process resident. La fenetre racine est un pixel transparent, traversant,
    // qui ne sert qu'a recevoir les raccourcis globaux ; liste et ajout sont des viewports enfants
    // crees a la demande et detruits a la fermeture (une fenetre cachee ne recoit plus de rendu).
    // `add` / `list`: fenetre unique, one-shot (raccourci classique, script).
    let arg = std::env::args().nth(1).unwrap_or_default();
    let cal = std::env::args().nth(2).as_deref() == Some("cal");
    let (title, size) = match arg.as_str() {
        "add" => ("Nouvelle priorité", [ADD_SIZE[0], ADD_SIZE[1] + if cal { CAL_HEIGHT } else { 0.0 }]),
        "list" => ("Prio", LIST_SIZE),
        _ => ("prio-resident", [1.0, 1.0]),
    };
    let resident = arg.is_empty();

    // Instance unique: les raccourcis sont pris AVANT de creer la moindre fenetre. S'ils sont
    // deja pris, un resident tourne: on lui demande d'afficher la liste et on s'arrete la.
    let hotkey: Arc<std::sync::atomic::AtomicUsize> = Default::default();
    let ctx_cell: Arc<std::sync::OnceLock<egui::Context>> = Default::default();
    if resident {
        let (tx, rx) = std::sync::mpsc::channel();
        let (cell, flag) = (ctx_cell.clone(), hotkey.clone());
        std::thread::spawn(move || hotkey_loop(tx, cell, flag));
        if !rx.recv().unwrap_or(false) {
            if wake_resident() {
                return Ok(());
            }
            return main_with("Prio", LIST_SIZE, false, hotkey, ctx_cell);
        }
    }
    main_with(title, size, resident, hotkey, ctx_cell)
}

fn main_with(
    title: &'static str,
    size: [f32; 2],
    resident: bool,
    hotkey: Arc<std::sync::atomic::AtomicUsize>,
    ctx_cell: Arc<std::sync::OnceLock<egui::Context>>,
) -> eframe::Result {
    let mut viewport = window_builder(title, size).with_resizable(title == "Prio");
    if resident {
        viewport = viewport
            .with_position(pos2(0.0, 0.0))
            .with_taskbar(false)
            .with_mouse_passthrough(true);
    }
    // wgpu (DX12) plutot que glow: le pilote OpenGL Intel plante (0xC0000005) a la fermeture d'un viewport enfant.
    let opts = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "prio",
        opts,
        Box::new(move |cc| {
            style(&cc.egui_ctx);
            let _ = ctx_cell.set(cc.egui_ctx.clone());
            dbg_log(&format!("app cree: {title}"));
            Ok(match title {
                "Nouvelle priorité" => Box::new(Add::new()) as Box<dyn eframe::App>,
                "Prio" => Box::new(List::new(None)),
                _ => {
                    let r = Resident::new(hotkey);
                    // test sans clavier: PRIO_TEST_HOTKEY=1|2 simule Ctrl+Alt+A / Ctrl+Alt+P au demarrage
                    if let Some(n) = std::env::var("PRIO_TEST_HOTKEY").ok().and_then(|v| v.parse().ok()) {
                        r.hotkey.store(n, std::sync::atomic::Ordering::SeqCst);
                    }
                    Box::new(r)
                }
            })
        }),
    )
}

/// Fenetre sans decoration, transparente (coins arrondis peints), toujours au-dessus.
fn window_builder(title: &str, size: [f32; 2]) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(size)
        .with_always_on_top()
        .with_decorations(false)
        .with_transparent(true)
}

/// Trace de debug (variable PRIO_DEBUG=1) dans %APPDATA%\priority\debug.log.
fn dbg_log(msg: &str) {
    if std::env::var_os("PRIO_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path("debug.log")) {
        let _ = writeln!(f, "{:?} {msg}", std::time::SystemTime::now());
    }
}

/// Taille physique de l'ecran principal.
fn screen_px() -> (f32, f32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    unsafe { (GetSystemMetrics(SM_CXSCREEN) as f32, GetSystemMetrics(SM_CYSCREEN) as f32) }
}

/// Position (en points egui) pour centrer une fenetre de `size` points, un peu au-dessus du milieu.
fn centered(size: [f32; 2], ppp: f32) -> egui::Pos2 {
    let (w, h) = screen_px();
    pos2((w / ppp - size[0]) / 2.0, (h / ppp - size[1]) / 3.0)
}

const LIST_SIZE: [f32; 2] = [520.0, 640.0];
const HK_ADD: usize = 1;
const HK_LIST: usize = 2;
const TID_FILE: &str = "resident.tid";

/// Demande au resident existant d'afficher la liste: message poste a son thread de raccourcis,
/// dont l'id est publie dans %APPDATA%\priority\resident.tid. false si aucun resident joignable.
fn wake_resident() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_HOTKEY};
    let Some(tid) = std::fs::read_to_string(path(TID_FILE))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    else {
        return false;
    };
    unsafe { PostThreadMessageW(tid, WM_HOTKEY, HK_LIST, 0) != 0 }
}

const HK_QUIT: usize = 3;

/// Icone de zone de notification, 32x32, dessinee (pas d'asset): tuile bleue, trois barres.
fn tray_icon_rgba() -> Vec<u8> {
    let (w, h) = (32usize, 32usize);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // tuile arrondie (rayon 7)
            let (cx, cy) = (fx.clamp(7.0, 25.0), fy.clamp(7.0, 25.0));
            let inside = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() <= 7.0;
            if !inside {
                continue;
            }
            let bar = |y0: usize, len: usize| y >= y0 && y < y0 + 4 && x >= 8 && x < 8 + len;
            let (r, g, b) = if bar(8, 16) || bar(14, 12) || bar(20, 8) {
                (0x0F, 0x11, 0x18)
            } else {
                (0x8A, 0xB4, 0xFF)
            };
            let i = (y * w + x) * 4;
            px[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    px
}

/// Thread dedie: RegisterHotKey lie les raccourcis au thread appelant, et winit
/// n'expose pas WM_HOTKEY. Heberge aussi l'icone de zone de notification (meme boucle
/// de messages). Reveille l'UI via request_repaint quand un evenement tombe.
fn hotkey_loop(tx: std::sync::mpsc::Sender<bool>, ctx: Arc<std::sync::OnceLock<egui::Context>>, flag: Arc<std::sync::atomic::AtomicUsize>) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey};
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage, WM_HOTKEY};
    let ok = unsafe {
        RegisterHotKey(
            std::ptr::null_mut(),
            HK_ADD as i32,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            'A' as u32,
        ) != 0
            && RegisterHotKey(
                std::ptr::null_mut(),
                HK_LIST as i32,
                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                'P' as u32,
            ) != 0
    };
    dbg_log(&format!("RegisterHotKey ok={ok}"));
    let _ = tx.send(ok);
    if !ok {
        return;
    }
    let _ = std::fs::write(path(TID_FILE), unsafe { GetCurrentThreadId() }.to_string());

    let menu = Menu::new();
    let m_list = MenuItem::new("Prio	Ctrl+Alt+P", true, None);
    let m_add = MenuItem::new("Ajouter	Ctrl+Alt+A", true, None);
    let m_quit = MenuItem::new("Quitter", true, None);
    let _ = menu.append_items(&[&m_list, &m_add, &PredefinedMenuItem::separator(), &m_quit]);
    let icon = tray_icon::Icon::from_rgba(tray_icon_rgba(), 32, 32).ok();
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Prio  ·  Ctrl+Alt+P")
        .with_menu_on_left_click(false);
    if let Some(i) = icon {
        builder = builder.with_icon(i);
    }
    let _tray = builder.build(); // garde l'icone vivante jusqu'a la fin du thread

    let fire = |n: usize| {
        flag.store(n, std::sync::atomic::Ordering::SeqCst);
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

// ---------- resident: pixel racine + viewports enfants ----------

struct Resident {
    hotkey: Arc<std::sync::atomic::AtomicUsize>,
    list: Arc<std::sync::Mutex<List>>,
    list_open: Arc<std::sync::atomic::AtomicBool>,
    list_pos: Arc<std::sync::Mutex<Option<egui::Pos2>>>, // derniere position, restauree a la reouverture
    add: Arc<std::sync::Mutex<Add>>,
    add_open: Arc<std::sync::atomic::AtomicBool>,
    quit: Arc<std::sync::atomic::AtomicBool>,
}

impl Resident {
    fn new(hotkey: Arc<std::sync::atomic::AtomicUsize>) -> Resident {
        let quit: Arc<std::sync::atomic::AtomicBool> = Default::default();
        Resident {
            hotkey,
            list: Arc::new(std::sync::Mutex::new(List::new(Some(quit.clone())))),
            list_open: Default::default(),
            list_pos: Default::default(),
            add: Arc::new(std::sync::Mutex::new(Add::default())),
            add_open: Default::default(),
            quit,
        }
    }
}

impl eframe::App for Resident {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        use std::sync::atomic::Ordering::SeqCst;
        let hk = self.hotkey.swap(0, SeqCst);
        if self.quit.load(SeqCst) || hk == HK_QUIT {
            std::process::exit(0);
        }
        if hk != 0 {
            dbg_log(&format!(
                "resident: hotkey {hk} list_open={} add_open={}",
                self.list_open.load(SeqCst),
                self.add_open.load(SeqCst)
            ));
        }
        match hk {
            HK_LIST => {
                if self.list_open.load(SeqCst) {
                    ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of("list"), ViewportCommand::Focus);
                } else {
                    self.list_open.store(true, SeqCst);
                }
            }
            HK_ADD => {
                // une seule fenetre d'ajout a la fois: deja ouverte => on la ramene devant
                if self.add_open.load(SeqCst) {
                    ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of("add"), ViewportCommand::Focus);
                } else {
                    *self.add.lock().unwrap() = Add::new();
                    self.add_open.store(true, SeqCst);
                }
            }
            _ => {}
        }
        // La racine (1 px, traversante) n'a rien a dessiner.
        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |_| {});

        if self.list_open.load(SeqCst) {
            let (list, open, pos) = (self.list.clone(), self.list_open.clone(), self.list_pos.clone());
            let at = pos.lock().unwrap().unwrap_or_else(|| centered(LIST_SIZE, ctx.pixels_per_point()));
            let builder = window_builder("Prio", LIST_SIZE).with_position(at).with_resizable(true);
            ctx.show_viewport_deferred(egui::ViewportId::from_hash_of("list"), builder, move |ctx, _| {
                list.lock().unwrap().ui(ctx);
                if ctx.input(|i| i.viewport().close_requested()) {
                    *pos.lock().unwrap() = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min);
                    open.store(false, SeqCst);
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            });
        }
        if self.add_open.load(SeqCst) {
            let (add, open) = (self.add.clone(), self.add_open.clone());
            let builder = window_builder("Nouvelle priorité", ADD_SIZE)
                .with_position(centered(ADD_SIZE, ctx.pixels_per_point()))
                .with_resizable(false);
            ctx.show_viewport_deferred(egui::ViewportId::from_hash_of("add"), builder, move |ctx, _| {
                add.lock().unwrap().ui(ctx);
                if ctx.input(|i| i.viewport().close_requested()) {
                    open.store(false, SeqCst);
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            });
        }
    }
}

fn style(ctx: &egui::Context) {
    // Polices systeme Windows: Segoe UI (regular + semibold), zero asset embarque.
    let mut fonts = egui::FontDefinitions::default();
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Ok(b) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(egui::FontData::from_owned(b)));
        prop.insert(0, "segoe".into());
    }
    let mut sb = prop.clone();
    if let Ok(b) = std::fs::read(r"C:\Windows\Fonts\seguisb.ttf") {
        fonts.font_data.insert("segoe-sb".into(), Arc::new(egui::FontData::from_owned(b)));
        sb.insert(0, "segoe-sb".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(semibold(), sb);
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = BG;
    v.extreme_bg_color = FIELD;
    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = ACCENT.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.5_f32, ACCENT);
    v.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(8);
        w.bg_fill = FIELD;
        w.weak_bg_fill = FIELD;
        w.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.expansion = 0.0;
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT.linear_multiply(0.5));
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.popup_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(10.0, 6.0);
        s.interaction.tooltip_delay = 0.3;
    });
}

/// Fond arrondi + bordure de la fenetre sans decoration, et gestion Echap.
fn window_chrome(ctx: &egui::Context, ui: &egui::Ui) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && ctx.memory(|m| m.focused().is_none()) {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    let r = ui.max_rect();
    ui.painter().rect(r, RADIUS, BG, Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
}

/// Barre de titre maison: zone de drag + bouton fermer. Retourne le rect libre entre les deux.
fn title_bar(ui: &mut egui::Ui, height: f32) -> Rect {
    let bar = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), height));
    let drag = ui.interact(bar, Id::new("titlebar"), Sense::drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    let close = Rect::from_center_size(pos2(bar.right() - 22.0, bar.center().y), vec2(28.0, 28.0));
    let c = ui.interact(close, Id::new("close"), Sense::click());
    if c.hovered() {
        ui.painter().circle_filled(close.center(), 13.0, RED.linear_multiply(0.25));
    }
    let col = if c.hovered() { TEXT } else { MUTED };
    let (cc, k) = (close.center(), 4.5);
    ui.painter()
        .line_segment([cc + vec2(-k, -k), cc + vec2(k, k)], Stroke::new(1.5_f32, col));
    ui.painter()
        .line_segment([cc + vec2(-k, k), cc + vec2(k, -k)], Stroke::new(1.5_f32, col));
    if c.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
    Rect::from_min_max(bar.min, pos2(close.left() - 8.0, bar.max.y))
}

/// Poignee de redimensionnement en bas a droite.
fn resize_grip(ui: &mut egui::Ui) {
    let r = ui.max_rect();
    let grip = Rect::from_min_max(pos2(r.right() - 18.0, r.bottom() - 18.0), r.max);
    let g = ui
        .interact(grip, Id::new("grip"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
    if g.drag_started() {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::BeginResize(egui::ResizeDirection::SouthEast));
    }
    let c = grip.right_bottom() - vec2(7.0, 7.0);
    for k in 0..3 {
        let p = c - vec2(k as f32 * 4.0, 0.0);
        let q = c - vec2(0.0, k as f32 * 4.0);
        ui.painter().line_segment([p, q], Stroke::new(1.0_f32, DIM));
    }
}

fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let (rect, _) = ui.allocate_exact_size(g.size() + vec2(18.0, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, 10.0, color.linear_multiply(0.14));
    ui.painter().galley(rect.center() - g.size() / 2.0, g, color);
}

fn avatar(ui: &mut egui::Ui, name: &str) {
    let idx = name.bytes().map(usize::from).sum::<usize>() % AVATARS.len();
    let words: Vec<&str> = name.split_whitespace().collect();
    let initials: String = match words[..] {
        [one] => one.chars().take(2).collect(),
        _ => words.iter().take(2).filter_map(|w| w.chars().next()).collect(),
    };
    let initials = initials.to_uppercase();
    let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 9.0, AVATARS[idx]);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        initials,
        FontId::new(8.5, semibold()),
        Color32::WHITE,
    );
}

/// Bouton rond "fait" / "restaurer". Retourne true si clique.
fn round_button(ui: &mut egui::Ui, id: Id, glyph: &str, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let t = ui.ctx().animate_bool(id, resp.hovered());
    let fill = Color32::TRANSPARENT.lerp_to_gamma(ACCENT, t);
    let stroke = DIM.lerp_to_gamma(ACCENT, t);
    ui.painter().circle(rect.center(), 11.0, fill, Stroke::new(1.5_f32, stroke));
    let fg = MUTED.lerp_to_gamma(BG, t);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(12.0), fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
}

/// Petit bouton texte sans cadre (chevron, actions secondaires).
fn icon_button(ui: &mut egui::Ui, glyph: &str, size: f32, color: Color32, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let c = if resp.hovered() { TEXT } else { color };
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(size), c);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked()
}

fn text_button(ui: &mut egui::Ui, text: &str, color: Color32) -> bool {
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(13.0), color);
    let (rect, resp) = ui.allocate_exact_size(g.size() + vec2(20.0, 10.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8.0, color.linear_multiply(0.18));
    }
    ui.painter().galley(rect.center() - g.size() / 2.0, g, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

// ---------- fenetre d'ajout ----------

#[derive(Default)]
struct Add {
    title: String,
    from: String,
    deadline: String,
    notes: String,
    tags: String,
    tags_all: Vec<(String, usize)>,
    focused: bool,
    notes_focused: bool,
    cal_view: Option<(i32, u32)>,
    names: Vec<String>, // demandeurs deja saisis, du plus frequent au moins frequent
}

const ADD_SIZE: [f32; 2] = [560.0, 304.0];
const CAL_HEIGHT: f32 = 330.0;

/// Personnes deja saisies (demandeurs et "en attente de"), de la plus frequente a la moins frequente.
fn names(store: &Store) -> Vec<String> {
    let mut count: std::collections::HashMap<&str, usize> = Default::default();
    for t in store.active.iter().chain(&store.done) {
        for n in [t.from.trim(), t.waiting.trim()].into_iter().filter(|n| !n.is_empty()) {
            *count.entry(n).or_default() += 1;
        }
    }
    let mut v: Vec<(&str, usize)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v.into_iter().map(|(n, _)| n.to_string()).collect()
}

/// Rangee de chips: noms filtres par ce qui est tape. Retourne le nom choisi.
fn name_chips(ui: &mut egui::Ui, names: &[String], typed: &str) -> Option<String> {
    let typed = typed.trim().to_lowercase();
    let sugg: Vec<&String> = names
        .iter()
        .filter(|n| typed.is_empty() || (n.to_lowercase().contains(&typed) && n.to_lowercase() != typed))
        .take(6)
        .collect();
    let mut pick = None;
    if !sugg.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for n in sugg {
                if chip(ui, n) {
                    pick = Some(n.clone());
                }
            }
        });
    }
    pick
}

/// Suffixe qui completerait `text` (ou son dernier segment apres virgule si `multi`)
/// avec le premier candidat qui commence pareil (insensible a la casse).
fn completion(text: &str, candidates: &[String], multi: bool) -> Option<String> {
    let seg = if multi { text.rsplit(',').next().unwrap_or("") } else { text }.trim_start();
    if seg.is_empty() {
        return None;
    }
    let low = seg.to_lowercase();
    candidates
        .iter()
        .find(|c| c.to_lowercase().starts_with(&low) && c.to_lowercase() != low)
        .map(|c| c.chars().skip(seg.chars().count()).collect())
}

/// Champ texte avec completion fantome: le suffixe propose s'affiche en grise apres la saisie,
/// Tab l'accepte. Sans proposition, Tab garde son role (champ suivant).
fn complete_field(ui: &mut egui::Ui, text: &mut String, hint_s: &str, width: f32, candidates: &[String], multi: bool) -> egui::Response {
    let id = Id::new("complete").with(hint_s);
    // Tab n'est intercepte (et insere '\t') que si le tour precedent avait une proposition.
    let had = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let out = TextEdit::singleline(text)
        .hint_text(hint(hint_s))
        .margin(vec2(10.0, 5.0))
        .desired_width(width)
        .lock_focus(had)
        .show(ui);
    let tab = text.contains('\t');
    if tab {
        text.retain(|c| c != '\t');
    }
    let sugg = completion(text, candidates, multi);
    if tab && let Some(suf) = &sugg {
        text.push_str(suf);
    }
    let sugg = if tab { None } else { sugg };
    if out.response.has_focus()
        && let Some(suf) = &sugg
    {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let at = out.galley_pos + vec2(out.galley.size().x, 0.0);
        ui.painter()
            .text(at, Align2::LEFT_TOP, format!("{suf}   Tab"), font, DIM.lerp_to_gamma(MUTED, 0.5));
    }
    ui.ctx().data_mut(|d| d.insert_temp(id, sugg.is_some() && out.response.has_focus()));
    out.response
}

/// "rh, Support N2, #mira" -> ["rh", "Support N2", "mira"], sans doublon (insensible a la casse).
fn parse_tags(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in s
        .split([',', ';'])
        .map(|t| t.trim().trim_start_matches('#').trim())
        .filter(|t| !t.is_empty())
    {
        if !out.iter().any(|o| o.eq_ignore_ascii_case(t)) {
            out.push(t.to_string());
        }
    }
    out
}

/// Ajoute le tag au champ texte s'il n'y est pas, le retire sinon.
fn toggle_tag(text: &mut String, tag: &str) {
    let mut tags = parse_tags(text);
    match tags.iter().position(|t| t.eq_ignore_ascii_case(tag)) {
        Some(i) => {
            tags.remove(i);
        }
        None => tags.push(tag.to_string()),
    }
    *text = tags.join(", ");
}

/// Tous les tags du store, du plus utilise au moins utilise, avec leur nombre d'occurrences.
fn tags_all(store: &Store) -> Vec<(String, usize)> {
    let mut count: Vec<(String, usize)> = Vec::new();
    for t in store.active.iter().chain(&store.done).flat_map(|t| &t.tags) {
        match count.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(t)) {
            Some(c) => c.1 += 1,
            None => count.push((t.clone(), 1)),
        }
    }
    count.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    count
}

fn tag_color(tag: &str) -> Color32 {
    AVATARS[tag.to_lowercase().bytes().map(usize::from).sum::<usize>() % AVATARS.len()]
}

/// Petite pastille de tag. Retourne true si cliquee.
fn tag_pill(ui: &mut egui::Ui, tag: &str, selected: bool) -> bool {
    let c = tag_color(tag);
    let fg = if selected { BG } else { c };
    let g = ui.painter().layout_no_wrap(tag.to_string(), FontId::new(10.5, semibold()), fg);
    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(12.0, 5.0), Sense::click());
    let fill = if selected {
        c
    } else {
        c.linear_multiply(if resp.hovered() { 0.32 } else { 0.18 })
    };
    ui.painter().rect_filled(r, 6.0, fill);
    ui.painter().galley(r.center() - g.size() / 2.0, g, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Rangee des tags connus, ceux presents dans `text` en surbrillance ; clic = bascule dans `text`.
fn tag_chips(ui: &mut egui::Ui, all: &[(String, usize)], text: &mut String) -> bool {
    if all.is_empty() {
        return false;
    }
    let current = parse_tags(text);
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for (tag, _) in all {
            let on = current.iter().any(|c| c.eq_ignore_ascii_case(tag));
            if tag_pill(ui, tag, on) {
                toggle_tag(text, tag);
                changed = true;
            }
        }
    });
    changed
}

impl Add {
    fn new() -> Add {
        // `priority add cal` ouvre directement le calendrier (captures d'ecran / tests visuels).
        let cal_view = (std::env::args().nth(2).as_deref() == Some("cal")).then(|| (Date::today().0, Date::today().1));
        let store = load();
        Add {
            names: names(&store),
            tags_all: tags_all(&store),
            cal_view,
            ..Default::default()
        }
    }
}

impl eframe::App for Add {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl Add {
    /// Fenetre d'ajout. Sert au one-shot (viewport racine) et au resident (viewport enfant).
    fn ui(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        let today = Date::today();
        let deadline = Date::parse(&self.deadline, today);
        let bad_deadline = !self.deadline.trim().is_empty() && deadline.is_none();

        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            window_chrome(ctx, ui);
            let bar = title_bar(ui, 30.0);
            ui.painter().text(
                bar.left_center() + vec2(16.0, 0.0),
                Align2::LEFT_CENTER,
                "NOUVELLE PRIORITÉ",
                FontId::new(10.5, semibold()),
                MUTED,
            );

            let inner = ui.max_rect().shrink2(vec2(14.0, 0.0));
            let mut ui =
                ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(inner.left(), bar.bottom() + 2.0), inner.max)));
            let r = ui.add(
                TextEdit::singleline(&mut self.title)
                    .hint_text(hint("Ex. : relire la doc partenaire avant vendredi"))
                    .hint_text_font(FontId::proportional(17.0))
                    .font(FontId::new(17.0, semibold()))
                    .margin(vec2(12.0, 8.0))
                    .desired_width(f32::INFINITY),
            );
            if !self.focused {
                r.request_focus();
                self.focused = true;
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                complete_field(ui, &mut self.from, "Demandé par", 190.0, &self.names, false);
                let mut e = TextEdit::singleline(&mut self.deadline)
                    .hint_text(hint("Échéance  JJ/MM/AAAA"))
                    .margin(vec2(10.0, 5.0))
                    .desired_width(150.0);
                if bad_deadline {
                    e = e.text_color(RED);
                }
                ui.add(e);
                let (rect, cal) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
                let cal = cal.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Calendrier");
                ui.painter().rect_filled(rect, 8.0, if cal.hovered() { CARD_HOVER } else { FIELD });
                ui.painter().rect_stroke(
                    rect,
                    8.0,
                    Stroke::new(1.0_f32, if cal.hovered() { ACCENT } else { BORDER }),
                    StrokeKind::Inside,
                );
                calendar_icon(
                    ui.painter(),
                    rect.center(),
                    if cal.hovered() || self.cal_view.is_some() { ACCENT } else { MUTED },
                );
                if cal.clicked() {
                    // Calendrier inline: la fenetre grandit le temps de la saisie.
                    self.cal_view = if self.cal_view.is_some() {
                        None
                    } else {
                        deadline.or(Some(today)).map(|d| (d.0, d.1))
                    };
                    let h = if self.cal_view.is_some() {
                        ADD_SIZE[1] + CAL_HEIGHT
                    } else {
                        ADD_SIZE[1]
                    };
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(ADD_SIZE[0], h)));
                }
            });
            if let Some(n) = name_chips(&mut ui, &self.names, &self.from) {
                self.from = n;
            }
            ui.add_space(2.0);
            let tag_names: Vec<String> = self.tags_all.iter().map(|(t, _)| t.clone()).collect();
            complete_field(
                &mut ui,
                &mut self.tags,
                "Tags, séparés par des virgules",
                f32::INFINITY,
                &tag_names,
                true,
            );
            tag_chips(&mut ui, &self.tags_all, &mut self.tags);
            if let Some(view) = &mut self.cal_view {
                ui.add_space(4.0);
                let picked = Frame::new()
                    .fill(FIELD)
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::symmetric(10, 10))
                    .show(&mut ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| calendar(ui, view, deadline, today)).inner
                    })
                    .inner;
                if let Some(d) = picked {
                    self.deadline = d.fr();
                    self.cal_view = None;
                    ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(ADD_SIZE[0], ADD_SIZE[1])));
                }
                ui.add_space(4.0);
            }
            ui.add_space(2.0);
            let n = ui.add(
                TextEdit::multiline(&mut self.notes)
                    .hint_text(hint("Notes, liens, contexte"))
                    .desired_rows(3)
                    .margin(vec2(10.0, 6.0))
                    .desired_width(f32::INFINITY),
            );
            self.notes_focused = n.has_focus();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new("Entrée ✔ valider   ·   Ctrl+Entrée depuis les notes   ·   Échap annuler")
                        .size(12.0)
                        .color(MUTED),
                );
            });
        });

        // Entree valide partout, sauf dans les notes ou elle fait un retour a la ligne (Ctrl+Entree valide).
        let (enter, ctrl) = ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.command));
        if enter && (ctrl || !self.notes_focused) && !self.title.trim().is_empty() && !bad_deadline {
            let mut store = load();
            store.active.push(Task {
                title: self.title.trim().to_string(),
                from: self.from.trim().to_string(),
                added: today.iso(),
                deadline: deadline.map(Date::iso).unwrap_or_default(),
                notes: self.notes.trim().to_string(),
                tags: parse_tags(&self.tags),
                ..Default::default()
            });
            save(&store);
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }
}

// ---------- fenetre liste ----------

struct List {
    store: Store,
    names: Vec<String>,
    tags_all: Vec<(String, usize)>,
    filter: Option<String>, // tag selectionne en tete de liste
    show_done: bool,
    focused: bool,
    open: Option<(bool, usize)>, // carte depliee: (archives ?, index)
    copied_at: Option<f64>,
    quit: Option<Arc<std::sync::atomic::AtomicBool>>, // Some en mode resident: bouton "Quitter"
}

impl List {
    fn new(quit: Option<Arc<std::sync::atomic::AtomicBool>>) -> List {
        let mut store = load();
        sort_waiting(&mut store.active);
        let names = names(&store);
        let tags_all = tags_all(&store);
        List {
            store,
            names,
            tags_all,
            filter: None,
            show_done: false,
            focused: false,
            open: None,
            copied_at: None,
            quit,
        }
    }
}

impl eframe::App for List {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl List {
    /// Fenetre liste. Sert au one-shot (viewport racine) et au resident (viewport enfant).
    fn ui(&mut self, ctx: &egui::Context) {
        let today = Date::today();

        // Une autre instance (Ctrl+Alt+A) a pu ecrire pendant qu'on etait ouvert.
        let focused = ctx.input(|i| i.focused);
        if focused && !self.focused {
            self.store = load();
            sort_waiting(&mut self.store.active);
            self.names = names(&self.store);
            self.tags_all = tags_all(&self.store);
        }
        self.focused = focused;

        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            window_chrome(ctx, ui);
            let bar = title_bar(ui, 56.0);
            let (title, n) = if self.show_done {
                ("Archives", self.store.done.len())
            } else {
                ("Prio", self.store.active.len())
            };
            let p = ui.painter();
            let g = p.layout_no_wrap(title.into(), FontId::new(22.0, semibold()), TEXT);
            let at = bar.left_center() + vec2(18.0, 0.0);
            p.galley(at - vec2(0.0, g.size().y / 2.0), g.clone(), TEXT);
            p.text(
                at + vec2(g.size().x + 8.0, 0.0),
                Align2::LEFT_CENTER,
                n.to_string(),
                FontId::new(22.0, semibold()),
                MUTED,
            );

            // bascule en cours / archives, dans la barre de titre
            let other = if self.show_done {
                format!("En cours  {}", self.store.active.len())
            } else {
                format!("Archives  {}", self.store.done.len())
            };
            let og = p.layout_no_wrap(other, FontId::proportional(13.0), ACCENT);
            let orect = Rect::from_center_size(
                pos2(bar.right() - og.size().x / 2.0 - 8.0, bar.center().y),
                og.size() + vec2(20.0, 10.0),
            );
            let o = ui
                .interact(orect, Id::new("toggle"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            ui.painter().rect_filled(
                orect,
                8.0,
                if o.hovered() {
                    ACCENT.linear_multiply(0.18)
                } else {
                    Color32::TRANSPARENT
                },
            );
            ui.painter().galley(orect.center() - og.size() / 2.0, og, ACCENT);
            if o.clicked() {
                self.show_done = !self.show_done;
                self.open = None;
            }

            // pied: rappel des gestes
            let foot = pos2(ui.max_rect().left() + 16.0, ui.max_rect().bottom() - 14.0);
            let hint = if self.show_done {
                "clic = notes  ·  ↩ remet en cours  ·  Échap ferme"
            } else {
                "Ctrl+Alt+A ajouter  ·  glisser le numéro pour réordonner  ·  clic = notes  ·  Échap ferme"
            };
            ui.painter().text(
                foot,
                Align2::LEFT_CENTER,
                hint,
                FontId::proportional(11.0),
                DIM.lerp_to_gamma(MUTED, 0.5),
            );
            if let Some(quit) = &self.quit {
                // seul moyen propre d'arreter le process resident (pas d'icone de zone de notification)
                let q = Rect::from_center_size(pos2(ui.max_rect().right() - 46.0, foot.y), vec2(48.0, 20.0));
                let r = ui
                    .interact(q, Id::new("quit"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Arrêter Prio (les raccourcis Ctrl+Alt+A / P ne répondront plus)");
                ui.painter().text(
                    q.center(),
                    Align2::CENTER_CENTER,
                    "Quitter",
                    FontId::proportional(11.0),
                    if r.hovered() { RED } else { DIM.lerp_to_gamma(MUTED, 0.5) },
                );
                if r.clicked() {
                    quit.store(true, std::sync::atomic::Ordering::SeqCst);
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            }

            // filtre par tag: une rangee de pastilles sous le titre, le tag actif en plein
            let mut top = bar.bottom() + 4.0;
            if !self.tags_all.is_empty() {
                let row = Rect::from_min_max(
                    pos2(ui.max_rect().left() + 16.0, top),
                    pos2(ui.max_rect().right() - 16.0, top + 26.0),
                );
                let mut fui = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(Layout::left_to_right(Align::Center)));
                fui.spacing_mut().item_spacing.x = 4.0;
                for (tag, n) in self.tags_all.clone() {
                    let on = self.filter.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&tag));
                    if tag_pill(&mut fui, &format!("{tag}  {n}"), on) {
                        self.filter = if on { None } else { Some(tag.clone()) };
                        self.open = None;
                    }
                }
                top += 32.0;
            }
            let body = Rect::from_min_max(pos2(ui.max_rect().left() + 14.0, top), ui.max_rect().max - vec2(14.0, 30.0));
            let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(body));
            if self.show_done {
                self.archive_view(&mut ui, today);
            } else {
                self.active_view(&mut ui, today);
            }
        });
        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, resize_grip);
    }
}

fn empty_state(ui: &mut egui::Ui, msg: &str) {
    ui.add_space(ui.available_height() * 0.35);
    ui.vertical_centered(|ui| ui.label(RichText::new(msg).size(14.0).color(DIM)));
}

struct CardOut {
    action: bool,  // archiver / restaurer
    toggle: bool,  // plier / deplier
    changed: bool, // notes ou "en attente" modifiees
    rect: Rect,    // emprise de la carte (cible du drag & drop)
}

impl Default for CardOut {
    fn default() -> Self {
        CardOut {
            action: false,
            toggle: false,
            changed: false,
            rect: Rect::NOTHING,
        }
    }
}

impl List {
    fn active_view(&mut self, ui: &mut egui::Ui, today: Date) {
        if self.store.active.is_empty() {
            empty_state(ui, "Rien en attente.  Ctrl+Alt+A pour ajouter.");
        }
        let mut out = Vec::new();
        let mut rows = Vec::new();
        let open = self.open;
        let names = self.names.clone();
        let tags_all = self.tags_all.clone();
        let filter = self.filter.clone();
        let keep = |t: &Task| filter.as_deref().is_none_or(|f| t.tags.iter().any(|x| x.eq_ignore_ascii_case(f)));
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let mut waiting_header = false;
            for (i, t) in self.store.active.iter_mut().enumerate().filter(|(_, t)| keep(t)) {
                if !t.waiting.is_empty() && !waiting_header {
                    waiting_header = true;
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("EN ATTENTE").font(FontId::new(10.5, semibold())).color(DIM));
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                        ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, BORDER));
                    });
                    ui.add_space(4.0);
                }
                let o = card(ui, i, Some(i + 1), t, today, open == Some((false, i)), &names, &tags_all);
                rows.push(o.rect);
                if o.action || o.toggle || o.changed {
                    out.push((i, o));
                }
                ui.add_space(8.0);
            }
        });

        // Drag & drop: la cible d'insertion est deduite de la position du
        // pointeur par rapport au milieu de chaque carte.
        // ponytail: pas de reordonnancement sous filtre (les positions affichees ne sont pas les index reels).
        let ctx = ui.ctx().clone();
        if filter.is_some() {
            egui::DragAndDrop::clear_payload(&ctx);
        }
        if let (Some(from), Some(pos)) = (egui::DragAndDrop::payload::<usize>(&ctx).map(|p| *p), ctx.pointer_interact_pos()) {
            {
                let to = rows.iter().position(|r| pos.y < r.center().y).unwrap_or(rows.len());
                if let (Some(first), Some(last)) = (rows.first(), rows.last()) {
                    let y = rows.get(to).map(|r| r.top() - 4.0).unwrap_or(last.bottom() + 4.0);
                    ui.painter().hline(first.x_range(), y, Stroke::new(2.0_f32, ACCENT));
                    ui.painter().circle_filled(pos2(first.left(), y), 4.0, ACCENT);
                }
                if ui.input(|i| i.pointer.any_released()) {
                    egui::DragAndDrop::clear_payload(&ctx);
                    reorder(&mut self.store.active, from, to);
                    // ponytail: le tri actives/en attente est reapplique, un drag qui
                    // traverse la frontiere est annule silencieusement.
                    sort_waiting(&mut self.store.active);
                    self.open = None;
                    save(&self.store);
                }
            }
        }

        for (i, o) in out.into_iter().rev() {
            if o.toggle {
                self.open = if self.open == Some((false, i)) { None } else { Some((false, i)) };
            }
            if o.changed {
                save(&self.store);
            }
            if o.action {
                let mut t = self.store.active.remove(i);
                t.archived = today.iso();
                self.store.done.insert(0, t);
                self.open = None;
                save(&self.store);
            }
        }
        // Une tache passee "en attente" (ou l'inverse) change de section quand on quitte le champ.
        if self.open.is_none()
            && self
                .store
                .active
                .windows(2)
                .any(|w| !w[0].waiting.is_empty() && w[1].waiting.is_empty())
        {
            sort_waiting(&mut self.store.active);
            save(&self.store);
        }
    }

    fn archive_view(&mut self, ui: &mut egui::Ui, today: Date) {
        let week = |t: &Task| Date::parse(&t.archived, today).is_some_and(|d| today.days() - d.days() < 7);
        let n_week = self.store.done.iter().filter(|t| week(t)).count();
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let now = ui.input(|i| i.time);
                let just_copied = self.copied_at.is_some_and(|t| now - t < 2.0);
                let label = if just_copied {
                    "Copié ✔".to_string()
                } else {
                    format!("Copier la semaine ({n_week})")
                };
                if text_button(ui, &label, if just_copied { GREEN } else { ACCENT }) && n_week > 0 {
                    let md: Vec<String> = self
                        .store
                        .done
                        .iter()
                        .filter(|t| week(t))
                        .map(|t| {
                            let mut s = format!("- [x] {}", t.title);
                            for tag in &t.tags {
                                s += &format!(" #{}", tag.replace(' ', "_"));
                            }
                            if !t.from.is_empty() {
                                s += &format!(" (de {})", t.from);
                            }
                            if let Some(d) = Date::parse(&t.archived, today) {
                                s += &format!(" - fait le {}", d.fr());
                            }
                            s
                        })
                        .collect();
                    ui.ctx().copy_text(md.join("\n"));
                    self.copied_at = Some(now);
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(2));
                }
            });
        });
        if self.store.done.is_empty() {
            empty_state(ui, "Rien d'archivé pour l'instant.");
        }
        let mut out = Vec::new();
        let open = self.open;
        let names = self.names.clone();
        let tags_all = self.tags_all.clone();
        let filter = self.filter.clone();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (i, t) in self
                .store
                .done
                .iter_mut()
                .enumerate()
                .filter(|(_, t)| filter.as_deref().is_none_or(|f| t.tags.iter().any(|x| x.eq_ignore_ascii_case(f))))
            {
                let o = card(ui, i, None, t, today, open == Some((true, i)), &names, &tags_all);
                if o.action || o.toggle || o.changed {
                    out.push((i, o));
                }
                ui.add_space(8.0);
            }
        });
        for (i, o) in out.into_iter().rev() {
            if o.toggle {
                self.open = if self.open == Some((true, i)) { None } else { Some((true, i)) };
            }
            if o.changed {
                save(&self.store);
            }
            if o.action {
                let mut t = self.store.done.remove(i);
                t.archived.clear();
                self.store.active.push(t);
                sort_waiting(&mut self.store.active);
                self.open = None;
                save(&self.store);
            }
        }
    }
}

/// Une carte, pliee ou depliee (notes + en attente de).
#[allow(clippy::too_many_arguments)] // ponytail: un struct de contexte le jour ou un 9e parametre arrive
fn card(
    ui: &mut egui::Ui,
    idx: usize,
    rank: Option<usize>,
    t: &mut Task,
    today: Date,
    open: bool,
    names: &[String],
    tags_all: &[(String, usize)],
) -> CardOut {
    let mut out = CardOut::default();
    let archived = Date::parse(&t.archived, today);
    let deadline = Date::parse(&t.deadline, today);
    let days = deadline.map(|d| d.days() - today.days());
    let waiting = archived.is_none() && !t.waiting.is_empty();
    let age = Date::parse(&t.added, today).map(|d| today.days() - d.days()).unwrap_or(0);
    let stale = archived.is_none() && !waiting && deadline.is_none() && age >= STALE_DAYS;
    let urgency = match (archived, days) {
        (Some(_), _) => Some(GREEN),
        _ if waiting => Some(DIM),
        (None, Some(n)) if n <= 0 => Some(RED),
        (None, Some(n)) if n <= 3 => Some(AMBER),
        (None, Some(_)) => Some(ACCENT),
        (None, None) if stale => Some(AMBER.linear_multiply(0.6)),
        (None, None) => None,
    };
    let id = Id::new("card").with(idx).with(archived.is_some());
    let was_hovered = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let t_hover = ui.ctx().animate_bool(id.with("anim"), was_hovered);
    let dimmed = archived.is_some() || waiting || stale;

    // Pastille de droite (echeance ou stagnation), calculee d'avance pour borner la largeur du titre.
    let pill_txt: Option<(String, Color32)> = match (deadline, days) {
        (Some(d), Some(n)) => Some(match (archived.is_some(), n) {
            (true, _) => (format!("échéance {}", d.fr()), MUTED),
            _ if waiting => (format!("échéance {}", d.fr()), MUTED),
            (_, n) if n < 0 => (format!("retard {} j · {}", -n, d.fr()), RED),
            (_, 0) => ("aujourd'hui".to_string(), RED),
            (_, 1) => ("demain".to_string(), AMBER),
            (_, n) => (format!("J-{n} · {}", d.fr()), if n <= 3 { AMBER } else { ACCENT }),
        }),
        _ if stale => Some((
            if age >= 21 {
                format!("stagne depuis {} sem.", age / 7)
            } else {
                format!("stagne depuis {age} j")
            },
            AMBER,
        )),
        _ => None,
    };
    let pill_w = pill_txt
        .as_ref()
        .map(|(s, _)| ui.painter().layout_no_wrap(s.clone(), FontId::proportional(12.0), MUTED).size().x + 18.0 + 8.0)
        .unwrap_or(0.0);
    let right_w = 26.0 + 8.0 + 22.0 + 8.0 + pill_w; // bouton rond + chevron + pastille

    // Clic sur le corps de la carte = deplier. Enregistre AVANT le contenu (avec
    // l'emprise du tour precedent) pour que boutons et champs gardent la priorite.
    if !open
        && let Some(prev) = ui.ctx().data(|d| d.get_temp::<Rect>(id.with("rect")))
        && ui.interact(prev, id.with("body"), Sense::click()).clicked()
    {
        out.toggle = true;
    }

    let mut f = Frame::new()
        .fill(CARD.lerp_to_gamma(CARD_HOVER, t_hover))
        .stroke(Stroke::new(1.0_f32, BORDER.lerp_to_gamma(ACCENT.linear_multiply(0.6), t_hover)))
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .begin(ui);
    {
        let ui = &mut f.content_ui;
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            if let Some(r) = rank {
                // Poignee de drag: points + numero de rang. Seule zone qui declenche le deplacement.
                ui.dnd_drag_source(Id::new("prio").with(idx), idx, |ui| {
                    let (dots, _) = ui.allocate_exact_size(vec2(10.0, 34.0), Sense::hover());
                    for row in 0..3 {
                        for col in 0..2 {
                            let p = dots.center() + vec2(col as f32 * 4.0 - 2.0, row as f32 * 5.0 - 5.0);
                            ui.painter().circle_filled(p, 1.3, DIM);
                        }
                    }
                    ui.allocate_ui_with_layout(vec2(28.0, 34.0), Layout::left_to_right(Align::Center), |ui| {
                        let c = if dimmed { MUTED } else { ACCENT };
                        ui.label(RichText::new(format!("{r:02}")).font(FontId::new(19.0, semibold())).color(c));
                    });
                });
            }
            let left_w = ui.available_width() - right_w;
            ui.vertical(|ui| {
                ui.set_max_width(left_w);
                ui.spacing_mut().item_spacing.y = 5.0;
                let color = if dimmed { MUTED } else { TEXT };
                ui.label(RichText::new(&t.title).font(FontId::new(15.0, semibold())).color(color));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let meta = |s: String| RichText::new(s).size(12.0).color(MUTED);
                    for tag in &t.tags {
                        tag_pill(ui, tag, false);
                    }
                    if !t.from.is_empty() {
                        avatar(ui, &t.from);
                        ui.label(meta(t.from.clone()));
                        ui.label(meta("·".into()));
                    }
                    if let Some(d) = Date::parse(&t.added, today) {
                        ui.label(meta(format!("ajouté le {}", d.fr())));
                    }
                    if waiting {
                        ui.label(meta("·".into()));
                        ui.label(RichText::new(format!("⏳ attend {}", t.waiting)).size(12.0).color(AMBER));
                    }
                    if let Some(d) = archived {
                        ui.label(meta("·".into()));
                        ui.label(RichText::new(format!("archivé le {}", d.fr())).size(12.0).color(GREEN));
                    }
                    if !t.notes.trim().is_empty() && !open {
                        ui.label(meta("·".into()));
                        ui.label(meta("≡ notes".into()));
                    }
                });
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (glyph, tip) = if archived.is_some() {
                    ("↩", "Remettre en cours")
                } else {
                    ("✔", "Terminé, archiver")
                };
                if round_button(ui, id.with("btn"), glyph, tip) {
                    out.action = true;
                }
                if icon_button(
                    ui,
                    if open { "▲" } else { "▼" },
                    11.0,
                    MUTED,
                    if open { "Replier" } else { "Notes, liens, en attente de…" },
                ) {
                    out.toggle = true;
                }
                if let Some((txt, col)) = &pill_txt {
                    pill(ui, txt, *col);
                }
            });
        });

        if open {
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, BORDER));
            ui.add_space(6.0);
            let notes = ui.add(
                TextEdit::multiline(&mut t.notes)
                    .hint_text(hint("Notes, liens, contexte"))
                    .desired_rows(3)
                    .margin(vec2(10.0, 8.0))
                    .desired_width(f32::INFINITY),
            );
            let links: Vec<String> = urls(&t.notes).into_iter().map(String::from).collect();
            if !links.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for u in &links {
                        let short: String = u
                            .trim_start_matches("https://")
                            .trim_start_matches("http://")
                            .chars()
                            .take(48)
                            .collect();
                        let short = if short.len() < u.len() - 8 { format!("{short}…") } else { short };
                        ui.hyperlink_to(RichText::new(format!("↗ {short}")).size(12.5), u);
                    }
                });
            }
            // tags: champ texte + pastilles a bascule (memes tags que dans l'ajout)
            let mut tags_text = t.tags.join(", ");
            ui.horizontal(|ui| {
                ui.label(RichText::new("tags").size(12.5).color(MUTED));
                let tag_names: Vec<String> = tags_all.iter().map(|(t, _)| t.clone()).collect();
                if complete_field(ui, &mut tags_text, "rh, support n2…", 260.0, &tag_names, true).changed() {
                    t.tags = parse_tags(&tags_text);
                    out.changed = true;
                }
            });
            if tag_chips(ui, tags_all, &mut tags_text) {
                t.tags = parse_tags(&tags_text);
                out.changed = true;
            }
            let mut wait = None;
            if archived.is_none() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("⏳ en attente de").size(12.5).color(MUTED));
                    wait = Some(complete_field(ui, &mut t.waiting, "nom", 180.0, names, false));
                    if !t.waiting.is_empty() && text_button(ui, "Débloqué", GREEN) {
                        t.waiting.clear();
                        out.changed = true;
                    }
                });
                // memes personnes que les demandeurs: chips de suggestion
                if let Some(n) = name_chips(ui, names, &t.waiting) {
                    t.waiting = n;
                    out.changed = true;
                }
            }
            if notes.changed() || wait.as_ref().is_some_and(|w| w.changed()) {
                out.changed = true;
            }
        }
    }
    let rect = f.content_ui.min_rect().expand2(vec2(14.0, 12.0));
    let hovered = ui.rect_contains_pointer(rect);
    ui.ctx().data_mut(|d| {
        d.insert_temp(id, hovered);
        d.insert_temp(id.with("rect"), rect);
    });
    out.rect = rect;
    f.end(ui);
    if let Some(c) = urgency {
        let bar = Rect::from_min_max(pos2(rect.left(), rect.top() + 10.0), pos2(rect.left() + 3.0, rect.bottom() - 10.0));
        ui.painter().rect_filled(bar, 2.0, c);
    }
    out
}

fn reorder<T>(items: &mut Vec<T>, from: usize, to: usize) {
    let it = items.remove(from);
    let to = if to > from { to - 1 } else { to };
    items.insert(to.min(items.len()), it);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drag_reorder() {
        let mut v: Vec<char> = "abcd".chars().collect();
        reorder(&mut v, 0, 4);
        assert_eq!(v, vec!['b', 'c', 'd', 'a']);
        reorder(&mut v, 3, 0);
        assert_eq!(v, vec!['a', 'b', 'c', 'd']);
        reorder(&mut v, 2, 1);
        assert_eq!(v, vec!['a', 'c', 'b', 'd']);
        reorder(&mut v, 1, 1);
        assert_eq!(v, vec!['a', 'c', 'b', 'd']);
    }

    #[test]
    fn dates() {
        let today = Date(2026, 9, 9);
        assert_eq!(Date(1970, 1, 1).days(), 0);
        assert_eq!(Date(2000, 3, 1).days() - Date(2000, 2, 28).days(), 2); // bissextile
        assert_eq!(Date::parse("15/09", today), Some(Date(2026, 9, 15)));
        assert_eq!(Date::parse("15/09/26", today), Some(Date(2026, 9, 15)));
        assert_eq!(Date::parse("2026-09-15", today), Some(Date(2026, 9, 15)));
        assert_eq!(Date::parse("32/09", today), None);
        assert_eq!(Date::parse("lundi", today), None);
        assert_eq!(Date(2026, 9, 15).days() - today.days(), 6);
        // aller-retour days <-> date, jour de semaine (2026-09-09 est un mercredi)
        for d in [
            Date(1970, 1, 1),
            Date(2000, 2, 29),
            Date(2026, 9, 9),
            Date(2026, 12, 31),
            Date(2027, 1, 1),
        ] {
            assert_eq!(Date::from_days(d.days()), d);
        }
        assert_eq!(today.weekday(), 2);
        assert_eq!(Date(2026, 9, 14).weekday(), 0);
        assert_eq!(today.plus(30), Date(2026, 10, 9));
        assert_eq!(Date::parse(&Date(2026, 9, 15).fr(), today), Some(Date(2026, 9, 15)));
    }

    #[test]
    fn completion_suffix() {
        let names: Vec<String> = ["Anne Sophie", "BK", "Client"].iter().map(|s| s.to_string()).collect();
        assert_eq!(completion("an", &names, false), Some("ne Sophie".into()));
        assert_eq!(completion("BK", &names, false), None);
        assert_eq!(completion("", &names, false), None);
        assert_eq!(completion("rh, cl", &names, true), Some("ient".into()));
        assert_eq!(completion("rh,", &names, true), None);
    }

    #[test]
    fn tags() {
        assert_eq!(parse_tags(" rh, #Support N2 ;mira,, RH "), vec!["rh", "Support N2", "mira"]);
        let mut s = "rh, mira".to_string();
        toggle_tag(&mut s, "MIRA");
        assert_eq!(s, "rh");
        toggle_tag(&mut s, "connecteurs");
        assert_eq!(s, "rh, connecteurs");
    }

    #[test]
    fn links_and_waiting() {
        assert_eq!(
            urls("voir https://docs.example.com/x et <http://a.b> fin"),
            vec!["https://docs.example.com/x", "http://a.b"]
        );
        let mk = |w: &str| Task {
            waiting: w.into(),
            ..Default::default()
        };
        let mut v = vec![mk("Alice"), mk(""), mk("Bob"), mk("")];
        sort_waiting(&mut v);
        assert_eq!(
            v.iter().map(|t| t.waiting.as_str()).collect::<Vec<_>>(),
            vec!["", "", "Alice", "Bob"]
        );
        let s: Store = serde_json::from_str(r#"{"active":[{"title":"x"}]}"#).unwrap();
        assert_eq!(s.active[0].title, "x");
        assert!(s.active[0].notes.is_empty());
    }
}
