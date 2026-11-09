//! The shared song table used by Browse, Search, Up Next and AI Studio.
//!
//! Virtualized (only visible rows are laid out, so 2000-song libraries cost
//! the same as 20), Music.app-style rows, right-click and "…" menus, drag
//! songs onto mixes in the Library, and drag to reorder when allowed.

use crate::app::{AppState, MixTarget, RowAction};
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align2, Color32, CornerRadius, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use medley_core::{Playability, Track, format_duration};

pub const ROW_HEIGHT: f32 = 50.0;

/// Drag payload for a song row. Library mix rows accept it as a drop.
#[derive(Clone, Debug)]
pub struct DragTrack {
    pub list: Id,
    pub index: usize,
    pub track: Track,
}

#[derive(Clone, Copy)]
pub struct TrackListOpts<'a> {
    /// Unique per list (also scopes scroll position and selection).
    pub id: &'a str,
    /// Show the row number instead of a play glyph in the first column.
    pub numbered: bool,
    /// Offer "Remove" in the menu with this label (e.g. "Remove from Mix").
    pub remove_label: Option<&'a str>,
    /// Allow drag-to-reorder within the list (emits `RowAction::Move`).
    pub reorderable: bool,
    /// Render all rows without an inner scroll area (for short lists inside a scrolling panel).
    pub inline: bool,
}

impl<'a> TrackListOpts<'a> {
    pub fn new(id: &'a str) -> Self {
        Self { id, numbered: false, remove_label: None, reorderable: false, inline: false }
    }
}

/// Draw `tracks`. Returns at most one action per frame for the caller to apply
/// (usually via [`AppState::row_action`]).
pub fn track_list(ui: &mut Ui, state: &AppState, tracks: &[Track], opts: TrackListOpts<'_>) -> Option<RowAction> {
    let list_id = Id::new(("medley.tracks", opts.id));
    let mut action = None;
    let show_album = ui.available_width() > 620.0;
    if opts.inline {
        for (i, t) in tracks.iter().enumerate() {
            if let Some(a) = row(ui, state, list_id, i, t, opts, show_album) {
                action = Some(a);
            }
        }
        return action;
    }
    egui::ScrollArea::vertical().id_salt(list_id).auto_shrink([false, false]).show_rows(
        ui,
        ROW_HEIGHT,
        tracks.len(),
        |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                if let Some(a) = row(ui, state, list_id, i, &tracks[i], opts, show_album) {
                    action = Some(a);
                }
            }
        },
    );
    action
}

fn row(
    ui: &mut Ui,
    state: &AppState,
    list_id: Id,
    index: usize,
    track: &Track,
    opts: TrackListOpts<'_>,
    show_album: bool,
) -> Option<RowAction> {
    let pal = theme::palette(ui.ctx());
    let row_id = list_id.with(index);
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let mut action = None;
    let current = state.is_current(track);
    let selected_id = list_id.with("selected");
    let selected = ui.ctx().data(|d| d.get_temp::<usize>(selected_id)) == Some(index);
    let hovered = response.hovered() || response.context_menu_opened();

    // Background.
    let hover_t = theme::fade(ui.ctx(), row_id.with("hover"), hovered);
    let bg_rect = rect.shrink2(vec2(0.0, 1.0));
    if selected {
        ui.painter().rect_filled(bg_rect, CornerRadius::same(theme::R_CONTROL), pal.accent_soft);
    } else if hover_t > 0.0 {
        let c = pal.hover;
        let a = (c.a() as f32 * hover_t) as u8;
        ui.painter().rect_filled(
            bg_rect,
            CornerRadius::same(theme::R_CONTROL),
            Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), a),
        );
    }

    let mut x = rect.left() + 6.0;
    let cy = rect.center().y;

    // Leading column: number / playing bars.
    let lead_w = if opts.numbered { 28.0 } else { 0.0 };
    if opts.numbered {
        let lead = Rect::from_center_size(pos2(x + lead_w / 2.0, cy), vec2(lead_w, ROW_HEIGHT));
        if current {
            playing_bars(ui, lead.center(), pal.accent, state.is_playing());
        } else {
            ui.painter().text(
                lead.center(),
                Align2::CENTER_CENTER,
                (index + 1).to_string(),
                theme::regular(12.0),
                pal.secondary,
            );
        }
        x += lead_w + 4.0;
    }

    // Artwork with hover play overlay.
    let art = Rect::from_min_size(pos2(x, cy - 19.0), vec2(38.0, 38.0));
    widgets::paint_artwork(ui, art, track.artwork.as_deref(), theme::R_ART - 2, &track.key());
    let art_resp = ui.interact(art, row_id.with("art"), Sense::click());
    if art_resp.hovered() || (current && !opts.numbered) {
        ui.painter().rect_filled(art, CornerRadius::same(theme::R_ART - 2), Color32::from_black_alpha(110));
        if current && !art_resp.hovered() {
            playing_bars(ui, art.center(), Color32::WHITE, state.is_playing());
        } else {
            widgets::paint_icon(
                ui.painter(),
                Rect::from_center_size(art.center(), vec2(16.0, 16.0)),
                Icon::Play,
                Color32::WHITE,
            );
        }
    }
    if art_resp.clicked() {
        action = Some(RowAction::Play(index));
    }
    x = art.right() + 12.0;

    // Trailing columns, right to left: more, duration, badges.
    let right = rect.right() - 6.0;
    let more_rect = Rect::from_center_size(pos2(right - 12.0, cy), vec2(24.0, 24.0));
    let dur_right = more_rect.left() - 8.0;
    let dur_text = if track.duration_ms > 0 { format_duration(track.duration_ms) } else { String::new() };
    ui.painter().text(pos2(dur_right, cy), Align2::RIGHT_CENTER, dur_text, theme::regular(12.0), pal.secondary);
    let mut badge_right = dur_right - 50.0;
    ui.painter().circle_filled(pos2(badge_right, cy), 3.5, theme::provider_color(track.provider));
    badge_right -= 10.0;
    match track.playability {
        Playability::Preview => {
            badge_right = paint_badge(ui, badge_right, cy, "PREVIEW", pal.secondary);
        }
        Playability::Unavailable => {
            badge_right = paint_badge(ui, badge_right, cy, "UNAVAILABLE", pal.danger);
        }
        Playability::Full => {}
    }

    // Title + artist, and album column when wide.
    let text_right = badge_right - 10.0;
    let (title_right, album_rect) = if show_album {
        let album_w = ((text_right - x) * 0.38).clamp(80.0, 260.0);
        (
            text_right - album_w - 12.0,
            Some(Rect::from_min_max(pos2(text_right - album_w, rect.top()), pos2(text_right, rect.bottom()))),
        )
    } else {
        (text_right, None)
    };
    let title_color = if current { pal.accent } else { pal.label };
    let max_w = (title_right - x).max(20.0);
    let title = ui.painter().layout_job(truncated(&track.title, theme::regular(13.5), title_color, max_w));
    let artist = ui.painter().layout_job(truncated(&track.artist_line(), theme::regular(12.0), pal.secondary, max_w));
    ui.painter().galley(pos2(x, cy - title.size().y + 1.0), title, title_color);
    ui.painter().galley(pos2(x, cy + 2.0), artist, pal.secondary);
    if let (Some(r), Some(album)) = (album_rect, track.album.as_deref()) {
        let g = ui.painter().layout_job(truncated(album, theme::regular(12.5), pal.secondary, r.width()));
        ui.painter().galley(pos2(r.left(), cy - g.size().y / 2.0), g, pal.secondary);
    }

    // "…" button (visible on hover).
    let more = ui.interact(more_rect, row_id.with("more"), Sense::click());
    if hovered || more.hovered() {
        if more.hovered() {
            ui.painter().circle_filled(more_rect.center(), 12.0, pal.fill);
        }
        widgets::paint_icon(ui.painter(), more_rect.shrink(5.0), Icon::More, pal.label);
    }
    egui::Popup::menu(&more).width(200.0).show(|ui| {
        if let Some(a) = menu(ui, state, index, track, opts) {
            action = Some(a);
        }
    });
    response.context_menu(|ui| {
        if let Some(a) = menu(ui, state, index, track, opts) {
            action = Some(a);
        }
    });

    // Click selects, double-click plays.
    if response.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(selected_id, index));
    }
    if response.double_clicked() {
        action = Some(RowAction::Play(index));
    }

    // Drag: payload for mixes / reordering.
    if response.drag_started() {
        response.dnd_set_drag_payload(DragTrack { list: list_id, index, track: track.clone() });
    }
    if response.dragged() {
        drag_ghost(ui, track);
    }
    if opts.reorderable
        && let Some(payload) = response.dnd_hover_payload::<DragTrack>()
        && payload.list == list_id
        && payload.index != index
    {
        let below = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| p.y > cy);
        let y = if below { rect.bottom() } else { rect.top() };
        ui.painter().hline(rect.x_range(), y, Stroke::new(2.0, pal.accent));
        if let Some(payload) = response.dnd_release_payload::<DragTrack>() {
            let mut to = if below { index + 1 } else { index };
            if payload.index < to {
                to -= 1;
            }
            if to != payload.index {
                action = Some(RowAction::Move { from: payload.index, to });
            }
        }
    }
    action
}

fn menu(ui: &mut Ui, state: &AppState, index: usize, track: &Track, opts: TrackListOpts<'_>) -> Option<RowAction> {
    let mut action = None;
    if ui.button("Play").clicked() {
        action = Some(RowAction::Play(index));
    }
    if ui.button("Play Next").clicked() {
        action = Some(RowAction::PlayNext(index));
    }
    if ui.button("Add to Up Next").clicked() {
        action = Some(RowAction::AddToQueue(index));
    }
    ui.menu_button("Add to Mix", |ui| {
        if ui.button("New Mix").clicked() {
            action = Some(RowAction::AddToMix(index, MixTarget::New));
        }
        if !state.mixes.is_empty() {
            ui.separator();
        }
        for mix in &state.mixes {
            if ui.button(&mix.name).clicked() {
                action = Some(RowAction::AddToMix(index, MixTarget::Existing(mix.id.clone())));
            }
        }
    });
    if track.permalink.is_some() {
        ui.separator();
        if ui.button(format!("Open in {}", track.provider.name())).clicked() {
            action = Some(RowAction::OpenInService(index));
        }
    }
    if let Some(label) = opts.remove_label {
        ui.separator();
        if ui.button(label).clicked() {
            action = Some(RowAction::Remove(index));
        }
    }
    if action.is_some() {
        ui.close();
    }
    action
}

fn paint_badge(ui: &Ui, right: f32, cy: f32, text: &str, color: Color32) -> f32 {
    let g = ui.painter().layout_no_wrap(text.to_owned(), theme::semibold(9.0), color);
    let r = Rect::from_min_max(pos2(right - g.size().x - 10.0, cy - 8.0), pos2(right, cy + 8.0));
    ui.painter().rect_stroke(
        r,
        CornerRadius::same(4),
        Stroke::new(1.0, color.gamma_multiply(0.6)),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(r.center() - g.size() / 2.0, g, color);
    r.left() - 8.0
}

fn truncated(text: &str, font: egui::FontId, color: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    job
}

/// Three bouncing bars, the "now playing" glyph (static when paused).
pub fn playing_bars(ui: &Ui, center: egui::Pos2, color: Color32, animate: bool) {
    let t = ui.input(|i| i.time) as f32;
    let reduce = ui.ctx().global_style().animation_time <= 0.0;
    for (k, phase) in [0.0f32, 1.7, 0.9].iter().enumerate() {
        let h =
            if animate && !reduce { 4.0 + 8.0 * (0.5 + 0.5 * (t * 7.0 + phase).sin()) } else { 6.0 + k as f32 * 2.0 };
        let x = center.x - 5.0 + k as f32 * 5.0;
        let r = Rect::from_min_max(pos2(x - 1.5, center.y + 6.0 - h), pos2(x + 1.5, center.y + 6.0));
        ui.painter().rect_filled(r, CornerRadius::same(1), color);
    }
    if animate && !reduce {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// A small floating label that follows the pointer while dragging a song.
fn drag_ghost(ui: &Ui, track: &Track) {
    let Some(pos) = ui.ctx().pointer_interact_pos() else { return };
    let pal = theme::palette(ui.ctx());
    egui::Area::new(Id::new("medley.drag-ghost"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + vec2(12.0, 8.0))
        .interactable(false)
        .show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(pal.elevated)
                .corner_radius(CornerRadius::same(theme::R_CONTROL))
                .stroke(Stroke::new(1.0, pal.separator))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        widgets::artwork(ui, track.artwork.as_deref(), 24.0, 4, &track.key());
                        ui.label(&track.title);
                    });
                });
        });
}
