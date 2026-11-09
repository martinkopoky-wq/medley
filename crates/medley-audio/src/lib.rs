//! Medley's playback engine.
//!
//! Two backends behind one handle: librespot for Spotify (feature `spotify`)
//! and rodio/symphonia for everything streamable over HTTP (SoundCloud,
//! Apple Music previews). The output device opens on first play and closes
//! after 30 s of silence, so an idle Medley costs no audio CPU.

mod buffer;
mod download;
#[cfg(feature = "spotify")]
mod spotify;

use anyhow::{Context, Result, anyhow};
use medley_core::StreamSource;
use rodio::mixer::Mixer;
use rodio::{Decoder, OutputStreamBuilder, Sink};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

pub use buffer::StreamBuffer;

const OUTPUT_IDLE_RELEASE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Started { generation: u64 },
    Ended { generation: u64 },
    Failed { generation: u64, message: String },
    Buffering { generation: u64, buffering: bool },
    SpotifySession { connected: bool, message: Option<String> },
}

/// Position clock for backends that report position as events.
#[derive(Debug, Default, Clone, Copy)]
pub struct Clock {
    base_ms: u64,
    since: Option<Instant>,
}

impl Clock {
    pub fn now_ms(&self) -> u64 {
        self.base_ms + self.since.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0)
    }
    pub fn play_at(&mut self, ms: u64) {
        self.base_ms = ms;
        self.since = Some(Instant::now());
    }
    pub fn pause_at(&mut self, ms: u64) {
        self.base_ms = ms;
        self.since = None;
    }
    pub fn rebase(&mut self, ms: u64) {
        self.base_ms = ms;
        if self.since.is_some() {
            self.since = Some(Instant::now());
        }
    }
}

enum Current {
    Idle,
    Local {
        sink: Arc<Sink>,
        buffer: StreamBuffer,
    },
    #[cfg_attr(not(feature = "spotify"), allow(dead_code))]
    Spotify,
}

struct State {
    generation: u64,
    current: Current,
    volume: f32,
    paused: bool,
    duration_ms: u64,
    idle_since: Option<Instant>,
}

struct Inner {
    http: reqwest::Client,
    events: UnboundedSender<AudioEvent>,
    output: OutputHost,
    state: Mutex<State>,
    stream_dir: PathBuf,
    clock: Arc<Mutex<Clock>>,
    #[cfg(feature = "spotify")]
    spotify: spotify::SpotifyEngine,
}

/// Cheap to clone; all clones control the same playback.
#[derive(Clone)]
pub struct AudioEngine {
    inner: Arc<Inner>,
}

impl AudioEngine {
    /// Must be called inside a Tokio runtime.
    pub fn new(events: UnboundedSender<AudioEvent>, cache_dir: PathBuf, volume: f32) -> Self {
        let stream_dir = cache_dir.join("streams");
        buffer::cleanup(&stream_dir);
        // Audio downloads can outlive the API client's 30 s total timeout.
        let http = reqwest::Client::builder()
            .user_agent(medley_core::USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .build()
            .expect("TLS backend available");
        let clock = Arc::new(Mutex::new(Clock::default()));
        let engine = Self {
            inner: Arc::new(Inner {
                http,
                output: OutputHost::spawn(),
                state: Mutex::new(State {
                    generation: 0,
                    current: Current::Idle,
                    volume: volume.clamp(0.0, 1.0),
                    paused: false,
                    duration_ms: 0,
                    idle_since: Some(Instant::now()),
                }),
                stream_dir,
                #[cfg(feature = "spotify")]
                spotify: spotify::SpotifyEngine::new(cache_dir.join("librespot"), events.clone(), clock.clone()),
                clock,
                events,
            }),
        };
        engine.spawn_idle_reaper();
        engine
    }

    pub fn spotify_supported(&self) -> bool {
        cfg!(feature = "spotify")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn generation(&self) -> u64 {
        self.lock().generation
    }

    pub fn is_paused(&self) -> bool {
        let st = self.lock();
        st.paused || matches!(st.current, Current::Idle)
    }

    pub fn is_active(&self) -> bool {
        !matches!(self.lock().current, Current::Idle)
    }

    pub fn duration_ms(&self) -> u64 {
        self.lock().duration_ms
    }

    pub fn position_ms(&self) -> u64 {
        let st = self.lock();
        let pos = match &st.current {
            Current::Idle => 0,
            Current::Local { sink, .. } => sink.get_pos().as_millis() as u64,
            Current::Spotify => self.inner.clock.lock().expect("clock").now_ms(),
        };
        if st.duration_ms > 0 { pos.min(st.duration_ms) } else { pos }
    }

    /// Start playing `source`. Returns the generation that identifies this
    /// playback in later [`AudioEvent`]s; stale events carry older numbers.
    pub async fn play(&self, source: StreamSource, duration_ms: u64) -> Result<u64> {
        let generation = {
            let mut st = self.lock();
            st.generation += 1;
            self.stop_current(&mut st);
            st.paused = false;
            st.duration_ms = duration_ms;
            st.idle_since = None;
            st.generation
        };

        match source {
            StreamSource::Spotify { uri } => self.play_spotify(uri, generation).await?,
            other => self.play_local(other, generation).await?,
        }
        let _ = self.inner.events.send(AudioEvent::Started { generation });
        Ok(generation)
    }

    #[cfg(feature = "spotify")]
    async fn play_spotify(&self, uri: String, generation: u64) -> Result<()> {
        self.inner.spotify.load(&uri, generation).await?;
        let mut st = self.lock();
        if st.generation == generation {
            st.current = Current::Spotify;
        }
        Ok(())
    }

    #[cfg(not(feature = "spotify"))]
    async fn play_spotify(&self, _uri: String, _generation: u64) -> Result<()> {
        Err(anyhow!("this build of Medley has Spotify playback disabled"))
    }

    async fn play_local(&self, source: StreamSource, generation: u64) -> Result<()> {
        let hint = match &source {
            StreamSource::Http { hint, .. } | StreamSource::Hls { hint, .. } => hint.clone(),
            StreamSource::Spotify { .. } => unreachable!(),
        };
        let buffer = StreamBuffer::new(&self.inner.stream_dir).context("creating stream buffer")?;
        tokio::spawn(download::fill(self.inner.http.clone(), source, buffer.clone()));

        let mixer = self.inner.output.acquire().await?;
        let probe_buf = buffer.clone();
        // Probing reads the first bytes, which may still be downloading.
        let decoder = tokio::task::spawn_blocking(move || -> Result<Decoder<buffer::StreamReader>> {
            let total = probe_buf.wait_ready(Duration::from_secs(20)).map_err(|e| anyhow!(e))?;
            let mut builder = Decoder::builder()
                .with_data(probe_buf.reader())
                .with_hint(&hint)
                .with_seekable(true)
                .with_gapless(true);
            if let Some(len) = total {
                builder = builder.with_byte_len(len);
            }
            builder.build().map_err(|e| anyhow!("can't decode this stream: {e}"))
        })
        .await??;

        let mut st = self.lock();
        if st.generation != generation {
            buffer.cancel(); // superseded while we were buffering
            return Ok(());
        }
        let sink = Arc::new(Sink::connect_new(&mixer));
        sink.set_volume(perceptual(st.volume));
        sink.append(decoder);
        let events = self.inner.events.clone();
        sink.append(rodio::source::EmptyCallback::new(Box::new(move || {
            let _ = events.send(AudioEvent::Ended { generation });
        })));
        st.current = Current::Local { sink, buffer };
        Ok(())
    }

    fn stop_current(&self, st: &mut State) {
        match std::mem::replace(&mut st.current, Current::Idle) {
            Current::Local { sink, buffer } => {
                buffer.cancel();
                sink.stop();
            }
            Current::Spotify => {
                #[cfg(feature = "spotify")]
                {
                    let engine = self.clone();
                    tokio::spawn(async move { engine.inner.spotify.stop().await });
                }
            }
            Current::Idle => {}
        }
        st.idle_since = Some(Instant::now());
    }

    pub async fn pause(&self) {
        let spotify = {
            let mut st = self.lock();
            st.paused = true;
            match &st.current {
                Current::Local { sink, .. } => {
                    sink.pause();
                    false
                }
                Current::Spotify => true,
                Current::Idle => false,
            }
        };
        #[cfg(feature = "spotify")]
        if spotify {
            self.inner.spotify.pause().await;
        }
        let _ = spotify;
    }

    pub async fn resume(&self) {
        let spotify = {
            let mut st = self.lock();
            st.paused = false;
            match &st.current {
                Current::Local { sink, .. } => {
                    sink.play();
                    false
                }
                Current::Spotify => true,
                Current::Idle => false,
            }
        };
        #[cfg(feature = "spotify")]
        if spotify {
            self.inner.spotify.resume().await;
        }
        let _ = spotify;
    }

    pub fn stop(&self) {
        let mut st = self.lock();
        st.generation += 1;
        st.paused = false;
        self.stop_current(&mut st);
    }

    pub async fn seek(&self, ms: u64) -> Result<()> {
        let local = {
            let st = self.lock();
            match &st.current {
                Current::Local { sink, .. } => Some(sink.clone()),
                Current::Spotify => None,
                Current::Idle => return Ok(()),
            }
        };
        match local {
            Some(sink) => tokio::task::spawn_blocking(move || sink.try_seek(Duration::from_millis(ms)))
                .await?
                .map_err(|e| anyhow!("seek failed: {e}")),
            None => {
                #[cfg(feature = "spotify")]
                self.inner.spotify.seek(ms).await;
                Ok(())
            }
        }
    }

    pub async fn set_volume(&self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        {
            let mut st = self.lock();
            st.volume = volume;
            if let Current::Local { sink, .. } = &st.current {
                sink.set_volume(perceptual(volume));
            }
        }
        #[cfg(feature = "spotify")]
        self.inner.spotify.set_volume(volume).await;
    }

    pub fn volume(&self) -> f32 {
        self.lock().volume
    }

    /// Bring up the librespot session. Returns the Spotify username.
    pub async fn connect_spotify(
        &self,
        access_token: Option<String>,
        client_id: String,
        bitrate: u16,
    ) -> Result<String> {
        #[cfg(feature = "spotify")]
        {
            self.inner.spotify.set_volume(self.volume()).await;
            let result = self.inner.spotify.connect(access_token, client_id, bitrate).await;
            let _ = self.inner.events.send(AudioEvent::SpotifySession {
                connected: result.is_ok(),
                message: result.as_ref().err().map(|e| e.to_string()),
            });
            result
        }
        #[cfg(not(feature = "spotify"))]
        {
            let _ = (access_token, client_id, bitrate);
            Err(anyhow!("this build of Medley has Spotify playback disabled"))
        }
    }

    pub async fn spotify_ready(&self) -> bool {
        #[cfg(feature = "spotify")]
        return self.inner.spotify.is_ready().await;
        #[cfg(not(feature = "spotify"))]
        false
    }

    pub async fn logout_spotify(&self) {
        #[cfg(feature = "spotify")]
        self.inner.spotify.logout().await;
    }

    /// Close the output device once nothing has played for a while.
    fn spawn_idle_reaper(&self) {
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            loop {
                tick.tick().await;
                let Some(inner) = weak.upgrade() else { break };
                let release = {
                    let st = inner.state.lock().unwrap_or_else(|p| p.into_inner());
                    matches!(st.current, Current::Idle)
                        && st.idle_since.is_some_and(|t| t.elapsed() >= OUTPUT_IDLE_RELEASE)
                };
                if release {
                    inner.output.release();
                }
            }
        });
    }
}

/// Map a linear slider to a 60 dB log curve (same as librespot's default).
fn perceptual(v: f32) -> f32 {
    if v <= 0.001 { 0.0 } else { 10f32.powf((v - 1.0) * 3.0) }
}

enum OutCmd {
    Acquire(std_mpsc::SyncSender<Result<Mixer, String>>),
    Release,
}

/// Owns the (non-`Send`) output stream on its own thread.
struct OutputHost {
    tx: std_mpsc::Sender<OutCmd>,
}

impl OutputHost {
    fn spawn() -> Self {
        let (tx, rx) = std_mpsc::channel::<OutCmd>();
        std::thread::Builder::new()
            .name("medley-audio-out".into())
            .spawn(move || {
                let mut stream: Option<rodio::OutputStream> = None;
                let mut last_acquire = Instant::now();
                for cmd in rx {
                    match cmd {
                        OutCmd::Acquire(reply) => {
                            last_acquire = Instant::now();
                            if stream.is_none() {
                                match OutputStreamBuilder::open_default_stream() {
                                    Ok(mut s) => {
                                        s.log_on_drop(false);
                                        tracing::debug!("audio output opened");
                                        stream = Some(s);
                                    }
                                    Err(e) => {
                                        let _ = reply.send(Err(format!("no audio output: {e}")));
                                        continue;
                                    }
                                }
                            }
                            let mixer =
                                stream.as_ref().map(|s| s.mixer().clone()).ok_or_else(|| "no stream".to_owned());
                            let _ = reply.send(mixer);
                        }
                        // A release decided before a fresh acquire must not kill the new sink.
                        OutCmd::Release if last_acquire.elapsed() >= OUTPUT_IDLE_RELEASE => {
                            if stream.take().is_some() {
                                tracing::debug!("audio output released (idle)");
                            }
                        }
                        OutCmd::Release => {}
                    }
                }
            })
            .expect("spawn audio thread");
        Self { tx }
    }

    async fn acquire(&self) -> Result<Mixer> {
        let (reply_tx, reply_rx) = std_mpsc::sync_channel(1);
        self.tx.send(OutCmd::Acquire(reply_tx)).map_err(|_| anyhow!("audio thread stopped"))?;
        tokio::task::spawn_blocking(move || reply_rx.recv())
            .await?
            .map_err(|_| anyhow!("audio thread stopped"))?
            .map_err(|e| anyhow!(e))
    }

    fn release(&self) {
        let _ = self.tx.send(OutCmd::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perceptual_curve() {
        assert_eq!(perceptual(0.0), 0.0);
        assert!((perceptual(1.0) - 1.0).abs() < 1e-6);
        assert!(perceptual(0.5) < 0.05); // -30 dB
    }

    #[test]
    fn clock_tracks_play_and_pause() {
        let mut c = Clock::default();
        c.pause_at(5_000);
        assert_eq!(c.now_ms(), 5_000);
        c.play_at(1_000);
        std::thread::sleep(Duration::from_millis(20));
        assert!(c.now_ms() >= 1_020);
    }
}
