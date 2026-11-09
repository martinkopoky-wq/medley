//! On-disk state: settings, secrets, mixes and layouts as small JSON files.
//!
//! Secrets (OAuth tokens, API keys) live in `credentials.json`, written with
//! `0600` permissions on Unix. Writes are atomic (temp file + rename) so a crash
//! mid-write never corrupts state.

use crate::model::{Mix, ProviderKind, now_unix};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("app", "Medley", "medley")
            .context("could not determine a home directory")?;
        let paths = Self {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            cache: dirs.cache_dir().to_path_buf(),
        };
        for dir in [&paths.config, &paths.data, &paths.cache, &paths.layouts_dir()] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(paths)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.json")
    }
    pub fn credentials_file(&self) -> PathBuf {
        self.config.join("credentials.json")
    }
    pub fn mixes_file(&self) -> PathBuf {
        self.data.join("mixes.json")
    }
    pub fn dock_file(&self) -> PathBuf {
        self.data.join("dock.json")
    }
    pub fn layouts_dir(&self) -> PathBuf {
        self.data.join("layouts")
    }
    pub fn librespot_dir(&self) -> PathBuf {
        self.cache.join("librespot")
    }
}

/// Read JSON, falling back to `T::default()` when the file is missing or broken.
pub fn load_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            tracing::warn!("ignoring unreadable {}: {e}", path.display());
            T::default()
        }),
        Err(_) => T::default(),
    }
}

pub fn save_json<T: Serialize + ?Sized>(path: &Path, value: &T, private: bool) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_atomic(path, &bytes, private)
}

pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{:x}", rand::random::<u32>()));
    {
        use std::io::Write;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        let mut f = opts.open(&tmp).with_context(|| format!("writing {}", tmp.display()))?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub spotify: SpotifySettings,
    pub soundcloud: SoundCloudSettings,
    pub apple: AppleSettings,
    pub ai: AiSettings,
    pub appearance: Appearance,
    pub volume: f32,
    /// Preferred order when AI features look a song up across services.
    pub provider_order: Vec<ProviderKind>,
    pub layout_locked: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            spotify: SpotifySettings::default(),
            soundcloud: SoundCloudSettings::default(),
            apple: AppleSettings::default(),
            ai: AiSettings::default(),
            appearance: Appearance::default(),
            volume: 0.7,
            provider_order: ProviderKind::ALL.to_vec(),
            layout_locked: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SpotifySettings {
    /// Your own Spotify app's client id. Empty = librespot's built-in id.
    pub client_id: String,
    pub redirect_port: u16,
    /// 96, 160 or 320 kbps.
    pub bitrate: u16,
}

impl Default for SpotifySettings {
    fn default() -> Self {
        Self { client_id: String::new(), redirect_port: 8898, bitrate: 320 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SoundCloudSettings {
    pub client_id: String,
    pub redirect_port: u16,
}

impl Default for SoundCloudSettings {
    fn default() -> Self {
        Self { client_id: String::new(), redirect_port: 8899 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppleSettings {
    /// MusicKit identifiers, used to mint a developer token from a `.p8` key.
    pub team_id: String,
    pub key_id: String,
    pub private_key_path: String,
    pub redirect_port: u16,
}

impl Default for AppleSettings {
    fn default() -> Self {
        Self { team_id: String::new(), key_id: String::new(), private_key_path: String::new(), redirect_port: 8900 }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiBackend {
    Off,
    #[default]
    Ollama,
    Claude,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiSettings {
    pub backend: AiBackend,
    pub ollama_url: String,
    pub ollama_model: String,
    pub claude_model: String,
    /// low | medium | high
    pub claude_effort: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            backend: AiBackend::Ollama,
            ollama_url: "http://127.0.0.1:11434".into(),
            ollama_model: "llama3.2".into(),
            claude_model: "claude-opus-5-5".into(),
            claude_effort: "medium".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

/// The macOS accent palette.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    Blue,
    Purple,
    #[default]
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Graphite,
}

impl Accent {
    pub const ALL: [Accent; 8] =
        [Self::Blue, Self::Purple, Self::Pink, Self::Red, Self::Orange, Self::Yellow, Self::Green, Self::Graphite];

    /// Apple system colors as (light, dark) RGB.
    pub fn rgb(self, dark: bool) -> [u8; 3] {
        let (l, d) = match self {
            Self::Blue => ([0x00, 0x7A, 0xFF], [0x0A, 0x84, 0xFF]),
            Self::Purple => ([0xAF, 0x52, 0xDE], [0xBF, 0x5A, 0xF2]),
            Self::Pink => ([0xFF, 0x2D, 0x55], [0xFF, 0x37, 0x5F]),
            Self::Red => ([0xFF, 0x3B, 0x30], [0xFF, 0x45, 0x3A]),
            Self::Orange => ([0xFF, 0x95, 0x00], [0xFF, 0x9F, 0x0A]),
            Self::Yellow => ([0xFF, 0xCC, 0x00], [0xFF, 0xD6, 0x0A]),
            Self::Green => ([0x34, 0xC7, 0x59], [0x30, 0xD1, 0x58]),
            Self::Graphite => ([0x8E, 0x8E, 0x93], [0x98, 0x98, 0x9D]),
        };
        if dark { d } else { l }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Graphite => "Graphite",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(&s))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Appearance {
    pub theme: ThemeMode,
    pub accent: Accent,
    /// Transparent window so the compositor (macOS vibrancy, Hyprland/KWin blur) shows through.
    pub translucent: bool,
    pub reduce_motion: bool,
    pub ui_scale: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self { theme: ThemeMode::System, accent: Accent::Pink, translucent: false, reduce_motion: false, ui_scale: 1.0 }
    }
}

pub type SettingsHandle = Arc<RwLock<Settings>>;

// ---------------------------------------------------------------------------
// Credentials (secrets)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
    #[serde(default)]
    pub scope: Option<String>,
    /// Client id the token was issued to (refreshes must use the same one).
    #[serde(default)]
    pub client_id: Option<String>,
}

impl OAuthTokens {
    pub fn from_response(
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<u64>,
        scope: Option<String>,
        client_id: Option<String>,
    ) -> Self {
        Self { access_token, refresh_token, expires_at: now_unix() + expires_in.unwrap_or(3600), scope, client_id }
    }

    /// Treat tokens as expired a minute early to avoid racing the deadline.
    pub fn is_expired(&self) -> bool {
        now_unix() + 60 >= self.expires_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Credentials {
    pub spotify: Option<OAuthTokens>,
    pub soundcloud: Option<OAuthTokens>,
    pub soundcloud_client_secret: Option<String>,
    pub apple_developer_token: Option<String>,
    pub apple_music_user_token: Option<String>,
    pub anthropic_api_key: Option<String>,
}

/// Shared, persisted credentials. Every mutation is written straight to disk.
#[derive(Clone)]
pub struct CredentialStore {
    path: PathBuf,
    inner: Arc<Mutex<Credentials>>,
}

impl CredentialStore {
    pub fn load(path: PathBuf) -> Self {
        let creds: Credentials = load_json(&path);
        Self { path, inner: Arc::new(Mutex::new(creds)) }
    }

    pub fn get(&self) -> Credentials {
        self.inner.lock().expect("credentials lock").clone()
    }

    pub fn update(&self, f: impl FnOnce(&mut Credentials)) -> Result<()> {
        let snapshot = {
            let mut guard = self.inner.lock().expect("credentials lock");
            f(&mut guard);
            guard.clone()
        };
        save_json(&self.path, &snapshot, true)
    }
}

// ---------------------------------------------------------------------------
// Mixes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MixLibrary {
    #[serde(default)]
    pub mixes: Vec<Mix>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_roundtrip_and_permissions() {
        let dir = std::env::temp_dir().join(format!("medley-test-{:x}", rand::random::<u32>()));
        let path = dir.join("creds.json");
        let creds = Credentials { anthropic_api_key: Some("sk-test".into()), ..Default::default() };
        save_json(&path, &creds, true).unwrap();
        let back: Credentials = load_json(&path);
        assert_eq!(back, creds);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn settings_tolerate_missing_fields() {
        let s: Settings = serde_json::from_str(r#"{"volume":0.3}"#).unwrap();
        assert_eq!(s.volume, 0.3);
        assert_eq!(s.spotify.redirect_port, 8898);
        assert_eq!(s.provider_order.len(), 3);
    }

    #[test]
    fn accent_lookup() {
        assert_eq!(Accent::from_name(" purple "), Some(Accent::Purple));
        assert_eq!(Accent::from_name("teal"), None);
    }
}
