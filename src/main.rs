// =====================================================================
//  RBX Account Manager (Rust + egui), version in Cargo.toml
//  - Account data is encrypted with Windows DPAPI: only this Windows user
//    on this PC can decrypt it.
//  - Talks ONLY to roblox.com (+ rbxcdn.com for avatar/game images).
//  - Cookies are never shown again after they are saved.
// =====================================================================
#![windows_subsystem = "windows"]

mod discord;
mod fps;
mod monitor;
mod roblox;
mod store;
#[cfg(test)]
mod tests;
mod tray;
mod win;

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Painter,
    Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use roblox::{Details, Presence, ServerInfo, UserInfo};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use store::{Account, Settings};

// ------------------------------------------------------------------ themes
/// 0 = Graphite (dark, default), 1 = Terrava (light: aubergine ink, lilac, cobalt blue),
/// 2 = Neon (isready.ai "dark terminal": near-black with a lime accent, mono labels).
static THEME: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

fn is_light() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 1
}

fn set_theme(name: &str) {
    let v = match name.to_ascii_lowercase().as_str() {
        "terrava" => 1,
        "neon" => 2,
        _ => 0,
    };
    THEME.store(v, std::sync::atomic::Ordering::Relaxed);
}

fn is_neon() -> bool {
    THEME.load(std::sync::atomic::Ordering::Relaxed) == 2
}

/// Pick the colour for the active theme (Neon uses the dark value).
#[allow(dead_code)]
fn pick(dark: u32, light: u32) -> Color32 {
    hex(if is_light() { light } else { dark })
}

/// Pick the colour for the active theme: Graphite, Terrava, Neon.
fn pick3(graphite: u32, terrava: u32, neon: u32) -> Color32 {
    hex(match THEME.load(std::sync::atomic::Ordering::Relaxed) {
        1 => terrava,
        2 => neon,
        _ => graphite,
    })
}

/// Header band: Terrava uses a bright cobalt panel like its hero sections.
#[allow(non_snake_case)]
fn HEADER_BG() -> Color32 {
    pick3(0x0F1012, 0x3A7FE3, 0x0A0A08)
}
#[allow(non_snake_case)]
fn HEADER_TEXT() -> Color32 {
    pick3(0xE8E6E1, 0xFFFFFF, 0xE8E8E5)
}
#[allow(non_snake_case)]
fn HEADER_SUB() -> Color32 {
    pick3(0x65625C, 0xD6E6FF, 0x6B6B64)
}
// ------------------------------------------------------------------ palette (neutral graphite / Terrava)
const fn hex(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}
#[allow(non_snake_case)]
fn BG() -> Color32 {
    pick3(0x0F1012, 0xF7F9FF, 0x0A0A08)
}
#[allow(non_snake_case)]
fn SURFACE() -> Color32 {
    pick3(0x16171A, 0xFFFFFF, 0x11110E)
}
#[allow(non_snake_case)]
fn CARD() -> Color32 {
    pick3(0x1C1D21, 0xF1F3FA, 0x1A1A16)
}
#[allow(non_snake_case)]
fn CARD_HOVER() -> Color32 {
    pick3(0x232428, 0xE8ECF8, 0x23231F)
}
#[allow(non_snake_case)]
fn FIELD() -> Color32 {
    pick3(0x0B0C0E, 0xFFFFFF, 0x0A0A08)
}
#[allow(non_snake_case)]
fn BORDER() -> Color32 {
    pick3(0x26272C, 0xE5E7EB, 0x2E2E2B)
}
#[allow(non_snake_case)]
fn BORDER_HI() -> Color32 {
    pick3(0x36373D, 0xCDD2DE, 0x3F3F3A)
}
#[allow(non_snake_case)]
fn TEXT() -> Color32 {
    pick3(0xE8E6E1, 0x23143F, 0xE8E8E5)
}
#[allow(non_snake_case)]
fn MUTED() -> Color32 {
    pick3(0x9B9890, 0x5E5675, 0xA3A39B)
}
#[allow(non_snake_case)]
fn FAINT() -> Color32 {
    pick3(0x65625C, 0x9891AB, 0x6B6B64)
}
#[allow(non_snake_case)]
fn PRIMARY() -> Color32 {
    pick3(0xE8E6E1, 0xD1C1FF, 0x9FE737)
}
#[allow(non_snake_case)]
fn PRIMARY_HI() -> Color32 {
    pick3(0xFFFFFF, 0x78BFF7, 0xB6F55E)
}
#[allow(non_snake_case)]
fn ON_PRIMARY() -> Color32 {
    pick3(0x111111, 0x23143F, 0x0A0A08)
}
#[allow(non_snake_case)]
fn SELECT_BG() -> Color32 {
    pick3(0x25262B, 0xF0EBFF, 0x161B0D)
}
#[allow(non_snake_case)]
fn SELECT_BORDER() -> Color32 {
    pick3(0xC9C6BE, 0x3A7FE3, 0x9FE737)
}
#[allow(non_snake_case)]
fn GREEN() -> Color32 {
    pick3(0x5DB37E, 0x1F9D82, 0x9FE737)
}
#[allow(non_snake_case)]
fn BLUE() -> Color32 {
    pick3(0x6FA3D8, 0x3A7FE3, 0x3AA6D8)
}
#[allow(non_snake_case)]
fn RED() -> Color32 {
    pick3(0xE0625A, 0xE2501F, 0xD94B4B)
}
#[allow(non_snake_case)]
fn RED_BG() -> Color32 {
    pick3(0x2A1716, 0xFFEDE6, 0x2A1414)
}
#[allow(non_snake_case)]
fn RED_BG_HI() -> Color32 {
    pick3(0x3A1E1C, 0xFFDCCF, 0x3A1B1B)
}
#[allow(non_snake_case)]
fn AMBER() -> Color32 {
    pick3(0xD7A54A, 0xB07D00, 0xE0B341)
}
#[allow(non_snake_case)]
fn AMBER_BG() -> Color32 {
    pick3(0x2A2214, 0xFFF1C2, 0x2A2412)
}

fn sb(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
fn rg(size: f32) -> FontId {
    FontId::proportional(size)
}
fn line(w: f32, c: Color32) -> Stroke {
    Stroke::new(w, c)
}

// ------------------------------------------------------------------ state types
enum Msg {
    Added(Result<Option<UserInfo>, String>),
    Checked(Vec<(String, Result<Option<UserInfo>, String>)>),
    Progress(String),
    Open { user_id: String, uri: String, target: Option<roblox::GameTarget> },
    Toast(String, ToastKind),
    LaunchDone { launched: usize, failed: Vec<(String, String)>, remaining: Vec<String>, fatal: Option<String> },
    Avatar { id: String, w: usize, h: usize, rgba: Vec<u8> },
    Details(String, Details),
    Presence(HashMap<String, Presence>),
    Servers(String, Result<Vec<ServerInfo>, String>),
    GameInfo { place: String, name: Option<String>, icon: Option<(usize, usize, Vec<u8>)> },
    BulkProgress(usize, usize),
    BulkDone(Vec<(String, Result<Option<UserInfo>, String>)>),
}

#[derive(Clone, Copy, PartialEq)]
enum ToastKind {
    Ok,
    Err,
    Info,
}
struct Toast {
    text: String,
    kind: ToastKind,
    secs: f64,
    until: Option<f64>,
}

enum Modal {
    None,
    Add { cookie: String, alias: String, show: bool, error: Option<String>, checking: bool },
    Bulk { text: String, progress: Option<(usize, usize)>, error: Option<String> },
    Rename { user_id: String, value: String },
    ConfirmDelete(Vec<String>),
    ConfirmStop(usize),
    Import { path: String, error: Option<String> },
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Play,
    Servers,
    Detail,
    Settings,
}

/// What the big left area shows.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Accounts,
    Clients,
}

#[derive(Clone, PartialEq)]
enum Filter {
    All,
    Favorite,
    Main,
    Expired,
    Group(String),
}

#[derive(Clone, Copy, PartialEq)]
enum ServerSort {
    Players,
    Ping,
}

enum CardAction {
    Select(String, bool),
    Launch(String),
    LaunchHome(String),
    Rename(String),
    Copy(String),
    Profile(String),
    Delete(String),
    ToggleFav(String),
    ToggleMain(String),
}

/// Edit buffer for the Account tab (written back when Save is clicked).
struct EditBuf {
    id: String,
    alias: String,
    group: String,
    note: String,
}

struct App {
    accounts: Vec<Account>,
    settings: Settings,
    selected: Vec<String>,
    search: String,
    filter: Filter,
    tab: Tab,
    game: String,
    job: String,
    multi: Option<win::NamedMutex>,
    /// Another tool (Roblox Account Manager, RoALT, ...) already holds the Roblox singleton
    /// mutex, so multi-instance works without us owning it.
    multi_external: bool,
    busy: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
    toasts: Vec<Toast>,
    modal: Modal,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    avatars: HashMap<String, egui::TextureHandle>,
    avatars_requested: HashSet<String>,
    game_icons: HashMap<String, egui::TextureHandle>,
    games_requested: HashSet<String>,
    details: HashMap<String, Details>,
    details_loading: HashSet<String>,
    presence: HashMap<String, Presence>,
    next_presence: f64,
    running: usize,
    next_proc: f64,
    next_trim: f64,
    last_activity: f64,
    idle_trimmed: bool,
    servers: Vec<ServerInfo>,
    servers_place: String,
    servers_loading: bool,
    servers_error: Option<String>,
    server_sort: ServerSort,
    edit: Option<EditBuf>,
    hwnd: isize,
    shared: monitor::SharedRef,
    mon_rx: Option<Receiver<monitor::Event>>,
    view: View,
    windows: Vec<win::RbxWindow>,
    next_windows: f64,
    /// Roblox window -> DWM thumbnail handle
    thumbs: HashMap<isize, isize>,
    /// Where each live preview should be drawn this frame (hwnd, rect in points, minimized)
    thumb_targets: Vec<(isize, Rect, bool)>,
    logo_tex: egui::TextureHandle,
    demo: bool,
    screenshot: Option<String>,
    frame_no: u64,
}

// ------------------------------------------------------------------ setup
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let read = |p: &str| std::fs::read(p).ok();
    if let Some(b) = read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(egui::FontData::from_owned(b)));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "segoe".into());
    }
    let mut semibold: Vec<String> = Vec::new();
    if let Some(b) = read(r"C:\Windows\Fonts\seguisb.ttf").or_else(|| read(r"C:\Windows\Fonts\segoeuib.ttf")) {
        fonts.font_data.insert("segoe_sb".into(), Arc::new(egui::FontData::from_owned(b)));
        semibold.push("segoe_sb".into());
    }
    semibold.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    fonts.families.insert(FontFamily::Name("semibold".into()), semibold);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(if is_light() { egui::ThemePreference::Light } else { egui::ThemePreference::Dark });
    let mut style = (*ctx.style()).clone();
    style.visuals = if is_light() { egui::Visuals::light() } else { egui::Visuals::dark() };
    let v = &mut style.visuals;
    v.panel_fill = BG();
    v.window_fill = SURFACE();
    v.extreme_bg_color = FIELD();
    v.faint_bg_color = CARD();
    v.window_stroke = line(1.0, BORDER());
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.selection.bg_fill = pick3(0x45464D, 0xD1C1FF, 0x3D5A16);
    v.selection.stroke = line(1.0, SELECT_BORDER());
    v.hyperlink_color = TEXT();
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(8);
        w.expansion = 0.0;
        w.fg_stroke = line(1.0, TEXT());
    }
    v.widgets.noninteractive.bg_stroke = line(1.0, BORDER());
    v.widgets.inactive.bg_fill = CARD();
    v.widgets.inactive.weak_bg_fill = CARD();
    v.widgets.inactive.bg_stroke = line(1.0, BORDER());
    v.widgets.hovered.bg_fill = CARD_HOVER();
    v.widgets.hovered.weak_bg_fill = CARD_HOVER();
    v.widgets.hovered.bg_stroke = line(1.0, BORDER_HI());
    v.widgets.active.bg_fill = CARD_HOVER();
    v.widgets.active.weak_bg_fill = CARD_HOVER();
    v.widgets.active.bg_stroke = line(1.0, SELECT_BORDER());
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(12.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.slider_width = 200.0;
    style.spacing.menu_margin = Margin::same(6);
    use egui::TextStyle::*;
    style.text_styles = [
        (Heading, sb(20.0)),
        (Body, rg(14.0)),
        (Button, rg(14.0)),
        (Small, rg(12.0)),
        (Monospace, FontId::monospace(13.0)),
    ]
    .into();
    ctx.set_style(style);
}

// ------------------------------------------------------------------ precise text & icons
/// Half the Segoe UI cap height (~0.70 em).
const CAP: f32 = 0.35;

/// Paint text that is visually centred on `anchor.y`.
/// egui places text using the font's line box (with empty space above the glyphs),
/// so here the position is computed from the baseline + cap height instead.
#[allow(clippy::too_many_arguments)]
fn paint_text(ui: &egui::Ui, p: &Painter, anchor: egui::Pos2, align: Align, text: &str, font: FontId, color: Color32, max_w: f32) -> f32 {
    let size = font.size;
    let g = elide(ui, text, font, color, max_w);
    let baseline = g.rows.first().and_then(|r| r.glyphs.first()).map(|gl| gl.pos.y).unwrap_or(g.size().y * 0.8);
    let w = g.size().x;
    let x = match align {
        Align::Min => anchor.x,
        Align::Center => anchor.x - w / 2.0,
        Align::Max => anchor.x - w,
    };
    let y = anchor.y + size * CAP - baseline;
    p.galley(pos2(x.round(), y.round()), g, color);
    w
}

fn text_width(ui: &egui::Ui, text: &str, font: FontId) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font, TEXT()).size().x)
}

#[derive(Clone, Copy)]
enum Icon {
    Plus,
    Minus,
}

fn paint_icon(p: &Painter, c: egui::Pos2, icon: Icon, color: Color32, s: f32) {
    let st = line(1.7, color);
    let h = s / 2.0;
    p.line_segment([pos2(c.x - h, c.y), pos2(c.x + h, c.y)], st);
    if let Icon::Plus = icon {
        p.line_segment([pos2(c.x, c.y - h), pos2(c.x, c.y + h)], st);
    }
}

fn paint_star(p: &Painter, c: egui::Pos2, r: f32, filled: bool, color: Color32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let a = -FRAC_PI_2 + i as f32 * PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.46 };
            c + vec2(a.cos(), a.sin()) * rr
        })
        .collect();
    if filled {
        let mut m = egui::Mesh::default();
        m.colored_vertex(c, color);
        for pt in &pts {
            m.colored_vertex(*pt, color);
        }
        for i in 0..10u32 {
            m.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
        }
        p.add(egui::Shape::mesh(m));
        p.add(egui::Shape::closed_line(pts, line(0.8, color)));
    } else {
        p.add(egui::Shape::closed_line(pts, line(1.3, color)));
    }
}

// ------------------------------------------------------------------ widgets
#[allow(clippy::too_many_arguments)]
fn button_ex(ui: &mut egui::Ui, label: &str, icon: Option<Icon>, size: Vec2, fill: Color32, hover: Color32, fg: Color32, font: FontId, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let (fill, fg) = if !enabled {
        (CARD(), FAINT())
    } else if resp.hovered() && !resp.is_pointer_button_down_on() {
        (hover, fg)
    } else {
        (fill, fg)
    };
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(8), fill);
    let text_w = if label.is_empty() { 0.0 } else { text_width(ui, label, font.clone()) };
    let icon_w = if icon.is_some() { 10.0 } else { 0.0 };
    let gap = if icon.is_some() && !label.is_empty() { 8.0 } else { 0.0 };
    let x0 = rect.center().x - (icon_w + gap + text_w) / 2.0;
    if let Some(ic) = icon {
        paint_icon(p, pos2((x0 + icon_w / 2.0).round(), rect.center().y.round()), ic, fg, 10.0);
    }
    if !label.is_empty() {
        paint_text(ui, p, pos2(x0 + icon_w + gap, rect.center().y), Align::Min, label, font, fg, f32::INFINITY);
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

#[allow(clippy::too_many_arguments)]
fn button(ui: &mut egui::Ui, label: &str, size: Vec2, fill: Color32, hover: Color32, fg: Color32, font: FontId, enabled: bool) -> egui::Response {
    button_ex(ui, label, None, size, fill, hover, fg, font, enabled)
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, size: Vec2) -> egui::Response {
    button_ex(ui, "", Some(icon), size, CARD(), CARD_HOVER(), TEXT(), rg(13.5), true)
}

fn primary(ui: &mut egui::Ui, label: &str, size: Vec2, enabled: bool) -> egui::Response {
    button(ui, label, size, PRIMARY(), PRIMARY_HI(), ON_PRIMARY(), sb(14.0), enabled)
}

fn ghost(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    button(ui, label, size, CARD(), CARD_HOVER(), TEXT(), rg(13.5), true)
}

fn danger(ui: &mut egui::Ui, label: &str, size: Vec2) -> egui::Response {
    button(ui, label, size, RED_BG(), RED_BG_HI(), RED(), rg(13.5), true)
}

fn menu_item(ui: &mut egui::Ui, label: &str, sub: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(270.0, 46.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(8), CARD_HOVER());
    }
    let p = ui.painter();
    paint_text(ui, p, rect.left_top() + vec2(12.0, 16.0), Align::Min, label, sb(13.5), TEXT(), 250.0);
    paint_text(ui, p, rect.left_top() + vec2(12.0, 32.0), Align::Min, sub, rg(12.0), MUTED(), 250.0);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn field(ui: &mut egui::Ui, value: &mut String, hint: &str, password: bool) -> egui::Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(RichText::new(hint).color(FAINT()))
            .password(password)
            .desired_width(f32::INFINITY)
            .margin(vec2(10.0, 8.0))
            .font(rg(14.0)),
    )
}

/// Small uppercase labels: semibold, or terminal-style mono in the Neon theme.
fn label_font(size: f32) -> FontId {
    if is_neon() { FontId::monospace(size - 0.5) } else { sb(size) }
}

fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(label_font(11.5)).color(MUTED()));
}

fn toggle(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    let t = ui.ctx().animate_bool(resp.id, on);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(11), if on { PRIMARY() } else { BORDER_HI() });
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, if on { ON_PRIMARY() } else { MUTED() });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn chip(ui: &mut egui::Ui, label: &str, active: bool) -> egui::Response {
    let w = ui.fonts(|f| f.layout_no_wrap(label.to_owned(), rg(12.5), TEXT()).size().x) + 22.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let (fill, fg, stroke) = if active {
        (PRIMARY(), ON_PRIMARY(), Stroke::NONE)
    } else if resp.hovered() {
        (CARD_HOVER(), TEXT(), line(1.0, BORDER_HI()))
    } else {
        (Color32::TRANSPARENT, MUTED(), line(1.0, BORDER()))
    };
    ui.painter().rect(rect, CornerRadius::same(13), fill, stroke, StrokeKind::Inside);
    paint_text(ui, ui.painter(), rect.center(), Align::Center, label, rg(12.5), fg, f32::INFINITY);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn panel_frame() -> Frame {
    Frame::new().fill(SURFACE()).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(16)).stroke(line(1.0, BORDER()))
}

fn elide(ui: &egui::Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(10.0));
    ui.fonts(|f| f.layout_job(job))
}

/// Paint a small pill; returns its rect.
fn pill(ui: &egui::Ui, p: &Painter, at: egui::Pos2, text: &str, fg: Color32, bg: Color32) -> Rect {
    let w = text_width(ui, text, sb(10.5)).min(140.0);
    let r = Rect::from_min_size(at, vec2(w + 14.0, 19.0));
    p.rect_filled(r, CornerRadius::same(5), bg);
    paint_text(ui, p, r.center(), Align::Center, text, sb(10.5), fg, 140.0);
    r
}

fn initials(a: &Account) -> String {
    a.title().chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase()
}

fn thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

// app logo (assets/logo.svg rendered to PNG), embedded in the .exe
const LOGO_64: &[u8] = include_bytes!("../assets/logo-64.png");
const LOGO_128: &[u8] = include_bytes!("../assets/logo-128.png");

fn png_rgba(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).expect("embedded logo").to_rgba8();
    (img.width(), img.height(), img.into_raw())
}

/// Column positions of the account list (shared by header and rows so they always line up).
struct Cols {
    check: f32,
    avatar: f32,
    name_end: f32,
    group: Option<f32>,
    group_w: f32,
    status: f32,
    status_w: f32,
    last: Option<f32>,
    star: f32,
}

impl Cols {
    fn new(r: Rect) -> Cols {
        let w = r.width();
        let show_last = w > 760.0;
        let show_group = w > 600.0;
        let star_w = 44.0;
        let last_w = if show_last { 118.0 } else { 0.0 };
        let group_w = if show_group { 118.0 } else { 0.0 };
        let status_w = if w > 700.0 { 200.0 } else { 150.0 };
        let star = r.right() - star_w / 2.0 - 4.0;
        let last = r.right() - star_w - last_w;
        let status = last - status_w;
        let group = status - group_w;
        Cols {
            check: r.left() + 16.0,
            avatar: r.left() + 46.0,
            name_end: group - 12.0,
            group: show_group.then_some(group),
            group_w,
            status,
            status_w,
            last: show_last.then_some(last),
            star,
        }
    }
}

fn paint_checkbox(p: &Painter, r: Rect, on: bool, partial: bool, hot: bool) {
    if on || partial {
        p.rect_filled(r, CornerRadius::same(4), PRIMARY());
        if on {
            let a = pos2(r.left() + 3.5, r.center().y + 0.5);
            let b = pos2(r.left() + 6.8, r.bottom() - 4.0);
            let c = pos2(r.right() - 3.5, r.top() + 4.0);
            p.line_segment([a, b], line(2.0, ON_PRIMARY()));
            p.line_segment([b, c], line(2.0, ON_PRIMARY()));
        } else {
            p.line_segment([pos2(r.left() + 4.0, r.center().y), pos2(r.right() - 4.0, r.center().y)], line(2.0, ON_PRIMARY()));
        }
    } else {
        p.rect_stroke(r, CornerRadius::same(4), line(1.2, if hot { MUTED() } else { BORDER_HI() }), StrokeKind::Inside);
    }
}

/// Segmented control (like small tabs). Returns the new index when clicked.
fn segmented(ui: &mut egui::Ui, id: egui::Id, options: &[&str], active: usize) -> Option<usize> {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(8), FIELD(), line(1.0, BORDER()), StrokeKind::Inside);
    let tw = (w - 6.0) / options.len() as f32;
    let mut out = None;
    for (i, label) in options.iter().enumerate() {
        let r = Rect::from_min_size(rect.min + vec2(3.0 + i as f32 * tw, 3.0), vec2(tw, 26.0));
        let resp = ui.interact(r, id.with(i), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        let on = i == active;
        if on {
            ui.painter().rect_filled(r, CornerRadius::same(6), CARD_HOVER());
        }
        let col = if on { TEXT() } else if resp.hovered() { TEXT() } else { MUTED() };
        paint_text(ui, ui.painter(), r.center(), Align::Center, label, if on { sb(13.0) } else { rg(13.0) }, col, tw);
        if resp.clicked() && !on {
            out = Some(i);
        }
    }
    out
}

fn paint_x(p: &Painter, c: egui::Pos2, h: f32, color: Color32) {
    let st = line(1.6, color);
    p.line_segment([c + vec2(-h, -h), c + vec2(h, h)], st);
    p.line_segment([c + vec2(-h, h), c + vec2(h, -h)], st);
}

fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, BORDER());
}

/// Settings row: title + description on the left, control on the right, fixed height.
fn setting_row(ui: &mut egui::Ui, title: &str, sub: &str, ctrl_w: f32, control: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
    let p = ui.painter();
    paint_text(ui, p, pos2(rect.left(), rect.center().y - 8.0), Align::Min, title, sb(13.5), TEXT(), rect.width() - ctrl_w - 12.0);
    paint_text(ui, p, pos2(rect.left(), rect.center().y + 9.0), Align::Min, sub, rg(12.0), MUTED(), rect.width() - ctrl_w - 12.0);
    let ctrl = Rect::from_min_max(pos2(rect.right() - ctrl_w, rect.top()), rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(ctrl).layout(Layout::right_to_left(Align::Center)), control);
}

fn local_time_label() -> String {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{:02} {} {:02}:{:02}", st.wDay, M[(st.wMonth.clamp(1, 12) - 1) as usize], st.wHour, st.wMinute)
}

/// Look for Roblox Account Manager's AccountData.json in common locations.
fn detect_ram_file() -> Option<String> {
    use std::path::{Path, PathBuf};
    let home = std::env::var("USERPROFILE").unwrap_or_default();
    let mut roots: Vec<PathBuf> = vec!["D:\\".into(), "E:\\".into(), "C:\\".into()];
    for d in ["Desktop", "Downloads", "Documents", "OneDrive\\Desktop"] {
        roots.push(Path::new(&home).join(d));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        roots.insert(0, dir);
    }
    for r in &roots {
        let f = r.join("AccountData.json");
        if f.is_file() {
            return Some(f.display().to_string());
        }
    }
    for r in &roots {
        if let Ok(rd) = std::fs::read_dir(r) {
            for e in rd.flatten() {
                let f = e.path().join("AccountData.json");
                if f.is_file() {
                    return Some(f.display().to_string());
                }
            }
        }
    }
    None
}

/// Pull a cookie out of one line of text (free format, e.g. "username:cookie").
fn extract_cookie(line: &str) -> Option<String> {
    let l = line.trim();
    if l.is_empty() {
        return None;
    }
    let c = match l.find("_|WARNING") {
        Some(i) => &l[i..],
        None => l.rsplit([':', ' ', '\t', ',']).next().unwrap_or(l),
    };
    let c: String = c.chars().filter(|ch| !ch.is_whitespace()).collect();
    let c = c.strip_prefix(".ROBLOSECURITY=").map(|s| s.to_string()).unwrap_or(c);
    if c.len() >= 100 { Some(c) } else { None }
}

// ------------------------------------------------------------------ app
impl App {
    fn new(cc: &eframe::CreationContext<'_>, accounts: Vec<Account>, args: &[String]) -> Self {
        set_theme(&store::load_settings().theme);
        if let Some(i) = args.iter().position(|a| a == "--theme") {
            set_theme(args.get(i + 1).map(|s| s.as_str()).unwrap_or(""));
        }
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let mut hwnd = 0isize;
        if let Ok(h) = raw_window_handle::HasWindowHandle::window_handle(cc) {
            if let raw_window_handle::RawWindowHandle::Win32(w) = h.as_raw() {
                hwnd = w.hwnd.get();
                win::set_title_bar_dark(hwnd as *mut std::ffi::c_void, !is_light());
            }
        }
        tray::MAIN_HWND.store(hwnd, std::sync::atomic::Ordering::Relaxed);
        let has = |a: &str| args.iter().any(|x| x == a);
        let arg_val = |a: &str| args.iter().position(|x| x == a).and_then(|i| args.get(i + 1).cloned());
        let mut settings = store::load_settings();
        if settings.stagger_secs == 0 {
            settings.stagger_secs = 6;
        }
        let (tx, rx) = channel();
        let mut app = App {
            game: settings.last_game.clone(),
            job: settings.last_job.clone(),
            accounts,
            settings,
            selected: vec![],
            search: String::new(),
            filter: Filter::All,
            tab: Tab::Play,
            multi: None,
            multi_external: false,
            busy: None,
            cancel: None,
            toasts: vec![],
            modal: Modal::None,
            tx,
            rx,
            avatars: HashMap::new(),
            avatars_requested: HashSet::new(),
            game_icons: HashMap::new(),
            games_requested: HashSet::new(),
            details: HashMap::new(),
            details_loading: HashSet::new(),
            presence: HashMap::new(),
            next_presence: 0.0,
            running: 0,
            next_proc: 0.0,
            next_trim: 3.0,
            last_activity: 0.0,
            idle_trimmed: false,
            servers: vec![],
            servers_place: String::new(),
            servers_loading: false,
            servers_error: None,
            server_sort: ServerSort::Players,
            edit: None,
            hwnd,
            shared: Arc::new(std::sync::Mutex::new(monitor::Shared::default())),
            mon_rx: None,
            view: View::Accounts,
            windows: vec![],
            next_windows: 0.0,
            thumbs: HashMap::new(),
            thumb_targets: vec![],
            logo_tex: {
                let (w, h, rgba) = png_rgba(LOGO_128);
                cc.egui_ctx.load_texture("logo", egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba), egui::TextureOptions::LINEAR)
            },
            demo: has("--demo"),
            screenshot: arg_val("--screenshot"),
            frame_no: 0,
        };
        if app.settings.multi_roblox {
            app.multi = win::NamedMutex::acquire("ROBLOX_singletonMutex");
        }
        app.detect_external_multi();
        if app.settings.afk_minutes == 0 {
            app.settings.afk_minutes = 15;
        }
        if app.demo {
            app.load_demo();
        } else {
            // background monitor (anti-AFK, auto-rejoin, tray commands) + Discord presence + tray icon
            app.sync_monitor();
            let (ev_tx, ev_rx) = channel();
            let (tray_tx, tray_rx) = channel();
            let ctx = cc.egui_ctx.clone();
            monitor::start(app.shared.clone(), ev_tx, tray_rx, hwnd, Arc::new(move || ctx.request_repaint()));
            monitor::start_rpc(app.shared.clone());
            app.mon_rx = Some(ev_rx);
            if app.screenshot.is_none() {
                tray::start(tray_tx);
            }
        }
        match arg_val("--tab").as_deref() {
            Some("servers") => app.tab = Tab::Servers,
            Some("detail") => app.tab = Tab::Detail,
            Some("settings") => app.tab = Tab::Settings,
            _ => {}
        }
        if has("--clients") {
            app.view = View::Clients;
        }
        if has("--home") {
            app.settings.launch_home = true;
        }
        if has("--show-add") {
            app.open_add();
        }
        if has("--show-bulk") {
            app.modal = Modal::Bulk { text: String::new(), progress: None, error: None };
        }
        if has("--show-import") {
            app.open_import();
        }
        if has("--test-net") {
            // test mode: select the first active account & load servers for the last game
            if let Some(a) = app.accounts.iter().find(|a| a.is_ok()) {
                app.selected = vec![a.user_id.clone()];
            }
            if app.game.trim().is_empty() {
                app.game = "920587237".into();
            }
            let ctx = cc.egui_ctx.clone();
            app.load_servers(&ctx);
        }
        if let Some(p) = arg_val("--auto-import") {
            let ctx = cc.egui_ctx.clone();
            if let Err(e) = app.import_from(&ctx, &p) {
                app.toast(e, ToastKind::Err, 60.0);
            }
        }
        app
    }

    fn load_demo(&mut self) {
        let mk = |id: &str, user: &str, alias: &str, st: &str, last: &str, group: &str, fav: bool, main: bool| Account {
            user_id: id.into(),
            username: user.into(),
            display_name: alias.into(),
            alias: alias.into(),
            status: st.into(),
            last_played: last.into(),
            group: group.into(),
            favorite: fav,
            main,
            ..Default::default()
        };
        self.accounts = vec![
            mk("1001", "alex_main", "Main", "OK", "08 Nov 20:14", "", true, true),
            mk("1002", "alex_alt01", "Alt 1", "OK", "08 Nov 19:02", "Farming", true, false),
            mk("1003", "alex_farm", "Farming", "EXPIRED", "", "Farming", false, false),
            mk("1004", "sky_builder", "Builder", "OK", "07 Nov 22:40", "Trade", false, false),
            mk("1005", "night_trader", "Trading", "OK", "", "Trade", false, false),
        ];
        self.presence.insert("1001".into(), Presence { kind: 2, location: "Blox Fruits".into() });
        self.presence.insert("1004".into(), Presence { kind: 1, location: "Website".into() });
        self.details.insert(
            "1001".into(),
            Details { robux: Some(12450), premium: Some(true), friends: Some(87), followers: Some(1204), following: Some(56), created: "14 Mar 2019".into(), banned: false },
        );
        self.selected = vec!["1001".into()];
        self.game = "https://www.roblox.com/games/2753915549/Blox-Fruits".into();
        self.settings.recent_games = vec!["2753915549".into(), "920587237".into(), "@builderman".into()];
        self.settings.favorite_games = vec!["2753915549".into(), "920587237".into(), "@builderman".into()];
        self.settings.game_names.insert("2753915549".into(), "Blox Fruits".into());
        self.settings.game_names.insert("920587237".into(), "Adopt Me!".into());
        self.servers_place = "2753915549".into();
        self.servers = (0..9)
            .map(|i| ServerInfo { id: format!("1a2b3c4d-0000-4000-8000-00000000000{i}"), playing: 12 - i, max: 12, ping: Some(40 + i * 17), fps: Some(59.8) })
            .collect();
        self.running = 2;
    }

    fn toast(&mut self, text: impl Into<String>, kind: ToastKind, secs: f64) {
        self.toasts.push(Toast { text: text.into(), kind, secs, until: None });
    }

    fn save(&mut self) {
        if self.demo {
            return;
        }
        if let Err(e) = store::save_accounts(&self.accounts) {
            self.toast(format!("Failed to save: {e}"), ToastKind::Err, 6.0);
        }
    }

    fn save_settings(&mut self) {
        self.settings.last_game = self.game.clone();
        self.settings.last_job = self.job.clone();
        self.settings.multi_roblox = self.multi.is_some();
        if !self.demo {
            store::save_settings(&self.settings);
        }
    }

    fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.user_id == id)
    }
    fn account_mut(&mut self, id: &str) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|a| a.user_id == id)
    }
    fn any_valid_cookie(&self) -> Option<String> {
        self.accounts.iter().find(|a| a.is_ok() && !a.cookie.is_empty()).map(|a| a.cookie.clone())
    }
    fn groups(&self) -> Vec<String> {
        let mut g: Vec<String> = self.accounts.iter().map(|a| a.group.trim().to_string()).filter(|g| !g.is_empty()).collect();
        g.sort_by_key(|s| s.to_lowercase());
        g.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        g
    }
    fn select_all(&mut self) {
        // MAIN accounts are intentionally left out
        self.selected = self.visible_ids().into_iter().filter(|id| !self.account(id).map(|a| a.main).unwrap_or(false)).collect();
    }
    fn visible_ids(&self) -> Vec<String> {
        let q = self.search.trim().to_lowercase();
        self.accounts
            .iter()
            .filter(|a| match &self.filter {
                Filter::All => true,
                Filter::Favorite => a.favorite,
                Filter::Main => a.main,
                Filter::Expired => !a.is_ok(),
                Filter::Group(g) => a.group.trim().eq_ignore_ascii_case(g),
            })
            .filter(|a| q.is_empty() || format!("{} {} {} {} {}", a.alias, a.username, a.display_name, a.group, a.note).to_lowercase().contains(&q))
            .map(|a| a.user_id.clone())
            .collect()
    }

    // -------------------------------------------------------------- background work
    fn request_avatars(&mut self, ctx: &egui::Context) {
        if self.demo {
            return;
        }
        let ids: Vec<String> = self
            .accounts
            .iter()
            .map(|a| a.user_id.clone())
            .filter(|id| !id.is_empty() && !self.avatars_requested.contains(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        self.avatars_requested.extend(ids.iter().cloned());
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            for (id, png) in roblox::fetch_headshots(&ids) {
                if let Ok(img) = image::load_from_memory_with_format(&png, image::ImageFormat::Png) {
                    let rgba = img.to_rgba8();
                    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                    let _ = tx.send(Msg::Avatar { id, w, h, rgba: rgba.into_raw() });
                    ctx.request_repaint();
                }
            }
        });
    }

    /// Fetch name + icon for pinned games (once per game per session).
    fn request_game_info(&mut self, ctx: &egui::Context) {
        if self.demo {
            return;
        }
        let places: Vec<String> = self
            .settings
            .favorite_games
            .iter()
            .filter_map(|pin| roblox::parse_game(pin, "").ok().map(|g| g.place_id))
            .filter(|p| !p.is_empty() && !self.games_requested.contains(p))
            .collect();
        if places.is_empty() {
            return;
        }
        self.games_requested.extend(places.iter().cloned());
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            for place in places {
                let (name, png) = roblox::game_info(&place);
                let icon = png
                    .and_then(|b| image::load_from_memory_with_format(&b, image::ImageFormat::Png).ok())
                    .map(|img| {
                        let rgba = img.to_rgba8();
                        (rgba.width() as usize, rgba.height() as usize, rgba.into_raw())
                    });
                let _ = tx.send(Msg::GameInfo { place, name, icon });
                ctx.request_repaint();
            }
        });
    }

    /// Save RAM: trim the working set while the app is in the background / minimized / idle.
    fn memory(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_some() {
            return;
        }
        let (now, focused, minimized, active) = ctx.input(|i| {
            let vp = i.viewport();
            (
                i.time,
                vp.focused.unwrap_or(true),
                vp.minimized.unwrap_or(false),
                !i.events.is_empty() || i.pointer.is_moving() || i.pointer.any_down(),
            )
        });
        if active {
            self.last_activity = now;
            self.idle_trimmed = false;
        }
        if !focused || minimized {
            if now >= self.next_trim {
                win::trim_memory();
                self.next_trim = now + 10.0;
            }
        } else if !self.idle_trimmed && now - self.last_activity > 8.0 && self.busy.is_none() && self.toasts.is_empty() {
            win::trim_memory();
            self.idle_trimmed = true;
        }
    }

    fn periodic(&mut self, ctx: &egui::Context) {
        let (now, background) = ctx.input(|i| {
            let vp = i.viewport();
            (i.time, !vp.focused.unwrap_or(true) || vp.minimized.unwrap_or(false))
        });
        if self.demo {
            return;
        }
        // in the background, checking every 10 s is enough (fewer repaints = less memory)
        let every = if background { 10.0 } else { 3.0 };
        if now >= self.next_proc {
            self.next_proc = now + every;
            self.running = win::roblox_pids().len();
            self.detect_external_multi();
        }
        // live client list (faster while the Clients view is open)
        if now >= self.next_windows {
            self.next_windows = now + if self.view == View::Clients { 1.5 } else { every };
            self.windows = win::roblox_windows();
        }
        self.sync_monitor();
        if now >= self.next_presence && !self.accounts.is_empty() {
            self.next_presence = now + 45.0;
            if let Some(cookie) = self.any_valid_cookie() {
                let ids: Vec<String> = self.accounts.iter().map(|a| a.user_id.clone()).collect();
                let (tx, ctx2) = (self.tx.clone(), ctx.clone());
                std::thread::spawn(move || {
                    if let Ok(map) = roblox::get_presence(&ids, &cookie) {
                        let _ = tx.send(Msg::Presence(map));
                        ctx2.request_repaint();
                    }
                });
            }
        }
        ctx.request_repaint_after(Duration::from_secs_f64(every));
    }

    fn load_details(&mut self, ctx: &egui::Context, id: &str) {
        if self.demo || self.details_loading.contains(id) {
            return;
        }
        let Some(cookie) = self.account(id).map(|a| a.cookie.clone()) else { return };
        self.details_loading.insert(id.to_string());
        let (tx, ctx, id) = (self.tx.clone(), ctx.clone(), id.to_string());
        std::thread::spawn(move || {
            let d = roblox::get_details(&id, &cookie);
            let _ = tx.send(Msg::Details(id, d));
            ctx.request_repaint();
        });
    }

    fn load_servers(&mut self, ctx: &egui::Context) {
        if self.servers_loading {
            return;
        }
        let place = match roblox::parse_game(&self.game, "") {
            Ok(g) if !g.place_id.is_empty() => g.place_id,
            _ => {
                self.servers_error = Some("Enter a game link or Place ID in the Play tab first.".into());
                return;
            }
        };
        if self.demo {
            return;
        }
        self.servers_loading = true;
        self.servers_error = None;
        let cookie = self.any_valid_cookie();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = roblox::list_servers(&place, cookie.as_deref());
            let _ = tx.send(Msg::Servers(place, r));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Avatar { id, w, h, rgba } => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                    let tex = ctx.load_texture(format!("av-{id}"), img, egui::TextureOptions::LINEAR);
                    self.avatars.insert(id, tex);
                }
                Msg::Progress(t) => self.busy = Some(t),
                // (monitor events are drained in poll_monitor)
                Msg::Toast(t, kind) => self.toast(t, kind, 6.0),
                Msg::Open { user_id, uri, target } => {
                    if win::open_url(&uri) {
                        let now = local_time_label();
                        if let Some(a) = self.account_mut(&user_id) {
                            a.last_played = now;
                            a.status = "OK".into();
                        }
                        // let the monitor match the new Roblox process to this account
                        if let Some(a) = self.account(&user_id).cloned() {
                            self.shared.lock().unwrap().expect_launch(&a.user_id, &a.username, &a.cookie, target);
                        }
                        self.save();
                    } else {
                        self.toast("Couldn't open Roblox. Make sure Roblox Player is installed.", ToastKind::Err, 7.0);
                    }
                }
                Msg::LaunchDone { launched, failed, remaining, fatal } => {
                    self.busy = None;
                    self.cancel = None;
                    if self.settings.discord_alerts && fatal.is_none() && launched + failed.len() > 1 {
                        discord::alert(
                            &self.settings.discord_webhook,
                            format!(":white_check_mark: Batch finished: {launched} launched, {} failed, {} cancelled.", failed.len(), remaining.len()),
                        );
                    }
                    if let Some(e) = fatal {
                        self.toast(e, ToastKind::Err, 8.0);
                        continue;
                    }
                    for (id, err) in &failed {
                        if err.contains("HTTP 401") {
                            if let Some(a) = self.account_mut(id) {
                                a.status = "EXPIRED".into();
                            }
                        }
                    }
                    if !failed.is_empty() {
                        self.save();
                    }
                    let mut redo: Vec<String> = failed.iter().map(|(id, _)| id.clone()).collect();
                    redo.extend(remaining.iter().cloned());
                    if redo.is_empty() {
                        self.toast(format!("{launched} account(s) launched. Waiting for Roblox to open."), ToastKind::Ok, 5.0);
                    } else {
                        let mut parts = vec![format!("{launched} launched")];
                        if !failed.is_empty() {
                            parts.push(format!("{} failed", failed.len()));
                        }
                        if !remaining.is_empty() {
                            parts.push(format!("{} cancelled", remaining.len()));
                        }
                        self.selected = redo;
                        self.toast(format!("{}. Accounts that didn't launch were re-selected.", parts.join(", ")), ToastKind::Info, 8.0);
                        if let Some((_, e)) = failed.first() {
                            self.toast(e.clone(), ToastKind::Err, 8.0);
                        }
                    }
                }
                Msg::Checked(results) => {
                    self.busy = None;
                    let (mut ok, mut bad, mut err) = (0, 0, None);
                    for (id, r) in results {
                        match r {
                            Ok(Some(u)) => {
                                if let Some(a) = self.account_mut(&id) {
                                    a.status = "OK".into();
                                    a.username = u.name;
                                    a.display_name = u.display_name;
                                }
                                ok += 1;
                            }
                            Ok(None) => {
                                if let Some(a) = self.account_mut(&id) {
                                    a.status = "EXPIRED".into();
                                }
                                bad += 1;
                            }
                            Err(e) => err = Some(e),
                        }
                    }
                    self.save();
                    match err {
                        Some(e) => self.toast(e, ToastKind::Err, 7.0),
                        None if bad > 0 => self.toast(format!("{ok} active, {bad} expired. Re-add the expired ones."), ToastKind::Info, 7.0),
                        None => self.toast(format!("All {ok} accounts are active."), ToastKind::Ok, 4.0),
                    }
                }
                Msg::Details(id, d) => {
                    self.details_loading.remove(&id);
                    self.details.insert(id, d);
                }
                Msg::Presence(map) => self.presence = map,
                Msg::Servers(place, r) => {
                    self.servers_loading = false;
                    match r {
                        Ok(list) => {
                            self.servers = list;
                            self.servers_place = place;
                        }
                        Err(e) => self.servers_error = Some(e),
                    }
                }
                Msg::GameInfo { place, name, icon } => {
                    if let Some((w, h, rgba)) = icon {
                        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                        self.game_icons.insert(place.clone(), ctx.load_texture(format!("game-{place}"), img, egui::TextureOptions::LINEAR));
                    }
                    if let Some(n) = name {
                        if self.settings.game_names.get(&place) != Some(&n) {
                            self.settings.game_names.insert(place, n);
                            self.save_settings();
                        }
                    }
                }
                Msg::BulkProgress(i, n) => {
                    if let Modal::Bulk { progress, .. } = &mut self.modal {
                        *progress = Some((i, n));
                    }
                }
                Msg::BulkDone(results) => {
                    let (mut added, mut updated, mut bad) = (0, 0, 0);
                    let mut err = None;
                    for (cookie, r) in results {
                        match r {
                            Ok(Some(u)) => {
                                if self.upsert(u, cookie, String::new()) {
                                    added += 1;
                                } else {
                                    updated += 1;
                                }
                            }
                            Ok(None) => bad += 1,
                            Err(e) => {
                                bad += 1;
                                err = Some(e);
                            }
                        }
                    }
                    self.save();
                    self.modal = Modal::None;
                    self.request_avatars(ctx);
                    self.toast(format!("{added} added, {updated} updated, {bad} failed/expired."), if bad > 0 { ToastKind::Info } else { ToastKind::Ok }, 7.0);
                    if let Some(e) = err {
                        self.toast(e, ToastKind::Err, 7.0);
                    }
                }
                Msg::Added(r) => {
                    let Modal::Add { cookie, alias, error, checking, .. } = &mut self.modal else { continue };
                    *checking = false;
                    match r {
                        Err(e) => *error = Some(e),
                        Ok(None) => *error = Some("Invalid or expired cookie. Sign in again and copy a fresh cookie.".into()),
                        Ok(Some(u)) => {
                            let cookie = std::mem::take(cookie);
                            let alias = alias.trim().to_string();
                            let name = u.name.clone();
                            let id = u.id.clone();
                            let new = self.upsert(u, cookie, alias);
                            self.selected = vec![id];
                            self.modal = Modal::None;
                            self.save();
                            self.toast(if new { format!("Added {name}.") } else { format!("{name} already exists, cookie updated.") }, ToastKind::Ok, 4.0);
                            self.request_avatars(ctx);
                        }
                    }
                }
            }
        }
    }

    /// Add or update an account. Returns true if it is new.
    fn upsert(&mut self, u: UserInfo, cookie: String, alias: String) -> bool {
        if let Some(a) = self.accounts.iter_mut().find(|a| a.user_id == u.id) {
            a.cookie = cookie;
            a.username = u.name;
            a.display_name = u.display_name;
            a.status = "OK".into();
            if !alias.is_empty() {
                a.alias = alias;
            }
            false
        } else {
            self.accounts.push(Account {
                user_id: u.id,
                alias: if alias.is_empty() { u.name.clone() } else { alias },
                username: u.name,
                display_name: u.display_name,
                cookie,
                added: local_time_label(),
                status: "OK".into(),
                ..Default::default()
            });
            true
        }
    }

    // -------------------------------------------------------------- actions
    fn open_add(&mut self) {
        self.modal = Modal::Add { cookie: String::new(), alias: String::new(), show: false, error: None, checking: false };
    }

    fn open_import(&mut self) {
        self.modal = Modal::Import { path: detect_ram_file().unwrap_or_default(), error: None };
    }

    fn import_from(&mut self, ctx: &egui::Context, path: &str) -> Result<(), String> {
        let list = store::import_ram(path)?;
        let (mut added, mut updated) = (0, 0);
        for a in list {
            if let Some(e) = self.accounts.iter_mut().find(|x| x.user_id == a.user_id) {
                e.cookie = a.cookie;
                e.status = a.status;
                if e.username.is_empty() {
                    e.username = a.username;
                }
                updated += 1;
            } else {
                let mut a = a;
                a.added = local_time_label();
                self.accounts.push(a);
                added += 1;
            }
        }
        self.save();
        self.toast(format!("Import done: {added} added, {updated} updated. Checking status..."), ToastKind::Ok, 8.0);
        self.request_avatars(ctx);
        self.check_all(ctx);
        Ok(())
    }

    fn submit_add(&mut self, ctx: &egui::Context) {
        let Modal::Add { cookie, error, checking, .. } = &mut self.modal else { return };
        let Some(c) = extract_cookie(cookie) else {
            *error = Some("That cookie looks incomplete. Copy the ENTIRE .ROBLOSECURITY value.".into());
            return;
        };
        *cookie = c.clone();
        *error = None;
        *checking = true;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Added(roblox::get_user(&c)));
            ctx.request_repaint();
        });
    }

    fn submit_bulk(&mut self, ctx: &egui::Context) {
        let Modal::Bulk { text, progress, error } = &mut self.modal else { return };
        let mut cookies: Vec<String> = text.lines().filter_map(extract_cookie).collect();
        cookies.dedup();
        if cookies.is_empty() {
            *error = Some("No valid cookies found. One cookie per line (username:cookie also works).".into());
            return;
        }
        *error = None;
        *progress = Some((0, cookies.len()));
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let n = cookies.len();
            let mut out = Vec::with_capacity(n);
            for (i, c) in cookies.into_iter().enumerate() {
                let _ = tx.send(Msg::BulkProgress(i + 1, n));
                ctx.request_repaint();
                let r = roblox::get_user(&c);
                out.push((c, r));
            }
            let _ = tx.send(Msg::BulkDone(out));
            ctx.request_repaint();
        });
    }

    fn launch(&mut self, ctx: &egui::Context) {
        let home = self.settings.launch_home;
        self.launch_mode(ctx, home);
    }

    /// `home = true`: open Roblox on the home screen (without joining a game).
    fn launch_mode(&mut self, ctx: &egui::Context, home: bool) {
        if self.busy.is_some() {
            return;
        }
        let sel: Vec<Account> = self.selected.iter().filter_map(|id| self.account(id).cloned()).collect();
        if sel.is_empty() {
            return self.toast("Select an account to launch first (Ctrl+click to select several).", ToastKind::Info, 5.0);
        }
        if sel.len() > 1 && !self.multi_on() {
            return self.toast("Launching more than one account? Turn on Multi Roblox first (close every Roblox window before).", ToastKind::Info, 7.0);
        }
        let game = if home {
            None
        } else {
            match roblox::parse_game(&self.game, &self.job) {
                Ok(g) => Some(g),
                Err(e) => return self.toast(e, ToastKind::Err, 7.0),
            }
        };
        if !home {
            let g = self.game.trim().to_string();
            self.settings.recent_games.retain(|x| !x.eq_ignore_ascii_case(&g));
            self.settings.recent_games.insert(0, g);
            self.settings.recent_games.truncate(8);
        }
        self.save_settings();
        if self.demo {
            return;
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.busy = Some("Preparing launch...".into());
        let stagger = self.settings.stagger_secs.clamp(1, 120) as u64;
        let fps_cap = self.settings.fps_cap;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let send = |m: Msg| {
                let _ = tx.send(m);
                ctx.request_repaint();
            };
            let ids = |from: usize| sel[from..].iter().map(|a| a.user_id.clone()).collect::<Vec<_>>();
            // FPS limit: re-applied on every launch because Roblox updates create new version folders
            if fps_cap > 0 {
                if let Err(e) = fps::apply(fps_cap) {
                    send(Msg::Toast(format!("FPS limit not applied: {e}"), ToastKind::Info));
                }
            }
            let mut game = game;
            if let Some(g) = game.as_mut() {
                if let Err(e) = g.resolve(&sel[0].cookie) {
                    return send(Msg::LaunchDone { launched: 0, failed: vec![], remaining: ids(0), fatal: Some(e) });
                }
            }
            let n = sel.len();
            let (mut launched, mut failed) = (0, Vec::new());
            for (i, acc) in sel.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    return send(Msg::LaunchDone { launched, failed, remaining: ids(i), fatal: None });
                }
                send(Msg::Progress(format!("Launching {} ({}/{n})...", acc.username, i + 1)));
                match roblox::get_ticket(&acc.cookie) {
                    Ok(t) => {
                        let uri = match &game {
                            Some(g) => roblox::launch_uri(&t, g),
                            None => roblox::home_uri(&t),
                        };
                        send(Msg::Open { user_id: acc.user_id.clone(), uri, target: game.clone() });
                        launched += 1;
                    }
                    Err(e) => failed.push((acc.user_id.clone(), format!("{}: {e}", acc.username))),
                }
                if i + 1 < n {
                    for left in (1..=stagger * 10).rev() {
                        if cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        if left % 10 == 0 {
                            send(Msg::Progress(format!("Next account in {}s... ({}/{n} done)", left / 10, i + 1)));
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            send(Msg::LaunchDone { launched, failed, remaining: vec![], fatal: None });
        });
    }

    fn check_all(&mut self, ctx: &egui::Context) {
        if self.busy.is_some() || self.demo {
            return;
        }
        if self.accounts.is_empty() {
            return self.toast("No accounts yet.", ToastKind::Info, 4.0);
        }
        self.busy = Some("Checking all accounts...".into());
        let list: Vec<(String, String)> = self.accounts.iter().map(|a| (a.user_id.clone(), a.cookie.clone())).collect();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let res = list.into_iter().map(|(id, c)| (id, roblox::get_user(&c))).collect();
            let _ = tx.send(Msg::Checked(res));
            ctx.request_repaint();
        });
    }

    fn multi_on(&self) -> bool {
        self.multi.is_some() || self.multi_external
    }

    /// The singleton mutex exists while no Roblox client runs: another manager owns it.
    fn detect_external_multi(&mut self) {
        if self.multi.is_some() {
            self.multi_external = false;
            return;
        }
        let exists = win::mutex_exists("ROBLOX_singletonMutex");
        if !exists {
            self.multi_external = false;
        } else if win::roblox_pids().is_empty() {
            self.multi_external = true;
        }
    }

    fn set_multi(&mut self, on: bool) {
        if on {
            match win::NamedMutex::acquire("ROBLOX_singletonMutex") {
                Some(m) => {
                    self.multi = Some(m);
                    self.multi_external = false;
                    self.toast("Multi Roblox is on. Keep this app open while you play.", ToastKind::Ok, 5.0);
                }
                None if win::roblox_pids().is_empty() => {
                    self.multi_external = true;
                    self.toast("Multi Roblox is already on: another app (like Roblox Account Manager) is handling it.", ToastKind::Ok, 7.0);
                }
                None => self.toast("Roblox is already running. Close every Roblox window, then turn this on again.", ToastKind::Err, 7.0),
            }
        } else if self.multi_external {
            self.toast("Multi Roblox is handled by another app (like Roblox Account Manager). Close that app to turn it off.", ToastKind::Info, 7.0);
        } else {
            self.multi = None;
            self.toast("Multi Roblox is off.", ToastKind::Info, 3.0);
        }
        self.save_settings();
    }

    fn apply(&mut self, ctx: &egui::Context, action: CardAction) {
        match action {
            CardAction::Select(id, additive) => {
                if additive {
                    if let Some(p) = self.selected.iter().position(|x| *x == id) {
                        self.selected.remove(p);
                    } else {
                        self.selected.push(id);
                    }
                } else {
                    self.selected = vec![id];
                }
            }
            CardAction::Launch(id) => {
                self.selected = vec![id];
                self.launch(ctx);
            }
            CardAction::LaunchHome(id) => {
                self.selected = vec![id];
                self.launch_mode(ctx, true);
            }
            CardAction::Rename(id) => {
                let value = self.account(&id).map(|a| a.alias.clone()).unwrap_or_default();
                self.modal = Modal::Rename { user_id: id, value };
            }
            CardAction::Copy(id) => {
                if let Some(u) = self.account(&id).map(|a| a.username.clone()) {
                    ctx.copy_text(u.clone());
                    self.toast(format!("Copied {u}."), ToastKind::Ok, 3.0);
                }
            }
            CardAction::Profile(id) => {
                win::open_url(&format!("https://www.roblox.com/users/{id}/profile"));
            }
            CardAction::Delete(id) => {
                let ids = if self.selected.contains(&id) { self.selected.clone() } else { vec![id] };
                self.modal = Modal::ConfirmDelete(ids);
            }
            CardAction::ToggleFav(id) => {
                if let Some(a) = self.account_mut(&id) {
                    a.favorite = !a.favorite;
                }
                self.save();
            }
            CardAction::ToggleMain(id) => {
                if let Some(a) = self.account_mut(&id) {
                    a.main = !a.main;
                }
                self.save();
            }
        }
    }

    // -------------------------------------------------------------- ui: header & status
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .exact_height(76.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(HEADER_BG()).inner_margin(Margin { left: 24, right: 24, top: 14, bottom: 14 }))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let (lr, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
                    egui::Image::new(egui::load::SizedTexture::new(self.logo_tex.id(), lr.size())).paint_at(ui, lr);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        ui.label(RichText::new("RBX Account Manager").font(sb(18.0)).color(HEADER_TEXT()));
                        ui.add_space(-7.0);
                        ui.label(RichText::new("Local, DPAPI-encrypted").font(rg(12.0)).color(HEADER_SUB()));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let resp = button_ex(ui, "Add account", Some(Icon::Plus), vec2(150.0, 36.0), PRIMARY(), PRIMARY_HI(), ON_PRIMARY(), sb(14.0), true);
                        let pid = ui.make_persistent_id("add_menu");
                        if resp.clicked() {
                            ui.memory_mut(|m| m.toggle_popup(pid));
                        }
                        let mut pick = 0;
                        egui::popup_below_widget(ui, pid, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            if menu_item(ui, "Use a cookie", "One account, paste .ROBLOSECURITY").clicked() {
                                pick = 1;
                            }
                            if menu_item(ui, "Many cookies at once", "One cookie per line").clicked() {
                                pick = 2;
                            }
                            if menu_item(ui, "Import from Roblox Account Manager", "Use your AccountData.json file").clicked() {
                                pick = 3;
                            }
                        });
                        match pick {
                            1 => self.open_add(),
                            2 => self.modal = Modal::Bulk { text: String::new(), progress: None, error: None },
                            3 => self.open_import(),
                            _ => {}
                        }
                        ui.add_space(4.0);
                        ui.scope(|ui| {
                            ui.set_width(280.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.search)
                                    .id(egui::Id::new("search"))
                                    .hint_text(RichText::new("Search accounts, groups, notes   Ctrl+F").color(FAINT()))
                                    .desired_width(f32::INFINITY)
                                    .margin(vec2(10.0, 8.0))
                                    .font(rg(14.0)),
                            );
                        });
                    });
                });
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status")
            .exact_height(34.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(SURFACE()).inner_margin(Margin::symmetric(24, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if let Some(b) = &self.busy {
                        ui.add(egui::Spinner::new().size(13.0).color(TEXT()));
                        ui.label(RichText::new(b).font(rg(12.5)).color(TEXT()));
                    } else {
                        ui.label(RichText::new("Ready").font(rg(12.5)).color(MUTED()));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).font(rg(12.0)).color(FAINT()));
                        ui.add_space(8.0);
                        if self.running > 0 {
                            if ui
                                .add(egui::Label::new(RichText::new("Stop all").font(sb(12.0)).color(RED())).sense(Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.modal = Modal::ConfirmStop(self.running);
                            }
                        }
                        let (dot, txt) = if self.running > 0 { (GREEN(), format!("{} Roblox running", self.running)) } else { (FAINT(), "No Roblox running".into()) };
                        ui.label(RichText::new(txt).font(rg(12.5)).color(MUTED()));
                        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 3.5, dot);
                    });
                });
            });
    }

    // -------------------------------------------------------------- monitor / tray / previews
    /// Push the current settings to the background monitor.
    fn sync_monitor(&mut self) {
        let s = &self.settings;
        let cfg = monitor::Config {
            anti_afk: s.anti_afk,
            afk_minutes: if s.afk_minutes == 0 { 15 } else { s.afk_minutes },
            auto_rejoin: s.auto_rejoin,
            alerts: s.discord_alerts,
            webhook: s.discord_webhook.clone(),
            rpc: s.discord_rpc,
            // the built-in app ID unless the user set their own
            rpc_app_id: if s.discord_app_id.trim().is_empty() { discord::DEFAULT_APP_ID.to_string() } else { s.discord_app_id.clone() },
            accounts: self.accounts.len(),
            fps_cap: s.fps_cap,
        };
        if let Ok(mut sh) = self.shared.lock() {
            sh.cfg = cfg;
        }
    }

    fn poll_monitor(&mut self) {
        let mut evs = Vec::new();
        if let Some(rx) = &self.mon_rx {
            while let Ok(e) = rx.try_recv() {
                evs.push(e);
            }
        }
        for e in evs {
            match e {
                monitor::Event::Toast(t, err) => self.toast(t, if err { ToastKind::Err } else { ToastKind::Ok }, 7.0),
                monitor::Event::AntiAfk(v) => {
                    self.settings.anti_afk = v;
                    self.save_settings();
                    self.toast(if v { "Anti-AFK on." } else { "Anti-AFK off." }, ToastKind::Info, 3.0);
                }
            }
        }
    }

    /// Close button: hide to tray instead of quitting (if enabled).
    fn handle_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        let quitting = tray::QUIT.load(Ordering::Relaxed);
        if self.settings.close_to_tray && !quitting && !self.demo && self.hwnd != 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.unregister_thumbs();
            win::show_window(self.hwnd, false);
            win::trim_memory();
        }
    }

    fn unregister_thumbs(&mut self) {
        for (_, t) in self.thumbs.drain() {
            win::thumb_unregister(t);
        }
    }

    /// Position the DWM live previews over the cards drawn this frame.
    fn sync_thumbs(&mut self, ctx: &egui::Context) {
        let targets = std::mem::take(&mut self.thumb_targets);
        if self.hwnd == 0 || targets.is_empty() {
            if !self.thumbs.is_empty() {
                self.unregister_thumbs();
            }
            return;
        }
        // previews are drawn by Windows on top of everything, so hide them under dialogs, menus and toasts
        let overlay = !matches!(self.modal, Modal::None) || ctx.memory(|m| m.any_popup_open());
        let screen = ctx.screen_rect();
        let toast_zone = if self.toasts.is_empty() {
            Rect::NOTHING
        } else {
            Rect::from_min_max(pos2(screen.right() - 420.0, screen.bottom() - 70.0 - 64.0 * self.toasts.len().min(4) as f32), screen.max)
        };
        let ppp = ctx.pixels_per_point();
        let wanted: HashSet<isize> = targets.iter().map(|(h, _, _)| *h).collect();
        let stale: Vec<isize> = self.thumbs.keys().filter(|h| !wanted.contains(h)).cloned().collect();
        for h in stale {
            if let Some(t) = self.thumbs.remove(&h) {
                win::thumb_unregister(t);
            }
        }
        for (h, r, minimized) in targets {
            let t = match self.thumbs.get(&h) {
                Some(t) => *t,
                None => match win::thumb_register(self.hwnd, h) {
                    Some(t) => {
                        self.thumbs.insert(h, t);
                        t
                    }
                    None => continue,
                },
            };
            // keep the source aspect ratio inside the card
            let (sw, sh) = win::thumb_source_size(t).unwrap_or((16, 9));
            let scale = (r.width() / sw as f32).min(r.height() / sh as f32);
            let fit = Rect::from_center_size(r.center(), vec2(sw as f32 * scale, sh as f32 * scale));
            let px = [
                (fit.left() * ppp).round() as i32,
                (fit.top() * ppp).round() as i32,
                (fit.right() * ppp).round() as i32,
                (fit.bottom() * ppp).round() as i32,
            ];
            let visible = !overlay && !minimized && !fit.intersects(toast_zone);
            win::thumb_update(t, px, visible);
        }
    }

    fn clients_view(&mut self, ui: &mut egui::Ui) {
        if self.windows.is_empty() {
            Frame::new().fill(SURFACE()).corner_radius(CornerRadius::same(12)).stroke(line(1.0, BORDER())).inner_margin(Margin::same(40)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No Roblox clients running").font(sb(17.0)).color(TEXT()));
                    ui.label(RichText::new("Launch an account and a live preview of it shows up here.").color(MUTED()));
                });
            });
            return;
        }
        let n = self.windows.len();
        let cols = match n {
            1 => 1,
            2..=4 => 2,
            5..=9 => 3,
            _ => 4,
        };
        let rows = n.div_ceil(cols);
        let gap = 12.0;
        let avail = ui.available_size();
        let card_w = (avail.x - gap * (cols as f32 - 1.0)) / cols as f32;
        let head = 48.0;
        let card_h = ((avail.y - gap * (rows as f32 - 1.0)) / rows as f32).min(card_w * 9.0 / 16.0 + head).max(head + 60.0);
        let windows = self.windows.clone();
        let mut focus = None;
        let mut close = None;
        for row in windows.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for w in row {
                    let (rect, _) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::hover());
                    let p = ui.painter().clone();
                    p.rect(rect, CornerRadius::same(12), SURFACE(), line(1.0, BORDER()), StrokeKind::Inside);
                    let name = self.shared.lock().ok().and_then(|s| s.username_for_pid(w.pid));
                    let title = name.as_deref().map(|u| self.accounts.iter().find(|a| a.username == u).map(|a| a.title().to_string()).unwrap_or(u.to_string()));
                    let title = title.unwrap_or_else(|| "Roblox client".into());
                    let sub = match &name {
                        Some(u) => format!("@{u}  •  PID {}", w.pid),
                        None => format!("Not launched from this app  •  PID {}", w.pid),
                    };
                    let hy = rect.top() + head / 2.0;
                    let bw = 70.0;
                    paint_text(ui, &p, pos2(rect.left() + 14.0, hy - 8.0), Align::Min, &title, sb(13.5), TEXT(), card_w - 2.0 * bw - 40.0);
                    paint_text(ui, &p, pos2(rect.left() + 14.0, hy + 9.0), Align::Min, &sub, rg(11.5), FAINT(), card_w - 2.0 * bw - 40.0);
                    let close_r = Rect::from_min_size(pos2(rect.right() - 12.0 - bw, hy - 14.0), vec2(bw, 28.0));
                    let focus_r = Rect::from_min_size(pos2(close_r.left() - 8.0 - bw, hy - 14.0), vec2(bw, 28.0));
                    if ui.put(focus_r, |ui: &mut egui::Ui| ghost(ui, "Focus", vec2(bw, 28.0))).clicked() {
                        focus = Some(w.hwnd);
                    }
                    if ui.put(close_r, |ui: &mut egui::Ui| danger(ui, "Close", vec2(bw, 28.0))).clicked() {
                        close = Some(w.hwnd);
                    }
                    let preview = Rect::from_min_max(pos2(rect.left() + 8.0, rect.top() + head), pos2(rect.right() - 8.0, rect.bottom() - 8.0));
                    p.rect_filled(preview, CornerRadius::same(8), FIELD());
                    if w.minimized {
                        paint_text(ui, &p, preview.center(), Align::Center, "Minimized. Click Focus to restore.", rg(12.5), FAINT(), preview.width());
                    }
                    self.thumb_targets.push((w.hwnd, preview.shrink(2.0), w.minimized));
                }
            });
            ui.add_space(gap - 8.0);
        }
        if let Some(h) = focus {
            win::focus_window(h);
        }
        if let Some(h) = close {
            win::close_window(h);
            self.next_windows = 0.0;
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        let section = |ui: &mut egui::Ui, title: &str, sub: &str| {
            ui.label(RichText::new(title).font(sb(15.0)).color(TEXT()));
            ui.add_space(-6.0);
            ui.label(RichText::new(sub).font(rg(12.0)).color(MUTED()));
            ui.add_space(4.0);
        };

        // ---- Appearance
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "Theme", "Graphite: original dark. Terrava: bright lilac & cobalt. Neon: black & lime terminal.");
            let cur = THEME.load(std::sync::atomic::Ordering::Relaxed) as usize;
            if let Some(i) = segmented(ui, ui.id().with("theme"), &["Graphite", "Terrava", "Neon"], cur) {
                let name = ["graphite", "terrava", "neon"][i];
                self.settings.theme = name.into();
                set_theme(name);
                setup_style(ui.ctx());
                if self.hwnd != 0 {
                    win::set_title_bar_dark(self.hwnd as *mut std::ffi::c_void, !is_light());
                }
                changed = true;
            }
        });
        ui.add_space(10.0);

        // ---- FPS limit
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            section(ui, "FPS limit", "Applied to Roblox before every launch and rejoin");
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                for (label, v) in [("Default", 0u32), ("30", 30), ("60", 60), ("120", 120), ("144", 144), ("240", 240), ("Unlimited", 9999)] {
                    if chip(ui, label, self.settings.fps_cap == v).clicked() && self.settings.fps_cap != v {
                        self.settings.fps_cap = v;
                        changed = true;
                        match fps::apply(v) {
                            Ok(_) => self.toast(
                                if v == 0 { "FPS limit reset to the Roblox default.".to_string() } else { format!("FPS limit set to {v}. Restart running Roblox clients to apply it.") },
                                ToastKind::Ok,
                                5.0,
                            ),
                            Err(e) => self.toast(e, ToastKind::Err, 6.0),
                        }
                    }
                }
            });
            ui.label(RichText::new("Sets Roblox's own Maximum Frame Rate (FramerateCap) plus the TargetFps fast flag. Low FPS on alts saves a lot of CPU/GPU. Bloxstrap/Fishstrap users: set it there instead.").font(rg(11.5)).color(FAINT()));
        });
        ui.add_space(10.0);

        // ---- Anti-AFK + auto-rejoin
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let on = self.settings.anti_afk;
            let mut flip = false;
            setting_row(ui, "Anti-AFK", "Stops Roblox kicking idle clients", 50.0, |ui| {
                flip = toggle(ui, on).clicked();
            });
            if flip {
                self.settings.anti_afk = !on;
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("Every").font(rg(12.5)).color(MUTED()));
                for m in [5u32, 10, 15, 19] {
                    if chip(ui, &format!("{m} min"), self.settings.afk_minutes == m).clicked() {
                        self.settings.afk_minutes = m;
                        changed = true;
                    }
                }
            });
            let last = self.shared.lock().ok().and_then(|s| s.last_nudge.clone());
            let status = match (self.settings.anti_afk, last) {
                (true, Some(t)) => format!("Last nudge {t}. Waits for a short pause in your typing first."),
                (true, None) => "Waits for a short pause in your typing, then briefly focuses each client.".to_string(),
                (false, _) => "Off".to_string(),
            };
            ui.label(RichText::new(status).font(rg(11.5)).color(FAINT()));
            ui.add_space(4.0);
            divider(ui);
            let on = self.settings.auto_rejoin;
            let mut flip = false;
            setting_row(ui, "Auto-rejoin (beta)", "Rejoins after disconnects and kicks", 50.0, |ui| {
                flip = toggle(ui, on).clicked();
            });
            if flip {
                self.settings.auto_rejoin = !on;
                changed = true;
            }
            let watching = self.shared.lock().map(|s| s.clients.len()).unwrap_or(0);
            ui.label(
                RichText::new(format!(
                    "Reads each client's Roblox log. Only clients launched from this app are watched ({watching} now). Stops after 3 rejoins in 10 minutes. Turn on Multi Roblox when running several clients."
                ))
                .font(rg(11.5))
                .color(FAINT()),
            );
        });
        ui.add_space(10.0);

        // ---- Window
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let on = self.settings.close_to_tray;
            let mut flip = false;
            setting_row(ui, "Close to tray", "The X button hides the app to the tray", 50.0, |ui| {
                flip = toggle(ui, on).clicked();
            });
            if flip {
                self.settings.close_to_tray = !on;
                changed = true;
            }
            ui.label(RichText::new("Anti-AFK and auto-rejoin keep running in the tray. Right-click the tray icon for Stop all and Quit.").font(rg(11.5)).color(FAINT()));
        });
        ui.add_space(10.0);

        // ---- Discord
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let on = self.settings.discord_alerts;
            let mut flip = false;
            setting_row(ui, "Discord alerts", "Disconnects, rejoins, finished batches", 50.0, |ui| {
                flip = toggle(ui, on).clicked();
            });
            if flip {
                self.settings.discord_alerts = !on;
                changed = true;
            }
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.settings.discord_webhook)
                    .hint_text(RichText::new("Webhook URL (Channel settings > Integrations)").color(FAINT()))
                    .password(true)
                    .desired_width(f32::INFINITY)
                    .margin(vec2(10.0, 8.0)),
            );
            if r.changed() {
                changed = true;
            }
            if ghost(ui, "Send test message", vec2(150.0, 30.0)).clicked() {
                let url = self.settings.discord_webhook.clone();
                let tx = self.tx.clone();
                let ctx = ui.ctx().clone();
                std::thread::spawn(move || {
                    let msg = match discord::send_webhook(&url, ":wave: Test message from RBX Account Manager.") {
                        Ok(()) => Msg::Toast("Test message sent to Discord.".into(), ToastKind::Ok),
                        Err(e) => Msg::Toast(e, ToastKind::Err),
                    };
                    let _ = tx.send(msg);
                    ctx.request_repaint();
                });
            }
            ui.add_space(4.0);
            divider(ui);
            let on = self.settings.discord_rpc;
            let mut flip = false;
            setting_row(ui, "Rich Presence", "Show the app on your Discord profile", 50.0, |ui| {
                flip = toggle(ui, on).clicked();
            });
            if flip {
                self.settings.discord_rpc = !on;
                changed = true;
            }
            let r = field(ui, &mut self.settings.discord_app_id, "Custom Application ID (optional)", false);
            if r.changed() {
                // keep digits only, so a pasted ID with spaces still works
                self.settings.discord_app_id.retain(|c| c.is_ascii_digit());
                changed = true;
            }
            let status = self.shared.lock().map(|s| s.rpc_status.clone()).unwrap_or_default();
            if self.settings.discord_rpc && !status.is_empty() {
                let (text, col) = match status.strip_prefix("OK: ") {
                    Some(t) => (t.to_string(), GREEN()),
                    None => (status, AMBER()),
                };
                ui.label(RichText::new(text).font(rg(12.5)).color(col));
            }
            ui.label(
                RichText::new("Works out of the box with the built-in RBX Account Manager app. Leave the ID empty, or paste your own Application ID from discord.com/developers to show a different name. Discord must be running.")
                    .font(rg(11.5))
                    .color(FAINT()),
            );
        });
        if changed {
            self.save_settings();
            self.sync_monitor();
        }
    }

    // -------------------------------------------------------------- ui: side panel
    fn side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("side")
            .exact_width(372.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG()).inner_margin(Margin { left: 0, right: 24, top: 18, bottom: 16 }))
            .show(ctx, |ui| {
                // tab bar: 32 px tall, aligned with the account list toolbar on the left
                let w = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
                ui.painter().rect(rect, CornerRadius::same(8), SURFACE(), line(1.0, BORDER()), StrokeKind::Inside);
                let tabs = [(Tab::Play, "Play"), (Tab::Servers, "Servers"), (Tab::Detail, "Account"), (Tab::Settings, "Settings")];
                let tw = (w - 6.0) / tabs.len() as f32;
                for (i, (t, label)) in tabs.iter().enumerate() {
                    let r = Rect::from_min_size(rect.min + vec2(3.0 + i as f32 * tw, 3.0), vec2(tw, 26.0));
                    let resp = ui.interact(r, ui.id().with(("tab", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                    let active = self.tab == *t;
                    if active {
                        ui.painter().rect_filled(r, CornerRadius::same(6), CARD_HOVER());
                    }
                    paint_text(ui, ui.painter(), r.center(), Align::Center, label, if active { sb(13.0) } else { rg(13.0) }, if active { TEXT() } else { MUTED() }, tw);
                    if resp.clicked() {
                        self.tab = *t;
                    }
                }
                ui.add_space(12.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Play => self.play_tab(ui),
                    Tab::Servers => self.servers_tab(ui),
                    Tab::Detail => self.detail_tab(ui),
                    Tab::Settings => self.settings_tab(ui),
                });
            });
    }

    fn play_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;

            // mode: join game / home screen
            let cur = if self.settings.launch_home { 1 } else { 0 };
            if let Some(i) = segmented(ui, ui.id().with("mode"), &["Join game", "Home screen"], cur) {
                self.settings.launch_home = i == 1;
                self.save_settings();
            }
            ui.add_space(8.0);
            if self.settings.launch_home {
                Frame::new().fill(CARD()).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("Opens Roblox on the home screen, already signed in, without joining a game. Pick a game from inside Roblox.").font(rg(12.5)).color(MUTED()));
                });
                ui.add_space(10.0);
            } else {
            // ---- pinned games: click to select, × to unpin
            let pins = self.settings.favorite_games.clone();
            if !pins.is_empty() {
                caption(ui, "PINNED GAMES");
                ui.spacing_mut().item_spacing.y = 2.0;
                let mut unpin: Option<String> = None;
                for pin in &pins {
                    let place = roblox::parse_game(pin, "").ok().map(|g| g.place_id).filter(|p| !p.is_empty());
                    let name = place.as_ref().and_then(|p| self.settings.game_names.get(p).cloned());
                    let active = self.game.trim().eq_ignore_ascii_case(pin);
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                    let xr = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), Vec2::splat(24.0));
                    let xresp = ui.interact(xr, ui.id().with(("unpin", pin)), Sense::click()).on_hover_text("Unpin");
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let p = ui.painter();
                    let bg = if active { CARD_HOVER() } else if resp.hovered() || xresp.hovered() { CARD() } else { Color32::TRANSPARENT };
                    p.rect(rect, CornerRadius::same(8), bg, if active { line(1.0, BORDER_HI()) } else { Stroke::NONE }, StrokeKind::Inside);
                    // game icon
                    let ir = Rect::from_min_size(rect.min + vec2(7.0, 7.0), Vec2::splat(32.0));
                    p.rect_filled(ir, CornerRadius::same(7), CARD_HOVER());
                    if let Some(tex) = place.as_ref().and_then(|pl| self.game_icons.get(pl)) {
                        egui::Image::new(egui::load::SizedTexture::new(tex.id(), ir.size())).corner_radius(CornerRadius::same(7)).paint_at(ui, ir);
                    } else {
                        let letter: String = name.as_deref().unwrap_or(pin).chars().filter(|c| c.is_alphanumeric()).take(1).collect::<String>().to_uppercase();
                        paint_text(ui, p, ir.center(), Align::Center, &letter, sb(13.0), MUTED(), 30.0);
                    }
                    let tx = ir.right() + 10.0;
                    let maxw = rect.right() - 40.0 - tx;
                    let (title, sub) = match (&place, &name) {
                        (Some(pl), Some(n)) => (n.clone(), format!("Place {pl}")),
                        (Some(pl), None) => (format!("Place {pl}"), "Loading game name...".to_string()),
                        (None, _) if pin.starts_with('@') => (pin.clone(), "Join this player's game".to_string()),
                        (None, _) => (pin.clone(), "Link".to_string()),
                    };
                    paint_text(ui, p, pos2(tx, rect.center().y - 8.0), Align::Min, &title, sb(13.0), TEXT(), maxw);
                    paint_text(ui, p, pos2(tx, rect.center().y + 9.0), Align::Min, &sub, rg(11.5), FAINT(), maxw);
                    if resp.hovered() || xresp.hovered() || active {
                        if xresp.hovered() {
                            p.rect_filled(xr, CornerRadius::same(6), RED_BG());
                        }
                        paint_x(p, xr.center(), 4.0, if xresp.hovered() { RED() } else { FAINT() });
                    }
                    if xresp.clicked() {
                        unpin = Some(pin.clone());
                    } else if resp.clicked() {
                        self.game = pin.clone();
                    }
                }
                ui.spacing_mut().item_spacing.y = 6.0;
                if let Some(u) = unpin {
                    self.settings.favorite_games.retain(|x| x != &u);
                    self.save_settings();
                }
                ui.add_space(8.0);
            }

            // ---- game input + pin button
            let g = self.game.trim().to_string();
            let pinned = !g.is_empty() && self.settings.favorite_games.iter().any(|x| x.eq_ignore_ascii_case(&g));
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
            paint_text(ui, ui.painter(), r.left_center(), Align::Min, if pins.is_empty() { "GAME" } else { "OR ENTER A GAME" }, label_font(11.5), MUTED(), 200.0);
            if !g.is_empty() {
                let label = if pinned { "Pinned" } else { "+ Pin this game" };
                let w = text_width(ui, label, rg(12.0));
                let lr = Rect::from_min_size(pos2(r.right() - w, r.top()), vec2(w, r.height()));
                let resp = ui.interact(lr.expand(4.0), ui.id().with("pingame"), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
                let col = if pinned { GREEN() } else if resp.hovered() { TEXT() } else { MUTED() };
                paint_text(ui, ui.painter(), pos2(lr.left(), r.center().y), Align::Min, label, rg(12.0), col, 200.0);
                if resp.clicked() {
                    if pinned {
                        self.settings.favorite_games.retain(|x| !x.eq_ignore_ascii_case(&g));
                    } else {
                        self.settings.favorite_games.push(g.clone());
                        self.toast("Game pinned. Click it under Pinned games to select it.", ToastKind::Ok, 3.0);
                    }
                    self.save_settings();
                }
            }
            field(ui, &mut self.game, "Link game, Place ID, link share, @username", false);
            let recents: Vec<String> = self
                .settings
                .recent_games
                .iter()
                .filter(|r| !self.settings.favorite_games.iter().any(|f| f.eq_ignore_ascii_case(r)))
                .take(4)
                .cloned()
                .collect();
            if !recents.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for g in &recents {
                        let label = match roblox::parse_game(g, "").ok().map(|x| x.place_id).and_then(|pl| self.settings.game_names.get(&pl).cloned()) {
                            Some(n) => n,
                            None => g.clone(),
                        };
                        let short: String = if label.chars().count() > 18 { label.chars().take(16).collect::<String>() + "…" } else { label };
                        if chip(ui, &short, self.game.trim() == g).on_hover_text(g).clicked() {
                            self.game = g.clone();
                        }
                    }
                });
            }
            ui.add_space(8.0);
            caption(ui, "JOB ID  (optional)");
            field(ui, &mut self.job, "Empty = random server", false);
            ui.add_space(10.0);
            }

            divider(ui);
            let mut flip_multi = false;
            let multi_on = self.multi_on();
            let multi_sub = if self.multi_external { "On, handled by another app" } else { "Run several clients at once" };
            setting_row(ui, "Multi Roblox", multi_sub, 50.0, |ui| {
                if toggle(ui, multi_on).clicked() {
                    flip_multi = true;
                }
            });
            if flip_multi {
                self.set_multi(!multi_on);
            }
            divider(ui);
            let v = self.settings.stagger_secs;
            let mut nv = v;
            setting_row(ui, "Launch delay", "Between accounts in a batch", 124.0, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if icon_button(ui, Icon::Plus, vec2(28.0, 28.0)).clicked() {
                    nv = (v + 1).min(30);
                }
                let (r, _) = ui.allocate_exact_size(vec2(52.0, 28.0), Sense::hover());
                paint_text(ui, ui.painter(), r.center(), Align::Center, &format!("{v} s"), sb(13.0), TEXT(), 60.0);
                if icon_button(ui, Icon::Minus, vec2(28.0, 28.0)).clicked() {
                    nv = v.saturating_sub(1).max(2);
                }
            });
            if nv != v {
                self.settings.stagger_secs = nv;
                self.save_settings();
            }
            divider(ui);
            ui.add_space(10.0);

            let n = self.selected.len();
            let names: Vec<String> = self.selected.iter().filter_map(|id| self.account(id).map(|a| a.title().to_string())).collect();
            let summary = if n == 0 { "Select accounts in the list on the left".to_string() } else { format!("{n} selected: {}", names.join(", ")) };
            ui.label(elide(ui, &summary, rg(12.5), if n == 0 { FAINT() } else { MUTED() }, ui.available_width()));
            if self.cancel.is_some() {
                if danger(ui, "Cancel batch", vec2(ui.available_width(), 44.0)).clicked() {
                    if let Some(c) = &self.cancel {
                        c.store(true, Ordering::Relaxed);
                    }
                }
            } else {
                let label = match (self.settings.launch_home, n > 1) {
                    (true, true) => format!("Open {n} accounts to home"),
                    (true, false) => "Open home screen".to_string(),
                    (false, true) => format!("Launch {n} accounts"),
                    (false, false) => "Launch".to_string(),
                };
                if primary(ui, &label, vec2(ui.available_width(), 44.0), n > 0 && self.busy.is_none()).clicked() {
                    self.launch(&ctx);
                }
            }
            ui.vertical_centered(|ui| ui.label(RichText::new("Ctrl+Enter").font(rg(11.5)).color(FAINT())));
        });
        ui.add_space(12.0);
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let w = (ui.available_width() - 8.0) / 2.0;
            ui.horizontal(|ui| {
                if ghost(ui, "Check all accounts", vec2(w, 34.0)).clicked() {
                    self.check_all(&ctx);
                }
                if ghost(ui, "Stop all Roblox", vec2(w, 34.0)).clicked() {
                    let n = win::roblox_pids().len();
                    if n == 0 {
                        self.toast("No Roblox is running.", ToastKind::Info, 3.0);
                    } else {
                        self.modal = Modal::ConfirmStop(n);
                    }
                }
            });
        });
    }

    fn servers_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new("Public servers").font(sb(15.0)).color(TEXT()));
            let place_now = roblox::parse_game(&self.game, "").ok().map(|g| g.place_id).filter(|p| !p.is_empty());
            let sub = match &place_now {
                Some(p) => format!("Place {p}"),
                None => "Enter a game in the Play tab first".into(),
            };
            ui.label(RichText::new(sub).font(rg(12.5)).color(MUTED()));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let label = if self.servers_loading { "Loading..." } else { "Load servers" };
                if primary(ui, label, vec2(120.0, 32.0), !self.servers_loading && place_now.is_some()).clicked() {
                    self.load_servers(&ctx);
                }
                if self.servers_loading {
                    ui.add(egui::Spinner::new().color(TEXT()));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if chip(ui, "Ping", self.server_sort == ServerSort::Ping).clicked() {
                        self.server_sort = ServerSort::Ping;
                    }
                    if chip(ui, "Players", self.server_sort == ServerSort::Players).clicked() {
                        self.server_sort = ServerSort::Players;
                    }
                });
            });
            if let Some(e) = &self.servers_error {
                ui.label(RichText::new(e).font(rg(12.5)).color(RED()));
            }
        });
        ui.add_space(8.0);
        if self.servers.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("No servers loaded yet.").color(FAINT())));
            return;
        }
        let mut list = self.servers.clone();
        match self.server_sort {
            ServerSort::Players => list.sort_by(|a, b| b.playing.cmp(&a.playing)),
            ServerSort::Ping => list.sort_by_key(|s| s.ping.unwrap_or(i64::MAX)),
        }
        ui.label(RichText::new(format!("{} servers  •  place {}", list.len(), self.servers_place)).font(rg(12.0)).color(FAINT()));
        let mut join: Option<String> = None;
        let mut pick: Option<String> = None;
        for s in list.iter().take(150) {
            Frame::new().fill(SURFACE()).corner_radius(CornerRadius::same(10)).inner_margin(Margin::symmetric(12, 8)).stroke(line(1.0, BORDER())).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{}/{} players", s.playing, s.max)).font(sb(13.0)).color(TEXT()));
                        ui.add_space(-6.0);
                        let (r, _) = ui.allocate_exact_size(vec2(110.0, 4.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(2), BORDER_HI());
                        let frac = if s.max > 0 { (s.playing as f32 / s.max as f32).clamp(0.0, 1.0) } else { 0.0 };
                        ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * frac, 4.0)), CornerRadius::same(2), TEXT());
                        let ping = s.ping.map(|p| format!("{p} ms")).unwrap_or_else(|| "- ms".into());
                        ui.label(RichText::new(ping).font(rg(12.0)).color(MUTED()));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary(ui, "Join", vec2(60.0, 30.0), !self.selected.is_empty() && self.busy.is_none()).on_hover_text("Launch the selected accounts into this server").clicked() {
                            join = Some(s.id.clone());
                        }
                        if ghost(ui, "Use ID", vec2(76.0, 30.0)).on_hover_text("Fill in the Job ID on the Play tab").clicked() {
                            pick = Some(s.id.clone());
                        }
                    });
                });
            });
            ui.add_space(2.0);
        }
        if let Some(id) = pick {
            self.job = id;
            self.tab = Tab::Play;
            self.toast("Job ID filled in. Launch from the Play tab.", ToastKind::Ok, 3.0);
        }
        if let Some(id) = join {
            self.job = id;
            if self.game.trim() != self.servers_place {
                self.game = self.servers_place.clone();
            }
            // Joining a server always joins the game, whatever mode the Play tab is in
            self.launch_mode(&ctx, false);
        }
    }

    fn detail_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.selected.len() != 1 {
            panel_frame().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Select one account").font(sb(15.0)).color(TEXT()));
                ui.label(RichText::new("Click an account to see its details, Robux, status and notes.").color(MUTED()));
            });
            return;
        }
        let id = self.selected[0].clone();
        let Some(acc) = self.account(&id).cloned() else { return };
        if self.edit.as_ref().map(|e| e.id != id).unwrap_or(true) {
            self.edit = Some(EditBuf { id: id.clone(), alias: acc.alias.clone(), group: acc.group.clone(), note: acc.note.clone() });
        }
        if !self.details.contains_key(&id) {
            self.load_details(&ctx, &id);
        }
        let det = self.details.get(&id).cloned();
        let pres = self.presence.get(&id).cloned();
        let groups = self.groups();
        let mut act = None;

        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                if let Some(tex) = self.avatars.get(&id) {
                    ui.painter().circle_filled(r.center(), 28.0, CARD_HOVER());
                    egui::Image::new(egui::load::SizedTexture::new(tex.id(), r.size())).corner_radius(CornerRadius::same(28)).paint_at(ui, r);
                } else {
                    ui.painter().circle_filled(r.center(), 28.0, CARD_HOVER());
                    paint_text(ui, ui.painter(), r.center(), Align::Center, &initials(&acc), sb(18.0), MUTED(), 60.0);
                }
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(if acc.display_name.is_empty() { &acc.username } else { &acc.display_name }).font(sb(16.0)).color(TEXT()));
                    ui.add_space(-7.0);
                    ui.label(RichText::new(format!("@{}  •  ID {}", acc.username, acc.user_id)).font(rg(12.5)).color(MUTED()));
                    let (txt, col) = match pres.as_ref().map(|p| p.kind) {
                        Some(2) => (format!("In game: {}", pres.as_ref().map(|p| p.location.as_str()).unwrap_or("")), GREEN()),
                        Some(1) => ("Online".to_string(), BLUE()),
                        Some(3) => ("In Studio".to_string(), AMBER()),
                        _ if !acc.is_ok() => ("Cookie expired".to_string(), RED()),
                        _ => ("Offline".to_string(), FAINT()),
                    };
                    ui.add_space(-5.0);
                    ui.label(RichText::new(txt).font(rg(12.5)).color(col));
                });
            });
            ui.add_space(8.0);
            // statistik
            let stat = |v: Option<i64>| v.map(thousands).unwrap_or_else(|| "–".into());
            let items = [
                ("Robux", det.as_ref().and_then(|d| d.robux).map(thousands).unwrap_or_else(|| "–".into())),
                ("Premium", det.as_ref().and_then(|d| d.premium).map(|p| if p { "Yes" } else { "No" }.to_string()).unwrap_or_else(|| "–".into())),
                ("Friends", stat(det.as_ref().and_then(|d| d.friends))),
                ("Followers", stat(det.as_ref().and_then(|d| d.followers))),
                ("Following", stat(det.as_ref().and_then(|d| d.following))),
                ("Joined", det.as_ref().map(|d| d.created.clone()).filter(|s| !s.is_empty()).unwrap_or_else(|| "–".into())),
            ];
            let cw = (ui.available_width() - 16.0) / 3.0;
            for row in items.chunks(3) {
                ui.horizontal(|ui| {
                    for (k, v) in row {
                        let (r, _) = ui.allocate_exact_size(vec2(cw, 48.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(r, CornerRadius::same(8), CARD());
                        paint_text(ui, p, r.min + vec2(10.0, 15.0), Align::Min, k, rg(11.5), FAINT(), cw - 20.0);
                        paint_text(ui, p, r.min + vec2(10.0, 32.0), Align::Min, v, sb(14.0), TEXT(), cw - 20.0);
                    }
                });
            }
            ui.horizontal(|ui| {
                let loading = self.details_loading.contains(&id);
                let l = if loading { "Loading..." } else { "Refresh" };
                if ui.add(egui::Label::new(RichText::new(l).font(rg(12.0)).color(MUTED())).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !loading {
                    self.details.remove(&id);
                }
                if det.as_ref().map(|d| d.banned).unwrap_or(false) {
                    ui.label(RichText::new("This account is banned").font(sb(12.0)).color(RED()));
                }
            });
        });
        ui.add_space(10.0);

        panel_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            let Some(buf) = self.edit.as_mut() else { return };
            caption(ui, "ALIAS");
            field(ui, &mut buf.alias, "Display name in the app", false);
            ui.add_space(4.0);
            caption(ui, "GROUP");
            field(ui, &mut buf.group, "e.g. Farming, Trade", false);
            if !groups.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                    for g in &groups {
                        if chip(ui, g, buf.group.trim().eq_ignore_ascii_case(g)).clicked() {
                            buf.group = g.clone();
                        }
                    }
                });
            }
            ui.add_space(4.0);
            caption(ui, "NOTES");
            ui.add(egui::TextEdit::multiline(&mut buf.note).desired_rows(3).desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)).hint_text(RichText::new("Private notes").color(FAINT())));
            let dirty = buf.alias != acc.alias || buf.group != acc.group || buf.note != acc.note;
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if primary(ui, "Save", vec2(100.0, 32.0), dirty).clicked() {
                    act = Some(0);
                }
                if dirty {
                    ui.label(RichText::new("Unsaved changes").font(rg(12.0)).color(AMBER()));
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Favorite").color(TEXT()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if toggle(ui, acc.favorite).clicked() {
                        act = Some(1);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("MAIN account").color(TEXT()));
                    ui.add_space(-7.0);
                    ui.label(RichText::new("Skipped by \"select all\"").font(rg(12.0)).color(MUTED()));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if toggle(ui, acc.main).clicked() {
                        act = Some(2);
                    }
                });
            });
            ui.add_space(8.0);
            let w = (ui.available_width() - 8.0) / 2.0;
            ui.horizontal(|ui| {
                if ghost(ui, "Copy username", vec2(w, 32.0)).clicked() {
                    act = Some(3);
                }
                if ghost(ui, "Open profile", vec2(w, 32.0)).clicked() {
                    act = Some(4);
                }
            });
            if danger(ui, "Delete account", vec2(ui.available_width(), 32.0)).clicked() {
                act = Some(5);
            }
        });

        match act {
            Some(0) => {
                if let Some(buf) = self.edit.take() {
                    if let Some(a) = self.account_mut(&id) {
                        a.alias = buf.alias.trim().chars().take(40).collect();
                        a.group = buf.group.trim().chars().take(30).collect();
                        a.note = buf.note.chars().take(1000).collect();
                    }
                    self.save();
                    self.toast("Changes saved.", ToastKind::Ok, 3.0);
                }
            }
            Some(1) => self.apply(&ctx, CardAction::ToggleFav(id)),
            Some(2) => self.apply(&ctx, CardAction::ToggleMain(id)),
            Some(3) => self.apply(&ctx, CardAction::Copy(id)),
            Some(4) => self.apply(&ctx, CardAction::Profile(id)),
            Some(5) => self.apply(&ctx, CardAction::Delete(id)),
            _ => {}
        }
    }

    // -------------------------------------------------------------- ui: account list
    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::new().fill(BG()).inner_margin(Margin { left: 24, right: 16, top: 18, bottom: 16 }))
            .show(ctx, |ui| {
                // toolbar: 32 px tall, aligned with the tab bar on the right
                let w = ui.available_width();
                let counts = (
                    self.accounts.len(),
                    self.accounts.iter().filter(|a| a.favorite).count(),
                    self.accounts.iter().filter(|a| a.main).count(),
                    self.accounts.iter().filter(|a| !a.is_ok()).count(),
                );
                let groups = self.groups();
                ui.allocate_ui_with_layout(vec2(w, 32.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    // view switch: account list / live client previews
                    let n_clients = self.windows.len();
                    if chip(ui, "Accounts", self.view == View::Accounts).clicked() {
                        self.view = View::Accounts;
                    }
                    if chip(ui, &format!("Clients  {n_clients}"), self.view == View::Clients).clicked() {
                        self.view = View::Clients;
                        self.next_windows = 0.0;
                    }
                    let (sep, _) = ui.allocate_exact_size(vec2(9.0, 20.0), Sense::hover());
                    ui.painter().vline(sep.center().x, sep.y_range(), line(1.0, BORDER_HI()));
                    if self.view == View::Clients {
                        ui.label(RichText::new("Live previews of every running Roblox client").font(rg(12.5)).color(FAINT()));
                        return;
                    }
                    let mut f = vec![(Filter::All, format!("All  {}", counts.0))];
                    for (filter, label, n) in [(Filter::Favorite, "Favorite", counts.1), (Filter::Main, "Main", counts.2), (Filter::Expired, "Expired", counts.3)] {
                        if n > 0 || self.filter == filter {
                            f.push((filter, format!("{label}  {n}")));
                        }
                    }
                    for g in &groups {
                        let n = self.accounts.iter().filter(|a| a.group.trim().eq_ignore_ascii_case(g)).count();
                        f.push((Filter::Group(g.clone()), format!("{g}  {n}")));
                    }
                    for (filter, label) in f {
                        if chip(ui, &label, self.filter == filter).clicked() {
                            self.filter = filter;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !self.selected.is_empty() {
                            if ui
                                .add(egui::Label::new(RichText::new("Clear selection").font(rg(12.5)).color(TEXT())).sense(Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.selected.clear();
                            }
                            ui.label(RichText::new(format!("{} selected  •", self.selected.len())).font(rg(12.5)).color(MUTED()));
                        }
                    });
                });
                ui.add_space(12.0);

                if self.view == View::Clients {
                    self.clients_view(ui);
                    return;
                }
                if self.accounts.is_empty() {
                    self.empty_state(ui);
                    return;
                }
                let ids = self.visible_ids();
                let mut actions = vec![];
                Frame::new().fill(SURFACE()).corner_radius(CornerRadius::same(12)).stroke(line(1.0, BORDER())).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let w = ui.available_width();
                    // column header
                    let (hr, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
                    let c = Cols::new(hr);
                    let p = ui.painter();
                    let cap = |x: f32, t: &str| {
                        paint_text(ui, p, pos2(x, hr.center().y), Align::Min, t, label_font(11.0), FAINT(), 200.0);
                    };
                    cap(c.avatar, "ACCOUNT");
                    if let Some(x) = c.group {
                        cap(x, "GROUP");
                    }
                    cap(c.status, "STATUS");
                    if let Some(x) = c.last {
                        cap(x, "LAST PLAYED");
                    }
                    // select-all checkbox (MAIN accounts are skipped)
                    let selectable: Vec<String> = ids.iter().filter(|id| !self.account(id).map(|a| a.main).unwrap_or(false)).cloned().collect();
                    let all = !selectable.is_empty() && selectable.iter().all(|id| self.selected.contains(id));
                    let some = !self.selected.is_empty() && !all;
                    let cb = Rect::from_center_size(pos2(c.check + 8.0, hr.center().y), Vec2::splat(16.0));
                    let cresp = ui
                        .interact(cb.expand(6.0), ui.id().with("check_all"), Sense::click())
                        .on_hover_text("Select all (except MAIN accounts)")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    paint_checkbox(ui.painter(), cb, all, some, cresp.hovered());
                    if cresp.clicked() {
                        if all {
                            self.selected.clear();
                        } else {
                            self.selected = selectable;
                        }
                    }
                    ui.painter().hline(hr.x_range(), hr.bottom(), line(1.0, BORDER()));

                    if ids.is_empty() {
                        ui.add_space(30.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("No matching accounts.").color(MUTED())));
                        ui.add_space(30.0);
                        return;
                    }
                    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let n = ids.len();
                        for (i, id) in ids.iter().enumerate() {
                            if let Some(acc) = self.account(id) {
                                if let Some(a) = self.account_row(ui, acc, w, i + 1 == n) {
                                    actions.push(a);
                                }
                            }
                        }
                    });
                });
                for a in actions {
                    self.apply(ctx, a);
                }
            });
    }

    fn account_row(&self, ui: &mut egui::Ui, acc: &Account, w: f32, last: bool) -> Option<CardAction> {
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 58.0), Sense::click());
        let c = Cols::new(rect);
        let selected = self.selected.contains(&acc.user_id);
        let hovered = resp.hovered();
        let p = ui.painter().clone();
        let radius = if last { CornerRadius { nw: 0, ne: 0, sw: 12, se: 12 } } else { CornerRadius::ZERO };
        if selected {
            p.rect_filled(rect.shrink2(vec2(1.0, 0.0)), radius, SELECT_BG());
            p.rect_filled(Rect::from_min_size(rect.min + vec2(1.0, 8.0), vec2(2.0, rect.height() - 16.0)), CornerRadius::same(1), PRIMARY());
        } else if hovered {
            p.rect_filled(rect.shrink2(vec2(1.0, 0.0)), radius, CARD_HOVER());
        }
        if !last {
            p.hline(rect.x_range(), rect.bottom(), line(1.0, BORDER()));
        }
        let cy = rect.center().y;

        // checkbox (click = add to / remove from selection)
        let cb = Rect::from_center_size(pos2(c.check + 8.0, cy), Vec2::splat(16.0));
        let cresp = ui.interact(cb.expand(8.0), ui.id().with(("chk", &acc.user_id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        paint_checkbox(&p, cb, selected, false, cresp.hovered() || hovered);

        // avatar + status dot
        let pres = self.presence.get(&acc.user_id);
        let r = 18.0;
        let ac = pos2(c.avatar + r, cy);
        let av = Rect::from_center_size(ac, Vec2::splat(r * 2.0));
        p.circle_filled(ac, r, CARD_HOVER());
        if let Some(tex) = self.avatars.get(&acc.user_id) {
            egui::Image::new(egui::load::SizedTexture::new(tex.id(), av.size())).corner_radius(CornerRadius::same(r as u8)).paint_at(ui, av);
        } else {
            paint_text(ui, &p, ac, Align::Center, &initials(acc), sb(12.5), MUTED(), 40.0);
        }
        let (dot_col, status, status_col) = if !acc.is_ok() {
            (RED(), "Cookie expired".to_string(), RED())
        } else {
            match pres.map(|p| p.kind) {
                Some(2) => {
                    let loc = pres.map(|p| p.location.clone()).unwrap_or_default();
                    (GREEN(), if loc.is_empty() { "In game".to_string() } else { loc }, GREEN())
                }
                Some(1) => (BLUE(), "Online".to_string(), BLUE()),
                Some(3) => (AMBER(), "In Studio".to_string(), AMBER()),
                _ => (FAINT(), "Offline".to_string(), FAINT()),
            }
        };
        let bg_now = if selected { SELECT_BG() } else if hovered { CARD_HOVER() } else { SURFACE() };
        let dot = ac + vec2(r * 0.72, r * 0.72);
        p.circle_filled(dot, 5.5, bg_now);
        p.circle_filled(dot, 3.5, dot_col);

        // name + username, MAIN badge next to the name
        let name_x = av.right() + 12.0;
        let name_w = c.name_end - name_x;
        // two text lines: line 1 centred at cy-9, line 2 at cy+10
        let tw = paint_text(ui, &p, pos2(name_x, cy - 9.0), Align::Min, acc.title(), sb(14.0), TEXT(), name_w - if acc.main { 52.0 } else { 0.0 });
        if acc.main {
            pill(ui, &p, pos2(name_x + tw + 8.0, cy - 9.0 - 9.5), "MAIN", AMBER(), AMBER_BG());
        }
        paint_text(ui, &p, pos2(name_x, cy + 10.0), Align::Min, &format!("@{}", acc.username), rg(12.5), MUTED(), name_w);

        // group
        if let Some(x) = c.group {
            if acc.group.trim().is_empty() {
                paint_text(ui, &p, pos2(x, cy), Align::Min, "—", rg(13.0), FAINT(), 40.0);
            } else {
                let gw = text_width(ui, acc.group.trim(), sb(11.0)).min(c.group_w - 30.0);
                let pr = Rect::from_min_size(pos2(x, cy - 10.0), vec2(gw + 14.0, 20.0));
                p.rect_filled(pr, CornerRadius::same(5), CARD_HOVER());
                paint_text(ui, &p, pr.center(), Align::Center, acc.group.trim(), sb(11.0), MUTED(), c.group_w - 30.0);
            }
        }
        // status
        p.circle_filled(pos2(c.status + 4.0, cy), 3.5, dot_col);
        paint_text(ui, &p, pos2(c.status + 14.0, cy), Align::Min, &status, rg(13.0), status_col, c.status_w - 24.0);
        // last played
        if let Some(x) = c.last {
            let t = if acc.last_played.is_empty() { "—".to_string() } else { acc.last_played.clone() };
            paint_text(ui, &p, pos2(x, cy), Align::Min, &t, rg(12.5), if acc.last_played.is_empty() { FAINT() } else { MUTED() }, 120.0);
        }
        // favorite star
        let sr = Rect::from_center_size(pos2(c.star, cy), vec2(26.0, 26.0));
        let star = ui.interact(sr, ui.id().with(("star", &acc.user_id)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
        if acc.favorite || hovered || star.hovered() {
            let col = if acc.favorite { AMBER() } else if star.hovered() { TEXT() } else { FAINT() };
            paint_star(&p, sr.center(), 7.0, acc.favorite, col);
        }

        if star.clicked() {
            return Some(CardAction::ToggleFav(acc.user_id.clone()));
        }
        if cresp.clicked() {
            return Some(CardAction::Select(acc.user_id.clone(), true));
        }
        let mut action = None;
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.double_clicked() {
            action = Some(CardAction::Launch(acc.user_id.clone()));
        } else if resp.clicked() {
            let additive = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            action = Some(CardAction::Select(acc.user_id.clone(), additive));
        } else if resp.secondary_clicked() && !selected {
            action = Some(CardAction::Select(acc.user_id.clone(), false));
        }
        resp.context_menu(|ui| {
            ui.set_min_width(190.0);
            let id = acc.user_id.clone();
            let items: [(&str, CardAction); 7] = [
                ("Launch into game", CardAction::Launch(id.clone())),
                ("Open home screen", CardAction::LaunchHome(id.clone())),
                (if acc.favorite { "Remove from favorites" } else { "Add to favorites" }, CardAction::ToggleFav(id.clone())),
                (if acc.main { "Unmark as MAIN" } else { "Mark as MAIN" }, CardAction::ToggleMain(id.clone())),
                ("Rename alias", CardAction::Rename(id.clone())),
                ("Copy username", CardAction::Copy(id.clone())),
                ("Open profile", CardAction::Profile(id.clone())),
            ];
            for (label, a) in items {
                if ui.button(label).clicked() {
                    action = Some(a);
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button(RichText::new("Delete account").color(RED())).clicked() {
                action = Some(CardAction::Delete(id));
                ui.close_menu();
            }
        });
        action
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        Frame::new().fill(SURFACE()).corner_radius(CornerRadius::same(12)).stroke(line(1.0, BORDER())).inner_margin(Margin::same(40)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No accounts yet").font(sb(18.0)).color(TEXT()));
                ui.label(RichText::new("Data is encrypted and only stored on this PC.").color(MUTED()));
                ui.add_space(14.0);
                if primary(ui, "Add with a cookie", vec2(240.0, 38.0), true).clicked() {
                    self.open_add();
                }
                ui.add_space(4.0);
                if ghost(ui, "Import from Roblox Account Manager", vec2(240.0, 36.0)).clicked() {
                    self.open_import();
                }
            });
        });
    }

    // -------------------------------------------------------------- ui: modals & toasts
    fn modals(&mut self, ctx: &egui::Context) {
        let mut modal = std::mem::replace(&mut self.modal, Modal::None);
        let mut keep = true;
        let frame = Frame::new().fill(SURFACE()).corner_radius(CornerRadius::same(14)).inner_margin(Margin::same(22)).stroke(line(1.0, BORDER_HI()));
        let title = |ui: &mut egui::Ui, t: &str, sub: &str| {
            ui.label(RichText::new(t).font(sb(19.0)).color(TEXT()));
            if !sub.is_empty() {
                ui.add_space(-4.0);
                ui.label(RichText::new(sub).color(MUTED()));
            }
            ui.add_space(10.0);
        };
        match &mut modal {
            Modal::None => keep = false,
            Modal::Add { cookie, alias, show, error, checking } => {
                let mut submit = false;
                let r = egui::Modal::new(egui::Id::new("add")).frame(frame).show(ctx, |ui| {
                    ui.set_width(520.0);
                    title(ui, "Add account", "Sign in once in a browser and copy the cookie.");
                    Frame::new().fill(CARD()).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(14)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let steps = [
                            "Open an Incognito window and sign in to roblox.com with this account.",
                            "Press F12, then open Application > Cookies > https://www.roblox.com",
                            "Copy the Value of .ROBLOSECURITY and paste it below.",
                            "Close the Incognito window. Don't click Log out (that kills the cookie).",
                        ];
                        for (i, s) in steps.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{}.", i + 1)).font(sb(13.0)).color(FAINT()));
                                ui.label(RichText::new(*s).font(rg(13.5)).color(TEXT()));
                            });
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Never share this cookie. Anyone who has it can sign in to your account.").font(rg(12.5)).color(RED()));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        caption(ui, "COOKIE .ROBLOSECURITY");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.checkbox(show, RichText::new("Show").font(rg(12.5)).color(MUTED()));
                        });
                    });
                    let resp = field(ui, cookie, "Paste the cookie here", !*show);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        submit = true;
                    }
                    ui.add_space(4.0);
                    caption(ui, "ALIAS  (optional)");
                    field(ui, alias, "e.g. Main, Alt 1", false);
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).font(rg(13.0)).color(RED()));
                    }
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if *checking {
                            primary(ui, "Checking...", vec2(120.0, 36.0), false);
                            ui.add(egui::Spinner::new().color(TEXT()));
                        } else if primary(ui, "Add", vec2(120.0, 36.0), true).clicked() {
                            submit = true;
                        }
                        if ghost(ui, "Cancel", vec2(84.0, 36.0)).clicked() && !*checking {
                            keep = false;
                        }
                    });
                });
                if r.should_close() && !*checking {
                    keep = false;
                }
                if submit && !*checking {
                    self.modal = modal;
                    self.submit_add(ctx);
                    return;
                }
            }
            Modal::Bulk { text, progress, error } => {
                let mut submit = false;
                let running = progress.is_some();
                let r = egui::Modal::new(egui::Id::new("bulk")).frame(frame).show(ctx, |ui| {
                    ui.set_width(560.0);
                    title(ui, "Add many accounts", "Paste cookies, one per line. username:cookie also works.");
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        ui.add_enabled(
                            !running,
                            egui::TextEdit::multiline(text)
                                .desired_rows(9)
                                .desired_width(f32::INFINITY)
                                .font(FontId::monospace(11.5))
                                .margin(vec2(10.0, 8.0))
                                .hint_text(RichText::new("_|WARNING:-DO-NOT-SHARE-THIS...\nusername:_|WARNING:-DO-NOT-SHARE-THIS...").color(FAINT())),
                        );
                    });
                    let n = text.lines().filter_map(extract_cookie).count();
                    ui.label(RichText::new(format!("{n} cookies detected")).font(rg(12.5)).color(MUTED()));
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).font(rg(13.0)).color(RED()));
                    }
                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some((i, n)) = progress {
                            primary(ui, &format!("Checking {i}/{n}"), vec2(150.0, 36.0), false);
                            ui.add(egui::Spinner::new().color(TEXT()));
                        } else {
                            if primary(ui, "Add all", vec2(130.0, 36.0), n > 0).clicked() {
                                submit = true;
                            }
                            if ghost(ui, "Cancel", vec2(84.0, 36.0)).clicked() {
                                keep = false;
                            }
                        }
                    });
                });
                if r.should_close() && !running {
                    keep = false;
                }
                if submit {
                    self.modal = modal;
                    self.submit_bulk(ctx);
                    return;
                }
            }
            Modal::Rename { user_id, value } => {
                let mut save = false;
                let name = self.account(user_id).map(|a| a.username.clone()).unwrap_or_default();
                let r = egui::Modal::new(egui::Id::new("rename")).frame(frame).show(ctx, |ui| {
                    ui.set_width(380.0);
                    title(ui, "Rename alias", &format!("For @{name}"));
                    let resp = field(ui, value, "New alias", false);
                    resp.request_focus();
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        save = true;
                    }
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary(ui, "Save", vec2(100.0, 36.0), true).clicked() {
                            save = true;
                        }
                        if ghost(ui, "Cancel", vec2(84.0, 36.0)).clicked() {
                            keep = false;
                        }
                    });
                });
                if r.should_close() {
                    keep = false;
                }
                if save {
                    let v: String = value.trim().chars().take(40).collect();
                    if !v.is_empty() {
                        let id = user_id.clone();
                        if let Some(a) = self.account_mut(&id) {
                            a.alias = v;
                        }
                        self.edit = None;
                        self.save();
                        self.toast("Alias saved.", ToastKind::Ok, 3.0);
                    }
                    keep = false;
                }
            }
            Modal::ConfirmDelete(ids) => {
                let names: Vec<String> = ids.iter().filter_map(|id| self.account(id).map(|a| a.username.clone())).collect();
                let mut confirm = false;
                let r = egui::Modal::new(egui::Id::new("delete")).frame(frame).show(ctx, |ui| {
                    ui.set_width(400.0);
                    title(ui, &format!("Delete {} account(s)?", names.len()), "");
                    ui.label(RichText::new(names.join(", ")).color(TEXT()));
                    ui.label(RichText::new("Accounts are only removed from this manager. The Roblox accounts themselves are not deleted.").color(MUTED()));
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, "Delete", vec2(100.0, 36.0), RED(), hex(0xEC7A72), ON_PRIMARY(), sb(14.0), true).clicked() {
                            confirm = true;
                        }
                        if ghost(ui, "Cancel", vec2(84.0, 36.0)).clicked() {
                            keep = false;
                        }
                    });
                });
                if r.should_close() {
                    keep = false;
                }
                if confirm {
                    let ids = ids.clone();
                    self.accounts.retain(|a| !ids.contains(&a.user_id));
                    self.selected.retain(|s| !ids.contains(s));
                    self.save();
                    self.toast(format!("Deleted: {}", names.join(", ")), ToastKind::Info, 4.0);
                    keep = false;
                }
            }
            Modal::ConfirmStop(n) => {
                let mut confirm = false;
                let n = *n;
                let r = egui::Modal::new(egui::Id::new("stop")).frame(frame).show(ctx, |ui| {
                    ui.set_width(380.0);
                    title(ui, &format!("Close {n} Roblox window(s)?"), "Every Roblox Player window will be force-closed. Unsaved in-game progress may be lost.");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, "Close all", vec2(120.0, 36.0), RED(), hex(0xEC7A72), ON_PRIMARY(), sb(14.0), true).clicked() {
                            confirm = true;
                        }
                        if ghost(ui, "Cancel", vec2(84.0, 36.0)).clicked() {
                            keep = false;
                        }
                    });
                });
                if r.should_close() {
                    keep = false;
                }
                if confirm {
                    self.shared.lock().unwrap().forget_all();
                    let killed = win::kill_pids(&win::roblox_pids());
                    self.running = win::roblox_pids().len();
                    self.toast(format!("Closed {killed} Roblox window(s)."), ToastKind::Ok, 4.0);
                    keep = false;
                }
            }
            Modal::Import { path, error } => {
                let mut go = false;
                let found = std::path::Path::new(path.trim().trim_matches('"')).is_file();
                let r = egui::Modal::new(egui::Id::new("import")).frame(frame).show(ctx, |ui| {
                    ui.set_width(500.0);
                    title(ui, "Import accounts", "Use your Roblox Account Manager data, no need to sign in again.");
                    caption(ui, "AccountData.json LOCATION");
                    field(ui, path, r"e.g. D:\AccountData.json", false);
                    if found {
                        ui.label(RichText::new("File found").font(rg(12.5)).color(GREEN()));
                    } else {
                        ui.label(RichText::new("It's usually in your Roblox Account Manager folder.").font(rg(12.5)).color(FAINT()));
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Cookies are re-encrypted with DPAPI and stored on this PC. The original file is not modified. Every account is checked after import.")
                            .font(rg(12.5))
                            .color(MUTED()),
                    );
                    if let Some(e) = error {
                        ui.label(RichText::new(e.as_str()).font(rg(13.0)).color(RED()));
                    }
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if primary(ui, "Import", vec2(100.0, 36.0), true).clicked() {
                            go = true;
                        }
                        if ghost(ui, "Cancel", vec2(84.0, 36.0)).clicked() {
                            keep = false;
                        }
                    });
                });
                if r.should_close() {
                    keep = false;
                }
                if go {
                    let p = path.clone();
                    match self.import_from(ctx, &p) {
                        Ok(()) => keep = false,
                        Err(e) => *error = Some(e),
                    }
                }
            }
        }
        if keep && matches!(self.modal, Modal::None) {
            self.modal = modal;
        }
    }

    fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        for t in &mut self.toasts {
            if t.until.is_none() {
                t.until = Some(now + t.secs);
            }
        }
        self.toasts.retain(|t| t.until.unwrap_or(0.0) > now);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-24.0, -46.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for t in self.toasts.iter().rev().take(4) {
                    let col = match t.kind {
                        ToastKind::Ok => GREEN(),
                        ToastKind::Err => RED(),
                        ToastKind::Info => AMBER(),
                    };
                    Frame::new()
                        .fill(CARD())
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 10))
                        .stroke(line(1.0, BORDER_HI()))
                        .shadow(egui::Shadow { offset: [0, 6], blur: 20, spread: 0, color: Color32::from_black_alpha(if is_light() { 28 } else { 110 }) })
                        .show(ui, |ui| {
                            ui.set_max_width(360.0);
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(3.0, 18.0), Sense::hover());
                                ui.painter().rect_filled(r, CornerRadius::same(1), col);
                                ui.add(egui::Label::new(RichText::new(&t.text).font(rg(13.5)).color(TEXT())).wrap());
                            });
                        });
                    ui.add_space(4.0);
                }
            });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if !matches!(self.modal, Modal::None) {
            return;
        }
        let (ctrl_enter, ctrl_f, ctrl_a, enter, del, esc) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::Enter),
                i.modifiers.command && i.key_pressed(egui::Key::F),
                i.modifiers.command && i.key_pressed(egui::Key::A),
                !i.modifiers.command && i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if ctrl_enter {
            self.launch(ctx);
            return;
        }
        if ctrl_f {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("search")));
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        if ctrl_a {
            self.select_all();
        }
        if enter && !self.selected.is_empty() {
            self.launch(ctx);
        }
        if del && !self.selected.is_empty() {
            self.modal = Modal::ConfirmDelete(self.selected.clone());
        }
        if esc {
            self.selected.clear();
        }
    }

    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.screenshot.clone() else { return };
        self.frame_no += 1;
        let delay = if self.demo { 5.0 } else { 10.0 };
        if self.frame_no < u64::MAX && ctx.input(|i| i.time) > delay {
            self.frame_no = u64::MAX;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint();
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = shot {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                let _ = buf.save(&path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        self.poll_monitor();
        self.handle_close(ctx);
        self.periodic(ctx);
        self.memory(ctx);
        self.request_avatars(ctx);
        self.request_game_info(ctx);
        self.shortcuts(ctx);
        self.header(ctx);
        self.status_bar(ctx);
        self.side_panel(ctx);
        self.central(ctx);
        self.modals(ctx);
        self.toasts_ui(ctx);
        self.sync_thumbs(ctx);
        self.handle_screenshot(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::from(BG()).to_array()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.save_settings();
        self.unregister_thumbs();
        tray::stop();
    }
}

// ------------------------------------------------------------------ icon
fn app_icon() -> egui::IconData {
    let (width, height, rgba) = png_rgba(LOGO_64);
    egui::IconData { rgba, width, height }
}

// ------------------------------------------------------------------ main
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let Some(_instance) = win::NamedMutex::acquire("LocalRbxManager_App_SingleInstance") else {
        win::message_box("RBX Account Manager is already running.", false);
        return;
    };
    let accounts = match store::load_accounts() {
        Ok(a) => a,
        Err(e) => {
            win::message_box(&format!("Couldn't open account data:\n{e}"), true);
            return;
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RBX Account Manager")
            .with_inner_size([1240.0, 760.0])
            .with_min_inner_size([1000.0, 640.0])
            .with_icon(Arc::new(app_icon())),
        centered: true,
        ..Default::default()
    };
    let res = eframe::run_native("RBX Account Manager", options, Box::new(move |cc| Ok(Box::new(App::new(cc, accounts, &args)))));
    if let Err(e) = res {
        win::message_box(&format!("Couldn't open the window (OpenGL):\n{e}"), true);
    }
}
