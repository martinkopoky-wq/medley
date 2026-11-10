//! Spotify playback through librespot (Spotify Premium required).
//!
//! Playback uses librespot's own OAuth identity. Developer Web API tokens cannot
//! be reused for login5; it rejects them even after the access point accepts them.
//! Reusable playback credentials live in their own cache, separate from legacy
//! Web API sessions, so later launches need no browser.
//! librespot closes its audio device while paused, so an idle Medley holds no
//! output stream.

use crate::{AudioEvent, Clock};
use anyhow::{Context, Result, anyhow};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::{Session, SessionConfig, SpotifyUri};
use librespot_playback::audio_backend;
use librespot_playback::config::{AudioFormat, Bitrate, PlayerConfig};
use librespot_playback::mixer::softmixer::SoftMixer;
use librespot_playback::mixer::{Mixer, MixerConfig};
use librespot_playback::player::{Player, PlayerEvent};
use medley_core::oauth::{Pkce, random_string, wait_for_code};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

struct Connected {
    session: Session,
    player: Arc<Player>,
    mixer: Arc<SoftMixer>,
    client_id: String,
    bitrate: u16,
}

pub struct SpotifyEngine {
    cache_dir: PathBuf,
    events: UnboundedSender<AudioEvent>,
    clock: Arc<Mutex<Clock>>,
    state: tokio::sync::Mutex<Option<Connected>>,
    /// Generation and URI of the track librespot is currently playing.
    current: Arc<Mutex<(u64, String)>>,
    volume: Arc<AtomicU64>,
}

impl SpotifyEngine {
    pub fn new(cache_dir: PathBuf, events: UnboundedSender<AudioEvent>, clock: Arc<Mutex<Clock>>) -> Self {
        Self {
            cache_dir,
            events,
            clock,
            state: tokio::sync::Mutex::new(None),
            current: Arc::new(Mutex::new((0, String::new()))),
            volume: Arc::new(AtomicU64::new(0.7f64.to_bits())),
        }
    }

    pub async fn is_ready(&self) -> bool {
        self.state.lock().await.as_ref().is_some_and(|c| !c.session.is_invalid())
    }

    /// Authenticate playback independently of a custom Web API developer app.
    pub async fn connect(&self, access_token: Option<String>, client_id: String, bitrate: u16) -> Result<String> {
        let mut state = self.state.lock().await;
        if let Some(old) = state.take() {
            old.player.stop();
            old.session.shutdown();
        }
        let playback_client_id = SessionConfig::default().client_id;
        let cache_dir = self.cache_dir.join("playback");
        let cache = Cache::new(Some(cache_dir.clone()), None, None, None)?;
        let cached = cache.credentials();

        let mut attempts: Vec<Credentials> = Vec::new();
        if let Some(c) = cached {
            attempts.push(c);
        }
        if client_id == playback_client_id
            && let Some(t) = access_token
        {
            attempts.push(Credentials::with_access_token(t));
        }
        for creds in attempts {
            match playback_session(&cache_dir, creds).await {
                Ok(session) => {
                    let connected = self.start_player(session, playback_client_id, bitrate)?;
                    let user = connected.session.username();
                    *state = Some(connected);
                    return Ok(user);
                }
                Err(e) => {
                    tracing::warn!("cached Spotify playback login failed: {e}");
                }
            }
        }
        let token = authorize_playback(&playback_client_id).await?;
        let session = playback_session(&cache_dir, Credentials::with_access_token(token))
            .await
            .context("Spotify playback login failed; playback requires Spotify Premium")?;
        let connected = self.start_player(session, playback_client_id, bitrate)?;
        let user = connected.session.username();
        *state = Some(connected);
        Ok(user)
    }

    fn start_player(&self, session: Session, client_id: String, bitrate: u16) -> Result<Connected> {
        let mixer = Arc::new(SoftMixer::open(MixerConfig::default()).map_err(|e| anyhow!("mixer: {e}"))?);
        mixer.set_volume(volume_to_u16(f64::from_bits(self.volume.load(Ordering::Relaxed))));
        let backend = audio_backend::find(None).context("librespot was built without an audio backend")?;
        let config = PlayerConfig {
            bitrate: match bitrate {
                96 => Bitrate::Bitrate96,
                160 => Bitrate::Bitrate160,
                _ => Bitrate::Bitrate320,
            },
            ..PlayerConfig::default()
        };
        let player = Player::new(config, session.clone(), mixer.get_soft_volume(), move || {
            backend(None, AudioFormat::default())
        });

        let mut rx = player.get_player_event_channel();
        let events = self.events.clone();
        let clock = self.clock.clone();
        let current = self.current.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let (generation, uri) = current.lock().expect("current lock").clone();
                let is_current = |id: &SpotifyUri| id.to_string() == uri;
                match event {
                    PlayerEvent::Playing { position_ms, track_id, .. } if is_current(&track_id) => {
                        clock.lock().expect("clock").play_at(position_ms as u64);
                        let _ = events.send(AudioEvent::Buffering { generation, buffering: false });
                    }
                    PlayerEvent::Paused { position_ms, track_id, .. } if is_current(&track_id) => {
                        clock.lock().expect("clock").pause_at(position_ms as u64);
                    }
                    PlayerEvent::Seeked { position_ms, track_id, .. }
                    | PlayerEvent::PositionCorrection { position_ms, track_id, .. }
                        if is_current(&track_id) =>
                    {
                        clock.lock().expect("clock").rebase(position_ms as u64);
                    }
                    PlayerEvent::Loading { track_id, .. } if is_current(&track_id) => {
                        let _ = events.send(AudioEvent::Buffering { generation, buffering: true });
                    }
                    PlayerEvent::EndOfTrack { track_id, .. } if is_current(&track_id) => {
                        let _ = events.send(AudioEvent::Ended { generation });
                    }
                    PlayerEvent::Unavailable { track_id, .. } => {
                        if let Some(failure) = unavailable_event(&current, &track_id) {
                            clock.lock().expect("clock").pause_at(0);
                            let _ = events.send(failure);
                        }
                    }
                    PlayerEvent::SessionDisconnected { .. } => {
                        let _ = events.send(AudioEvent::SpotifySession {
                            connected: false,
                            message: Some("Spotify session disconnected".into()),
                        });
                    }
                    _ => {}
                }
            }
        });

        Ok(Connected { session, player, mixer, client_id, bitrate })
    }

    async fn reconnect_if_needed(&self) -> Result<()> {
        let (client_id, bitrate) = {
            let state = self.state.lock().await;
            match state.as_ref() {
                Some(c) if !c.session.is_invalid() => return Ok(()),
                Some(c) => (c.client_id.clone(), c.bitrate),
                None => return Err(anyhow!("Spotify playback isn't ready yet")),
            }
        };
        tracing::info!("Spotify session dropped, reconnecting with cached credentials");
        self.connect(None, client_id, bitrate).await.map(|_| ())
    }

    pub async fn load(&self, uri: &str, generation: u64) -> Result<()> {
        self.reconnect_if_needed().await?;
        let parsed = SpotifyUri::from_uri(uri).map_err(|e| anyhow!("bad Spotify URI {uri}: {e}"))?;
        *self.current.lock().expect("current lock") = (generation, parsed.to_string());
        self.clock.lock().expect("clock").pause_at(0);
        let state = self.state.lock().await;
        let c = state.as_ref().context("Spotify playback isn't ready yet")?;
        c.player.load(parsed, true, 0);
        Ok(())
    }

    pub async fn pause(&self) {
        if self.current.lock().expect("current lock").1.is_empty() {
            return;
        }
        if let Some(c) = self.state.lock().await.as_ref() {
            c.player.pause();
        }
    }

    pub async fn resume(&self) {
        if self.current.lock().expect("current lock").1.is_empty() {
            return;
        }
        if let Some(c) = self.state.lock().await.as_ref() {
            c.player.play();
        }
    }

    pub async fn stop(&self) {
        *self.current.lock().expect("current lock") = (0, String::new());
        if let Some(c) = self.state.lock().await.as_ref() {
            c.player.stop();
        }
    }

    pub async fn seek(&self, ms: u64) {
        if self.current.lock().expect("current lock").1.is_empty() {
            return;
        }
        if let Some(c) = self.state.lock().await.as_ref() {
            c.player.seek(ms.min(u32::MAX as u64) as u32);
        }
    }

    pub async fn set_volume(&self, volume: f32) {
        self.volume.store((volume as f64).to_bits(), Ordering::Relaxed);
        if let Some(c) = self.state.lock().await.as_ref() {
            c.mixer.set_volume(volume_to_u16(volume as f64));
        }
    }

    /// Forget the session and the cached reusable credentials.
    pub async fn logout(&self) {
        if let Some(c) = self.state.lock().await.take() {
            c.player.stop();
            c.session.shutdown();
        }
        let _ = std::fs::remove_file(self.cache_dir.join("credentials.json"));
        let _ = std::fs::remove_file(self.cache_dir.join("playback/credentials.json"));
    }
}

async fn playback_session(cache_dir: &std::path::Path, credentials: Credentials) -> Result<Session> {
    let session = Session::new(SessionConfig::default(), Some(Cache::new(Some(cache_dir), None, None, None)?));
    let result = tokio::time::timeout(std::time::Duration::from_secs(45), async {
        session.connect(credentials, true).await?;
        // AP authentication alone also accepts grants that login5 cannot use.
        // Check this before reporting the playback engine as ready.
        session.login5().auth_token().await?;
        tracing::info!(
            "Spotify playback account tier: {}",
            session.get_user_attribute("type").unwrap_or_else(|| "unknown".into())
        );
        Ok::<_, librespot_core::Error>(())
    })
    .await;
    match result {
        Ok(Ok(())) => Ok(session),
        Ok(Err(error)) => {
            session.shutdown();
            Err(error.into())
        }
        Err(_) => {
            session.shutdown();
            Err(anyhow!("Spotify playback authentication timed out"))
        }
    }
}

async fn authorize_playback(client_id: &str) -> Result<String> {
    const REDIRECT: &str = "http://127.0.0.1:8898/login";
    let pkce = Pkce::new();
    let state = random_string(24);
    let mut url = url::Url::parse("https://accounts.spotify.com/authorize")?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", REDIRECT)
        .append_pair("scope", "streaming")
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("state", &state);
    let waiter = tokio::spawn(async move { wait_for_code(8898, "/login", &state, "Spotify playback").await });
    tokio::task::yield_now().await;
    open::that_detached(url.as_str()).context("couldn't open Spotify playback approval in the browser")?;
    let code = waiter.await??;
    #[derive(serde::Deserialize)]
    struct PlaybackToken {
        access_token: String,
    }
    let token: PlaybackToken = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .post("https://accounts.spotify.com/api/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", REDIRECT),
            ("client_id", client_id),
            ("code_verifier", pkce.verifier.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(token.access_token)
}

fn volume_to_u16(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * u16::MAX as f64).round() as u16
}

fn unavailable_event(current: &Mutex<(u64, String)>, track_id: &SpotifyUri) -> Option<AudioEvent> {
    let mut active = current.lock().expect("current lock");
    if active.1 != track_id.to_string() {
        return None;
    }
    active.1.clear();
    Some(AudioEvent::Failed {
        generation: active.0,
        message: "Spotify couldn't load this track. Playback authorization or audio delivery failed; try another track or open it in Spotify.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_stops_controls_without_claiming_a_country_restriction() {
        let uri = SpotifyUri::from_uri("spotify:track:398wIEUnHVOnTEMnxOHiY9").unwrap();
        let current = Mutex::new((42, uri.to_string()));
        let Some(AudioEvent::Failed { generation, message }) = unavailable_event(&current, &uri) else {
            panic!("expected a playback failure");
        };
        assert_eq!(generation, 42);
        assert!(current.lock().unwrap().1.is_empty());
        assert!(!message.contains("country"));
        assert!(unavailable_event(&current, &uri).is_none());
    }

    #[test]
    fn late_unavailable_cannot_clear_a_new_track() {
        let old = SpotifyUri::from_uri("spotify:track:398wIEUnHVOnTEMnxOHiY9").unwrap();
        let next = "spotify:track:0UqO1j46DW7r8ZeCRKcMCU";
        let current = Mutex::new((43, next.to_owned()));
        assert!(unavailable_event(&current, &old).is_none());
        assert_eq!(*current.lock().unwrap(), (43, next.to_owned()));
    }
}
