//! Offline demo data and automated screenshots, for docs and visual checks.
//!
//! * `MEDLEY_DEMO=1` — fake accounts, playlists, mixes and a paused queue; no
//!   network, nothing saved to disk.
//! * `MEDLEY_LAYOUT=<name>` — start with a built-in layout (e.g. "DJ Booth").
//! * `MEDLEY_THEME=light|dark`, `MEDLEY_ACCENT=<name>` — override appearance.
//! * `MEDLEY_PANEL=<slug>` — bring a panel to the front (e.g. `ai_studio`).
//! * `MEDLEY_SCREENSHOT=<file.png>` — save one frame after startup, then quit.

use crate::app::{AccountState, AppState, Loadable, NowPlaying, View};
use crate::player::Status;
use medley_ai::LinerNotes;
use medley_core::layout::PanelKind;
use medley_core::store::{Accent, ThemeMode};
use medley_core::{Account, Mix, Playability, PlaylistSummary, ProviderKind, Track};

pub fn enabled() -> bool {
    std::env::var_os("MEDLEY_DEMO").is_some()
}

pub fn screenshot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("MEDLEY_SCREENSHOT").map(Into::into)
}

/// Appearance / layout overrides that work with or without demo data.
pub fn apply_overrides(state: &mut AppState) {
    if let Ok(theme) = std::env::var("MEDLEY_THEME") {
        let mode = match theme.as_str() {
            "light" => ThemeMode::Light,
            "dark" => ThemeMode::Dark,
            _ => ThemeMode::System,
        };
        state.edit_settings(|s| s.appearance.theme = mode);
    }
    if let Some(accent) = std::env::var("MEDLEY_ACCENT").ok().and_then(|a| Accent::from_name(&a)) {
        state.edit_settings(|s| s.appearance.accent = accent);
    }
    if let Ok(name) = std::env::var("MEDLEY_LAYOUT")
        && let Some(spec) = state.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(&name)).cloned()
    {
        state.apply_layout(spec);
    }
    if let Some(panel) =
        std::env::var("MEDLEY_PANEL").ok().and_then(|p| PanelKind::ALL.into_iter().find(|k| k.slug() == p))
    {
        state.focus_panel(panel);
    }
}

fn track(provider: ProviderKind, title: &str, artist: &str, album: &str, secs: u64) -> Track {
    Track {
        provider,
        id: format!("demo-{}", title.to_lowercase().replace(' ', "-")),
        title: title.into(),
        artists: vec![artist.into()],
        album: Some(album.into()),
        duration_ms: secs * 1000,
        artwork: None,
        artwork_large: None,
        isrc: None,
        permalink: None,
        preview_url: None,
        playability: if provider == ProviderKind::AppleMusic { Playability::Preview } else { Playability::Full },
    }
}

pub fn seed(state: &mut AppState) {
    use ProviderKind::*;
    for (kind, name) in [(Spotify, "Martin"), (SoundCloud, "martin.k"), (AppleMusic, "Martin")] {
        state.accounts.insert(
            kind,
            AccountState::Connected(Account {
                provider: kind,
                user_id: format!("demo-{}", kind.slug()),
                display_name: name.into(),
                avatar: None,
            }),
        );
    }
    let pl = |provider, id: &str, name: &str, n: u32| PlaylistSummary {
        provider,
        id: id.into(),
        name: name.into(),
        description: None,
        owner: Some("Martin".into()),
        track_count: Some(n),
        artwork: None,
        editable: true,
    };
    state.playlists.insert(
        Spotify,
        Loadable::Ready(vec![
            pl(Spotify, "1", "Late Night Coding", 84),
            pl(Spotify, "2", "Gym Rotation", 41),
            pl(Spotify, "3", "Road Trip 2026", 63),
        ]),
    );
    state.playlists.insert(
        SoundCloud,
        Loadable::Ready(vec![pl(SoundCloud, "4", "Lo-fi Uploads", 120), pl(SoundCloud, "5", "Live Sets", 12)]),
    );
    state.playlists.insert(AppleMusic, Loadable::Ready(vec![pl(AppleMusic, "6", "Favourites", 230)]));

    let drive = vec![
        track(Spotify, "Midnight City", "M83", "Hurry Up, We're Dreaming", 244),
        track(Spotify, "Nightcall", "Kavinsky", "OutRun", 258),
        track(SoundCloud, "Rainy Window (Live Set Edit)", "nightdrive", "Night Tapes", 312),
        track(AppleMusic, "A Real Hero", "College & Electric Youth", "Drive (Original Motion Picture Soundtrack)", 268),
        track(Spotify, "Resonance", "HOME", "Odyssey", 213),
        track(Spotify, "Tadow", "Masego & FKJ", "Tadow", 301),
        track(SoundCloud, "Neon Rain", "lofi.kid", "Uploads", 176),
        track(Spotify, "Sunset Lover", "Petit Biscuit", "Presence", 237),
        track(AppleMusic, "Instant Crush", "Daft Punk", "Random Access Memories", 337),
        track(Spotify, "Intro", "The xx", "xx", 127),
        track(Spotify, "Breathe", "Télépopmusik", "Genetic World", 279),
        track(SoundCloud, "City Lights at 3AM", "Ambient Drives", "Uploads", 402),
    ];
    let sunday = vec![
        track(Spotify, "Holocene", "Bon Iver", "Bon Iver, Bon Iver", 336),
        track(AppleMusic, "Pink + White", "Frank Ocean", "Blonde", 184),
        track(Spotify, "Bloom", "The Paper Kites", "Woodland", 213),
        track(SoundCloud, "Morning Coffee", "chillhop", "Uploads", 151),
    ];
    let mut a = Mix::new("Rainy City Drive");
    a.description = "Synth-washed night driving through wet neon streets, warm rather than cold.".into();
    a.ai_prompt = Some("late-night drive through a rainy city, synthy but warm".into());
    a.tracks = drive.clone();
    let mut b = Mix::new("Slow Sunday");
    b.description = "Soft, sunlit songs for a slow start.".into();
    b.tracks = sunday;
    let id = a.id.clone();
    state.mixes = vec![a, b];
    state.view = View::Mix(id);

    state.player.load(drive, 1, "Rainy City Drive");
    if let Some(t) = state.player.current().cloned() {
        state.now = Some(NowPlaying {
            track: t,
            generation: None,
            notes: Loadable::Ready(LinerNotes {
                headline: "A slow-burning synth pulse built for empty highways.".into(),
                notes: "The song rides a single, patient arpeggio and lets the vocal sit low in the mix, more \
                        atmosphere than hook. Everything is drenched in reverb, so the drums feel far away while \
                        the bass stays close and warm."
                    .into(),
                listen_for: "The moment the filtered synth opens up after the second chorus.".into(),
                mood: vec!["nocturnal".into(), "driving".into(), "cinematic".into()],
            }),
        });
    }
    state.status = Status::Paused;
    state.keep_vibe = true;
    state.studio.vibe_prompt = "late-night drive through a rainy city, synthy but warm".into();
}
