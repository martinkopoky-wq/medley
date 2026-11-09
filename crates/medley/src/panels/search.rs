//! Search across every connected service at once.

use crate::app::{AccountState, AppState, Loadable, RowAction};
use crate::panels::tracks::{TrackListOpts, track_list};
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{self, Id, Key, RichText, Ui};
use medley_core::ProviderKind;
use medley_core::layout::PanelKind;

/// Rows per service in the "All" overview.
const PREVIEW_ROWS: usize = 5;

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    search_bar(ui, state);
    ui.add_space(10.0);

    let connected = state.connected();
    if connected.is_empty() {
        let restoring = state.accounts.values().any(|a| matches!(a, AccountState::Restoring));
        if restoring {
            widgets::loading_row(ui, "Connecting to your services…");
        } else if widgets::empty_state(
            ui,
            Icon::Search,
            "Connect a service to search",
            "Medley searches Spotify, SoundCloud and Apple Music together once you connect them.",
            Some("Open Accounts"),
        ) {
            state.focus_panel(PanelKind::Accounts);
        }
        return;
    }

    if state.search.query.trim().is_empty() && state.search.submitted.is_empty() {
        widgets::empty_state(
            ui,
            Icon::Search,
            "Search everything",
            "Songs from Spotify, SoundCloud and Apple Music in one place.",
            None,
        );
        return;
    }

    let order = provider_order(state, &connected);
    if state.search.filter.is_some_and(|k| !order.contains(&k)) {
        state.search.filter = None;
    }

    // Filter: All · each connected service.
    if order.len() > 1 {
        let mut options: Vec<(Option<ProviderKind>, &str)> = vec![(None, "All")];
        options.extend(order.iter().map(|k| (Some(*k), k.name())));
        let mut filter = state.search.filter;
        if widgets::segmented(ui, &mut filter, &options) {
            state.search.filter = filter;
        }
        ui.add_space(10.0);
    }

    if state.search.submitted.is_empty() {
        // Typed but the debounce hasn't fired yet.
        widgets::loading_row(ui, "Searching…");
        return;
    }

    match state.search.filter.or_else(|| (order.len() == 1).then(|| order[0])) {
        Some(kind) => single_service(ui, state, kind),
        None => overview(ui, state, &order),
    }
}

fn search_bar(ui: &mut Ui, state: &mut AppState) {
    let width = ui.available_width();
    let response = widgets::search_field(ui, &mut state.search.query, "Songs, artists, albums", width);
    if response.changed() {
        state.search_edited();
    }
    if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
        state.search_now();
    }

    // Focus the field when the panel comes into view (or on ⌘F), unless the
    // user is already typing somewhere else.
    let ctx = ui.ctx().clone();
    let frame = ctx.cumulative_frame_nr();
    let seen_id = Id::new("medley.search.last-frame");
    let last_seen = ctx.data(|d| d.get_temp::<u64>(seen_id));
    ctx.data_mut(|d| d.insert_temp(seen_id, frame));
    let just_shown = last_seen.is_none_or(|f| f + 1 < frame);
    let find = ui.input(|i| i.modifiers.command && i.key_pressed(Key::F));
    let nothing_focused = ctx.memory(|m| m.focused().is_none());
    if find || (just_shown && nothing_focused && state.search.query.is_empty()) {
        response.request_focus();
    }
}

/// Connected services in the user's preferred order.
fn provider_order(state: &AppState, connected: &[ProviderKind]) -> Vec<ProviderKind> {
    let mut order: Vec<ProviderKind> =
        state.settings().provider_order.iter().copied().filter(|k| connected.contains(k)).collect();
    for k in connected {
        if !order.contains(k) {
            order.push(*k);
        }
    }
    order
}

/// "All": a short list per service with "Show All".
fn overview(ui: &mut Ui, state: &mut AppState, order: &[ProviderKind]) {
    let mut action: Option<(ProviderKind, RowAction)> = None;
    let mut show_all: Option<ProviderKind> = None;
    let mut any = false;
    {
        let st: &AppState = state;
        egui::ScrollArea::vertical().id_salt("medley.search.overview").auto_shrink([false, false]).show(ui, |ui| {
            for &kind in order {
                let Some(result) = st.search.results.get(&kind) else { continue };
                any = true;
                let count = result.ready().map_or(0, Vec::len);
                ui.horizontal(|ui| {
                    widgets::provider_dot(ui, kind, 9.0);
                    ui.label(RichText::new(kind.name()).font(theme::semibold(15.0)));
                    if count > PREVIEW_ROWS {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::link_button(ui, "Show All").clicked() {
                                show_all = Some(kind);
                            }
                        });
                    }
                });
                ui.add_space(4.0);
                match result {
                    Loadable::Idle | Loadable::Loading => {
                        widgets::loading_row(ui, &format!("Searching {}…", kind.name()));
                    }
                    Loadable::Failed(e) => danger_caption(ui, e),
                    Loadable::Ready(tracks) if tracks.is_empty() => {
                        widgets::caption(ui, &format!("No results on {}", kind.name()));
                    }
                    Loadable::Ready(tracks) => {
                        let shown = &tracks[..tracks.len().min(PREVIEW_ROWS)];
                        let id = format!("search-{}", kind.slug());
                        let opts = TrackListOpts { inline: true, ..TrackListOpts::new(&id) };
                        if let Some(a) = track_list(ui, st, shown, opts) {
                            action = Some((kind, a));
                        }
                    }
                }
                ui.add_space(18.0);
            }
            if !any {
                widgets::caption(ui, "No services were searched — connect one in Accounts.");
            }
        });
    }
    if let Some(kind) = show_all {
        state.search.filter = Some(kind);
    }
    if let Some((kind, a)) = action {
        apply(state, kind, a);
    }
}

/// One service: the full, virtualized result list.
fn single_service(ui: &mut Ui, state: &mut AppState, kind: ProviderKind) {
    let mut action = None;
    {
        let st: &AppState = state;
        match st.search.results.get(&kind) {
            None => {
                widgets::caption(ui, &format!("{} wasn't searched — try again.", kind.name()));
            }
            Some(Loadable::Idle | Loadable::Loading) => {
                widgets::loading_row(ui, &format!("Searching {}…", kind.name()));
            }
            Some(Loadable::Failed(e)) => danger_caption(ui, e),
            Some(Loadable::Ready(tracks)) if tracks.is_empty() => {
                widgets::caption(ui, &format!("No results on {} for “{}”", kind.name(), st.search.submitted));
            }
            Some(Loadable::Ready(tracks)) => {
                let id = format!("search-full-{}", kind.slug());
                action = track_list(ui, st, tracks, TrackListOpts::new(&id));
            }
        }
    }
    if let Some(a) = action {
        apply(state, kind, a);
    }
}

fn apply(state: &mut AppState, kind: ProviderKind, action: RowAction) {
    let Some(tracks) = state.search.results.get(&kind).and_then(|r| r.ready()).cloned() else { return };
    let context = format!("Search: {}", state.search.submitted);
    state.row_action(&tracks, action, &context);
}

fn danger_caption(ui: &mut Ui, text: &str) {
    let pal = theme::palette(ui.ctx());
    ui.add(egui::Label::new(RichText::new(text).font(theme::regular(11.5)).color(pal.danger)).wrap());
}
