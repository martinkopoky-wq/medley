//! "Browse" — the main content area: a Home page, or the open playlist,
//! Liked Songs or Mix as a header plus a virtualized song list.

use crate::app::{AppState, Loadable, MixTarget, RowAction, View};
use crate::panels::tracks::{DragTrack, TrackListOpts, track_list};
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{
    self, Color32, CornerRadius, Id, Mesh, Rect, RichText, Sense, Shadow, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};
use medley_core::layout::PanelKind;
use medley_core::{Mix, ProviderKind, Track, format_long_duration};

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let view = state.view.clone();

    // Slim navigation bar.
    ui.horizontal(|ui| {
        let can_go_back = !state.view_history.is_empty();
        ui.add_enabled_ui(can_go_back, |ui| {
            if widgets::icon_button(ui, Icon::Back, 26.0, "Back").clicked() {
                state.go_back();
            }
        });
    });
    ui.add_space(2.0);

    match &view {
        View::Home => home(ui, state),
        _ => track_view(ui, state, &view),
    }
}

// ---------------------------------------------------------------------------
// Home
// ---------------------------------------------------------------------------

fn home(ui: &mut Ui, state: &mut AppState) {
    egui::ScrollArea::vertical().id_salt("medley.browse.home").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(4.0);
        widgets::large_title(ui, greeting());
        let connected = state.connected();
        let restoring = state.accounts.values().any(|a| matches!(a, crate::app::AccountState::Restoring));
        let subtitle = if connected.is_empty() {
            "All your music, one fast library.".to_owned()
        } else {
            format!("Your music from {} — in one place.", join_names(&connected))
        };
        widgets::secondary(ui, &subtitle);
        ui.add_space(18.0);

        if connected.is_empty() && !restoring {
            hero(ui, state);
            ui.add_space(22.0);
        }

        // Quick actions.
        let mut actions: Vec<QuickAction> = vec![
            QuickAction {
                id: "vibe".into(),
                icon: Icon::Sparkles,
                title: "Vibe → Mix".into(),
                subtitle: "Describe a mood, get a mix".into(),
                target: QuickTarget::Panel(PanelKind::AiStudio),
            },
            QuickAction {
                id: "search".into(),
                icon: Icon::Search,
                title: "Search Everything".into(),
                subtitle: "One search across your services".into(),
                target: QuickTarget::Panel(PanelKind::Search),
            },
        ];
        for kind in &connected {
            let count = state.view_tracks(&View::Liked(*kind)).map(<[Track]>::len);
            actions.push(QuickAction {
                id: format!("liked:{}", kind.slug()),
                icon: Icon::Heart,
                title: format!("Liked on {}", kind.name()),
                subtitle: match count {
                    Some(n) => songs_label(n),
                    None => "Your saved songs".into(),
                },
                target: QuickTarget::View(View::Liked(*kind)),
            });
        }
        if let Some(target) = quick_actions(ui, &actions) {
            match target {
                QuickTarget::Panel(p) => state.focus_panel(p),
                QuickTarget::View(v) => state.open_view(v),
            }
        }

        ui.add_space(26.0);
        ui.horizontal(|ui| {
            widgets::title(ui, "Your Mixes");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::link_button(ui, "New Mix").clicked() {
                    let name = format!("New Mix {}", state.mixes.len() + 1);
                    let id = state.create_mix(name, Vec::new());
                    state.open_view(View::Mix(id));
                }
            });
        });
        ui.add_space(8.0);
        if state.mixes.is_empty() {
            widgets::caption(
                ui,
                "Mixes hold songs from every service side by side. Create one from the Library, drag songs onto it, or let AI Studio build one from a vibe.",
            );
        } else if let Some(id) = mix_grid(ui, &state.mixes) {
            state.open_view(View::Mix(id));
        }
        ui.add_space(16.0);
    });
}

fn hero(ui: &mut Ui, state: &mut AppState) {
    let pal = theme::palette(ui.ctx());
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 26.0, pal.accent_soft);
            widgets::paint_icon(ui.painter(), r.shrink(13.0), Icon::Note, pal.accent);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Bring your music together").font(theme::semibold(17.0)));
                ui.add_space(2.0);
                widgets::caption(
                    ui,
                    "Connect Spotify, SoundCloud or Apple Music to see your library, search everything at once and build mixes across services.",
                );
                ui.add_space(10.0);
                if widgets::pill_button(ui, "Connect a Service", true).clicked() {
                    state.focus_panel(PanelKind::Accounts);
                }
            });
        });
    });
}

#[derive(Clone)]
enum QuickTarget {
    Panel(PanelKind),
    View(View),
}

struct QuickAction {
    id: String,
    icon: Icon,
    title: String,
    subtitle: String,
    target: QuickTarget,
}

/// Responsive grid of action cards. Returns the clicked target.
fn quick_actions(ui: &mut Ui, actions: &[QuickAction]) -> Option<QuickTarget> {
    let pal = theme::palette(ui.ctx());
    let gap = 10.0;
    let (cols, w) = grid_columns(ui.available_width(), 190.0, gap);
    let h = 64.0;
    let mut clicked = None;
    for chunk in actions.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for a in chunk {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
                let id = Id::new(("browse.quick", &a.id));
                let hover = theme::fade(ui.ctx(), id, resp.hovered());
                let pressed = resp.is_pointer_button_down_on();
                let scale = theme::spring_with(ui.ctx(), id.with("s"), if pressed { 0.97 } else { 1.0 }, 0.25, 0.7);
                let r = Rect::from_center_size(rect.center(), rect.size() * scale);
                let fill = theme::lerp_color(card_fill(&pal), pal.fill_strong, hover * 0.5);
                ui.painter().rect_filled(r, CornerRadius::same(theme::R_CARD), fill);
                let c = pos2(r.left() + 30.0, r.center().y);
                ui.painter().circle_filled(c, 17.0, pal.accent);
                widgets::paint_icon(ui.painter(), Rect::from_center_size(c, Vec2::splat(17.0)), a.icon, pal.on_accent);
                let text_left = r.left() + 56.0;
                let max_w = r.right() - 10.0 - text_left;
                let t = ui.painter().layout_job(single_line(&a.title, theme::semibold(13.5), pal.label, max_w));
                let s = ui.painter().layout_job(single_line(&a.subtitle, theme::regular(11.5), pal.secondary, max_w));
                ui.painter().galley(pos2(text_left, r.center().y - t.size().y), t, pal.label);
                ui.painter().galley(pos2(text_left, r.center().y + 1.0), s, pal.secondary);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    clicked = Some(a.target.clone());
                }
            }
        });
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
    clicked
}

/// Grid of mix tiles with a springy hover lift. Returns the clicked mix id.
fn mix_grid(ui: &mut Ui, mixes: &[Mix]) -> Option<String> {
    let pal = theme::palette(ui.ctx());
    let gap = 18.0;
    let (cols, w) = grid_columns(ui.available_width(), 140.0, gap);
    let art = w.min(180.0);
    let tile_h = art + 46.0;
    let mut clicked = None;
    for chunk in mixes.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for mix in chunk {
                let (rect, resp) = ui.allocate_exact_size(vec2(w, tile_h), Sense::click());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let id = Id::new(("browse.mix-tile", &mix.id));
                let lift = theme::spring(ui.ctx(), id, if resp.hovered() { -4.0 } else { 0.0 });
                let pressed = resp.is_pointer_button_down_on();
                let scale = theme::spring_with(ui.ctx(), id.with("s"), if pressed { 0.97 } else { 1.0 }, 0.25, 0.7);
                let art_rect = Rect::from_center_size(
                    pos2(rect.left() + art / 2.0, rect.top() + art / 2.0 + lift),
                    Vec2::splat(art * scale),
                );
                let shadow_alpha = if pal.dark { 110 } else { 34 } + (-lift * 6.0) as u8;
                let shadow = Shadow {
                    offset: [0, (6.0 - lift) as i8],
                    blur: (16.0 - lift * 2.0) as u8,
                    spread: 0,
                    color: Color32::from_black_alpha(shadow_alpha),
                };
                ui.painter().add(shadow.as_shape(art_rect, CornerRadius::same(theme::R_CARD)));
                paint_mix_cover(ui, art_rect, mix, theme::R_CARD);

                let text_left = rect.left();
                let name = ui.painter().layout_job(single_line(&mix.name, theme::semibold(13.0), pal.label, art));
                let meta =
                    format!("{} · {}", songs_label(mix.tracks.len()), format_long_duration(mix.total_duration_ms()));
                let meta_g = ui.painter().layout_job(single_line(&meta, theme::regular(11.5), pal.secondary, art));
                let y = rect.top() + art + 8.0;
                ui.painter().galley(pos2(text_left, y), name, pal.label);
                ui.painter().galley(pos2(text_left, y + 18.0), meta_g, pal.secondary);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    clicked = Some(mix.id.clone());
                }
            }
        });
        ui.add_space(8.0);
    }
    clicked
}

fn grid_columns(available: f32, min_w: f32, gap: f32) -> (usize, f32) {
    let cols = (((available + gap) / (min_w + gap)).floor() as usize).max(1);
    let w = ((available - gap * (cols as f32 - 1.0)) / cols as f32).max(40.0);
    (cols, w)
}

fn card_fill(pal: &theme::Palette) -> Color32 {
    if pal.dark { Color32::from_rgb(0x2A, 0x2A, 0x2D) } else { Color32::from_rgb(0xF5, 0xF5, 0xF7) }
}

// ---------------------------------------------------------------------------
// Track views (Liked / Playlist / Mix)
// ---------------------------------------------------------------------------

fn track_view(ui: &mut Ui, state: &mut AppState, view: &View) {
    let mix_id = match view {
        View::Mix(id) => Some(id.clone()),
        _ => None,
    };
    if let Some(id) = &mix_id
        && state.mix(id).is_none()
    {
        widgets::empty_state(ui, Icon::Note, "This mix no longer exists", "It may have been deleted.", None);
        return;
    }

    // Provider views load on demand (e.g. after a restart).
    let loadable = match view {
        View::Liked(_) | View::Playlist { .. } => {
            let l = state.collections.get(view).cloned();
            if matches!(l, None | Some(Loadable::Idle)) {
                state.load_view(view.clone());
            }
            Some(state.collections.get(view).map(loadable_shape).unwrap_or(Shape::Loading))
        }
        _ => None,
    };

    let compact = ui.available_height() < 560.0 || ui.available_width() < 520.0;
    header(ui, state, view, compact);
    ui.add_space(10.0);

    match loadable {
        Some(Shape::Loading) => {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                widgets::spinner(ui, 22.0);
                ui.add_space(6.0);
                widgets::secondary(ui, "Loading songs…");
            });
            return;
        }
        Some(Shape::Failed(e)) => {
            ui.add_space(12.0);
            if widgets::error_row(ui, &e) {
                state.load_view(view.clone());
            }
            return;
        }
        _ => {}
    }

    let list_key = list_key(view);
    let list_id = Id::new(("medley.tracks", list_key.as_str()));
    let area = ui.available_rect_before_wrap();
    let empty = state.view_tracks(view).is_none_or(<[Track]>::is_empty);

    if empty {
        let (title, message, button) = if mix_id.is_some() {
            (
                "This mix is empty",
                "Drag songs here from Search or any playlist, or use AI Studio.",
                Some("Search Songs"),
            )
        } else {
            ("No songs here yet", "Songs you add on the service will show up here.", None)
        };
        if widgets::empty_state(ui, Icon::Note, title, message, button) {
            state.focus_panel(PanelKind::Search);
        }
    } else {
        let mut opts = TrackListOpts::new(&list_key);
        opts.numbered = true;
        if mix_id.is_some() {
            opts.remove_label = Some("Remove from Mix");
            opts.reorderable = true;
        }
        let action = {
            let tracks = state.view_tracks(view).unwrap_or(&[]);
            track_list(ui, state, tracks, opts)
        };
        if let Some(action) = action {
            match (&mix_id, action) {
                (Some(id), RowAction::Remove(i)) => state.edit_mix(id, |m| {
                    if i < m.tracks.len() {
                        m.tracks.remove(i);
                    }
                }),
                (Some(id), RowAction::Move { from, to }) => state.edit_mix(id, |m| {
                    if from < m.tracks.len() {
                        let t = m.tracks.remove(from);
                        let to = to.min(m.tracks.len());
                        m.tracks.insert(to, t);
                    }
                }),
                (_, RowAction::Remove(_) | RowAction::Move { .. }) => {}
                (_, action) => {
                    let tracks = state.view_tracks(view).map(<[Track]>::to_vec).unwrap_or_default();
                    let context = state.view_title(view);
                    state.row_action(&tracks, action, &context);
                }
            }
        }
    }

    // Songs dragged in from other lists append to a mix.
    if let Some(id) = &mix_id {
        accept_foreign_drops(ui, state, id, list_id, area);
    }
}

enum Shape {
    Loading,
    Failed(String),
    Ready,
}

fn loadable_shape(l: &Loadable<Vec<Track>>) -> Shape {
    match l {
        Loadable::Idle | Loadable::Loading => Shape::Loading,
        Loadable::Failed(e) => Shape::Failed(e.clone()),
        Loadable::Ready(_) => Shape::Ready,
    }
}

fn list_key(view: &View) -> String {
    match view {
        View::Home => "home".into(),
        View::Liked(k) => format!("liked:{}", k.slug()),
        View::Playlist { provider, id, .. } => format!("pl:{}:{id}", provider.slug()),
        View::Mix(id) => format!("mix:{id}"),
    }
}

fn accept_foreign_drops(ui: &Ui, state: &mut AppState, mix_id: &str, list_id: Id, area: Rect) {
    let Some(payload) = egui::DragAndDrop::payload::<DragTrack>(ui.ctx()) else { return };
    if payload.list == list_id {
        return;
    }
    let over = ui.ctx().pointer_hover_pos().is_some_and(|p| area.contains(p));
    if !over {
        return;
    }
    let pal = theme::palette(ui.ctx());
    ui.painter().rect_stroke(
        area.shrink(1.0),
        CornerRadius::same(theme::R_CARD),
        Stroke::new(2.0, pal.accent),
        StrokeKind::Inside,
    );
    if ui.input(|i| i.pointer.any_released())
        && let Some(payload) = egui::DragAndDrop::take_payload::<DragTrack>(ui.ctx())
    {
        state.add_to_mix(MixTarget::Existing(mix_id.to_owned()), vec![payload.track.clone()]);
    }
}

// -- header ------------------------------------------------------------------

fn header(ui: &mut Ui, state: &mut AppState, view: &View, compact: bool) {
    let pal = theme::palette(ui.ctx());
    let art_size = if compact { 96.0 } else { 150.0 };
    let tracks_len = state.view_tracks(view).map_or(0, <[Track]>::len);
    let total_ms: u64 = state.view_tracks(view).map_or(0, |t| t.iter().map(|t| t.duration_ms).sum());

    ui.horizontal(|ui| {
        // Artwork.
        let (art_rect, _) = ui.allocate_exact_size(Vec2::splat(art_size), Sense::hover());
        let shadow = Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(if pal.dark { 120 } else { 45 }),
        };
        ui.painter().add(shadow.as_shape(art_rect, CornerRadius::same(theme::R_CARD)));
        match view {
            View::Mix(id) => {
                if let Some(mix) = state.mix(id) {
                    paint_mix_cover(ui, art_rect, mix, theme::R_CARD);
                }
            }
            View::Playlist { provider, id, .. } => {
                let summary_art = state
                    .playlists
                    .get(provider)
                    .and_then(Loadable::ready)
                    .and_then(|l| l.iter().find(|p| &p.id == id))
                    .and_then(|p| p.artwork.clone());
                let first = state
                    .view_tracks(view)
                    .and_then(|t| t.first())
                    .and_then(|t| t.artwork_large.clone().or(t.artwork.clone()));
                widgets::paint_artwork(ui, art_rect, summary_art.or(first).as_deref(), theme::R_CARD, id);
            }
            View::Liked(kind) => liked_cover(ui, art_rect, *kind),
            View::Home => {}
        }
        ui.add_space(14.0);

        ui.vertical(|ui| {
            ui.set_min_height(art_size);
            ui.add_space(if compact { 2.0 } else { 18.0 });
            let kind_label = match view {
                View::Mix(_) => "MIX",
                View::Playlist { .. } => "PLAYLIST",
                View::Liked(_) => "LIKED SONGS",
                View::Home => "",
            };
            ui.label(RichText::new(kind_label).font(theme::semibold(10.5)).color(pal.secondary));

            // Title, or an inline rename field for mixes.
            if let View::Mix(id) = view {
                mix_title(ui, state, id);
            } else {
                widgets::large_title(ui, &state.view_title(view));
            }

            // Subtitle line.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                match view {
                    View::Liked(kind) => {
                        widgets::provider_chip(ui, *kind);
                    }
                    View::Playlist { provider, id, .. } => {
                        widgets::provider_chip(ui, *provider);
                        let owner = state
                            .playlists
                            .get(provider)
                            .and_then(Loadable::ready)
                            .and_then(|l| l.iter().find(|p| &p.id == id))
                            .and_then(|p| p.owner.clone())
                            .filter(|o| !o.is_empty());
                        if let Some(owner) = owner {
                            widgets::secondary(ui, &format!("· by {owner}"));
                        }
                    }
                    View::Mix(id) => {
                        if let Some(mix) = state.mix(id) {
                            for (i, kind) in ProviderKind::ALL.iter().enumerate() {
                                if mix.provider_counts()[i] > 0 {
                                    widgets::provider_dot(ui, *kind, 7.0);
                                }
                            }
                        }
                    }
                    View::Home => {}
                }
                let meta = if total_ms > 0 {
                    format!("{} · {}", songs_label(tracks_len), format_long_duration(total_ms))
                } else {
                    songs_label(tracks_len)
                };
                widgets::secondary(ui, &meta);
            });

            // Description / AI prompt.
            let description = match view {
                View::Mix(id) => state.mix(id).map(|m| m.description.clone()),
                View::Playlist { provider, id, .. } => state
                    .playlists
                    .get(provider)
                    .and_then(Loadable::ready)
                    .and_then(|l| l.iter().find(|p| &p.id == id))
                    .and_then(|p| p.description.clone()),
                _ => None,
            }
            .filter(|d| !d.trim().is_empty());
            if let Some(d) = description
                && !compact
            {
                two_lines(ui, &d, ui.available_width().min(560.0));
            }
            if let View::Mix(id) = view
                && let Some(prompt) = state.mix(id).and_then(|m| m.ai_prompt.clone())
            {
                ui.horizontal(|ui| {
                    widgets::badge(ui, "AI", pal.accent);
                    let mut job = single_line(
                        &format!("“{prompt}”"),
                        theme::regular(11.5),
                        pal.secondary,
                        ui.available_width().min(480.0),
                    );
                    job.wrap.max_rows = 1;
                    ui.label(job);
                });
            }

            ui.add_space(8.0);
            buttons(ui, state, view, tracks_len);
            busy_line(ui, state, view);
        });
    });
}

fn mix_title(ui: &mut Ui, state: &mut AppState, id: &str) {
    let edit_key = Id::new(("browse.rename", id));
    let editing: Option<String> = ui.ctx().data(|d| d.get_temp(edit_key));
    let Some(mut text) = editing else {
        let name = state.mix(id).map(|m| m.name.clone()).unwrap_or_default();
        let r = widgets::large_title(ui, &name).interact(Sense::click());
        if r.on_hover_text("Double-click to rename").double_clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(edit_key, name));
        }
        return;
    };
    let edit_id = edit_key.with("field");
    let resp = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(edit_id)
            .font(theme::display(26.0))
            .desired_width(ui.available_width().min(520.0)),
    );
    let first = !ui.ctx().data(|d| d.get_temp::<bool>(edit_id.with("seen")).unwrap_or(false));
    if first {
        resp.request_focus();
        ui.ctx().data_mut(|d| d.insert_temp(edit_id.with("seen"), true));
    }
    let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    let finish = escape || enter || (resp.lost_focus() && !first);
    if finish {
        if !escape {
            let name = text.trim().to_owned();
            if !name.is_empty() {
                state.edit_mix(id, |m| m.name = name);
            }
        }
        ui.ctx().data_mut(|d| {
            d.remove::<String>(edit_key);
            d.remove::<bool>(edit_id.with("seen"));
        });
    } else {
        ui.ctx().data_mut(|d| d.insert_temp(edit_key, text));
    }
}

fn buttons(ui: &mut Ui, state: &mut AppState, view: &View, tracks_len: usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let has = tracks_len > 0;
        let play = ui.add_enabled_ui(has, |ui| widgets::pill_button_icon(ui, Some(Icon::Play), "Play", true)).inner;
        if play.clicked() {
            state.player.set_shuffle(false);
            let tracks = state.view_tracks(view).map(<[Track]>::to_vec).unwrap_or_default();
            let context = state.view_title(view);
            state.play_tracks(tracks, 0, context);
        }
        let shuffle =
            ui.add_enabled_ui(has, |ui| widgets::pill_button_icon(ui, Some(Icon::Shuffle), "Shuffle", false)).inner;
        if shuffle.clicked() {
            state.player.set_shuffle(true);
            let tracks = state.view_tracks(view).map(<[Track]>::to_vec).unwrap_or_default();
            let start = if tracks.is_empty() { 0 } else { rand::random_range(0..tracks.len()) };
            let context = state.view_title(view);
            state.play_tracks(tracks, start, context);
        }

        match view {
            View::Liked(_) | View::Playlist { .. } => {
                if widgets::icon_button(ui, Icon::Refresh, 28.0, "Reload").clicked() {
                    state.load_view(view.clone());
                }
            }
            View::Mix(id) => mix_menu(ui, state, id),
            View::Home => {}
        }
    });
}

fn mix_menu(ui: &mut Ui, state: &mut AppState, id: &str) {
    let more = widgets::icon_button(ui, Icon::More, 28.0, "More");
    let ai_reason = state.ai_unavailable();
    let len = state.mix(id).map_or(0, |m| m.tracks.len());
    let confirm_key = Id::new(("browse.confirm-delete", id));
    let popup = egui::Popup::menu(&more).width(220.0).show(|ui| {
        let pal = theme::palette(ui.ctx());
        if ui.button("Rename").clicked() {
            let name = state.mix(id).map(|m| m.name.clone()).unwrap_or_default();
            ui.ctx().data_mut(|d| d.insert_temp(Id::new(("browse.rename", id)), name));
            ui.close();
        }
        if ui.button("Add All to Up Next").clicked() {
            let tracks = state.mix(id).map(|m| m.tracks.clone()).unwrap_or_default();
            state.add_to_queue(tracks);
            ui.close();
        }
        ui.separator();

        let naming = state.is_busy(&format!("name:{id}"));
        let name_ok = ai_reason.is_none() && !naming && len > 0;
        let r = ui.add_enabled(name_ok, egui::Button::new(if naming { "Naming…" } else { "Name with AI" }));
        let r = disabled_hint(r, ai_reason.as_deref(), len == 0, "Add songs first");
        if r.clicked() {
            state.ai_name_mix(id);
            ui.close();
        }
        let flowing = state.is_busy(&format!("flow:{id}"));
        let flow_ok = ai_reason.is_none() && !flowing && len >= 3;
        let r = ui.add_enabled(flow_ok, egui::Button::new(if flowing { "Smoothing…" } else { "Smooth the Flow" }));
        let r = disabled_hint(r, ai_reason.as_deref(), len < 3, "Needs at least three songs");
        if r.clicked() {
            state.ai_smooth_flow(id);
            ui.close();
        }

        let connected = state.connected();
        if !connected.is_empty() {
            ui.separator();
            for kind in connected {
                let busy = state.is_busy(&format!("export:{id}:{}", kind.slug()));
                let label = if busy {
                    format!("Exporting to {}…", kind.name())
                } else {
                    format!("Export to {}", kind.name())
                };
                let r = ui.add_enabled(!busy && len > 0, egui::Button::new(label));
                let r = if len == 0 { r.on_disabled_hover_text("Add songs first") } else { r };
                if r.clicked() {
                    state.export_mix(id, kind);
                    ui.close();
                }
            }
        }

        ui.separator();
        let confirming = ui.ctx().data(|d| d.get_temp::<bool>(confirm_key).unwrap_or(false));
        if confirming {
            if ui.button(RichText::new("Confirm Delete").color(pal.danger)).clicked() {
                ui.ctx().data_mut(|d| d.remove::<bool>(confirm_key));
                state.delete_mix(id);
                ui.close();
            }
        } else if ui.button(RichText::new("Delete Mix…").color(pal.danger)).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(confirm_key, true));
        }
    });
    if popup.is_none() {
        ui.ctx().data_mut(|d| d.remove::<bool>(confirm_key));
    }
}

fn disabled_hint(r: egui::Response, ai_reason: Option<&str>, too_few: bool, few_text: &str) -> egui::Response {
    if let Some(reason) = ai_reason {
        r.on_disabled_hover_text(reason)
    } else if too_few {
        r.on_disabled_hover_text(few_text)
    } else {
        r
    }
}

/// "Naming with AI…" style status under the buttons while mix jobs run.
fn busy_line(ui: &mut Ui, state: &AppState, view: &View) {
    let View::Mix(id) = view else { return };
    let mut jobs = Vec::new();
    if state.is_busy(&format!("name:{id}")) {
        jobs.push("Naming with AI…".to_owned());
    }
    if state.is_busy(&format!("flow:{id}")) {
        jobs.push("Smoothing the flow…".to_owned());
    }
    for kind in ProviderKind::ALL {
        if state.is_busy(&format!("export:{id}:{}", kind.slug())) {
            jobs.push(format!("Exporting to {}…", kind.name()));
        }
    }
    if !jobs.is_empty() {
        ui.add_space(4.0);
        widgets::loading_row(ui, &jobs.join("  ·  "));
    }
}

// -- covers ------------------------------------------------------------------

/// A mix's cover: a 2×2 mosaic of the first four distinct artworks, else the first one.
fn paint_mix_cover(ui: &Ui, rect: Rect, mix: &Mix, radius: u8) {
    let mut urls: Vec<&str> = Vec::new();
    for t in &mix.tracks {
        if let Some(u) = t.artwork.as_deref()
            && !urls.contains(&u)
        {
            urls.push(u);
            if urls.len() == 4 {
                break;
            }
        }
    }
    if urls.len() < 4 {
        let big = mix.tracks.iter().find_map(|t| t.artwork_large.as_deref().or(t.artwork.as_deref()));
        widgets::paint_artwork(ui, rect, big, radius, &mix.id);
        return;
    }
    let half = rect.size() / 2.0;
    for (i, url) in urls.iter().enumerate() {
        let min = rect.min + vec2(if i % 2 == 1 { half.x } else { 0.0 }, if i >= 2 { half.y } else { 0.0 });
        let mut r = CornerRadius::ZERO;
        match i {
            0 => r.nw = radius,
            1 => r.ne = radius,
            2 => r.sw = radius,
            _ => r.se = radius,
        }
        widgets::paint_artwork_rounded(ui, Rect::from_min_size(min, half), Some(url), r, &mix.id);
    }
    let pal = theme::palette(ui.ctx());
    let edge = if pal.dark { Color32::from_white_alpha(18) } else { Color32::from_black_alpha(18) };
    ui.painter().rect_stroke(rect, CornerRadius::same(radius), Stroke::new(1.0, edge), StrokeKind::Inside);
}

/// Liked Songs cover: accent-tinted gradient with a heart.
fn liked_cover(ui: &Ui, rect: Rect, kind: ProviderKind) {
    let pal = theme::palette(ui.ctx());
    let top = theme::lerp_color(pal.accent, Color32::WHITE, 0.15);
    let bottom = theme::lerp_color(pal.accent, theme::provider_color(kind), 0.45);
    ui.painter().rect_filled(rect, CornerRadius::same(theme::R_CARD), theme::lerp_color(top, bottom, 0.5));
    let mut mesh = Mesh::default();
    let inner = rect.shrink(theme::R_CARD as f32 * 0.3);
    mesh.colored_vertex(inner.left_top(), top);
    mesh.colored_vertex(inner.right_top(), top);
    mesh.colored_vertex(inner.right_bottom(), bottom);
    mesh.colored_vertex(inner.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    widgets::paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), rect.size() * 0.42),
        Icon::Heart,
        Color32::WHITE,
    );
}

// -- text helpers --------------------------------------------------------------

fn single_line(text: &str, font: egui::FontId, color: Color32, width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    job
}

fn two_lines(ui: &mut Ui, text: &str, width: f32) {
    let pal = theme::palette(ui.ctx());
    let mut job = egui::text::LayoutJob::simple(text.to_owned(), theme::regular(12.0), pal.secondary, width);
    job.wrap.max_rows = 2;
    job.wrap.break_anywhere = false;
    job.wrap.overflow_character = Some('…');
    ui.label(job);
}

fn songs_label(n: usize) -> String {
    if n == 1 { "1 song".into() } else { format!("{n} songs") }
}

fn join_names(kinds: &[ProviderKind]) -> String {
    let names: Vec<&str> = kinds.iter().map(|k| k.name()).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// "Good morning/afternoon/evening", by local time.
fn greeting() -> &'static str {
    match local_hour() {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    }
}

fn local_hour() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().hour()
}
