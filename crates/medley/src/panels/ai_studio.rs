//! AI Studio: turn a vibe into a mix, or describe a layout and get one.

use super::appearance::layout_preview;
use super::tracks::{TrackListOpts, track_list};
use crate::app::{AppState, Loadable, RowAction};
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Color32, CornerRadius, Id, Rect, RichText, Sense, Ui, vec2};
use medley_core::layout::LayoutSpec;
use medley_core::{ProviderKind, Track};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Vibe,
    Layout,
}

const VIBE_EXAMPLES: [&str; 5] = [
    "Late-night drive through a rainy city, synthy but warm",
    "Sunday morning coffee, soft jazz and bossa nova",
    "Focus music for deep work, no vocals",
    "90s summer road trip with the windows down",
    "Dancing alone in the kitchen",
];

const LAYOUT_EXAMPLES: [&str; 4] = [
    "Minimal: just my music and what's playing",
    "DJ setup with search and the queue side by side",
    "Big now playing on the left, library and queue stacked on the right",
    "Everything in tabs, compact for a small laptop screen",
];

enum Intent {
    Vibe,
    SaveMix,
    PlayFound(Vec<Track>, String),
    Row(Vec<Track>, RowAction, String),
    Settings,
    DesignLayout,
    ApplyLayout(LayoutSpec),
    SaveLayout(LayoutSpec),
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let mode_id = Id::new("medley.ai_studio.mode");
    let mut mode = ui.ctx().data(|d| d.get_temp::<Mode>(mode_id)).unwrap_or(Mode::Vibe);
    let mut intents = Vec::new();

    crate::panels::scroll(ui, "ai_studio", |ui| {
        ui.horizontal(|ui| {
            let pal = theme::palette(ui.ctx());
            let (rect, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            widgets::paint_icon(ui.painter(), rect, Icon::Sparkles, pal.accent);
            widgets::large_title(ui, "AI Studio");
        });
        ui.add_space(2.0);
        if widgets::segmented(ui, &mut mode, &[(Mode::Vibe, "Vibe → Mix"), (Mode::Layout, "Design a Layout")]) {
            ui.ctx().data_mut(|d| d.insert_temp(mode_id, mode));
        }
        ui.add_space(8.0);
        match mode {
            Mode::Vibe => vibe(ui, state, &mut intents),
            Mode::Layout => layout(ui, state, &mut intents),
        }
        ui.add_space(8.0);
        widgets::caption(ui, &format!("Using {}", state.ai.backend_label()));
        ui.add_space(8.0);
    });

    for intent in intents {
        match intent {
            Intent::Vibe => state.ai_vibe_mix(),
            Intent::SaveMix => {
                state.save_vibe_mix();
            }
            Intent::PlayFound(tracks, name) => state.play_tracks(tracks, 0, name),
            Intent::Row(tracks, action, name) => state.row_action(&tracks, action, &name),
            Intent::Settings => state.focus_panel(medley_core::layout::PanelKind::Settings),
            Intent::DesignLayout => state.ai_layout(),
            Intent::ApplyLayout(spec) => state.apply_layout(spec),
            Intent::SaveLayout(spec) => state.save_layout(spec),
        }
    }
}

// ---------------------------------------------------------------------------
// Vibe → Mix
// ---------------------------------------------------------------------------

fn vibe(ui: &mut Ui, state: &mut AppState, intents: &mut Vec<Intent>) {
    widgets::caption(
        ui,
        "Describe a moment, a mood or a scene. Medley picks real songs and finds them on the services you've connected.",
    );
    ui.add_space(4.0);
    let submit = prompt_field(ui, &mut state.studio.vibe_prompt, VIBE_EXAMPLES[0], "vibe_prompt");
    chips(ui, &VIBE_EXAMPLES, &mut state.studio.vibe_prompt);
    ui.add_space(6.0);

    let busy = state.is_busy("vibe");
    let unavailable = state.ai_unavailable();
    let can_run = !state.studio.vibe_prompt.trim().is_empty() && !busy && unavailable.is_none();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let clicked = ui
            .add_enabled_ui(can_run, |ui| widgets::pill_button_icon(ui, Some(Icon::Sparkles), "Create Mix", true))
            .inner
            .clicked();
        if clicked || (submit && can_run) {
            intents.push(Intent::Vibe);
        }
        widgets::secondary(ui, "Songs");
        let mut count = match state.studio.vibe_count {
            0..=14 => 10,
            15..=24 => 20,
            25..=39 => 30,
            _ => 50,
        };
        if widgets::segmented(ui, &mut count, &[(10usize, "10"), (20, "20"), (30, "30"), (50, "50")]) {
            state.studio.vibe_count = count;
        }
    });
    unavailable_note(ui, unavailable.as_deref(), intents);
    ui.add_space(10.0);

    match &state.studio.vibe {
        Loadable::Idle => {}
        Loadable::Loading => {
            let (done, total) = state.studio.vibe_progress;
            if total == 0 {
                widgets::loading_row(ui, "Dreaming up songs…");
            } else {
                widgets::loading_row(ui, &format!("Finding songs on your services… {done} of {total}"));
                progress_bar(ui, done as f32 / total.max(1) as f32);
            }
        }
        Loadable::Failed(e) => {
            if widgets::error_row(ui, e) {
                intents.push(Intent::Vibe);
            }
        }
        Loadable::Ready(result) => {
            let found: Vec<Track> = result.tracks.iter().flatten().cloned().collect();
            let total = result.tracks.len();
            let name = result.idea.name.clone();
            widgets::card(ui, |ui| {
                let pal = theme::palette(ui.ctx());
                ui.horizontal(|ui| {
                    widgets::title(ui, &result.idea.name);
                    widgets::badge(ui, "AI", pal.accent);
                });
                if !result.idea.description.is_empty() {
                    widgets::secondary(ui, &result.idea.description);
                }
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} of {total} songs found", found.len()))
                            .font(theme::regular(12.0))
                            .color(pal.secondary),
                    );
                    for kind in ProviderKind::ALL {
                        let n = found.iter().filter(|t| t.provider == kind).count();
                        if n > 0 {
                            widgets::provider_dot(ui, kind, 7.0);
                            ui.label(RichText::new(n.to_string()).font(theme::regular(12.0)).color(pal.secondary));
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let has_songs = !found.is_empty();
                    if ui.add_enabled_ui(has_songs, |ui| widgets::pill_button(ui, "Save as Mix", true)).inner.clicked()
                    {
                        intents.push(Intent::SaveMix);
                    }
                    if ui
                        .add_enabled_ui(has_songs, |ui| widgets::pill_button_icon(ui, Some(Icon::Play), "Play", false))
                        .inner
                        .clicked()
                    {
                        intents.push(Intent::PlayFound(found.clone(), name.clone()));
                    }
                    if ui.add_enabled_ui(can_run, |ui| widgets::pill_button(ui, "Try Again", false)).inner.clicked() {
                        intents.push(Intent::Vibe);
                    }
                });
            });
            ui.add_space(6.0);
            let mut opts = TrackListOpts::new("ai_studio.vibe");
            opts.inline = true;
            opts.numbered = true;
            if let Some(action) = track_list(ui, state, &found, opts) {
                intents.push(Intent::Row(found.clone(), action, name.clone()));
            }
            let missing: Vec<String> = result
                .idea
                .songs
                .iter()
                .zip(&result.tracks)
                .filter(|(_, t)| t.is_none())
                .map(|(s, _)| format!("{} — {}", s.title, s.artist))
                .collect();
            if !missing.is_empty() {
                ui.add_space(6.0);
                widgets::section_header(ui, "Not found on your services");
                for m in missing {
                    widgets::caption(ui, &m);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Design a Layout
// ---------------------------------------------------------------------------

fn layout(ui: &mut Ui, state: &mut AppState, intents: &mut Vec<Intent>) {
    widgets::caption(
        ui,
        "Say how you like to listen and Medley arranges the panels for you. You can still drag everything around afterwards.",
    );
    ui.add_space(4.0);
    let submit = prompt_field(
        ui,
        &mut state.studio.layout_prompt,
        "Big artwork on the left, my library and queue stacked on the right",
        "layout_prompt",
    );
    chips(ui, &LAYOUT_EXAMPLES, &mut state.studio.layout_prompt);
    ui.add_space(6.0);

    let busy = state.is_busy("layout");
    let unavailable = state.ai_unavailable();
    let can_run = !state.studio.layout_prompt.trim().is_empty() && !busy && unavailable.is_none();
    let clicked = ui
        .add_enabled_ui(can_run, |ui| widgets::pill_button_icon(ui, Some(Icon::Sparkles), "Design It", true))
        .inner
        .clicked();
    if clicked || (submit && can_run) {
        intents.push(Intent::DesignLayout);
    }
    unavailable_note(ui, unavailable.as_deref(), intents);
    ui.add_space(10.0);

    match &state.studio.layout {
        Loadable::Idle => {}
        Loadable::Loading => widgets::loading_row(ui, "Designing your layout…"),
        Loadable::Failed(e) => {
            if widgets::error_row(ui, e) {
                intents.push(Intent::DesignLayout);
            }
        }
        Loadable::Ready(spec) => {
            widgets::card(ui, |ui| {
                let pal = theme::palette(ui.ctx());
                let w = ui.available_width().min(440.0);
                layout_preview(ui, spec, vec2(w, w * 10.0 / 16.0));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    widgets::title(ui, &spec.name);
                    if let Some(a) = spec.accent {
                        let [r, g, b] = a.rgb(pal.dark);
                        widgets::badge(ui, a.name(), Color32::from_rgb(r, g, b));
                    }
                    if let Some(t) = spec.theme {
                        let label = match t {
                            medley_core::store::ThemeMode::System => "Auto",
                            medley_core::store::ThemeMode::Light => "Light",
                            medley_core::store::ThemeMode::Dark => "Dark",
                        };
                        widgets::badge(ui, label, pal.secondary);
                    }
                });
                if !spec.description.is_empty() {
                    widgets::secondary(ui, &spec.description);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if widgets::pill_button(ui, "Apply", true).clicked() {
                        intents.push(Intent::ApplyLayout(spec.clone()));
                    }
                    if widgets::pill_button(ui, "Save to Gallery", false).clicked() {
                        intents.push(Intent::SaveLayout(spec.clone()));
                    }
                });
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Large rounded multiline prompt. Returns true on Ctrl/Cmd+Enter.
fn prompt_field(ui: &mut Ui, text: &mut String, hint: &str, id: &str) -> bool {
    let pal = theme::palette(ui.ctx());
    let mut submit = false;
    egui::Frame::NONE
        .fill(pal.fill)
        .corner_radius(CornerRadius::same(theme::R_CARD - 2))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            let resp = ui.add(
                egui::TextEdit::multiline(text)
                    .id_salt(id)
                    .hint_text(RichText::new(hint).color(pal.tertiary))
                    .font(theme::regular(15.0))
                    .frame(egui::Frame::NONE)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            if resp.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
        });
    submit
}

/// Example prompts as tappable capsules.
fn chips(ui: &mut Ui, examples: &[&str], target: &mut String) {
    let pal = theme::palette(ui.ctx());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for example in examples {
            let font = theme::regular(12.0);
            let galley = ui.painter().layout_no_wrap((*example).to_owned(), font, pal.secondary);
            let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(20.0, 10.0), Sense::click());
            let hover = theme::fade(ui.ctx(), resp.id.with("h"), resp.hovered());
            let fill = theme::lerp_color(pal.fill, pal.fill_strong, hover);
            ui.painter().rect_filled(rect, CornerRadius::same(255), fill);
            let color = if hover > 0.5 { pal.label } else { pal.secondary };
            ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *target = (*example).to_owned();
            }
        }
    });
}

fn unavailable_note(ui: &mut Ui, reason: Option<&str>, intents: &mut Vec<Intent>) {
    if let Some(reason) = reason {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            widgets::caption(ui, reason);
            if widgets::link_button(ui, "Open Settings").clicked() {
                intents.push(Intent::Settings);
            }
        });
    }
}

fn progress_bar(ui: &mut Ui, fraction: f32) {
    let pal = theme::palette(ui.ctx());
    let width = ui.available_width().min(360.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 4.0), Sense::hover());
    let shown = theme::spring(ui.ctx(), Id::new("medley.ai_studio.progress"), fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(rect, CornerRadius::same(255), pal.fill_strong);
    let filled =
        Rect::from_min_max(rect.min, egui::pos2(rect.left() + rect.width() * shown.clamp(0.0, 1.0), rect.max.y));
    ui.painter().rect_filled(filled, CornerRadius::same(255), pal.accent);
}
