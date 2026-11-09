use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// A music service Medley can talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Spotify,
    SoundCloud,
    AppleMusic,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 3] = [Self::Spotify, Self::SoundCloud, Self::AppleMusic];

    pub fn name(self) -> &'static str {
        match self {
            Self::Spotify => "Spotify",
            Self::SoundCloud => "SoundCloud",
            Self::AppleMusic => "Apple Music",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Spotify => "spotify",
            Self::SoundCloud => "soundcloud",
            Self::AppleMusic => "apple_music",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.slug() == s)
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// How much of a track Medley can play natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Playability {
    #[default]
    Full,
    /// Only a ~30 s preview is streamable (Apple Music, some SoundCloud tracks).
    Preview,
    Unavailable,
}

/// A track from any provider. Cheap to clone; stored inside mixes as-is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub provider: ProviderKind,
    /// Provider-local id (Spotify base62 id, SoundCloud URN, Apple catalog/library id).
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub duration_ms: u64,
    /// Small artwork (~64–100 px) for list rows.
    #[serde(default)]
    pub artwork: Option<String>,
    /// Large artwork (~500–640 px) for Now Playing and headers.
    #[serde(default)]
    pub artwork_large: Option<String>,
    #[serde(default)]
    pub isrc: Option<String>,
    /// Web link that opens the track in the provider's own app/site.
    #[serde(default)]
    pub permalink: Option<String>,
    /// Direct preview URL when the provider hands one out (Apple Music).
    #[serde(default)]
    pub preview_url: Option<String>,
    #[serde(default)]
    pub playability: Playability,
}

impl Track {
    /// Globally unique key: `provider:id`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider.slug(), self.id)
    }

    pub fn artist_line(&self) -> String {
        if self.artists.is_empty() { "Unknown artist".to_owned() } else { self.artists.join(", ") }
    }

    pub fn primary_artist(&self) -> &str {
        self.artists.first().map(String::as_str).unwrap_or("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistSummary {
    pub provider: ProviderKind,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub track_count: Option<u32>,
    #[serde(default)]
    pub artwork: Option<String>,
    #[serde(default)]
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub provider: ProviderKind,
    pub user_id: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar: Option<String>,
}

/// A cross-platform playlist that lives in Medley. Tracks keep their origin, so a
/// mix can hold Spotify, SoundCloud and Apple Music songs side by side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mix {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
    pub created_at: u64,
    pub updated_at: u64,
    /// The prompt that produced this mix, if an AI feature created it.
    #[serde(default)]
    pub ai_prompt: Option<String>,
}

impl Mix {
    pub fn new(name: impl Into<String>) -> Self {
        let now = now_unix();
        Self {
            id: new_id(),
            name: name.into(),
            description: String::new(),
            tracks: Vec::new(),
            created_at: now,
            updated_at: now,
            ai_prompt: None,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_unix();
    }

    /// Track counts per provider, in `ProviderKind::ALL` order.
    pub fn provider_counts(&self) -> [usize; 3] {
        let mut counts = [0; 3];
        for t in &self.tracks {
            let i = ProviderKind::ALL.iter().position(|k| *k == t.provider).unwrap_or(0);
            counts[i] += 1;
        }
        counts
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.tracks.iter().map(|t| t.duration_ms).sum()
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 16 hex chars of randomness — plenty for local ids.
pub fn new_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

/// `m:ss`, or `h:mm:ss` for anything an hour or longer.
pub fn format_duration(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// "1 hr 12 min" style summary used in collection headers.
pub fn format_long_duration(ms: u64) -> String {
    let mins = ms / 60_000;
    match (mins / 60, mins % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} hr"),
        (h, m) => format!("{h} hr {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(225_000), "3:45");
        assert_eq!(format_duration(3_725_000), "1:02:05");
        assert_eq!(format_long_duration(72 * 60_000), "1 hr 12 min");
        assert_eq!(format_long_duration(5 * 60_000), "5 min");
    }

    #[test]
    fn slugs_roundtrip() {
        for k in ProviderKind::ALL {
            assert_eq!(ProviderKind::from_slug(k.slug()), Some(k));
        }
    }
}
