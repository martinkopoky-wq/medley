//! Reusable Apple-style widgets. Icons are painted as vectors so they stay
//! crisp at any scale and follow the theme colors.

use crate::theme::{self, Palette};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui,
    Vec2, pos2, vec2,
};
use medley_core::ProviderKind;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Icons
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Previous,
    Shuffle,
    Repeat,
    RepeatOne,
    Plus,
    Close,
    More,
    Sparkles,
    Search,
    Speaker,
    SpeakerMuted,
    Queue,
    Heart,
    Layout,
    Check,
    Back,
    Note,
    Refresh,
    Trash,
}

/// Paint `icon` centered in `rect` (glyph occupies ~70% of the shorter side).
pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height()) * 0.7;
    let c = rect.center();
    let p = |x: f32, y: f32| pos2(c.x + (x - 0.5) * s, c.y + (y - 0.5) * s);
    let w = (s * 0.09).max(1.3);
    let stroke = Stroke::new(w, color);
    let tri = |pts: Vec<Pos2>| Shape::convex_polygon(pts, color, Stroke::NONE);
    match icon {
        Icon::Play => {
            painter.add(tri(vec![p(0.18, 0.05), p(0.95, 0.5), p(0.18, 0.95)]));
        }
        Icon::Pause => {
            let r = CornerRadius::same((s * 0.06) as u8);
            painter.rect_filled(Rect::from_min_max(p(0.16, 0.08), p(0.40, 0.92)), r, color);
            painter.rect_filled(Rect::from_min_max(p(0.60, 0.08), p(0.84, 0.92)), r, color);
        }
        Icon::Next => {
            painter.add(tri(vec![p(0.02, 0.18), p(0.5, 0.5), p(0.02, 0.82)]));
            painter.add(tri(vec![p(0.48, 0.18), p(0.96, 0.5), p(0.48, 0.82)]));
        }
        Icon::Previous => {
            painter.add(tri(vec![p(0.98, 0.18), p(0.5, 0.5), p(0.98, 0.82)]));
            painter.add(tri(vec![p(0.52, 0.18), p(0.04, 0.5), p(0.52, 0.82)]));
        }
        Icon::Shuffle => {
            painter.add(Shape::line(vec![p(0.0, 0.25), p(0.28, 0.25), p(0.62, 0.75), p(0.92, 0.75)], stroke));
            painter.add(Shape::line(vec![p(0.0, 0.75), p(0.28, 0.75), p(0.62, 0.25), p(0.92, 0.25)], stroke));
            arrow_head(painter, p(1.0, 0.25), vec2(1.0, 0.0), s * 0.2, color);
            arrow_head(painter, p(1.0, 0.75), vec2(1.0, 0.0), s * 0.2, color);
        }
        Icon::Repeat | Icon::RepeatOne => {
            painter.add(Shape::line(vec![p(0.08, 0.6), p(0.08, 0.25), p(0.85, 0.25)], stroke));
            painter.add(Shape::line(vec![p(0.92, 0.4), p(0.92, 0.75), p(0.15, 0.75)], stroke));
            arrow_head(painter, p(0.98, 0.25), vec2(1.0, 0.0), s * 0.2, color);
            arrow_head(painter, p(0.02, 0.75), vec2(-1.0, 0.0), s * 0.2, color);
            if icon == Icon::RepeatOne {
                painter.text(p(0.5, 0.52), Align2::CENTER_CENTER, "1", theme::semibold(s * 0.42), color);
            }
        }
        Icon::Plus => {
            painter.line_segment([p(0.5, 0.1), p(0.5, 0.9)], stroke);
            painter.line_segment([p(0.1, 0.5), p(0.9, 0.5)], stroke);
        }
        Icon::Close => {
            painter.line_segment([p(0.2, 0.2), p(0.8, 0.8)], stroke);
            painter.line_segment([p(0.8, 0.2), p(0.2, 0.8)], stroke);
        }
        Icon::More => {
            for x in [0.15, 0.5, 0.85] {
                painter.circle_filled(p(x, 0.5), s * 0.08, color);
            }
        }
        Icon::Sparkles => {
            star(painter, p(0.42, 0.55), s * 0.42, color);
            star(painter, p(0.85, 0.18), s * 0.18, color);
            star(painter, p(0.88, 0.85), s * 0.12, color);
        }
        Icon::Search => {
            painter.circle_stroke(p(0.42, 0.42), s * 0.3, stroke);
            painter.line_segment([p(0.64, 0.64), p(0.92, 0.92)], Stroke::new(w * 1.2, color));
        }
        Icon::Speaker | Icon::SpeakerMuted => {
            painter.add(tri(vec![
                p(0.05, 0.36),
                p(0.22, 0.36),
                p(0.48, 0.1),
                p(0.48, 0.9),
                p(0.22, 0.64),
                p(0.05, 0.64),
            ]));
            if icon == Icon::SpeakerMuted {
                painter.line_segment([p(0.62, 0.35), p(0.92, 0.65)], stroke);
                painter.line_segment([p(0.92, 0.35), p(0.62, 0.65)], stroke);
            } else {
                arc(painter, p(0.48, 0.5), s * 0.2, -0.9, 0.9, stroke);
                arc(painter, p(0.48, 0.5), s * 0.38, -0.95, 0.95, stroke);
            }
        }
        Icon::Queue => {
            for y in [0.2, 0.5, 0.8] {
                painter.circle_filled(p(0.08, y), s * 0.06, color);
                painter.line_segment([p(0.25, y), p(0.95, y)], stroke);
            }
        }
        Icon::Heart => {
            let pts: Vec<Pos2> = (0..48)
                .map(|i| {
                    let t = i as f32 / 48.0 * std::f32::consts::TAU;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    p(0.5 + x / 36.0, 0.47 - y / 34.0)
                })
                .collect();
            painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        }
        Icon::Layout => {
            let r = CornerRadius::same((s * 0.08) as u8);
            painter.rect_stroke(Rect::from_min_max(p(0.05, 0.1), p(0.95, 0.9)), r, stroke, StrokeKind::Middle);
            painter.line_segment([p(0.38, 0.1), p(0.38, 0.9)], stroke);
            painter.line_segment([p(0.38, 0.5), p(0.95, 0.5)], stroke);
        }
        Icon::Check => {
            painter.add(Shape::line(vec![p(0.1, 0.52), p(0.38, 0.8), p(0.92, 0.2)], Stroke::new(w * 1.2, color)));
        }
        Icon::Back => {
            painter.add(Shape::line(vec![p(0.68, 0.08), p(0.28, 0.5), p(0.68, 0.92)], Stroke::new(w * 1.2, color)));
        }
        Icon::Note => {
            painter.circle_filled(p(0.28, 0.78), s * 0.17, color);
            painter.circle_filled(p(0.82, 0.68), s * 0.17, color);
            painter.line_segment([p(0.43, 0.78), p(0.43, 0.12)], stroke);
            painter.line_segment([p(0.97, 0.68), p(0.97, 0.02)], stroke);
            painter.add(tri(vec![p(0.43, 0.12), p(0.97, 0.02), p(0.97, 0.18), p(0.43, 0.28)]));
        }
        Icon::Refresh => {
            arc(painter, p(0.5, 0.5), s * 0.38, -2.6, 2.2, stroke);
            let end = p(0.5, 0.5) + vec2(2.2f32.cos(), 2.2f32.sin()) * s * 0.38;
            arrow_head(painter, end + vec2(-0.05, 0.08) * s, vec2(-0.8, 0.6), s * 0.2, color);
        }
        Icon::Trash => {
            painter.line_segment([p(0.1, 0.2), p(0.9, 0.2)], stroke);
            painter.add(Shape::line(vec![p(0.38, 0.2), p(0.4, 0.05), p(0.6, 0.05), p(0.62, 0.2)], stroke));
            painter.add(Shape::line(vec![p(0.2, 0.2), p(0.27, 0.95), p(0.73, 0.95), p(0.8, 0.2)], stroke));
        }
    }
}

fn arrow_head(painter: &egui::Painter, tip: Pos2, dir: Vec2, size: f32, color: Color32) {
    let d = dir.normalized();
    let n = vec2(-d.y, d.x);
    let base = tip - d * size;
    painter.add(Shape::convex_polygon(vec![tip, base + n * size * 0.6, base - n * size * 0.6], color, Stroke::NONE));
}

fn arc(painter: &egui::Painter, c: Pos2, r: f32, from: f32, to: f32, stroke: Stroke) {
    let n = 20;
    let pts = (0..=n).map(|i| {
        let a = from + (to - from) * i as f32 / n as f32;
        c + vec2(a.cos(), a.sin()) * r
    });
    painter.add(Shape::line(pts.collect(), stroke));
}

fn star(painter: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    // Four-point sparkle: concave, so draw as two convex diamonds.
    let thin = r * 0.28;
    painter.add(Shape::convex_polygon(
        vec![c + vec2(0.0, -r), c + vec2(thin, 0.0), c + vec2(0.0, r), c + vec2(-thin, 0.0)],
        color,
        Stroke::NONE,
    ));
    painter.add(Shape::convex_polygon(
        vec![c + vec2(-r, 0.0), c + vec2(0.0, -thin), c + vec2(r, 0.0), c + vec2(0.0, thin)],
        color,
        Stroke::NONE,
    ));
}

/// A borderless icon button with a soft circular hover and a springy press.
pub fn icon_button(ui: &mut Ui, icon: Icon, size: f32, tooltip: &str) -> Response {
    icon_button_colored(ui, icon, size, tooltip, None)
}

/// Like [`icon_button`]; `tint` overrides the glyph color (e.g. accent when a toggle is on).
pub fn icon_button_colored(ui: &mut Ui, icon: Icon, size: f32, tooltip: &str, tint: Option<Color32>) -> Response {
    let pal = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered() && ui.is_enabled());
        let pressed = response.is_pointer_button_down_on();
        let scale = theme::spring_with(ui.ctx(), response.id.with("s"), if pressed { 0.86 } else { 1.0 }, 0.25, 0.7);
        if hover > 0.0 {
            let bg = Color32::from_rgba_unmultiplied(
                pal.fill_strong.r(),
                pal.fill_strong.g(),
                pal.fill_strong.b(),
                (pal.fill_strong.a() as f32 * hover * 0.8) as u8,
            );
            ui.painter().circle_filled(rect.center(), size * 0.5, bg);
        }
        let mut color = tint.unwrap_or(pal.label);
        if !ui.is_enabled() {
            color = pal.tertiary;
        }
        let glyph = Rect::from_center_size(rect.center(), rect.size() * 0.62 * scale);
        paint_icon(ui.painter(), glyph, icon, color);
    }
    if tooltip.is_empty() { response } else { response.on_hover_text(tooltip) }
}

/// The big round play/pause button (filled label color, like Music.app's mini player).
pub fn play_button(ui: &mut Ui, playing: bool, loading: bool, size: f32) -> Response {
    let pal = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let pressed = response.is_pointer_button_down_on();
        let scale = theme::spring_with(ui.ctx(), response.id.with("s"), if pressed { 0.9 } else { 1.0 }, 0.25, 0.65);
        let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered());
        let r = size * 0.5 * scale;
        let fill = theme::lerp_color(pal.label, pal.accent, hover * 0.0);
        ui.painter().circle_filled(rect.center(), r, fill);
        let glyph = Rect::from_center_size(
            rect.center() + vec2(if playing { 0.0 } else { size * 0.03 }, 0.0),
            Vec2::splat(r * 0.95),
        );
        let bg = pal.card;
        if loading {
            spinner_at(ui, rect.center(), r * 0.45, bg);
        } else {
            paint_icon(ui.painter(), glyph, if playing { Icon::Pause } else { Icon::Play }, bg);
        }
    }
    response.on_hover_text(if playing { "Pause (Space)" } else { "Play (Space)" })
}

/// A small rotating arc spinner (Apple-like, thinner than egui's).
pub fn spinner_at(ui: &Ui, center: Pos2, radius: f32, color: Color32) {
    let t = ui.input(|i| i.time) as f32;
    let start = t * 5.0;
    arc(ui.painter(), center, radius, start, start + 4.2, Stroke::new((radius * 0.22).max(1.5), color));
    ui.ctx().request_repaint_after(Duration::from_millis(16));
}

pub fn spinner(ui: &mut Ui, size: f32) -> Response {
    let pal = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    spinner_at(ui, rect.center(), size * 0.4, pal.secondary);
    response
}

// ---------------------------------------------------------------------------
// Artwork (with RAM-friendly texture eviction)
// ---------------------------------------------------------------------------

static ARTWORK_SEEN: LazyLock<Mutex<HashMap<String, Instant>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Images not drawn for this long are dropped from GPU and RAM.
const ARTWORK_TTL: Duration = Duration::from_secs(90);

fn note_artwork(url: &str) {
    if let Ok(mut seen) = ARTWORK_SEEN.lock() {
        match seen.get_mut(url) {
            Some(t) => *t = Instant::now(),
            None => {
                seen.insert(url.to_owned(), Instant::now());
            }
        }
    }
}

/// Forget artwork that hasn't been on screen recently. Called periodically.
pub fn sweep_artwork(ctx: &egui::Context) {
    let stale: Vec<String> = match ARTWORK_SEEN.lock() {
        Ok(mut seen) => {
            let stale: Vec<String> =
                seen.iter().filter(|(_, t)| t.elapsed() > ARTWORK_TTL).map(|(u, _)| u.clone()).collect();
            for u in &stale {
                seen.remove(u);
            }
            stale
        }
        Err(_) => return,
    };
    for url in &stale {
        ctx.forget_image(url);
    }
    if !stale.is_empty() {
        tracing::debug!("evicted {} artwork textures", stale.len());
    }
}

/// Square artwork with continuous corners and a hairline edge. Falls back to
/// a soft gradient tile with a note glyph (seeded by `seed` so it's stable).
pub fn artwork(ui: &mut Ui, url: Option<&str>, size: f32, radius: u8, seed: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    paint_artwork(ui, rect, url, radius, seed);
    response
}

pub fn paint_artwork(ui: &Ui, rect: Rect, url: Option<&str>, radius: u8, seed: &str) {
    paint_artwork_rounded(ui, rect, url, CornerRadius::same(radius), seed);
}

/// Like [`paint_artwork`] with per-corner radii (for mosaics).
pub fn paint_artwork_rounded(ui: &Ui, rect: Rect, url: Option<&str>, r: CornerRadius, seed: &str) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let pal = theme::palette(ui.ctx());
    placeholder(ui.painter(), rect, r, seed, &pal);
    if let Some(url) = url.filter(|u| !u.is_empty()) {
        note_artwork(url);
        egui::Image::new(url).corner_radius(r).show_loading_spinner(false).paint_at(ui, rect);
    }
    let edge = if pal.dark { Color32::from_white_alpha(18) } else { Color32::from_black_alpha(18) };
    ui.painter().rect_stroke(rect, r, Stroke::new(1.0, edge), StrokeKind::Inside);
}

fn placeholder(painter: &egui::Painter, rect: Rect, r: CornerRadius, seed: &str, pal: &Palette) {
    let h = seed.bytes().fold(5381u32, |h, b| h.wrapping_mul(33) ^ b as u32);
    // Harmonious pairs (Apple system colors), softened toward the card color.
    const PAIRS: [([u8; 3], [u8; 3]); 6] = [
        ([0xFF, 0x2D, 0x55], [0xFF, 0x9F, 0x0A]),
        ([0x5E, 0x5C, 0xE6], [0xBF, 0x5A, 0xF2]),
        ([0x30, 0xB0, 0xC7], [0x0A, 0x84, 0xFF]),
        ([0x34, 0xC7, 0x59], [0x30, 0xB0, 0xC7]),
        ([0xFF, 0x9F, 0x0A], [0xFF, 0xD6, 0x0A]),
        ([0xBF, 0x5A, 0xF2], [0xFF, 0x37, 0x5F]),
    ];
    let (a, b) = PAIRS[(h % PAIRS.len() as u32) as usize];
    let (a, b) = (Color32::from_rgb(a[0], a[1], a[2]), Color32::from_rgb(b[0], b[1], b[2]));
    let base = if pal.dark { Color32::from_rgb(0x2C, 0x2C, 0x2E) } else { Color32::from_rgb(0xF2, 0xF2, 0xF7) };
    let top = theme::lerp_color(base, a, if pal.dark { 0.55 } else { 0.6 });
    let bottom = theme::lerp_color(base, b, if pal.dark { 0.55 } else { 0.6 });
    // Vertical gradient via a mesh.
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    if r == CornerRadius::ZERO {
        painter.add(mesh);
    } else {
        // Rounded: fill with the mid color, then the gradient inset (corners stay rounded).
        painter.rect_filled(rect, r, theme::lerp_color(top, bottom, 0.5));
        let inset = r.nw as f32 * 0.3;
        let inner = rect.shrink(inset);
        let mut m = egui::Mesh::default();
        m.colored_vertex(inner.left_top(), top);
        m.colored_vertex(inner.right_top(), top);
        m.colored_vertex(inner.right_bottom(), bottom);
        m.colored_vertex(inner.left_bottom(), bottom);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        painter.add(m);
    }
    let glyph = Rect::from_center_size(rect.center(), rect.size() * 0.38);
    paint_icon(painter, glyph, Icon::Note, Color32::from_white_alpha(170));
}

// ---------------------------------------------------------------------------
// Buttons & controls
// ---------------------------------------------------------------------------

/// Capsule button. `prominent` = filled accent (primary action).
pub fn pill_button(ui: &mut Ui, text: &str, prominent: bool) -> Response {
    pill_button_icon(ui, None, text, prominent)
}

pub fn pill_button_icon(ui: &mut Ui, icon: Option<Icon>, text: &str, prominent: bool) -> Response {
    let pal = theme::palette(ui.ctx());
    let font = theme::semibold(13.0);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 18.0 } else { 0.0 };
    let size = vec2(galley.size().x + 28.0 + icon_w, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered() && enabled);
        let pressed = response.is_pointer_button_down_on();
        let scale = theme::spring_with(ui.ctx(), response.id.with("s"), if pressed { 0.96 } else { 1.0 }, 0.25, 0.7);
        let rr = Rect::from_center_size(rect.center(), rect.size() * scale);
        let (fill, fg) = if prominent {
            (theme::lerp_color(pal.accent, Color32::WHITE, hover * 0.12), pal.on_accent)
        } else {
            (theme::lerp_color(pal.fill, pal.fill_strong, hover), pal.label)
        };
        let (fill, fg) = if enabled { (fill, fg) } else { (pal.fill, pal.tertiary) };
        ui.painter().rect_filled(rr, CornerRadius::same(255), fill);
        let mut x = rr.left() + 14.0;
        if let Some(icon) = icon {
            paint_icon(ui.painter(), Rect::from_center_size(pos2(x + 7.0, rr.center().y), Vec2::splat(16.0)), icon, fg);
            x += icon_w;
        }
        ui.painter().galley(pos2(x, rr.center().y - galley.size().y / 2.0), galley, fg);
    }
    response
}

/// Text-only accent button ("See All", "Edit").
pub fn link_button(ui: &mut Ui, text: &str) -> Response {
    let pal = theme::palette(ui.ctx());
    let r = ui
        .add(egui::Label::new(RichText::new(text).color(pal.accent).font(theme::regular(13.0))).sense(Sense::click()));
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Apple segmented control. Returns true when the selection changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let pal = theme::palette(ui.ctx());
    let font = theme::regular(12.5);
    let widths: Vec<f32> = options
        .iter()
        .map(|(_, l)| ui.painter().layout_no_wrap((*l).to_owned(), font.clone(), Color32::PLACEHOLDER).size().x + 22.0)
        .collect();
    let total = widths.iter().sum::<f32>() + 4.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 26.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(R_SEG), pal.fill);
    let id = ui.id().with(("segmented", rect.min.x as i32, rect.min.y as i32));
    let mut changed = false;
    let mut x = rect.left() + 2.0;
    let selected_idx = options.iter().position(|(v, _)| v == value).unwrap_or(0);
    let target_x: f32 = rect.left() + 2.0 + widths[..selected_idx].iter().sum::<f32>();
    let anim_x = theme::spring(ui.ctx(), id.with("x"), target_x);
    let thumb = Rect::from_min_size(pos2(anim_x, rect.top() + 2.0), vec2(widths[selected_idx], rect.height() - 4.0));
    let thumb_fill = if pal.dark { Color32::from_rgb(0x63, 0x63, 0x66) } else { Color32::WHITE };
    let shadow = egui::Shadow { offset: [0, 1], blur: 3, spread: 0, color: Color32::from_black_alpha(30) };
    ui.painter().add(shadow.as_shape(thumb, CornerRadius::same(R_SEG - 1)));
    ui.painter().rect_filled(thumb, CornerRadius::same(R_SEG - 1), thumb_fill);
    for (i, ((v, label), w)) in options.iter().zip(&widths).enumerate() {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, rect.height()));
        let resp = ui.interact(cell, id.with(i), Sense::click());
        if resp.clicked() && *value != *v {
            *value = *v;
            changed = true;
        }
        let selected = i == selected_idx;
        let f = if selected { theme::semibold(12.5) } else { font.clone() };
        ui.painter().text(
            cell.center(),
            Align2::CENTER_CENTER,
            *label,
            f,
            if selected { pal.label } else { pal.secondary },
        );
        x += w;
    }
    changed
}

const R_SEG: u8 = 7;

/// iOS-style switch. Returns the response; `on` flips on click.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let pal = theme::palette(ui.ctx());
    let size = vec2(38.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let t = theme::spring_with(ui.ctx(), response.id, if *on { 1.0 } else { 0.0 }, 0.3, 0.75);
        let track = theme::lerp_color(pal.fill_strong, pal.success, t.clamp(0.0, 1.0));
        ui.painter().rect_filled(rect, CornerRadius::same(255), track);
        let r = rect.height() / 2.0 - 2.0;
        let x = egui::lerp(rect.left() + r + 2.0..=rect.right() - r - 2.0, t);
        let knob = pos2(x, rect.center().y);
        ui.painter().circle_filled(knob + vec2(0.0, 0.8), r, Color32::from_black_alpha(35));
        ui.painter().circle_filled(knob, r, Color32::WHITE);
    }
    response
}

/// Label on the left, switch on the right, full width.
pub fn switch_row(ui: &mut Ui, label: &str, detail: Option<&str>, on: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let text_w = (ui.available_width() - 52.0).max(80.0);
        ui.vertical(|ui| {
            ui.set_max_width(text_w);
            ui.label(label);
            if let Some(d) = detail {
                caption(ui, d);
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = switch(ui, on).changed();
        });
    });
    changed
}

/// Rounded search field with a magnifier and clear button.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str, width: f32) -> Response {
    let pal = theme::palette(ui.ctx());
    let height = 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(R_SEG + 1), pal.fill);
    paint_icon(
        ui.painter(),
        Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), Vec2::splat(15.0)),
        Icon::Search,
        pal.secondary,
    );
    let clear_w = if text.is_empty() { 0.0 } else { 24.0 };
    let edit_rect =
        Rect::from_min_max(pos2(rect.left() + 30.0, rect.top()), pos2(rect.right() - 6.0 - clear_w, rect.bottom()));
    let mut child = ui
        .new_child(egui::UiBuilder::new().max_rect(edit_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let response = child.add(
        egui::TextEdit::singleline(text)
            .hint_text(RichText::new(hint).color(pal.tertiary))
            .frame(egui::Frame::NONE)
            .font(theme::regular(14.0))
            .desired_width(edit_rect.width()),
    );
    if !text.is_empty() {
        let c = pos2(rect.right() - 16.0, rect.center().y);
        let cr = Rect::from_center_size(c, Vec2::splat(16.0));
        let clear = ui.interact(cr, response.id.with("clear"), Sense::click());
        ui.painter().circle_filled(c, 7.5, pal.tertiary);
        paint_icon(ui.painter(), Rect::from_center_size(c, Vec2::splat(9.0)), Icon::Close, pal.card);
        if clear.clicked() {
            text.clear();
            let mut r = response.clone();
            r.mark_changed();
            return r;
        }
    }
    response
}

/// Thin progress bar that thickens on hover; returns a seek target (ms) when released.
pub fn scrubber(ui: &mut Ui, id: Id, position_ms: u64, duration_ms: u64, width: f32) -> Option<u64> {
    let pal = theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, 14.0), Sense::hover());
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let active = response.hovered() || response.dragged();
    let thick = theme::spring_with(ui.ctx(), id.with("t"), if active { 6.0 } else { 3.0 }, 0.25, 0.85);
    let mut frac = if duration_ms > 0 { position_ms as f32 / duration_ms as f32 } else { 0.0 };
    let drag_frac = response.interact_pointer_pos().map(|p| ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
    if response.dragged()
        && let Some(f) = drag_frac
    {
        frac = f;
    }
    let bar = Rect::from_center_size(rect.center(), vec2(rect.width(), thick));
    let r = CornerRadius::same(255);
    ui.painter().rect_filled(bar, r, pal.fill_strong);
    let filled = Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * frac.clamp(0.0, 1.0), bar.max.y));
    ui.painter().rect_filled(filled, r, if active { pal.label } else { pal.secondary });
    if (response.drag_stopped() || response.clicked()) && duration_ms > 0 {
        return drag_frac.map(|f| (f * duration_ms as f32) as u64);
    }
    None
}

/// Compact volume slider with a speaker glyph that mutes on click.
pub fn volume_slider(ui: &mut Ui, volume: &mut f32, width: f32) -> bool {
    let pal = theme::palette(ui.ctx());
    let mut changed = false;
    let icon = if *volume <= 0.001 { Icon::SpeakerMuted } else { Icon::Speaker };
    let id = ui.id().with("volume");
    if icon_button(ui, icon, 22.0, "Mute").clicked() {
        let last = ui.ctx().data_mut(|d| *d.get_temp_mut_or::<f32>(id.with("last"), 0.7));
        if *volume > 0.001 {
            ui.ctx().data_mut(|d| d.insert_temp(id.with("last"), *volume));
            *volume = 0.0;
        } else {
            *volume = last.max(0.1);
        }
        changed = true;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    let response = ui.interact(rect, id, Sense::click_and_drag());
    if (response.dragged() || response.clicked())
        && let Some(p) = response.interact_pointer_pos()
    {
        *volume = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        changed = true;
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            *volume = (*volume + scroll / 600.0).clamp(0.0, 1.0);
            changed = true;
        }
    }
    let thick = 4.0;
    let bar = Rect::from_center_size(rect.center(), vec2(rect.width(), thick));
    ui.painter().rect_filled(bar, CornerRadius::same(255), pal.fill_strong);
    let filled = Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * *volume, bar.max.y));
    ui.painter().rect_filled(filled, CornerRadius::same(255), pal.secondary);
    let knob = pos2(filled.right(), rect.center().y);
    let hover = theme::fade(ui.ctx(), id.with("h"), response.hovered() || response.dragged());
    ui.painter().circle_filled(knob + vec2(0.0, 0.6), 5.0 + hover * 1.5, Color32::from_black_alpha(40));
    ui.painter().circle_filled(knob, 5.0 + hover * 1.5, Color32::WHITE);
    changed
}

// ---------------------------------------------------------------------------
// Typography helpers
// ---------------------------------------------------------------------------

/// 26 pt bold display title (collection headers).
pub fn large_title(ui: &mut Ui, text: &str) -> Response {
    let pal = theme::palette(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).font(theme::display(26.0)).color(pal.label)).truncate())
}

/// 17 pt semibold section title.
pub fn title(ui: &mut Ui, text: &str) -> Response {
    let pal = theme::palette(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).font(theme::semibold(17.0)).color(pal.label)).truncate())
}

/// Small uppercase-free section header in secondary color (sidebar groups).
pub fn section_header(ui: &mut Ui, text: &str) -> Response {
    let pal = theme::palette(ui.ctx());
    ui.add_space(6.0);
    let r = ui.label(RichText::new(text).font(theme::semibold(11.5)).color(pal.secondary));
    ui.add_space(2.0);
    r
}

/// Secondary-color body text.
pub fn secondary(ui: &mut Ui, text: &str) -> Response {
    let pal = theme::palette(ui.ctx());
    ui.label(RichText::new(text).color(pal.secondary))
}

/// 11.5 pt secondary text that wraps.
pub fn caption(ui: &mut Ui, text: &str) -> Response {
    let pal = theme::palette(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).font(theme::regular(11.5)).color(pal.secondary)).wrap())
}

/// Grouped "inset" card like iOS Settings.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let pal = theme::palette(ui.ctx());
    let fill = if pal.dark { Color32::from_rgb(0x2A, 0x2A, 0x2D) } else { Color32::from_rgb(0xF5, 0xF5, 0xF7) };
    egui::Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(theme::R_CARD - 2))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Hairline separator in the theme's separator color.
pub fn hairline(ui: &mut Ui) {
    let pal = theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, pal.separator));
}

/// Colored dot + service name.
pub fn provider_chip(ui: &mut Ui, kind: ProviderKind) -> Response {
    let pal = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        provider_dot(ui, kind, 7.0);
        ui.label(RichText::new(kind.name()).font(theme::regular(11.5)).color(pal.secondary));
    })
    .response
}

pub fn provider_dot(ui: &mut Ui, kind: ProviderKind, size: f32) -> Response {
    let (rect, r) = ui.allocate_exact_size(Vec2::splat(size + 2.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), size / 2.0, theme::provider_color(kind));
    r.on_hover_text(kind.name())
}

/// Tiny capsule badge ("PREVIEW", "AI").
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = theme::semibold(9.5);
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    let size = galley.size() + vec2(10.0, 4.0);
    let (rect, r) = ui.allocate_exact_size(size, Sense::hover());
    let bg = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 30);
    ui.painter().rect_filled(rect, CornerRadius::same(255), bg);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    r
}

/// Centered empty state: glyph, title, message, optional button. Returns true if the button was clicked.
pub fn empty_state(ui: &mut Ui, icon: Icon, title_text: &str, message: &str, action: Option<&str>) -> bool {
    let pal = theme::palette(ui.ctx());
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.22).clamp(12.0, 120.0));
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        paint_icon(ui.painter(), rect, icon, pal.tertiary);
        ui.add_space(10.0);
        ui.label(RichText::new(title_text).font(theme::semibold(17.0)).color(pal.label));
        ui.add_space(2.0);
        ui.set_max_width(ui.available_width().min(340.0));
        ui.add(egui::Label::new(RichText::new(message).color(pal.secondary)).wrap());
        if let Some(a) = action {
            ui.add_space(12.0);
            clicked = pill_button(ui, a, true).clicked();
        }
    });
    clicked
}

/// Inline "working…" row with a spinner.
pub fn loading_row(ui: &mut Ui, text: &str) {
    ui.horizontal(|ui| {
        spinner(ui, 16.0);
        secondary(ui, text);
    });
}

/// Inline error row with a retry button. Returns true if retry was clicked.
pub fn error_row(ui: &mut Ui, text: &str) -> bool {
    let pal = theme::palette(ui.ctx());
    let mut retry = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(text).color(pal.danger));
        retry = link_button(ui, "Try Again").clicked();
    });
    retry
}
