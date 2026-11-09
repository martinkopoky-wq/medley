//! The Library sidebar: Home, Medley Mixes (drop songs onto them), and each
//! connected service's Liked Songs and playlists — like Music.app's sidebar.

use crate::app::{AccountState, AppState, Loadable, MixTarget, View};
use crate::panels::tracks::DragTrack;
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{
    self, Color32, CornerRadius, Id, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use medley_core::ProviderKind;
use medley_core::layout::PanelKind;

const ROW_H: f32 = 30.0;
const LEAD: f32 = 22.0;

/// What sits at the start of a sidebar row.
enum Lead<'a> {
    Icon(Icon),
    Art { url: Option<&'a str>, seed: &'a str },
}

/// Per-row state that lives in egui's temp memory.
fn rename_id() -> Id {
    Id::new("medley.library.rename")
}
fn confirm_delete_id() -> Id {
    Id::new("medley.library.confirm-delete")
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().id_salt("medley.library.scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add_space(2.0);

        let home = nav_row(ui, Id::new("lib.home"), state.view == View::Home, Lead::Icon(Icon::Note), "Home", None);
        if home.clicked() {
            state.open_view(View::Home);
        }

        ui.add_space(10.0);
        mixes_section(ui, state);

        ui.add_space(6.0);
        providers_section(ui, state);
        ui.add_space(12.0);
    });
}

// ---------------------------------------------------------------------------
// Mixes
// ---------------------------------------------------------------------------

struct MixRow {
    id: String,
    name: String,
    art: Option<String>,
    counts: [usize; 3],
}

fn mixes_section(ui: &mut Ui, state: &mut AppState) {
    widgets::section_header(ui, "Medley Mixes");

    let rows: Vec<MixRow> = state
        .mixes
        .iter()
        .map(|m| MixRow {
            id: m.id.clone(),
            name: m.name.clone(),
            art: m.tracks.iter().find_map(|t| t.artwork.clone()),
            counts: m.provider_counts(),
        })
        .collect();

    let renaming: Option<(String, String)> = ui.ctx().data(|d| d.get_temp(rename_id()));
    let confirm: Option<String> = ui.ctx().data(|d| d.get_temp(confirm_delete_id()));

    for row in &rows {
        let row_id = Id::new(("lib.mix", &row.id));

        // Inline rename replaces the row.
        if let Some((rid, mut text)) = renaming.clone().filter(|(rid, _)| *rid == row.id) {
            let done = rename_row(ui, row_id, &row.art, &row.id, &mut text);
            match done {
                RenameOutcome::Editing => {
                    ui.ctx().data_mut(|d| d.insert_temp(rename_id(), (rid, text)));
                }
                RenameOutcome::Commit => {
                    let name = text.trim().to_owned();
                    if !name.is_empty() {
                        state.edit_mix(&row.id, |m| m.name = name);
                    }
                    ui.ctx().data_mut(|d| d.remove::<(String, String)>(rename_id()));
                }
                RenameOutcome::Cancel => {
                    ui.ctx().data_mut(|d| d.remove::<(String, String)>(rename_id()));
                }
            }
            continue;
        }

        let selected = state.view == View::Mix(row.id.clone());
        let dots = row.counts.iter().filter(|c| **c > 0).count() as f32 * 9.0;
        let resp = nav_row_padded(
            ui,
            row_id,
            selected,
            Lead::Art { url: row.art.as_deref(), seed: &row.id },
            &row.name,
            None,
            dots,
        );
        paint_provider_dots(ui, resp.rect, row.counts);

        // Drop songs onto a mix.
        if resp.dnd_hover_payload::<DragTrack>().is_some() {
            drop_outline(ui, resp.rect);
        }
        if let Some(payload) = resp.dnd_release_payload::<DragTrack>() {
            state.add_to_mix(MixTarget::Existing(row.id.clone()), vec![payload.track.clone()]);
        }

        if resp.clicked() {
            state.open_view(View::Mix(row.id.clone()));
        }

        let menu = resp.context_menu(|ui| {
            if ui.button("Rename…").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(rename_id(), (row.id.clone(), row.name.clone())));
                ui.close();
            }
            if ui.button("Play").clicked() {
                if let Some(m) = state.mix(&row.id) {
                    let tracks = m.tracks.clone();
                    let name = m.name.clone();
                    state.play_tracks(tracks, 0, name);
                }
                ui.close();
            }
            ui.separator();
            let pal = theme::palette(ui.ctx());
            if confirm.as_deref() == Some(row.id.as_str()) {
                let btn = ui.button(RichText::new("Confirm Delete").color(pal.danger));
                if btn.clicked() {
                    state.delete_mix(&row.id);
                    ui.ctx().data_mut(|d| d.remove::<String>(confirm_delete_id()));
                    ui.close();
                }
            } else if ui.button(RichText::new("Delete Mix…").color(pal.danger)).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(confirm_delete_id(), row.id.clone()));
            }
        });
        if menu.is_none() && confirm.as_deref() == Some(row.id.as_str()) {
            ui.ctx().data_mut(|d| d.remove::<String>(confirm_delete_id()));
        }
    }

    // "New Mix" — click to create, or drop a song on it.
    let new_resp = nav_row(ui, Id::new("lib.new-mix"), false, Lead::Icon(Icon::Plus), "New Mix", None);
    if new_resp.dnd_hover_payload::<DragTrack>().is_some() {
        drop_outline(ui, new_resp.rect);
    }
    if let Some(payload) = new_resp.dnd_release_payload::<DragTrack>() {
        state.add_to_mix(MixTarget::New, vec![payload.track.clone()]);
    } else if new_resp.clicked() {
        let name = format!("New Mix {}", state.mixes.len() + 1);
        let id = state.create_mix(name.clone(), Vec::new());
        state.open_view(View::Mix(id.clone()));
        ui.ctx().data_mut(|d| d.insert_temp(rename_id(), (id, name)));
    }
    new_resp.on_hover_text("Create a mix — or drop a song here");
}

enum RenameOutcome {
    Editing,
    Commit,
    Cancel,
}

fn rename_row(ui: &mut Ui, id: Id, art: &Option<String>, seed: &str, text: &mut String) -> RenameOutcome {
    let pal = theme::palette(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(theme::R_CONTROL), pal.accent_soft);
    let lead = Rect::from_center_size(pos2(rect.left() + 6.0 + LEAD / 2.0, rect.center().y), vec2(LEAD, LEAD));
    widgets::paint_artwork(ui, lead, art.as_deref(), 4, seed);
    let field =
        Rect::from_min_max(pos2(lead.right() + 8.0, rect.top() + 3.0), pos2(rect.right() - 6.0, rect.bottom() - 3.0));
    let mut child =
        ui.new_child(egui::UiBuilder::new().max_rect(field).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let edit_id = id.with("edit");
    let resp = child.add(
        egui::TextEdit::singleline(text)
            .id(edit_id)
            .font(theme::regular(13.0))
            .desired_width(field.width())
            .frame(egui::Frame::NONE),
    );
    // Focus the field the first frame it appears.
    let first = !ui.ctx().data(|d| d.get_temp::<bool>(edit_id.with("seen")).unwrap_or(false));
    if first {
        resp.request_focus();
        ui.ctx().data_mut(|d| d.insert_temp(edit_id.with("seen"), true));
    }
    let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    let outcome = if escape {
        RenameOutcome::Cancel
    } else if enter || (resp.lost_focus() && !first) {
        RenameOutcome::Commit
    } else {
        RenameOutcome::Editing
    };
    if !matches!(outcome, RenameOutcome::Editing) {
        ui.ctx().data_mut(|d| d.remove::<bool>(edit_id.with("seen")));
    }
    outcome
}

fn paint_provider_dots(ui: &Ui, rect: Rect, counts: [usize; 3]) {
    let mut x = rect.right() - 10.0;
    for (i, kind) in ProviderKind::ALL.iter().enumerate().rev() {
        if counts[i] > 0 {
            ui.painter().circle_filled(pos2(x, rect.center().y), 3.0, theme::provider_color(*kind));
            x -= 9.0;
        }
    }
}

fn drop_outline(ui: &Ui, rect: Rect) {
    let pal = theme::palette(ui.ctx());
    ui.painter().rect_filled(rect, CornerRadius::same(theme::R_CONTROL), pal.accent_soft);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::R_CONTROL),
        Stroke::new(1.5, pal.accent),
        StrokeKind::Inside,
    );
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

fn providers_section(ui: &mut Ui, state: &mut AppState) {
    let order = {
        let mut order = state.settings().provider_order.clone();
        for k in ProviderKind::ALL {
            if !order.contains(&k) {
                order.push(k);
            }
        }
        order
    };

    let restoring: Vec<ProviderKind> = order
        .iter()
        .copied()
        .filter(|k| matches!(state.accounts.get(k), Some(AccountState::Restoring | AccountState::Connecting)))
        .collect();
    let connected: Vec<ProviderKind> = order.iter().copied().filter(|k| state.is_connected(*k)).collect();

    for kind in &connected {
        provider_section(ui, state, *kind);
        ui.add_space(8.0);
    }

    for kind in &restoring {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            widgets::spinner(ui, 14.0);
            widgets::caption(ui, &format!("Signing in to {kind}…"));
        });
    }

    if connected.is_empty() && restoring.is_empty() {
        ui.add_space(6.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new("Connect a service").font(theme::semibold(13.5)));
            ui.add_space(2.0);
            widgets::caption(ui, "Sign in to Spotify, SoundCloud or Apple Music to see your songs and playlists here.");
            ui.add_space(8.0);
            if widgets::pill_button(ui, "Connect…", true).clicked() {
                state.focus_panel(PanelKind::Accounts);
            }
        });
    }
}

fn provider_section(ui: &mut Ui, state: &mut AppState, kind: ProviderKind) {
    let pal = theme::palette(ui.ctx());
    // Header: dot + name, refresh on the right.
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.spacing_mut().item_spacing.x = 5.0;
        widgets::provider_dot(ui, kind, 7.0);
        ui.label(RichText::new(kind.name()).font(theme::semibold(11.5)).color(pal.secondary));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let loading = state.playlists.get(&kind).is_some_and(Loadable::is_loading);
            if loading {
                widgets::spinner(ui, 16.0);
            } else if widgets::icon_button(ui, Icon::Refresh, 20.0, &format!("Refresh {kind}")).clicked() {
                state.refresh_library(kind);
                if state.collections.contains_key(&View::Liked(kind)) {
                    state.load_view(View::Liked(kind));
                }
            }
        });
    });
    ui.add_space(2.0);

    let liked = View::Liked(kind);
    let liked_count = state.view_tracks(&liked).map(|t| t.len().to_string());
    let resp = nav_row(
        ui,
        Id::new(("lib.liked", kind.slug())),
        state.view == liked,
        Lead::Icon(Icon::Heart),
        "Liked Songs",
        liked_count.as_deref(),
    );
    if resp.clicked() {
        state.open_view(liked);
    }

    // Draw with a shared borrow (no per-frame clone of the playlist list), act afterwards.
    let mut open: Option<View> = None;
    let mut retry = false;
    match state.playlists.get(&kind) {
        None | Some(Loadable::Idle) => {}
        Some(Loadable::Loading) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                widgets::spinner(ui, 14.0);
                widgets::caption(ui, "Loading playlists…");
            });
        }
        Some(Loadable::Failed(e)) => {
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                ui.vertical(|ui| retry = widgets::error_row(ui, e));
            });
        }
        Some(Loadable::Ready(list)) => {
            for pl in list {
                let selected =
                    matches!(&state.view, View::Playlist { provider, id, .. } if *provider == kind && *id == pl.id);
                let count = pl.track_count.map(|c| c.to_string());
                let resp = nav_row(
                    ui,
                    Id::new(("lib.pl", kind.slug(), &pl.id)),
                    selected,
                    Lead::Art { url: pl.artwork.as_deref(), seed: &pl.id },
                    &pl.name,
                    count.as_deref(),
                );
                let resp = match &pl.owner {
                    Some(owner) if !owner.is_empty() => resp.on_hover_text(format!("{} · by {owner}", pl.name)),
                    _ => resp,
                };
                if resp.clicked() {
                    open = Some(View::Playlist { provider: kind, id: pl.id.clone(), name: pl.name.clone() });
                }
            }
            if list.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    widgets::caption(ui, "No playlists yet");
                });
            }
        }
    }
    if retry {
        state.refresh_library(kind);
    }
    if let Some(view) = open {
        state.open_view(view);
    }
}

// ---------------------------------------------------------------------------
// Row
// ---------------------------------------------------------------------------

/// A full-width sidebar row with a soft rounded hover and accent selection.
fn nav_row(ui: &mut Ui, id: Id, selected: bool, lead: Lead<'_>, label: &str, trailing: Option<&str>) -> Response {
    nav_row_padded(ui, id, selected, lead, label, trailing, 0.0)
}

/// Like [`nav_row`], keeping `right_pad` points free at the end (for painted extras).
fn nav_row_padded(
    ui: &mut Ui,
    id: Id,
    selected: bool,
    lead: Lead<'_>,
    label: &str,
    trailing: Option<&str>,
    right_pad: f32,
) -> Response {
    let pal = theme::palette(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let hover = theme::fade(ui.ctx(), id.with("hover"), resp.hovered() || resp.context_menu_opened());
    let radius = CornerRadius::same(theme::R_CONTROL);
    if selected {
        ui.painter().rect_filled(rect, radius, pal.accent_soft);
    } else if hover > 0.0 {
        let c = pal.hover;
        let a = (c.a() as f32 * hover) as u8;
        ui.painter().rect_filled(rect, radius, Color32::from_rgba_premultiplied(c.r(), c.g(), c.b(), a));
    }

    let lead_rect = Rect::from_center_size(pos2(rect.left() + 6.0 + LEAD / 2.0, rect.center().y), vec2(LEAD, LEAD));
    match lead {
        Lead::Icon(icon) => {
            widgets::paint_icon(ui.painter(), lead_rect.shrink(1.0), icon, pal.accent);
        }
        Lead::Art { url, seed } => widgets::paint_artwork(ui, lead_rect, url, 4, seed),
    }

    let text_left = lead_rect.right() + 9.0;
    let mut text_right = rect.right() - 8.0 - right_pad;
    if let Some(t) = trailing {
        let g = ui.painter().layout_no_wrap(t.to_owned(), theme::regular(11.5), pal.tertiary);
        let gw = g.size().x;
        ui.painter().galley(pos2(text_right - gw, rect.center().y - g.size().y / 2.0), g, pal.tertiary);
        text_right -= gw + 8.0;
    }
    let color = pal.label;
    let font = if selected { theme::semibold(13.0) } else { theme::regular(13.0) };
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width((text_right - text_left).max(10.0));
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(pos2(text_left, rect.center().y - galley.size().y / 2.0), galley, color);

    resp
}
