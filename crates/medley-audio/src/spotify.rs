//! Spotify playback through librespot (Spotify Premium required).
//!
//! The session logs in with the OAuth access token from the Web API login, then
//! librespot caches reusable credentials so later launches need no browser.
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

    /// Log in. Uses cached reusable credentials when present, else the token.
    pub async fn connect(&self, access_token: Option<String>, client_id: String, bitrate: u16) -> Result<String> {
        let mut state = self.state.lock().await;
        if let Some(old) = state.take() {
            old.player.stop();
            old.session.shutdown();
        }
        let cache = Cache::new(Some(self.cache_dir.clone()), None, None, None)?;
        let cached = cache.credentials();

        let mut attempts: Vec<Credentials> = Vec::new();
        if let Some(c) = cached {
            attempts.push(c);
        }
        if let Some(t) = access_token.clone() {
            attempts.push(Credentials::with_access_token(t));
        }
        anyhow::ensure!(!attempts.is_empty(), "no Spotify credentials yet — connect your account first");

        let mut last_err = None;
        for creds in attempts {
            let config = SessionConfig { client_id: client_id.clone(), ..SessionConfig::default() };
            let session = Session::new(config, Some(Cache::new(Some(self.cache_dir.clone()), None, None, None)?));
            match session.connect(creds, true).await {
                Ok(()) => {
                    let connected = self.start_player(session, client_id, bitrate)?;
                    let user = connected.session.username();
                    *state = Some(connected);
                    return Ok(user);
                }
                Err(e) => {
                    tracing::warn!("librespot login attempt failed: {e}");
                    last_err = Some(e);
                }
            }
        }
        Err(anyhow!(
            "Spotify playback login failed ({}). Playback needs Spotify Premium.",
            last_err.map(|e| e.to_string()).unwrap_or_default()
        ))
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
                    PlayerEvent::Unavailable { track_id, .. } if is_current(&track_id) => {
                        let _ = events.send(AudioEvent::Failed {
                            generation,
                            message: "This song isn't available on Spotify in your country".into(),
                        });
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
        if let Some(c) = self.state.lock().await.as_ref() {
            c.player.pause();
        }
    }

    pub async fn resume(&self) {
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
    }
}

fn volume_to_u16(v: f64) -> u16 {
    (v.clamp(0.0, 1.0) * u16::MAX as f64).round() as u16
}
