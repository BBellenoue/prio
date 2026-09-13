#![cfg_attr(windows, windows_subsystem = "windows")]

mod platform;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Id, Layout, Rect, RichText, Sense, Stroke, StrokeKind, TextEdit,
    ViewportCommand, pos2, vec2,
};
use platform::{HK_ADD, HK_ADD_FAILED, HK_LIST, HK_LIST_FAILED, HK_QUIT};
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

/// Field hint text: paler than the input (override_text_color would otherwise force the same colour).
fn hint(s: &str) -> RichText {
    RichText::new(s).color(DIM.lerp_to_gamma(MUTED, 0.55))
}

fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

// ---------- dates (stdlib plus the system clock, no calendar crate) ----------

#[derive(Clone, Copy, PartialEq, Debug)]
struct Date(i32, u32, u32);

impl Date {
    fn today() -> Date {
        let (y, m, d) = platform::today();
        Date(y, m, d)
    }

    /// Days since 1970-01-01 (Howard Hinnant's algorithm).
    fn days(self) -> i64 {
        let Date(y, m, d) = self;
        let y = (if m <= 2 { y - 1 } else { y }) as i64;
        let era = (if y >= 0 { y } else { y - 399 }) / 400;
        let yoe = y - era * 400;
        let doy = (153 * ((m as i64 + 9) % 12) + 2) / 5 + d as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    /// Accepts YYYY-MM-DD, DD/MM/YYYY, DD/MM/YY, DD/MM (current year).
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
        // Inverse of days() (Hinnant, civil_from_days).
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

    /// 0 = Monday ... 6 = Sunday.
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

/// Quiet round button (the calendar arrows).
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

/// Outlined chip (the date shortcuts).
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

/// Small calendar: one month, click a day. `view` = the (year, month) on show.
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

/// Calendar icon, painted (no emoji).
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

// ---------- storage: one JSON file, a daily backup ----------

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Task {
    title: String,
    from: String,
    added: String,
    deadline: String,
    archived: String,
    notes: String,
    waiting: String, // "waiting on <who>"; empty = active
    tags: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Store {
    active: Vec<Task>,
    done: Vec<Task>,
}

const FILE: &str = "tasks.json";
const SETTINGS_FILE: &str = "settings.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    hotkey_add: String,
    hotkey_list: String,
    toggle_close: bool, // the shortcut closes the window again when it is already open
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey_add: platform::DEFAULT_HOTKEY_ADD.into(),
            hotkey_list: platform::DEFAULT_HOTKEY_LIST.into(),
            toggle_close: true,
        }
    }
}

fn load_settings() -> Settings {
    std::fs::read_to_string(path(SETTINGS_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(s: &Settings) {
    if let Ok(j) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path(SETTINGS_FILE), j);
    }
}

fn path(name: &str) -> PathBuf {
    let dir = platform::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join(name)
}

fn load() -> Store {
    if let Ok(s) = std::fs::read_to_string(path(FILE)) {
        return serde_json::from_str(&s).unwrap_or_default();
    }
    // Migration from the old TSV format (archived\tadded\tdeadline\tfrom\ttitle).
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

/// Daily backup: before the first write of the day, copy the previous state to
/// backup/tasks.<YYYY-MM-DD>.json. The last 30 are kept.
fn backup() {
    let src = path(FILE);
    if !src.exists() {
        return;
    }
    let dir = path("backup");
    let _ = std::fs::create_dir_all(&dir);
    let dst = dir.join(format!("tasks.{}.json", Date::today().iso()));
    if dst.exists() {
        return;
    }
    let _ = std::fs::copy(&src, &dst);
    let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    old.sort(); // ISO names, so sorting is chronological
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

/// Display order: active first, "waiting" at the bottom (stable sort).
fn sort_waiting(items: &mut [Task]) {
    items.sort_by_key(|t| !t.waiting.is_empty());
}

// ---------- app ----------

fn main() -> eframe::Result {
    // No argument: the resident process. The root window is one transparent pixel, click
    // through, whose only job is to receive the global shortcuts; the list and the capture
    // window are child viewports created on demand and destroyed on close (a hidden window
    // is no longer rendered). `add` / `list`: a single one-shot window, for a plain shortcut
    // or a script.
    let arg = std::env::args().nth(1).unwrap_or_default();
    let cal = std::env::args().nth(2).as_deref() == Some("cal");
    let (title, size) = match arg.as_str() {
        "add" => ("Nouvelle priorité", [ADD_SIZE[0], ADD_SIZE[1] + if cal { CAL_HEIGHT } else { 0.0 }]),
        "list" => ("Prio", LIST_SIZE),
        _ => ("prio-resident", [1.0, 1.0]),
    };
    let resident = arg.is_empty();

    // Single instance: the resident's place is taken BEFORE any window exists. If it is
    // already taken, a resident is running: ask it to show the list and stop there.
    let hotkey: Arc<std::sync::atomic::AtomicUsize> = Default::default();
    let hk_status: Arc<std::sync::atomic::AtomicUsize> = Default::default();
    let ctx_cell: Arc<std::sync::OnceLock<egui::Context>> = Default::default();
    if resident && !platform::start_resident(ctx_cell.clone(), hotkey.clone(), hk_status.clone()) {
        if platform::wake_resident() {
            return Ok(());
        }
        return main_with("Prio", LIST_SIZE, false, hotkey, hk_status, ctx_cell);
    }
    main_with(title, size, resident, hotkey, hk_status, ctx_cell)
}

fn main_with(
    title: &'static str,
    size: [f32; 2],
    resident: bool,
    hotkey: Arc<std::sync::atomic::AtomicUsize>,
    hk_status: Arc<std::sync::atomic::AtomicUsize>,
    ctx_cell: Arc<std::sync::OnceLock<egui::Context>>,
) -> eframe::Result {
    let mut viewport = window_builder(title, size).with_resizable(title == "Prio");
    if resident {
        viewport = viewport
            .with_position(pos2(0.0, 0.0))
            .with_taskbar(false)
            .with_mouse_passthrough(true);
    }
    // wgpu rather than glow: on Windows the Intel OpenGL driver crashes (0xC0000005) when a
    // child viewport is closed.
    let mut opts = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    platform::configure(&mut opts);
    eframe::run_native(
        "prio",
        opts,
        Box::new(move |cc| {
            style(&cc.egui_ctx);
            let _ = ctx_cell.set(cc.egui_ctx.clone());
            let cell = ctx_cell.clone();
            dbg_log(&format!("app created: {title}"));
            Ok(match title {
                "Nouvelle priorité" => Box::new(Add::new()) as Box<dyn eframe::App>,
                "Prio" => Box::new(List::new(None, None)),
                _ => {
                    let r = Resident::new(cell, hotkey, hk_status);
                    // testing without a keyboard: PRIO_TEST_HOTKEY=1|2 fires either shortcut at startup
                    if let Some(n) = std::env::var("PRIO_TEST_HOTKEY").ok().and_then(|v| v.parse().ok()) {
                        r.hotkey.store(n, std::sync::atomic::Ordering::SeqCst);
                    }
                    Box::new(r)
                }
            })
        }),
    )
}

/// Undecorated window, transparent (the rounded corners are painted), always on top.
fn window_builder(title: &str, size: [f32; 2]) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(size)
        .with_always_on_top()
        .with_decorations(false)
        .with_transparent(true)
        .with_icon(Arc::new(egui::IconData {
            rgba: logo_rgba(64),
            width: 64,
            height: 64,
        }))
}

/// Debug trace (PRIO_DEBUG=1) in debug.log, next to tasks.json.
fn dbg_log(msg: &str) {
    if std::env::var_os("PRIO_DEBUG").is_none() {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path("debug.log")) {
        let _ = writeln!(f, "{:?} {msg}", std::time::SystemTime::now());
    }
}

/// Position (in egui points) centring a window of `size` points, a little above the middle.
fn centered(ctx: &egui::Context, size: [f32; 2]) -> egui::Pos2 {
    let (w, h) = platform::screen_points(ctx);
    pos2((w - size[0]) / 2.0, (h - size[1]) / 3.0)
}

const LIST_SIZE: [f32; 2] = [520.0, 640.0];
/// The logo, drawn (no asset): a rounded blue tile, three decreasing priority bars. Same
/// proportions as docs/logo.svg (a 128 grid). Used for the notification area or menu bar
/// icon (32 px) and for the window icon (64 px).
fn logo_rgba(size: usize) -> Vec<u8> {
    let k = size as f32 / 128.0;
    let mut px = vec![0u8; size * size * 4];
    // (x, y, width, height, radius) in the 128 grid
    let tile = (8.0, 8.0, 112.0, 112.0, 28.0);
    let bars = [
        (32.0, 36.0, 64.0, 14.0, 7.0),
        (32.0, 57.0, 48.0, 14.0, 7.0),
        (32.0, 78.0, 32.0, 14.0, 7.0),
    ];
    let inside = |fx: f32, fy: f32, r: (f32, f32, f32, f32, f32)| {
        let (x, y, w, h, rad) = (r.0 * k, r.1 * k, r.2 * k, r.3 * k, r.4 * k);
        let (cx, cy) = (fx.clamp(x + rad, x + w - rad), fy.clamp(y + rad, y + h - rad));
        ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() <= rad
    };
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            if !inside(fx, fy, tile) {
                continue;
            }
            let (r, g, b) = if bars.iter().any(|&b| inside(fx, fy, b)) {
                (0x0F, 0x11, 0x18)
            } else {
                (0x8A, 0xB4, 0xFF)
            };
            let i = (y * size + x) * 4;
            px[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    px
}

// ---------- resident: the root pixel and its child viewports ----------

struct Resident {
    rt: platform::Runtime,
    hotkey: Arc<std::sync::atomic::AtomicUsize>,
    list: Arc<std::sync::Mutex<List>>,
    list_open: Arc<std::sync::atomic::AtomicBool>,
    list_pos: Arc<std::sync::Mutex<Option<egui::Pos2>>>, // last position, restored when it reopens
    add: Arc<std::sync::Mutex<Add>>,
    add_open: Arc<std::sync::atomic::AtomicBool>,
    quit: Arc<std::sync::atomic::AtomicBool>,
}

impl Resident {
    fn new(
        ctx: Arc<std::sync::OnceLock<egui::Context>>,
        hotkey: Arc<std::sync::atomic::AtomicUsize>,
        hk_status: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Resident {
        let quit: Arc<std::sync::atomic::AtomicBool> = Default::default();
        Resident {
            rt: platform::Runtime::start(ctx, hotkey.clone(), hk_status.clone()),
            hotkey,
            list: Arc::new(std::sync::Mutex::new(List::new(Some(quit.clone()), Some(hk_status)))),
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
        self.rt.tick();
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
                    // already open: the shortcut closes it (a setting) or brings it to the front
                    let cmd = if load_settings().toggle_close {
                        ViewportCommand::Close
                    } else {
                        ViewportCommand::Focus
                    };
                    ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of("list"), cmd);
                } else {
                    self.list_open.store(true, SeqCst);
                }
            }
            HK_ADD => {
                // one capture window at a time: already open, so close it (a setting) or bring it to the front
                if self.add_open.load(SeqCst) {
                    let cmd = if load_settings().toggle_close {
                        ViewportCommand::Close
                    } else {
                        ViewportCommand::Focus
                    };
                    ctx.send_viewport_cmd_to(egui::ViewportId::from_hash_of("add"), cmd);
                } else {
                    *self.add.lock().unwrap() = Add::new();
                    self.add_open.store(true, SeqCst);
                }
            }
            _ => {}
        }
        // The root (1 px, click through) has nothing to draw.
        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |_| {});

        if self.list_open.load(SeqCst) {
            let (list, open, pos) = (self.list.clone(), self.list_open.clone(), self.list_pos.clone());
            let at = pos.lock().unwrap().unwrap_or_else(|| centered(ctx, LIST_SIZE));
            let builder = window_builder("Prio", LIST_SIZE).with_position(at).with_resizable(true);
            ctx.show_viewport_deferred(egui::ViewportId::from_hash_of("list"), builder, move |ctx, _| {
                platform::activate(ctx);
                list.lock().unwrap().ui(ctx);
                if ctx.input(|i| i.viewport().close_requested()) {
                    list.lock().unwrap().cancel_capture();
                    *pos.lock().unwrap() = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min);
                    open.store(false, SeqCst);
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
            });
        }
        if self.add_open.load(SeqCst) {
            let (add, open) = (self.add.clone(), self.add_open.clone());
            let builder = window_builder("Nouvelle priorité", ADD_SIZE)
                .with_position(centered(ctx, ADD_SIZE))
                .with_resizable(false);
            ctx.show_viewport_deferred(egui::ViewportId::from_hash_of("add"), builder, move |ctx, _| {
                platform::activate(ctx);
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
    // System fonts, no asset shipped: the first candidate that reads wins.
    let mut fonts = egui::FontDefinitions::default();
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut load = |name: &str, candidates: &[(&str, u32)], family: &mut Vec<String>| {
        for (file, index) in candidates {
            if let Ok(bytes) = std::fs::read(file) {
                let data = egui::FontData {
                    font: bytes.into(),
                    index: *index,
                    tweak: Default::default(),
                };
                fonts.font_data.insert(name.into(), Arc::new(data));
                family.insert(0, name.into());
                return;
            }
        }
    };
    load("system", platform::FONT_REGULAR, &mut prop);
    let mut sb = prop.clone();
    load("system-semibold", platform::FONT_SEMIBOLD, &mut sb);
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

/// Rounded background and border of the undecorated window, and the Escape key.
fn window_chrome(ctx: &egui::Context, ui: &egui::Ui) {
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && ctx.memory(|m| m.focused().is_none()) {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    let r = ui.max_rect();
    ui.painter().rect(r, RADIUS, BG, Stroke::new(1.0_f32, BORDER), StrokeKind::Inside);
}

/// Home-made title bar: a drag area and a close button. Returns the free rect between them.
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

/// Resize grip, bottom right.
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

/// Round "done" / "restore" button. True when clicked.
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

/// Small frameless text button (chevron, secondary actions).
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

// ---------- capture window ----------

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
    names: Vec<String>, // requesters already entered, most frequent first
}

const ADD_SIZE: [f32; 2] = [560.0, 304.0];
const CAL_HEIGHT: f32 = 330.0;

/// People already entered (requesters and "waiting on"), most frequent first.
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

/// Row of chips: names filtered by what is typed. Returns the one picked.
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

/// The suffix that would complete `text` (or its last comma-separated segment when `multi`)
/// with the first candidate that starts the same way, case insensitive.
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

/// Text field with a ghost completion: the suggested suffix shows greyed after the input and
/// Tab accepts it. With nothing to suggest, Tab keeps its usual role (next field).
fn complete_field(ui: &mut egui::Ui, text: &mut String, hint_s: &str, width: f32, candidates: &[String], multi: bool) -> egui::Response {
    let id = Id::new("complete").with(hint_s);
    // Tab is held (lock_focus) only when the previous frame had something to suggest; egui
    // does not insert a tab in a single-line field, so the key is read directly.
    let had = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let out = TextEdit::singleline(text)
        .hint_text(hint(hint_s))
        .margin(vec2(10.0, 5.0))
        .desired_width(width)
        .lock_focus(had)
        .show(ui);
    let focused = out.response.has_focus();
    let mut sugg = completion(text, candidates, multi);
    if had && focused && ui.input(|i| i.key_pressed(egui::Key::Tab)) {
        if sugg.take().is_some() {
            // replace the typed segment with the candidate's canonical form ("cl" -> "Client")
            let seg_chars = if multi {
                text.rsplit(',').next().unwrap_or("")
            } else {
                text.as_str()
            }
            .trim_start()
            .chars()
            .count();
            let low = text
                .chars()
                .rev()
                .take(seg_chars)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
                .to_lowercase();
            if let Some(full) = candidates.iter().find(|c| c.to_lowercase().starts_with(&low)).cloned() {
                let keep = text.chars().count() - seg_chars;
                *text = text.chars().take(keep).collect::<String>() + &full;
            }
            if multi {
                text.push_str(", "); // ready for the next tag
            }
            // cursor at the end of the field (egui would have left it before the suffix)
            let mut state = out.state;
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(text.chars().count()))));
            state.store(ui.ctx(), out.response.id);
        }
    } else if focused && let Some(suf) = &sugg {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let at = out.galley_pos + vec2(out.galley.size().x, 0.0);
        ui.painter()
            .text(at, Align2::LEFT_TOP, format!("{suf}   Tab"), font, DIM.lerp_to_gamma(MUTED, 0.5));
    }
    // Anything read from the context (has_focus, input) has to be read BEFORE this write lock:
    // reaching the context from inside the closure deadlocks the thread (the lock is not
    // reentrant).
    let show_next = sugg.is_some() && focused;
    ui.ctx().data_mut(|d| d.insert_temp(id, show_next));
    out.response
}

/// "rh, Support N2, #mira" -> ["rh", "Support N2", "mira"], no duplicates, case insensitive.
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

/// Adds the tag to the text field if it is missing, removes it otherwise.
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

/// Every tag in the store, most used first, with how often it appears.
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

/// Small tag pill. True when clicked.
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

/// Row of known tags, the ones present in `text` highlighted; a click toggles it in `text`.
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
        // `prio add cal` opens straight on the calendar (screenshots, visual tests).
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
    /// The capture window. Serves the one-shot mode (root viewport) and the resident (child viewport).
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
                    // Inline calendar: the window grows while a date is being picked.
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
                    RichText::new(format!(
                        "Entrée ✔ valider   ·   {}+Entrée depuis les notes   ·   Échap annuler",
                        platform::CMD_LABEL
                    ))
                    .size(12.0)
                    .color(MUTED),
                );
            });
        });

        // Enter saves everywhere except in the notes, where it breaks the line (the command key saves).
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

// ---------- list window ----------

struct List {
    store: Store,
    names: Vec<String>,
    tags_all: Vec<(String, usize)>,
    filter: Option<String>, // tag picked at the top of the list
    show_done: bool,
    focused: bool,
    mtime: Option<std::time::SystemTime>, // tasks.json timestamp at the last load
    open: Option<(bool, usize)>,          // unfolded card: (archived?, index)
    copied_at: Option<f64>,
    quit: Option<Arc<std::sync::atomic::AtomicBool>>, // Some in resident mode: the "Quitter" button
    settings: Settings,
    show_settings: bool,
    capture: Option<usize>,                                 // shortcut being entered (HK_ADD / HK_LIST)
    hk_status: Option<Arc<std::sync::atomic::AtomicUsize>>, // registration failures (resident mode)
}

impl List {
    fn new(quit: Option<Arc<std::sync::atomic::AtomicBool>>, hk_status: Option<Arc<std::sync::atomic::AtomicUsize>>) -> List {
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
            mtime: std::fs::metadata(path(FILE)).and_then(|m| m.modified()).ok(),
            open: None,
            copied_at: None,
            quit,
            settings: load_settings(),
            // PRIO_TEST_SETTINGS=1 opens straight on the settings panel (screenshots)
            show_settings: std::env::var_os("PRIO_TEST_SETTINGS").is_some(),
            capture: None,
            hk_status,
        }
    }

    /// Drops a shortcut capture in progress and gives the resident its shortcuts back.
    fn cancel_capture(&mut self) {
        if self.capture.take().is_some() {
            platform::notify_hotkeys_changed();
        }
    }

    /// Settings panel: the two shortcuts, each set by pressing the combination.
    fn settings_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(RichText::new("Raccourcis clavier").font(FontId::new(15.0, semibold())).color(TEXT));
        ui.label(
            RichText::new(format!("{}, puis une lettre, un chiffre, F1 à F24 ou Espace.", platform::MODS_HINT))
                .size(12.5)
                .color(MUTED),
        );
        ui.add_space(10.0);
        let status = self
            .hk_status
            .as_ref()
            .map(|s| s.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(0);
        let rows = [
            (HK_ADD, "Ajouter une priorité", self.settings.hotkey_add.clone(), HK_ADD_FAILED),
            (HK_LIST, "Afficher la liste", self.settings.hotkey_list.clone(), HK_LIST_FAILED),
        ];
        for (which, label, value, bit) in rows {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(vec2(170.0, 30.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.label(RichText::new(label).size(14.0).color(TEXT));
                });
                let capturing = self.capture == Some(which);
                let text = if capturing {
                    "Appuie sur la combinaison…   (Échap annule)".to_string()
                } else {
                    value
                };
                if text_button(ui, &text, if capturing { AMBER } else { ACCENT }) {
                    if capturing {
                        self.capture = None;
                        platform::notify_hotkeys_changed();
                    } else {
                        self.capture = Some(which);
                        platform::suspend_hotkeys();
                    }
                }
                if status & bit != 0 {
                    ui.label(RichText::new("déjà pris par une autre application").size(12.0).color(RED));
                }
            });
        }
        ui.add_space(10.0);
        if ui
            .checkbox(
                &mut self.settings.toggle_close,
                RichText::new("Le raccourci referme la fenêtre si elle est déjà ouverte")
                    .size(14.0)
                    .color(TEXT),
            )
            .changed()
        {
            save_settings(&self.settings);
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            let d = Settings::default();
            if chip(ui, &format!("Réinitialiser ({} / {})", d.hotkey_add, d.hotkey_list)) {
                self.settings = d;
                save_settings(&self.settings);
                platform::notify_hotkeys_changed();
            }
        });
        ui.add_space(10.0);
        ui.label(
            RichText::new("Appliqué immédiatement au résident ; l'icône et le menu suivent.")
                .size(12.0)
                .color(MUTED),
        );
        ui.label(
            RichText::new(format!("Fichier : {}", path(SETTINGS_FILE).display()))
                .size(11.0)
                .color(DIM.lerp_to_gamma(MUTED, 0.5)),
        );
    }
}

/// Painted gear (the settings button).
fn gear_icon(p: &egui::Painter, c: egui::Pos2, color: Color32) {
    p.circle_stroke(c, 5.0, Stroke::new(1.6_f32, color));
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        let d = vec2(a.cos(), a.sin());
        p.line_segment([c + d * 6.2, c + d * 8.6], Stroke::new(2.0_f32, color));
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
    /// The list window. Serves the one-shot mode (root viewport) and the resident (child viewport).
    fn ui(&mut self, ctx: &egui::Context) {
        let today = Date::today();

        // The capture window (another viewport or another process) writes the file while the
        // list lives in memory: reload as soon as its timestamp changes, and again when focus
        // comes back, as a safety net should the timestamp not move.
        let focused = ctx.input(|i| i.focused);
        let mtime = std::fs::metadata(path(FILE)).and_then(|m| m.modified()).ok();
        if mtime != self.mtime || (focused && !self.focused) {
            self.store = load();
            // Our own writes come back through here (the file timestamp moves): no sorting
            // while a card is open, or its index would point at another task and the next
            // keystroke would land in it (every letter on a different ticket).
            if self.open.is_none() {
                sort_waiting(&mut self.store.active);
            }
            self.names = names(&self.store);
            self.tags_all = tags_all(&self.store);
            self.mtime = mtime;
        }
        self.focused = focused;

        // Capturing a shortcut: the first valid combination is saved, Escape cancels (and does
        // not reach window_chrome, which would close the window).
        if let Some(which) = self.capture {
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.capture = None;
                platform::notify_hotkeys_changed();
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            } else if let Some(label) = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => platform::hotkey_label(*key, *modifiers),
                    _ => None,
                })
            }) {
                if which == HK_ADD {
                    self.settings.hotkey_add = label;
                } else {
                    self.settings.hotkey_list = label;
                }
                save_settings(&self.settings);
                platform::notify_hotkeys_changed();
                self.capture = None;
            }
        }

        egui::CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
            window_chrome(ctx, ui);
            let bar = title_bar(ui, 56.0);
            let (title, n) = if self.show_settings {
                ("Réglages", 0)
            } else if self.show_done {
                ("Archives", self.store.done.len())
            } else {
                ("Prio", self.store.active.len())
            };
            let p = ui.painter();
            let g = p.layout_no_wrap(title.into(), FontId::new(22.0, semibold()), TEXT);
            let at = bar.left_center() + vec2(18.0, 0.0);
            p.galley(at - vec2(0.0, g.size().y / 2.0), g.clone(), TEXT);
            if !self.show_settings {
                p.text(
                    at + vec2(g.size().x + 8.0, 0.0),
                    Align2::LEFT_CENTER,
                    n.to_string(),
                    FontId::new(22.0, semibold()),
                    MUTED,
                );
            }

            // active / archived switch, in the title bar
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
                self.show_settings = false;
                self.cancel_capture();
                self.open = None;
            }

            // gear: settings
            let grect = Rect::from_center_size(pos2(orect.left() - 20.0, bar.center().y), vec2(28.0, 28.0));
            let gr = ui
                .interact(grect, Id::new("settings"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Réglages (raccourcis clavier)");
            if gr.hovered() || self.show_settings {
                ui.painter().circle_filled(grect.center(), 13.0, ACCENT.linear_multiply(0.18));
            }
            gear_icon(
                ui.painter(),
                grect.center(),
                if gr.hovered() || self.show_settings { ACCENT } else { MUTED },
            );
            if gr.clicked() {
                self.show_settings = !self.show_settings;
                self.cancel_capture();
            }

            // footer: a reminder of the gestures
            let foot = pos2(ui.max_rect().left() + 16.0, ui.max_rect().bottom() - 14.0);
            let hint = if self.show_done {
                "clic = notes  ·  ↩ remet en cours  ·  Échap ferme".to_string()
            } else {
                format!(
                    "{} ajouter  ·  glisser le numéro pour réordonner  ·  clic = notes  ·  Échap ferme",
                    self.settings.hotkey_add
                )
            };
            ui.painter().text(
                foot,
                Align2::LEFT_CENTER,
                hint,
                FontId::proportional(11.0),
                DIM.lerp_to_gamma(MUTED, 0.5),
            );
            if let Some(quit) = &self.quit {
                // the only clean way to stop the resident process when there is no tray icon
                let q = Rect::from_center_size(pos2(ui.max_rect().right() - 46.0, foot.y), vec2(48.0, 20.0));
                let r = ui
                    .interact(q, Id::new("quit"), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!(
                        "Arrêter Prio ({} / {} ne répondront plus)",
                        self.settings.hotkey_add, self.settings.hotkey_list
                    ));
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

            // tag filter: a row of pills under the title, the active one filled
            let mut top = bar.bottom() + 4.0;
            if !self.tags_all.is_empty() && !self.show_settings {
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
            if self.show_settings {
                self.settings_view(&mut ui);
            } else if self.show_done {
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
    action: bool,  // archive / restore
    toggle: bool,  // fold / unfold
    changed: bool, // notes or "waiting on" edited
    rect: Rect,    // the card's extent (drag and drop target)
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
            empty_state(ui, &format!("Rien en attente.  {} pour ajouter.", self.settings.hotkey_add));
        }
        let mut out = Vec::new();
        let mut rows = Vec::new();
        let open = self.open;
        let names = self.names.clone();
        let tags_all = self.tags_all.clone();
        let filter = self.filter.clone();
        let keep = |t: &Task| filter.as_deref().is_none_or(|f| t.tags.iter().any(|x| x.eq_ignore_ascii_case(f)));
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            // While dragging, a pointer near the top or bottom edge scrolls the list, or a
            // card could never travel past the edge of the screen.
            if egui::DragAndDrop::has_any_payload(ui.ctx())
                && let Some(pos) = ui.ctx().pointer_interact_pos()
            {
                let clip = ui.clip_rect();
                let edge = 48.0;
                let dy = (clip.top() + edge - pos.y).max(0.0) + (clip.bottom() - edge - pos.y).min(0.0);
                if dy != 0.0 {
                    // ponytail: speed per frame, not per second; good enough at 60 Hz.
                    ui.scroll_with_delta_animation(vec2(0.0, dy * 0.3), egui::style::ScrollAnimation::none());
                    ui.ctx().request_repaint();
                }
            }
            let mut waiting_header = false;
            for (i, t) in self.store.active.iter_mut().enumerate().filter(|(_, t)| keep(t)) {
                if !t.waiting.is_empty() && !waiting_header {
                    waiting_header = true;
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("EN ATTENTE").font(FontId::new(10.5, semibold())).color(MUTED));
                        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
                        ui.painter()
                            .hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, DIM.lerp_to_gamma(MUTED, 0.6)));
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

        // Drag and drop: the insertion target comes from where the pointer sits relative to
        // the middle of each card.
        // ponytail: no reordering under a filter (the positions on show are not the real indices).
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
                    // ponytail: the active/waiting sort is applied again, so a drag that
                    // crosses the boundary is silently undone.
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
        // A task moved to "waiting" (or back) changes section when the field loses focus.
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

/// One card, folded or unfolded (notes and waiting on).
#[allow(clippy::too_many_arguments)] // ponytail: a context struct the day a 9th parameter shows up
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

    // The right-hand pill (deadline or staleness), measured first to bound the title width.
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
    let right_w = 26.0 + 8.0 + 22.0 + 8.0 + pill_w; // round button, chevron, pill

    // A click on the body of the card unfolds it. Registered BEFORE the content (with the
    // extent from the previous frame) so buttons and fields keep priority.
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
                // Drag handle: the dots and the rank number. The only area that starts a move.
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
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend); // each item stays whole, wrapping happens between them
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
            // tags: a text field and toggle pills (the same tags as in the capture window)
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
                // the same people as the requesters: suggestion chips
                if let Some(n) = name_chips(ui, names, &t.waiting) {
                    t.waiting = n;
                    out.changed = true;
                }
            } else {
                // The archive date can be corrected (a task finished yesterday, archived today).
                // The draft lives in egui's memory while the field has focus; only valid dates,
                // in ISO, reach the store.
                let bid = id.with("arch");
                let mut buf: String = ui
                    .ctx()
                    .data(|d| d.get_temp(bid))
                    .unwrap_or_else(|| archived.map(Date::fr).unwrap_or_default());
                let parsed = Date::parse(&buf, today);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("archivé le").size(12.5).color(MUTED));
                    let mut e = TextEdit::singleline(&mut buf).margin(vec2(10.0, 5.0)).desired_width(110.0);
                    if parsed.is_none() {
                        e = e.text_color(RED);
                    }
                    let r = ui.add(e);
                    if r.changed()
                        && let Some(d) = Date::parse(&buf, today)
                    {
                        t.archived = d.iso();
                        out.changed = true;
                    }
                    if r.lost_focus() {
                        ui.ctx().data_mut(|d| d.remove_temp::<String>(bid));
                    } else if r.has_focus() {
                        ui.ctx().data_mut(|d| d.insert_temp(bid, buf.clone()));
                    }
                });
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
        assert_eq!(Date(2000, 3, 1).days() - Date(2000, 2, 28).days(), 2); // leap year
        assert_eq!(Date::parse("15/09", today), Some(Date(2026, 9, 15)));
        assert_eq!(Date::parse("15/09/26", today), Some(Date(2026, 9, 15)));
        assert_eq!(Date::parse("2026-09-15", today), Some(Date(2026, 9, 15)));
        assert_eq!(Date::parse("32/09", today), None);
        assert_eq!(Date::parse("lundi", today), None);
        assert_eq!(Date(2026, 9, 15).days() - today.days(), 6);
        // round trip days <-> date, and weekday (2026-09-09 is a Wednesday)
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
