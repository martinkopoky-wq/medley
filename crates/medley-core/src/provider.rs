use crate::model::{Account, PlaylistSummary, ProviderKind, Track};
use async_trait::async_trait;

/// Where the audio for a track comes from, as resolved by its provider.
#[derive(Debug, Clone)]
pub enum StreamSource {
    /// Played by the embedded librespot engine (Spotify Premium).
    Spotify { uri: String },
    /// A progressive HTTP file (MP3 / M4A).
    Http { url: String, headers: Vec<(String, String)>, hint: String },
    /// An HLS playlist whose segments are concatenated into one stream.
    Hls { url: String, headers: Vec<(String, String)>, hint: String },
}

impl StreamSource {
    pub fn is_spotify(&self) -> bool {
        matches!(self, Self::Spotify { .. })
    }
}

/// Everything Medley needs from a music service. Each provider owns its auth
/// state; the app only ever sees these calls.
#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    /// `Some(reason)` when credentials the user must supply are missing.
    fn setup_hint(&self) -> Option<String>;

    /// Re-establish a session from stored tokens, without user interaction.
    async fn restore(&self) -> anyhow::Result<Option<Account>>;

    /// Run the interactive login (opens the system browser).
    async fn connect(&self) -> anyhow::Result<Account>;

    async fn disconnect(&self) -> anyhow::Result<()>;

    async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Track>>;

    /// Exact lookup by ISRC, used to move tracks between platforms.
    async fn find_by_isrc(&self, _isrc: &str) -> anyhow::Result<Option<Track>> {
        Ok(None)
    }

    async fn liked_tracks(&self, limit: usize) -> anyhow::Result<Vec<Track>>;

    async fn playlists(&self) -> anyhow::Result<Vec<PlaylistSummary>>;

    async fn playlist_tracks(&self, playlist_id: &str) -> anyhow::Result<Vec<Track>>;

    /// Create a playlist on this platform containing `tracks` (all of this provider).
    async fn create_playlist(&self, name: &str, description: &str, tracks: &[Track])
    -> anyhow::Result<PlaylistSummary>;

    async fn stream(&self, track: &Track) -> anyhow::Result<StreamSource>;

    /// A fresh access token for the playback engine (Spotify only).
    async fn playback_token(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}
