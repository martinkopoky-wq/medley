//! The big "Now Playing" view: artwork, transport, and AI liner notes.

use crate::app::{AppState, Loadable};
use crate::player::Status;
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Align2, Color32, CornerRadius, Id, Rect, RichText, Sense, Shadow, Ui, pos2, vec2};
use medley_ai::LinerNotes;
use medley_core::layout::PanelKind;
use medley_core::{Playability, ProviderKind, Track, format_duration};

/// Width of the text/control column under the artwork.
const COLUMN_MAX: f32 = 460.0;

#[derive(Default)]
struct Actions {
    previous: bool,
    toggle: bool,
    next: bool,
    seek: Option<u64>,
    open: Option<String>,
    notes: bool,
    settings: bool,
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let Some(now) = &state.now else {
        widgets::empty_state(ui, Icon::Note, "Not Playing", "Pick a song anywhere to start.", None);
        return;
    };
    let track = now.track.clone();
    let notes = now.notes.clone();
    let playing = state.is_playing();
    let loading = state.status == Status::Loading;
    let position = state.position_ms();
    let duration = state.duration_ms();
    let ai_unavailable = state.ai_unavailable();
    let has_queue = state.player.current().is_some();

    let panel_height = ui.available_height();
    let mut act = Actions::default();
    egui::ScrollArea::vertical().id_salt("medley.now-playing").auto_shrink([false, false]).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            let avail_w = ui.available_width();
            let column = (avail_w - 24.0).clamp(160.0, COLUMN_MAX);
            ui.add_space(10.0);

            artwork(ui, &track, playing, avail_w, panel_height);
            ui.add_space(20.0);

            // Title block.
            let pal = theme::palette(ui.ctx());
            ui.add(
                egui::Label::new(RichText::new(&track.title).font(theme::semibold(20.0)).color(pal.label)).truncate(),
            );
            ui.add(
                egui::Label::new(RichText::new(track.artist_line()).font(theme::regular(15.0)).color(pal.secondary))
                    .truncate(),
            );
            if let Some(album) = track.album.as_deref().filter(|a| !a.is_empty()) {
                ui.add(
                    egui::Label::new(RichText::new(album).font(theme::regular(12.5)).color(pal.tertiary)).truncate(),
                );
            }
            ui.add_space(8.0);
            meta_row(ui, &track);
            if track.playability == Playability::Preview {
                ui.add_space(4.0);
                widgets::caption(ui, preview_note(track.provider));
            }
            if track.permalink.is_some() {
                ui.add_space(2.0);
                if widgets::link_button(ui, &format!("Open in {}", track.provider.name())).clicked() {
                    act.open = track.permalink.clone();
                }
            }
            ui.add_space(18.0);

            // Scrubber with elapsed / remaining underneath.
            if let Some(ms) = widgets::scrubber(ui, Id::new("medley.now-playing.scrub"), position, duration, column) {
                act.seek = Some(ms);
            }
            let (times, _) = ui.allocate_exact_size(vec2(column, 14.0), Sense::hover());
            let font = theme::regular(11.0);
            ui.painter().text(
                times.left_center(),
                Align2::LEFT_CENTER,
                format_duration(position),
                font.clone(),
                pal.secondary,
            );
            let remaining = format!("-{}", format_duration(duration.saturating_sub(position)));
            ui.painter().text(times.right_center(), Align2::RIGHT_CENTER, remaining, font, pal.secondary);
            ui.add_space(12.0);

            transport(ui, &mut act, playing, loading, has_queue);
            ui.add_space(26.0);

            // Liner notes card, centered at the column width.
            let card_w = (avail_w - 24.0).clamp(160.0, COLUMN_MAX + 60.0);
            ui.horizontal(|ui| {
                ui.add_space(((avail_w - card_w) / 2.0).max(0.0));
                ui.vertical(|ui| {
                    ui.set_width(card_w);
                    liner_notes(ui, &notes, ai_unavailable.as_deref(), &mut act);
                });
            });
            ui.add_space(16.0);
        });
    });

    if act.previous {
        state.previous();
    }
    if act.toggle {
        state.toggle_play();
    }
    if act.next {
        state.next();
    }
    if let Some(ms) = act.seek {
        state.seek(ms);
    }
    if let Some(url) = act.open {
        state.open_url(&url);
    }
    if act.notes {
        state.ai_liner_notes();
    }
    if act.settings {
        state.focus_panel(PanelKind::Settings);
    }
}

/// Large artwork with a soft shadow; shrinks a little while paused, like Music.app.
fn artwork(ui: &mut Ui, track: &Track, playing: bool, avail_w: f32, panel_h: f32) {
    let pal = theme::palette(ui.ctx());
    let side = (avail_w - 48.0).min(panel_h * 0.52).clamp(120.0, 420.0);
    let (slot, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let scale = theme::spring(ui.ctx(), Id::new("medley.now-playing.art-scale"), if playing { 1.0 } else { 0.88 });
    let rect = Rect::from_center_size(slot.center(), slot.size() * scale);
    let radius = CornerRadius::same(theme::R_CARD);
    let shadow = Shadow {
        offset: [0, (18.0 * scale) as i8],
        blur: 48,
        spread: 0,
        color: Color32::from_black_alpha(if pal.dark { 70 } else { 40 }),
    };
    ui.painter().add(shadow.as_shape(rect, radius));
    let url = track.artwork_large.as_deref().or(track.artwork.as_deref());
    widgets::paint_artwork(ui, rect, url, theme::R_CARD, &track.key());
}

/// Service dot + name, plus a PREVIEW badge, as one centered row.
fn meta_row(ui: &mut Ui, track: &Track) {
    let pal = theme::palette(ui.ctx());
    let name = ui.painter().layout_no_wrap(track.provider.name().to_owned(), theme::regular(12.0), pal.secondary);
    let badge = (track.playability == Playability::Preview)
        .then(|| ui.painter().layout_no_wrap("PREVIEW".to_owned(), theme::semibold(9.5), pal.secondary));
    let badge_w = badge.as_ref().map_or(0.0, |g| g.size().x + 10.0 + 8.0);
    let width = 8.0 + 6.0 + name.size().x + badge_w;
    let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    let cy = rect.center().y;
    ui.painter().circle_filled(pos2(rect.left() + 4.0, cy), 4.0, theme::provider_color(track.provider));
    let name_w = name.size().x;
    ui.painter().galley(pos2(rect.left() + 14.0, cy - name.size().y / 2.0), name, pal.secondary);
    if let Some(g) = badge {
        let left = rect.left() + 14.0 + name_w + 8.0;
        let b = Rect::from_min_size(pos2(left, cy - 8.0), vec2(g.size().x + 10.0, 16.0));
        ui.painter().rect_filled(b, CornerRadius::same(255), pal.fill);
        ui.painter().galley(b.center() - g.size() / 2.0, g, pal.secondary);
    }
}

fn preview_note(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::AppleMusic => "Apple Music plays 30-second previews here.",
        _ => "Only a 30-second preview is available for this track.",
    }
}

fn transport(ui: &mut Ui, act: &mut Actions, playing: bool, loading: bool, has_queue: bool) {
    let size = vec2(40.0 + 20.0 + 56.0 + 20.0 + 40.0, 58.0);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 20.0;
        ui.add_enabled_ui(has_queue, |ui| {
            if widgets::icon_button(ui, Icon::Previous, 40.0, "Previous").clicked() {
                act.previous = true;
            }
        });
        if widgets::play_button(ui, playing, loading, 56.0).clicked() {
            act.toggle = true;
        }
        ui.add_enabled_ui(has_queue, |ui| {
            if widgets::icon_button(ui, Icon::Next, 40.0, "Next").clicked() {
                act.next = true;
            }
        });
    });
}

fn liner_notes(ui: &mut Ui, notes: &Loadable<LinerNotes>, ai_unavailable: Option<&str>, act: &mut Actions) {
    let pal = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                widgets::paint_icon(ui.painter(), r, Icon::Sparkles, pal.accent);
                ui.label(RichText::new("Liner Notes").font(theme::semibold(14.0)).color(pal.label));
                if matches!(notes, Loadable::Ready(_)) && ai_unavailable.is_none() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::icon_button(ui, Icon::Refresh, 22.0, "Write new notes").clicked() {
                            act.notes = true;
                        }
                    });
                }
            });
            ui.add_space(6.0);
            match notes {
                Loadable::Idle => {
                    widgets::caption(ui, "A few words about this song — what to listen for and how it feels.");
                    ui.add_space(8.0);
                    ui.add_enabled_ui(ai_unavailable.is_none(), |ui| {
                        if widgets::pill_button_icon(ui, Some(Icon::Sparkles), "Write Liner Notes", true).clicked() {
                            act.notes = true;
                        }
                    });
                    if let Some(reason) = ai_unavailable {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            widgets::caption(ui, reason);
                            if widgets::link_button(ui, "Settings").clicked() {
                                act.settings = true;
                            }
                        });
                    }
                }
                Loadable::Loading => widgets::loading_row(ui, "Listening closely…"),
                Loadable::Failed(e) => {
                    if widgets::error_row(ui, e) {
                        act.notes = true;
                    }
                }
                Loadable::Ready(n) => written_notes(ui, n),
            }
        });
    });
}

fn written_notes(ui: &mut Ui, notes: &LinerNotes) {
    let pal = theme::palette(ui.ctx());
    ui.add(egui::Label::new(RichText::new(&notes.headline).font(theme::semibold(15.0)).color(pal.label)).wrap());
    ui.add_space(6.0);
    for para in notes.notes.split("\n").map(str::trim).filter(|p| !p.is_empty()) {
        ui.add(
            egui::Label::new(RichText::new(para).font(theme::regular(13.5)).color(pal.label).line_height(Some(20.0)))
                .wrap(),
        );
        ui.add_space(6.0);
    }
    if !notes.listen_for.trim().is_empty() {
        ui.add_space(2.0);
        ui.label(RichText::new("Listen for").font(theme::semibold(12.0)).color(pal.secondary));
        ui.add(
            egui::Label::new(
                RichText::new(notes.listen_for.trim())
                    .font(theme::regular(13.0))
                    .color(pal.label)
                    .line_height(Some(19.0)),
            )
            .wrap(),
        );
    }
    let moods: Vec<&str> = notes.mood.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).take(5).collect();
    if !moods.is_empty() {
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for mood in moods {
                capsule(ui, mood);
            }
        });
    }
    ui.add_space(8.0);
    ui.label(RichText::new("AI-written · may contain mistakes").font(theme::regular(10.5)).color(pal.tertiary));
}

fn capsule(ui: &mut Ui, text: &str) {
    let pal = theme::palette(ui.ctx());
    let g = ui.painter().layout_no_wrap(text.to_lowercase(), theme::regular(11.5), pal.label);
    let (rect, _) = ui.allocate_exact_size(g.size() + vec2(18.0, 8.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(255), pal.fill);
    ui.painter().galley(rect.center() - g.size() / 2.0, g, pal.label);
}
