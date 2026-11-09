//! "Up Next": the queue, shuffle/repeat, Keep the Vibe, and recent history.

use crate::app::{AppState, RowAction};
use crate::panels::tracks::{TrackListOpts, playing_bars, track_list};
use crate::player::Repeat;
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Color32, CornerRadius, Id, Rect, RichText, Sense, Ui, pos2, vec2};
use medley_core::Track;

/// How many past songs the History view lists.
const HISTORY_LIMIT: usize = 50;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    UpNext,
    History,
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    header(ui, state);
    ui.add_space(8.0);
    keep_the_vibe(ui, state);
    ui.add_space(12.0);

    let mode_id = Id::new("medley.queue.mode");
    let mut mode = ui.ctx().data(|d| d.get_temp::<Mode>(mode_id)).unwrap_or_default();
    if widgets::segmented(ui, &mut mode, &[(Mode::UpNext, "Playing Next"), (Mode::History, "History")]) {
        ui.ctx().data_mut(|d| d.insert_temp(mode_id, mode));
    }
    ui.add_space(10.0);

    match mode {
        Mode::UpNext => up_next(ui, state),
        Mode::History => history(ui, state),
    }
}

fn header(ui: &mut Ui, state: &mut AppState) {
    let pal = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        widgets::title(ui, "Up Next");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if !state.player.upcoming().is_empty() && widgets::link_button(ui, "Clear").clicked() {
                state.player.clear_upcoming();
            }
            let (icon, tint, tip) = match state.player.repeat {
                Repeat::Off => (Icon::Repeat, None, "Repeat"),
                Repeat::All => (Icon::Repeat, Some(pal.accent), "Repeat All"),
                Repeat::One => (Icon::RepeatOne, Some(pal.accent), "Repeat One"),
            };
            if widgets::icon_button_colored(ui, icon, 26.0, tip, tint).clicked() {
                state.player.repeat = state.player.repeat.cycle();
            }
            let shuffle_tint = state.player.shuffle.then_some(pal.accent);
            if widgets::icon_button_colored(ui, Icon::Shuffle, 26.0, "Shuffle", shuffle_tint).clicked() {
                let on = !state.player.shuffle;
                state.player.set_shuffle(on);
            }
        });
    });
}

fn keep_the_vibe(ui: &mut Ui, state: &mut AppState) {
    let pal = theme::palette(ui.ctx());
    let unavailable = state.ai_unavailable();
    let busy = state.is_busy("keep_vibe");
    let mut add_now = false;
    let mut open_settings = false;
    let mut on = state.keep_vibe;
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            widgets::paint_icon(ui.painter(), rect, Icon::Sparkles, pal.accent);
            let title_w = (ui.available_width() - 48.0).max(40.0);
            ui.allocate_ui(vec2(title_w, 22.0), |ui| {
                ui.add(egui::Label::new(RichText::new("Keep the Vibe").font(theme::semibold(14.0))).truncate());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(unavailable.is_none(), |ui| {
                    widgets::switch(ui, &mut on);
                });
            });
        });
        // Full width below the title row, so it reads well in narrow panels too.
        widgets::caption(ui, "When Up Next runs low, AI adds songs that fit what's playing.");
        ui.add_space(6.0);
        match &unavailable {
            Some(reason) => {
                ui.horizontal_wrapped(|ui| {
                    widgets::caption(ui, reason);
                    if widgets::link_button(ui, "Settings").clicked() {
                        open_settings = true;
                    }
                });
            }
            None if busy => widgets::loading_row(ui, "Finding songs that fit…"),
            None => {
                let can_add = state.player.current().is_some();
                ui.add_enabled_ui(can_add, |ui| {
                    if widgets::pill_button_icon(ui, Some(Icon::Sparkles), "Add 8 Now", false).clicked() {
                        add_now = true;
                    }
                });
            }
        }
    });
    if on != state.keep_vibe {
        state.keep_vibe = on;
    }
    if add_now {
        state.ai_keep_vibe_now();
    }
    if open_settings {
        state.focus_panel(medley_core::layout::PanelKind::Settings);
    }
}

fn up_next(ui: &mut Ui, state: &mut AppState) {
    if state.player.current().is_none() && state.player.upcoming().is_empty() {
        widgets::empty_state(ui, Icon::Queue, "Nothing up next", "Play something, or turn on Keep the Vibe.", None);
        return;
    }

    if let Some(track) = state.player.current().cloned() {
        widgets::section_header(ui, "Now Playing");
        if now_row(ui, state, &track) {
            state.toggle_play();
        }
        if !state.player.context.is_empty() {
            ui.add_space(2.0);
            widgets::caption(ui, &format!("Playing from {}", state.player.context));
        }
        ui.add_space(10.0);
    }

    let count = state.player.upcoming().len();
    if count == 0 {
        widgets::caption(ui, "Nothing up next. Songs you add with Play Next or Add to Up Next appear here.");
        return;
    }
    ui.horizontal(|ui| {
        widgets::section_header(ui, "Playing Next");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::caption(ui, &songs_label(count));
        });
    });

    let action = {
        let st: &AppState = state;
        let opts = TrackListOpts {
            remove_label: Some("Remove from Up Next"),
            reorderable: true,
            ..TrackListOpts::new("queue")
        };
        track_list(ui, st, st.player.upcoming(), opts)
    };
    if let Some(action) = action {
        match action {
            RowAction::Play(i) => state.play_upcoming(i),
            RowAction::Remove(i) => {
                state.player.remove_upcoming(i);
            }
            RowAction::Move { from, to } => state.player.move_upcoming(from, to),
            other => {
                let upcoming = state.player.upcoming().to_vec();
                state.row_action(&upcoming, other, "Up Next");
            }
        }
    }
}

fn history(ui: &mut Ui, state: &mut AppState) {
    let recent: Vec<Track> = state.player.history().iter().rev().take(HISTORY_LIMIT).cloned().collect();
    if recent.is_empty() {
        widgets::empty_state(
            ui,
            Icon::Queue,
            "No history yet",
            "Songs you've played in this session show up here.",
            None,
        );
        return;
    }
    let action = {
        let st: &AppState = state;
        track_list(ui, st, &recent, TrackListOpts::new("queue-history"))
    };
    match action {
        // Replay a past song now without throwing away the rest of the queue.
        Some(RowAction::Play(i)) => {
            if let Some(track) = recent.get(i).cloned() {
                if state.player.current().is_some() {
                    state.player.play_next(track);
                    state.next();
                } else {
                    state.play_tracks(vec![track], 0, "History");
                }
            }
        }
        Some(other) => state.row_action(&recent, other, "History"),
        None => {}
    }
}

/// The current song as a compact row. Returns true when clicked (toggle play).
fn now_row(ui: &mut Ui, state: &AppState, track: &Track) -> bool {
    let pal = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered());
    let bg = theme::lerp_color(pal.accent_soft, pal.fill_strong, hover * 0.4);
    ui.painter().rect_filled(rect, CornerRadius::same(theme::R_CONTROL + 2), bg);

    let art = Rect::from_min_size(pos2(rect.left() + 8.0, rect.center().y - 20.0), vec2(40.0, 40.0));
    widgets::paint_artwork(ui, art, track.artwork.as_deref(), theme::R_ART - 1, &track.key());
    ui.painter().rect_filled(art, CornerRadius::same(theme::R_ART - 1), Color32::from_black_alpha(90));
    if response.hovered() {
        let icon = if state.is_playing() { Icon::Pause } else { Icon::Play };
        widgets::paint_icon(ui.painter(), Rect::from_center_size(art.center(), vec2(16.0, 16.0)), icon, Color32::WHITE);
    } else {
        playing_bars(ui, art.center(), Color32::WHITE, state.is_playing());
    }

    let x = art.right() + 12.0;
    let max_w = (rect.right() - 12.0 - x).max(20.0);
    let title = ui.painter().layout_job(one_line(&track.title, theme::semibold(13.5), pal.label, max_w));
    let artist = ui.painter().layout_job(one_line(&track.artist_line(), theme::regular(12.0), pal.secondary, max_w));
    let cy = rect.center().y;
    ui.painter().galley(pos2(x, cy - title.size().y + 1.0), title, pal.label);
    ui.painter().galley(pos2(x, cy + 2.0), artist, pal.secondary);
    ui.painter().circle_filled(pos2(rect.right() - 14.0, cy), 3.5, theme::provider_color(track.provider));

    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn one_line(text: &str, font: egui::FontId, color: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    job
}

fn songs_label(n: usize) -> String {
    if n == 1 { "1 song".into() } else { format!("{n} songs") }
}
