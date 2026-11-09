//! Accounts: connect each music service, enter the credentials it needs, and
//! choose the order services are searched in.

use super::settings::{row, secret_field, text_setting};
use crate::app::{AccountState, AppState, SpotifyPlayback, ToastKind};
use crate::theme;
use crate::widgets;
use eframe::egui::{self, Align, Color32, Layout, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use medley_core::ProviderKind;

enum Intent {
    Connect(ProviderKind),
    Disconnect(ProviderKind),
    Move(usize, usize),
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let mut intents = Vec::new();
    crate::panels::scroll(ui, "accounts", |ui| {
        widgets::large_title(ui, "Accounts");
        widgets::caption(
            ui,
            "Sign in to each service you use. Medley keeps one library across all of them, and Mixes can hold songs from any service.",
        );
        ui.add_space(4.0);
        for kind in ProviderKind::ALL {
            service_card(ui, state, kind, &mut intents);
            ui.add_space(4.0);
        }
        order_section(ui, state, &mut intents);
        ui.add_space(12.0);
    });
    for intent in intents {
        match intent {
            Intent::Connect(k) => state.connect(k),
            Intent::Disconnect(k) => state.disconnect(k),
            Intent::Move(from, to) => state.edit_settings(|s| {
                complete_order(&mut s.provider_order);
                if from < s.provider_order.len() && to < s.provider_order.len() {
                    s.provider_order.swap(from, to);
                }
            }),
        }
    }
}

fn service_card(ui: &mut Ui, state: &mut AppState, kind: ProviderKind, intents: &mut Vec<Intent>) {
    let pal = theme::palette(ui.ctx());
    let account = state.accounts.get(&kind).cloned().unwrap_or(AccountState::Disconnected);
    // Provider hints point at "(Accounts panel)" for toasts; we're already here.
    let hint = state.providers.get(kind).setup_hint().map(|h| h.replace(" (Accounts panel)", ""));
    widgets::section_header(ui, kind.name());
    widgets::card(ui, |ui| {
        // Header: service, status, primary action.
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            widgets::provider_dot(ui, kind, 10.0);
            ui.label(RichText::new(kind.name()).font(theme::semibold(15.0)).color(pal.label));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match &account {
                AccountState::Restoring => {
                    ui.label(RichText::new("Checking…").color(pal.secondary));
                    widgets::spinner(ui, 16.0);
                }
                AccountState::Connecting => {
                    ui.label(RichText::new("Waiting for the browser…").color(pal.secondary));
                    widgets::spinner(ui, 16.0);
                }
                AccountState::Connected(_) => {
                    if widgets::link_button(ui, "Disconnect").clicked() {
                        intents.push(Intent::Disconnect(kind));
                    }
                }
                AccountState::Disconnected | AccountState::Failed(_) => {
                    ui.add_enabled_ui(hint.is_none(), |ui| {
                        let resp = widgets::pill_button(ui, "Connect", true);
                        let resp = if hint.is_some() {
                            resp.on_disabled_hover_text("Finish the setup below first")
                        } else {
                            resp
                        };
                        if resp.clicked() {
                            intents.push(Intent::Connect(kind));
                        }
                    });
                }
            });
        });
        match &account {
            AccountState::Connected(acc) => {
                ui.label(RichText::new(format!("Signed in as {}", acc.display_name)).color(pal.secondary));
            }
            AccountState::Connecting => {
                widgets::caption(ui, "Finish signing in in your browser.");
            }
            AccountState::Failed(e) => {
                ui.add(egui::Label::new(RichText::new(e).font(theme::regular(11.5)).color(pal.danger)).wrap());
            }
            AccountState::Disconnected => {
                widgets::caption(ui, intro(kind));
            }
            AccountState::Restoring => {}
        }
        if let Some(h) = &hint {
            ui.add(
                egui::Label::new(
                    RichText::new(h).font(theme::regular(11.5)).color(Color32::from_rgb(0xFF, 0x9F, 0x0A)),
                )
                .wrap(),
            );
        }

        // Setup (collapsed once everything needed is filled in).
        let open_id = ui.id().with(("setup_open", kind.slug()));
        let open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(hint.is_some());
        ui.add_space(2.0);
        if widgets::link_button(ui, if open { "Hide Setup" } else { "Setup" }).clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(open_id, !open));
        }
        if open {
            widgets::hairline(ui);
            match kind {
                ProviderKind::Spotify => spotify_setup(ui, state),
                ProviderKind::SoundCloud => soundcloud_setup(ui, state),
                ProviderKind::AppleMusic => apple_setup(ui, state),
            }
        }
    });
}

fn intro(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Spotify => "Your liked songs and playlists, with full playback for Premium accounts.",
        ProviderKind::SoundCloud => "Likes, playlists and full-length streams from SoundCloud.",
        ProviderKind::AppleMusic => "Your Apple Music library and playlists, with 30-second previews.",
    }
}

fn spotify_setup(ui: &mut Ui, state: &mut AppState) {
    let pal = theme::palette(ui.ctx());
    let s = state.settings().spotify.clone();
    if let Some(v) = text_setting(ui, "Client ID", &s.client_id, "Built-in", 260.0) {
        state.edit_settings(|st| st.spotify.client_id = v.trim().to_owned());
    }
    widgets::caption(
        ui,
        &format!(
            "Leave empty to use the built-in ID. Using your own Spotify app avoids shared rate limits; add http://127.0.0.1:{}/login as its redirect URI.",
            s.redirect_port
        ),
    );
    widgets::hairline(ui);
    row(ui, "Quality", |ui| {
        let mut bitrate: u16 = match s.bitrate {
            0..=128 => 96,
            129..=240 => 160,
            _ => 320,
        };
        if widgets::segmented(ui, &mut bitrate, &[(96, "Low"), (160, "Normal"), (320, "High")]) {
            state.edit_settings(|st| st.spotify.bitrate = bitrate);
        }
    });
    widgets::caption(ui, "96, 160 or 320 kbps. Applies the next time the playback engine connects.");
    widgets::hairline(ui);
    row(ui, "Playback engine", |ui| {
        let (text, color) = match &state.spotify_playback {
            SpotifyPlayback::Unknown => ("Starts with your first Spotify song".to_owned(), pal.secondary),
            SpotifyPlayback::Connecting => ("Connecting…".to_owned(), pal.secondary),
            SpotifyPlayback::Ready => ("Ready".to_owned(), pal.success),
            SpotifyPlayback::Failed(_) => ("Unavailable".to_owned(), pal.danger),
        };
        ui.label(RichText::new(text).color(color));
    });
    if let SpotifyPlayback::Failed(msg) = &state.spotify_playback {
        ui.add(egui::Label::new(RichText::new(msg).font(theme::regular(11.5)).color(pal.danger)).wrap());
    }
    widgets::caption(ui, "Playback needs Spotify Premium.");
}

fn soundcloud_setup(ui: &mut Ui, state: &mut AppState) {
    let s = state.settings().soundcloud.clone();
    if let Some(v) = text_setting(ui, "Client ID", &s.client_id, "From your SoundCloud app", 260.0) {
        state.edit_settings(|st| st.soundcloud.client_id = v.trim().to_owned());
    }
    let creds = state.creds.clone();
    let mut error = None;
    row(ui, "Client secret", |ui| {
        if let Err(e) = secret_field(
            ui,
            &creds,
            "soundcloud_secret",
            "Client secret",
            |c| c.soundcloud_client_secret.clone(),
            |c, v| c.soundcloud_client_secret = v,
        ) {
            error = Some(e.to_string());
        }
    });
    if let Some(e) = error {
        state.toast(ToastKind::Error, format!("Couldn't save the secret: {e}"));
    }
    widgets::caption(
        ui,
        &format!(
            "Register an app on SoundCloud and set its redirect URI to http://127.0.0.1:{}/soundcloud/callback.",
            s.redirect_port
        ),
    );
    if widgets::link_button(ui, "Register an app").clicked() {
        state.open_url("https://soundcloud.com/you/apps");
    }
    widgets::caption(ui, "SoundCloud tracks link back to SoundCloud, as their terms require.");
}

fn apple_setup(ui: &mut Ui, state: &mut AppState) {
    let s = state.settings().apple.clone();
    widgets::caption(
        ui,
        "Apple Music needs a MusicKit key from the Apple Developer Program. Sign-in happens with MusicKit in your browser.",
    );
    if let Some(v) = text_setting(ui, "Team ID", &s.team_id, "ABCDE12345", 200.0) {
        state.edit_settings(|st| st.apple.team_id = v.trim().to_owned());
    }
    if let Some(v) = text_setting(ui, "Key ID", &s.key_id, "XYZ987WXYZ", 200.0) {
        state.edit_settings(|st| st.apple.key_id = v.trim().to_owned());
    }
    if let Some(v) = text_setting(ui, "Private key (.p8)", &s.private_key_path, "~/keys/AuthKey_XYZ.p8", 260.0) {
        state.edit_settings(|st| st.apple.private_key_path = v.trim().to_owned());
    }
    widgets::hairline(ui);
    let creds = state.creds.clone();
    let mut error = None;
    row(ui, "Or developer token", |ui| {
        if let Err(e) = secret_field(
            ui,
            &creds,
            "apple_dev_token",
            "Paste a token",
            |c| c.apple_developer_token.clone(),
            |c, v| c.apple_developer_token = v,
        ) {
            error = Some(e.to_string());
        }
    });
    if let Some(e) = error {
        state.toast(ToastKind::Error, format!("Couldn't save the token: {e}"));
    }
    widgets::caption(
        ui,
        &format!(
            "A pasted token is used instead of the key. Sign-in opens a page on http://127.0.0.1:{}. Full songs are DRM-protected, so Medley plays 30-second previews and opens the Music app for full tracks.",
            s.redirect_port
        ),
    );
}

fn order_section(ui: &mut Ui, state: &mut AppState, intents: &mut Vec<Intent>) {
    let mut order = state.settings().provider_order.clone();
    complete_order(&mut order);
    widgets::section_header(ui, "Service order");
    widgets::card(ui, |ui| {
        let pal = theme::palette(ui.ctx());
        let n = order.len();
        for (i, kind) in order.iter().enumerate() {
            if i > 0 {
                widgets::hairline(ui);
            }
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.label(RichText::new(format!("{}", i + 1)).color(pal.secondary));
                widgets::provider_dot(ui, *kind, 8.0);
                ui.label(kind.name());
                if state.is_connected(*kind) {
                    ui.label(RichText::new("Connected").font(theme::regular(11.5)).color(pal.secondary));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(i + 1 < n, |ui| {
                        if chevron_button(ui, false, "Move down").clicked() {
                            intents.push(Intent::Move(i, i + 1));
                        }
                    });
                    ui.add_enabled_ui(i > 0, |ui| {
                        if chevron_button(ui, true, "Move up").clicked() {
                            intents.push(Intent::Move(i, i - 1));
                        }
                    });
                });
            });
        }
        widgets::caption(ui, "AI features and exports look for songs on services in this order.");
    });
}

/// Make sure every service appears exactly once.
fn complete_order(order: &mut Vec<ProviderKind>) {
    let mut seen = Vec::new();
    order.retain(|k| {
        let fresh = !seen.contains(k);
        seen.push(*k);
        fresh
    });
    for k in ProviderKind::ALL {
        if !order.contains(&k) {
            order.push(k);
        }
    }
}

/// Up/down chevron in the same style as `widgets::icon_button`.
fn chevron_button(ui: &mut Ui, up: bool, tooltip: &str) -> egui::Response {
    let pal = theme::palette(ui.ctx());
    let size = 24.0;
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered() && ui.is_enabled());
        if hover > 0.0 {
            let c = pal.fill_strong;
            let bg = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (c.a() as f32 * hover) as u8);
            ui.painter().circle_filled(rect.center(), size * 0.5, bg);
        }
        let color = if ui.is_enabled() { pal.label } else { pal.tertiary };
        let g = Rect::from_center_size(rect.center(), vec2(size * 0.4, size * 0.22));
        let (a, b, c) = if up {
            (pos2(g.left(), g.bottom()), pos2(g.center().x, g.top()), pos2(g.right(), g.bottom()))
        } else {
            (pos2(g.left(), g.top()), pos2(g.center().x, g.bottom()), pos2(g.right(), g.top()))
        };
        ui.painter().add(Shape::line(vec![a, b, c], Stroke::new(1.8, color)));
    }
    response.on_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_completed_and_deduped() {
        let mut o = vec![ProviderKind::AppleMusic, ProviderKind::AppleMusic];
        complete_order(&mut o);
        assert_eq!(o, vec![ProviderKind::AppleMusic, ProviderKind::Spotify, ProviderKind::SoundCloud]);
    }
}
