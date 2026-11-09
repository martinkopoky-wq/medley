//! Application state and every action the UI can trigger.
//!
//! The UI thread owns [`AppState`]. Anything slow (network, AI, audio setup)
//! runs on the Tokio runtime and reports back as a [`Msg`]; [`AppState::handle`]
//! applies it and the next frame shows the result. Panels never block.

use crate::dock::{self, Dock};
use crate::player::{Player, Status};
use crate::theme;
use eframe::egui::{self, Context};
use medley_ai::{Ai, LinerNotes, MixIdea, MixName};
use medley_audio::{AudioEngine, AudioEvent};
use medley_core::layout::{LayoutSpec, PanelKind, builtin_layouts};
use medley_core::store::{CredentialStore, MixLibrary, Paths, Settings, SettingsHandle, load_json, save_json};
use medley_core::{Account, Mix, PlaylistSummary, Provider, ProviderKind, Track};
use medley_providers::{AppleMusic, SoundCloud, Spotify};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// How many liked songs to pull per service.
const LIKED_LIMIT: usize = 2000;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(350);
const SAVE_DEBOUNCE: Duration = Duration::from_secs(1);
/// ~60 fps ceiling.
const MIN_FRAME_TIME: Duration = Duration::from_micros(16_667);

// ---------------------------------------------------------------------------
// Small state types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub enum Loadable<T> {
    #[default]
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

impl<T> Loadable<T> {
    pub fn ready(&self) -> Option<&T> {
        match self {
            Self::Ready(v) => Some(v),
            _ => None,
        }
    }
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }
}

/// What the main content panel ("Browse") shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum View {
    #[default]
    Home,
    Liked(ProviderKind),
    Playlist {
        provider: ProviderKind,
        id: String,
        name: String,
    },
    Mix(String),
}

#[derive(Debug, Clone)]
pub enum AccountState {
    /// Restoring a saved session at startup.
    Restoring,
    Disconnected,
    Connecting,
    Connected(Account),
    Failed(String),
}

impl AccountState {
    pub fn account(&self) -> Option<&Account> {
        match self {
            Self::Connected(a) => Some(a),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpotifyPlayback {
    Unknown,
    Connecting,
    Ready,
    Failed(String),
}

#[derive(Default)]
pub struct SearchState {
    pub query: String,
    /// The query the current results belong to.
    pub submitted: String,
    pub results: BTreeMap<ProviderKind, Loadable<Vec<Track>>>,
    /// `None` = all services.
    pub filter: Option<ProviderKind>,
    edited_at: Option<Instant>,
}

pub struct NowPlaying {
    pub track: Track,
    /// Audio-engine generation once playback actually started.
    pub generation: Option<u64>,
    pub notes: Loadable<LinerNotes>,
}

#[derive(Debug, Clone)]
pub struct VibeResult {
    pub idea: MixIdea,
    pub prompt: String,
    /// Same order as `idea.songs`; `None` = not found on any connected service.
    pub tracks: Vec<Option<Track>>,
}

pub struct AiStudio {
    pub vibe_prompt: String,
    pub vibe_count: usize,
    pub vibe: Loadable<VibeResult>,
    /// (resolved, total) while matching songs to services.
    pub vibe_progress: (usize, usize),
    pub layout_prompt: String,
    pub layout: Loadable<LayoutSpec>,
}

impl Default for AiStudio {
    fn default() -> Self {
        Self {
            vibe_prompt: String::new(),
            vibe_count: 20,
            vibe: Loadable::Idle,
            vibe_progress: (0, 0),
            layout_prompt: String::new(),
            layout: Loadable::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub struct Toast {
    pub id: u64,
    pub text: String,
    pub kind: ToastKind,
    pub born: Instant,
}

impl Toast {
    pub fn lifetime(&self) -> Duration {
        match self.kind {
            ToastKind::Error => Duration::from_secs(7),
            _ => Duration::from_secs(4),
        }
    }
}

/// Changes to the dock, applied by the app after the dock area has drawn
/// (panels can't touch the dock while it's being shown).
#[derive(Debug, Clone)]
pub enum DockOp {
    Apply(LayoutSpec),
    Focus(PanelKind),
    /// Save the current arrangement under a name into the user's layouts.
    SaveCurrent(String),
    /// Copy the current arrangement as shareable JSON.
    CopyCurrent,
}

/// Where "Add to Mix" should put tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixTarget {
    Existing(String),
    /// Create a new mix (named after the first track's context).
    New,
}

/// Something the user did on a track row. Panels get these from
/// [`crate::panels::tracks::track_list`] and usually pass them to
/// [`AppState::row_action`]; they can intercept any of them first.
#[derive(Debug, Clone, PartialEq)]
pub enum RowAction {
    /// Play the list starting at this row.
    Play(usize),
    PlayNext(usize),
    AddToQueue(usize),
    AddToMix(usize, MixTarget),
    /// Remove from the list being shown (only offered when the list allows it).
    Remove(usize),
    /// Reorder within the list being shown (drag and drop).
    Move {
        from: usize,
        to: usize,
    },
    OpenInService(usize),
}

// ---------------------------------------------------------------------------
// Messages from background tasks
// ---------------------------------------------------------------------------

pub enum Msg {
    /// Session restore (`Ok(None)` = not logged in) or interactive connect.
    Account(ProviderKind, Result<Option<Account>, String>),
    Playlists(ProviderKind, Result<Vec<PlaylistSummary>, String>),
    Tracks(View, Result<Vec<Track>, String>),
    Search {
        query: String,
        provider: ProviderKind,
        result: Result<Vec<Track>, String>,
    },
    PlayStarted {
        request: u64,
        result: Result<u64, String>,
    },
    Audio(AudioEvent),
    LinerNotes {
        key: String,
        result: Result<LinerNotes, String>,
    },
    VibeProgress(usize, usize),
    Vibe(Result<VibeResult, String>),
    KeepVibe(Result<Vec<Track>, String>),
    MixNamed {
        mix: String,
        result: Result<MixName, String>,
    },
    MixFlow {
        mix: String,
        result: Result<Vec<usize>, String>,
    },
    AiLayout(Result<LayoutSpec, String>),
    Exported {
        busy_key: String,
        provider: ProviderKind,
        result: Result<String, String>,
    },
    Toast(ToastKind, String),
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Providers {
    pub spotify: Arc<Spotify>,
    pub soundcloud: Arc<SoundCloud>,
    pub apple: Arc<AppleMusic>,
}

impl Providers {
    pub fn get(&self, kind: ProviderKind) -> Arc<dyn Provider> {
        match kind {
            ProviderKind::Spotify => self.spotify.clone(),
            ProviderKind::SoundCloud => self.soundcloud.clone(),
            ProviderKind::AppleMusic => self.apple.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub ctx: Context,
    rt: tokio::runtime::Handle,
    tx: UnboundedSender<Msg>,
    rx: UnboundedReceiver<Msg>,

    pub paths: Paths,
    /// Shared with providers and AI. Edit through [`AppState::edit_settings`].
    pub settings: SettingsHandle,
    pub creds: CredentialStore,
    pub providers: Providers,
    pub audio: AudioEngine,
    pub ai: Ai,

    pub accounts: BTreeMap<ProviderKind, AccountState>,
    pub spotify_playback: SpotifyPlayback,
    pub playlists: BTreeMap<ProviderKind, Loadable<Vec<PlaylistSummary>>>,
    pub collections: HashMap<View, Loadable<Vec<Track>>>,
    pub view: View,
    /// Back stack for the Browse panel.
    pub view_history: Vec<View>,

    pub mixes: Vec<Mix>,
    pub search: SearchState,

    pub player: Player,
    pub status: Status,
    pub now: Option<NowPlaying>,
    /// Automatically extend the queue with AI picks when it runs low.
    pub keep_vibe: bool,
    play_request: u64,
    latest_request: Arc<AtomicU64>,
    failures_in_a_row: u32,
    last_keep_vibe: Option<Instant>,

    pub studio: AiStudio,
    /// Built-in presets first, then the user's saved layouts.
    pub layouts: Vec<LayoutSpec>,
    pub dock_ops: Vec<DockOp>,

    pub toasts: Vec<Toast>,
    next_toast: u64,
    busy: HashSet<String>,

    settings_dirty: Option<Instant>,
    mixes_dirty: Option<Instant>,
    /// Bumped whenever appearance settings change, so the app re-themes.
    pub appearance_rev: u64,
    /// Offline demo (`MEDLEY_DEMO`): no network, nothing written to disk.
    pub demo: bool,
}

impl AppState {
    pub fn new(ctx: Context, rt: tokio::runtime::Handle, paths: Paths) -> Self {
        let settings: Settings = load_json(&paths.settings_file());
        let volume = settings.volume;
        let settings: SettingsHandle = Arc::new(RwLock::new(settings));
        let creds = CredentialStore::load(paths.credentials_file());
        let http = medley_providers::http::client();
        let providers = Providers {
            spotify: Arc::new(Spotify::new(http.clone(), settings.clone(), creds.clone())),
            soundcloud: Arc::new(SoundCloud::new(http.clone(), settings.clone(), creds.clone())),
            apple: Arc::new(AppleMusic::new(http, settings.clone(), creds.clone())),
        };
        let (tx, rx) = unbounded_channel();

        // The audio engine must be created inside the runtime.
        let (audio_tx, mut audio_rx) = unbounded_channel::<AudioEvent>();
        let audio = {
            let _guard = rt.enter();
            AudioEngine::new(audio_tx, paths.cache.clone(), volume)
        };
        {
            let tx = tx.clone();
            let ctx = ctx.clone();
            rt.spawn(async move {
                while let Some(ev) = audio_rx.recv().await {
                    if tx.send(Msg::Audio(ev)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }

        let mixes = load_json::<MixLibrary>(&paths.mixes_file()).mixes;
        let mut layouts = builtin_layouts();
        layouts.extend(load_user_layouts(&paths));

        let ai = Ai::new(settings.clone(), creds.clone());
        let mut state = Self {
            ctx,
            rt,
            tx,
            rx,
            paths,
            settings,
            creds,
            providers,
            audio,
            ai,
            accounts: ProviderKind::ALL.iter().map(|k| (*k, AccountState::Restoring)).collect(),
            spotify_playback: SpotifyPlayback::Unknown,
            playlists: BTreeMap::new(),
            collections: HashMap::new(),
            view: View::Home,
            view_history: Vec::new(),
            mixes,
            search: SearchState::default(),
            player: Player::default(),
            status: Status::Idle,
            now: None,
            keep_vibe: false,
            play_request: 0,
            latest_request: Arc::new(AtomicU64::new(0)),
            failures_in_a_row: 0,
            last_keep_vibe: None,
            studio: AiStudio::default(),
            layouts,
            dock_ops: Vec::new(),
            toasts: Vec::new(),
            next_toast: 0,
            busy: HashSet::new(),
            settings_dirty: None,
            mixes_dirty: None,
            appearance_rev: 0,
            demo: crate::demo::enabled(),
        };
        for kind in ProviderKind::ALL {
            if state.demo {
                break;
            }
            state.restore(kind);
        }
        state
    }

    // -- plumbing ------------------------------------------------------------

    /// Run `fut` on the runtime and deliver its message to the UI.
    pub fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = Msg> + Send + 'static,
    {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.rt.spawn(async move {
            let msg = fut.await;
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// Fire-and-forget work whose result the UI doesn't need.
    pub fn spawn_quiet<F>(&self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.rt.spawn(fut);
    }

    pub fn settings(&self) -> std::sync::RwLockReadGuard<'_, Settings> {
        self.settings.read().unwrap_or_else(|p| p.into_inner())
    }

    /// Mutate settings; they're saved shortly after. Appearance changes re-theme.
    pub fn edit_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        let mut guard = self.settings.write().unwrap_or_else(|p| p.into_inner());
        let before = guard.appearance.clone();
        f(&mut guard);
        if guard.appearance != before {
            self.appearance_rev += 1;
        }
        drop(guard);
        self.settings_dirty = Some(Instant::now());
    }

    pub fn is_busy(&self, key: &str) -> bool {
        self.busy.contains(key)
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let text = text.into();
        if kind == ToastKind::Error {
            tracing::warn!("{text}");
        }
        // Collapse duplicates that arrive together.
        if let Some(t) = self.toasts.iter_mut().find(|t| t.text == text) {
            t.born = Instant::now();
            return;
        }
        self.next_toast += 1;
        self.toasts.push(Toast { id: self.next_toast, text, kind, born: Instant::now() });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn open_url(&self, url: &str) {
        if let Err(e) = open::that_detached(url) {
            tracing::warn!("opening {url}: {e}");
        }
    }

    pub fn focus_panel(&mut self, panel: PanelKind) {
        self.dock_ops.push(DockOp::Focus(panel));
    }

    /// Called once per frame before drawing.
    pub fn tick(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.handle(msg);
        }
        let now = Instant::now();
        if let Some(at) = self.search.edited_at
            && now.duration_since(at) >= SEARCH_DEBOUNCE
        {
            self.search.edited_at = None;
            self.run_search();
        } else if let Some(at) = self.search.edited_at {
            self.ctx.request_repaint_after(SEARCH_DEBOUNCE.saturating_sub(now.duration_since(at)));
        }
        if self.settings_dirty.is_some_and(|t| now.duration_since(t) >= SAVE_DEBOUNCE) {
            self.save_settings();
        } else if self.settings_dirty.is_some() {
            self.ctx.request_repaint_after(SAVE_DEBOUNCE);
        }
        if self.mixes_dirty.is_some_and(|t| now.duration_since(t) >= SAVE_DEBOUNCE) {
            self.save_mixes();
        } else if self.mixes_dirty.is_some() {
            self.ctx.request_repaint_after(SAVE_DEBOUNCE);
        }
        self.toasts.retain(|t| now.duration_since(t.born) < t.lifetime());
        if let Some(t) = self.toasts.iter().map(|t| t.lifetime().saturating_sub(now.duration_since(t.born))).min() {
            self.ctx.request_repaint_after(t);
        }
        self.maybe_keep_vibe();
    }

    pub fn save_all(&mut self) {
        if self.settings_dirty.is_some() {
            self.save_settings();
        }
        if self.mixes_dirty.is_some() {
            self.save_mixes();
        }
    }

    fn save_settings(&mut self) {
        self.settings_dirty = None;
        if self.demo {
            return;
        }
        let snapshot = self.settings().clone();
        if let Err(e) = save_json(&self.paths.settings_file(), &snapshot, false) {
            self.toast(ToastKind::Error, format!("Couldn't save settings: {e}"));
        }
    }

    fn save_mixes(&mut self) {
        self.mixes_dirty = None;
        if self.demo {
            return;
        }
        let lib = MixLibrary { mixes: self.mixes.clone() };
        if let Err(e) = save_json(&self.paths.mixes_file(), &lib, false) {
            self.toast(ToastKind::Error, format!("Couldn't save mixes: {e}"));
        }
    }

    // -- accounts ------------------------------------------------------------

    pub fn connected(&self) -> Vec<ProviderKind> {
        self.accounts.iter().filter(|(_, s)| s.account().is_some()).map(|(k, _)| *k).collect()
    }

    pub fn is_connected(&self, kind: ProviderKind) -> bool {
        self.accounts.get(&kind).is_some_and(|s| s.account().is_some())
    }

    /// Connected providers in the user's preferred order.
    pub fn connected_in_order(&self) -> Vec<Arc<dyn Provider>> {
        let order = self.settings().provider_order.clone();
        let mut kinds: Vec<ProviderKind> = order.into_iter().filter(|k| self.is_connected(*k)).collect();
        for k in self.connected() {
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        kinds.into_iter().map(|k| self.providers.get(k)).collect()
    }

    fn restore(&mut self, kind: ProviderKind) {
        self.accounts.insert(kind, AccountState::Restoring);
        let provider = self.providers.get(kind);
        self.spawn(async move { Msg::Account(kind, provider.restore().await.map_err(|e| e.to_string())) });
    }

    /// Interactive login in the browser.
    pub fn connect(&mut self, kind: ProviderKind) {
        let provider = self.providers.get(kind);
        if let Some(hint) = provider.setup_hint() {
            self.toast(ToastKind::Error, hint);
            self.focus_panel(PanelKind::Accounts);
            return;
        }
        self.accounts.insert(kind, AccountState::Connecting);
        self.spawn(async move { Msg::Account(kind, provider.connect().await.map(Some).map_err(|e| e.to_string())) });
    }

    pub fn disconnect(&mut self, kind: ProviderKind) {
        let provider = self.providers.get(kind);
        let audio = self.audio.clone();
        self.spawn_quiet(async move {
            if let Err(e) = provider.disconnect().await {
                tracing::warn!("disconnect {kind}: {e}");
            }
            if kind == ProviderKind::Spotify {
                audio.logout_spotify().await;
            }
        });
        if kind == ProviderKind::Spotify {
            self.spotify_playback = SpotifyPlayback::Unknown;
        }
        self.accounts.insert(kind, AccountState::Disconnected);
        self.playlists.remove(&kind);
        self.collections.retain(|v, _| match v {
            View::Liked(k) | View::Playlist { provider: k, .. } => *k != kind,
            _ => true,
        });
        self.search.results.remove(&kind);
        if matches!(&self.view, View::Liked(k) | View::Playlist { provider: k, .. } if *k == kind) {
            self.view = View::Home;
        }
        self.toast(ToastKind::Info, format!("Disconnected {kind}"));
    }

    pub fn refresh_library(&mut self, kind: ProviderKind) {
        if !self.is_connected(kind) {
            return;
        }
        self.playlists.insert(kind, Loadable::Loading);
        let provider = self.providers.get(kind);
        self.spawn(async move { Msg::Playlists(kind, provider.playlists().await.map_err(|e| e.to_string())) });
    }

    // -- browsing ------------------------------------------------------------

    /// Show `view` in the Browse panel, loading it if needed.
    pub fn open_view(&mut self, view: View) {
        if self.view != view {
            let prev = std::mem::replace(&mut self.view, view.clone());
            self.view_history.push(prev);
            if self.view_history.len() > 50 {
                self.view_history.remove(0);
            }
        }
        self.focus_panel(PanelKind::Collection);
        if matches!(view, View::Liked(_) | View::Playlist { .. })
            && !matches!(self.collections.get(&view), Some(Loadable::Ready(_) | Loadable::Loading))
        {
            self.load_view(view);
        }
    }

    pub fn go_back(&mut self) {
        if let Some(prev) = self.view_history.pop() {
            self.view = prev;
        }
    }

    /// (Re)load a provider-backed view.
    pub fn load_view(&mut self, view: View) {
        let (kind, playlist) = match &view {
            View::Liked(k) => (*k, None),
            View::Playlist { provider, id, .. } => (*provider, Some(id.clone())),
            _ => return,
        };
        self.collections.insert(view.clone(), Loadable::Loading);
        let provider = self.providers.get(kind);
        self.spawn(async move {
            let result = match playlist {
                Some(id) => provider.playlist_tracks(&id).await,
                None => provider.liked_tracks(LIKED_LIMIT).await,
            };
            Msg::Tracks(view, result.map_err(|e| e.to_string()))
        });
    }

    /// Tracks of a view, if loaded (mixes are always loaded).
    pub fn view_tracks(&self, view: &View) -> Option<&[Track]> {
        match view {
            View::Mix(id) => self.mixes.iter().find(|m| &m.id == id).map(|m| m.tracks.as_slice()),
            View::Home => None,
            _ => self.collections.get(view).and_then(|l| l.ready()).map(Vec::as_slice),
        }
    }

    pub fn view_title(&self, view: &View) -> String {
        match view {
            View::Home => "Home".into(),
            View::Liked(k) => format!("Liked on {k}"),
            View::Playlist { name, .. } => name.clone(),
            View::Mix(id) => self.mix(id).map(|m| m.name.clone()).unwrap_or_else(|| "Mix".into()),
        }
    }

    // -- search --------------------------------------------------------------

    /// Call when the search text changed; searching starts after a short pause.
    pub fn search_edited(&mut self) {
        self.search.edited_at = Some(Instant::now());
        self.ctx.request_repaint_after(SEARCH_DEBOUNCE);
    }

    /// Search immediately (Enter key).
    pub fn search_now(&mut self) {
        self.search.edited_at = None;
        self.run_search();
    }

    fn run_search(&mut self) {
        let query = self.search.query.trim().to_owned();
        if query.is_empty() {
            self.search.submitted.clear();
            self.search.results.clear();
            return;
        }
        if query == self.search.submitted && !self.search.results.is_empty() {
            return;
        }
        self.search.submitted = query.clone();
        self.search.results.clear();
        for kind in self.connected() {
            self.search.results.insert(kind, Loadable::Loading);
            let provider = self.providers.get(kind);
            let query = query.clone();
            self.spawn(async move {
                let result = provider.search(&query, 20).await.map_err(|e| e.to_string());
                Msg::Search { query, provider: kind, result }
            });
        }
    }

    // -- playback ------------------------------------------------------------

    /// Replace the queue with `tracks`, starting at `start`.
    pub fn play_tracks(&mut self, tracks: Vec<Track>, start: usize, context: impl Into<String>) {
        if self.player.load(tracks, start, context).is_some() {
            self.failures_in_a_row = 0;
            self.start_current();
        }
    }

    pub fn play_next(&mut self, track: Track) {
        let title = track.title.clone();
        self.player.play_next(track);
        self.toast(ToastKind::Info, format!("“{title}” plays next"));
        if self.player.current().is_none() {
            self.player.index = Some(0);
            self.start_current();
        }
    }

    pub fn add_to_queue(&mut self, tracks: Vec<Track>) {
        let n = tracks.len();
        let was_empty = self.player.current().is_none();
        self.player.enqueue(tracks);
        if n == 1 {
            self.toast(ToastKind::Info, "Added to Up Next");
        } else if n > 1 {
            self.toast(ToastKind::Info, format!("Added {n} songs to Up Next"));
        }
        if was_empty && !self.player.queue.is_empty() {
            self.player.index = Some(0);
            self.start_current();
        }
    }

    /// Jump to the n-th song in Up Next.
    pub fn play_upcoming(&mut self, n: usize) {
        if self.player.jump_upcoming(n).is_some() {
            self.start_current();
        }
    }

    pub fn toggle_play(&mut self) {
        match self.status {
            Status::Playing => {
                self.status = Status::Paused;
                let audio = self.audio.clone();
                self.spawn_quiet(async move { audio.pause().await });
            }
            Status::Paused => {
                self.status = Status::Playing;
                let audio = self.audio.clone();
                self.spawn_quiet(async move { audio.resume().await });
            }
            Status::Idle => {
                if self.player.current().is_some() {
                    self.start_current();
                }
            }
            Status::Loading => {}
        }
    }

    pub fn next(&mut self) {
        if self.player.advance(true).is_some() {
            self.start_current();
        }
    }

    pub fn previous(&mut self) {
        if self.position_ms() > 3000 {
            self.seek(0);
        } else if self.player.back().is_some() {
            self.start_current();
        }
    }

    pub fn seek(&mut self, ms: u64) {
        let audio = self.audio.clone();
        self.spawn(async move {
            match audio.seek(ms).await {
                Ok(()) => Msg::Toast(ToastKind::Info, String::new()),
                Err(e) => Msg::Toast(ToastKind::Error, e.to_string()),
            }
        });
    }

    pub fn set_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.edit_settings(|s| s.volume = volume);
        let audio = self.audio.clone();
        self.spawn_quiet(async move { audio.set_volume(volume).await });
    }

    pub fn volume(&self) -> f32 {
        self.settings().volume
    }

    pub fn position_ms(&self) -> u64 {
        if self.now.as_ref().is_some_and(|n| n.generation.is_some()) { self.audio.position_ms() } else { 0 }
    }

    pub fn duration_ms(&self) -> u64 {
        let engine = self.audio.duration_ms();
        if engine > 0 { engine } else { self.now.as_ref().map_or(0, |n| n.track.duration_ms) }
    }

    pub fn is_playing(&self) -> bool {
        self.status == Status::Playing
    }

    pub fn is_current(&self, track: &Track) -> bool {
        self.now.as_ref().is_some_and(|n| n.track.provider == track.provider && n.track.id == track.id)
    }

    fn start_current(&mut self) {
        let Some(track) = self.player.current().cloned() else {
            self.stop();
            return;
        };
        self.play_request += 1;
        let request = self.play_request;
        self.latest_request.store(request, Ordering::SeqCst);
        self.status = Status::Loading;
        if track.provider == ProviderKind::Spotify
            && matches!(self.spotify_playback, SpotifyPlayback::Unknown | SpotifyPlayback::Failed(_))
        {
            self.spotify_playback = SpotifyPlayback::Connecting;
        }
        let keep_notes = self.now.as_ref().filter(|n| n.track.key() == track.key()).map(|n| n.notes.clone());
        self.now = Some(NowPlaying { track: track.clone(), generation: None, notes: keep_notes.unwrap_or_default() });

        let provider = self.providers.get(track.provider);
        let spotify = self.providers.spotify.clone();
        let audio = self.audio.clone();
        let latest = self.latest_request.clone();
        let bitrate = self.settings().spotify.bitrate;
        self.spawn(async move {
            let result = async {
                let source = provider.stream(&track).await?;
                if latest.load(Ordering::SeqCst) != request {
                    anyhow::bail!("superseded");
                }
                if source.is_spotify() && !audio.spotify_ready().await {
                    let token = provider.playback_token().await?;
                    audio.connect_spotify(token, spotify.token_client_id(), bitrate).await?;
                }
                if latest.load(Ordering::SeqCst) != request {
                    anyhow::bail!("superseded");
                }
                audio.play(source, track.duration_ms).await
            }
            .await
            .map_err(|e| e.to_string());
            Msg::PlayStarted { request, result }
        });
    }

    pub fn stop(&mut self) {
        self.audio.stop();
        self.status = Status::Idle;
        if let Some(now) = &mut self.now {
            now.generation = None;
        }
    }

    fn on_track_finished(&mut self) {
        if self.player.advance(false).is_some() {
            self.start_current();
        } else {
            self.status = Status::Idle;
            if let Some(now) = &mut self.now {
                now.generation = None;
            }
        }
    }

    fn on_play_failed(&mut self, message: String) {
        let title = self.now.as_ref().map(|n| n.track.title.clone()).unwrap_or_default();
        self.toast(ToastKind::Error, format!("Couldn't play “{title}”: {message}"));
        self.failures_in_a_row += 1;
        if self.failures_in_a_row < 3 && self.player.advance(true).is_some() {
            self.start_current();
        } else {
            self.status = Status::Idle;
        }
    }

    // -- mixes ---------------------------------------------------------------

    pub fn mix(&self, id: &str) -> Option<&Mix> {
        self.mixes.iter().find(|m| m.id == id)
    }

    /// Edit a mix in place; it's saved shortly after.
    pub fn edit_mix(&mut self, id: &str, f: impl FnOnce(&mut Mix)) {
        if let Some(m) = self.mixes.iter_mut().find(|m| m.id == id) {
            f(m);
            m.touch();
            self.mixes_dirty = Some(Instant::now());
        }
    }

    /// Create a mix and return its id.
    pub fn create_mix(&mut self, name: impl Into<String>, tracks: Vec<Track>) -> String {
        let mut mix = Mix::new(name);
        mix.tracks = tracks;
        let id = mix.id.clone();
        self.mixes.insert(0, mix);
        self.mixes_dirty = Some(Instant::now());
        id
    }

    pub fn add_to_mix(&mut self, target: MixTarget, tracks: Vec<Track>) {
        let n = tracks.len();
        let id = match target {
            MixTarget::Existing(id) => id,
            MixTarget::New => {
                let name = format!("New Mix {}", self.mixes.len() + 1);
                let id = self.create_mix(name, Vec::new());
                self.open_view(View::Mix(id.clone()));
                id
            }
        };
        let mut added = 0;
        self.edit_mix(&id, |m| {
            for t in tracks {
                if !m.tracks.iter().any(|x| x.key() == t.key()) {
                    m.tracks.push(t);
                    added += 1;
                }
            }
        });
        let name = self.mix(&id).map(|m| m.name.clone()).unwrap_or_default();
        match (added, n) {
            (0, _) => self.toast(ToastKind::Info, format!("Already in “{name}”")),
            (1, _) => self.toast(ToastKind::Success, format!("Added to “{name}”")),
            (a, _) => self.toast(ToastKind::Success, format!("Added {a} songs to “{name}”")),
        }
    }

    pub fn delete_mix(&mut self, id: &str) {
        self.mixes.retain(|m| m.id != id);
        if self.view == View::Mix(id.to_owned()) {
            self.view = View::Home;
        }
        self.view_history.retain(|v| v != &View::Mix(id.to_owned()));
        self.mixes_dirty = Some(Instant::now());
    }

    /// Create a real playlist on `provider` from a mix, matching songs from other services.
    pub fn export_mix(&mut self, mix_id: &str, provider: ProviderKind) {
        let Some(mix) = self.mix(mix_id).cloned() else { return };
        if !self.is_connected(provider) {
            self.toast(ToastKind::Error, format!("Connect {provider} first"));
            self.focus_panel(PanelKind::Accounts);
            return;
        }
        let key = format!("export:{mix_id}:{}", provider.slug());
        if !self.busy.insert(key.clone()) {
            return;
        }
        let target = self.providers.get(provider);
        self.spawn(async move {
            let result = async {
                let matched = medley_ai::match_on(&target, &mix.tracks).await;
                let found: Vec<Track> = matched.into_iter().flatten().collect();
                if found.is_empty() {
                    anyhow::bail!("none of the songs could be found on {provider}");
                }
                let mut desc = mix.description.clone();
                if desc.is_empty() {
                    desc = "Made with Medley".into();
                }
                let pl = target.create_playlist(&mix.name, &desc, &found).await?;
                let missing = mix.tracks.len() - found.len();
                Ok(if missing == 0 {
                    format!("Created “{}” on {provider} with all {} songs", pl.name, found.len())
                } else {
                    format!("Created “{}” on {provider} — {} of {} songs found", pl.name, found.len(), mix.tracks.len())
                })
            }
            .await
            .map_err(|e: anyhow::Error| e.to_string());
            Msg::Exported { busy_key: key, provider, result }
        });
    }

    /// Apply a row action on `tracks` (the list the row came from).
    pub fn row_action(&mut self, tracks: &[Track], action: RowAction, context: &str) {
        let get = |i: usize| tracks.get(i).cloned();
        match action {
            RowAction::Play(i) => self.play_tracks(tracks.to_vec(), i, context),
            RowAction::PlayNext(i) => {
                if let Some(t) = get(i) {
                    self.play_next(t);
                }
            }
            RowAction::AddToQueue(i) => {
                if let Some(t) = get(i) {
                    self.add_to_queue(vec![t]);
                }
            }
            RowAction::AddToMix(i, target) => {
                if let Some(t) = get(i) {
                    self.add_to_mix(target, vec![t]);
                }
            }
            RowAction::OpenInService(i) => {
                if let Some(url) = get(i).and_then(|t| t.permalink) {
                    self.open_url(&url);
                }
            }
            // Only meaningful for lists the caller owns (mixes, queue).
            RowAction::Remove(_) | RowAction::Move { .. } => {}
        }
    }

    // -- AI ------------------------------------------------------------------

    pub fn ai_unavailable(&self) -> Option<String> {
        self.ai.unavailable_reason()
    }

    /// Turn `studio.vibe_prompt` into a mix preview.
    pub fn ai_vibe_mix(&mut self) {
        let prompt = self.studio.vibe_prompt.trim().to_owned();
        if prompt.is_empty() || !self.busy.insert("vibe".into()) {
            return;
        }
        let providers = self.connected_in_order();
        if providers.is_empty() {
            self.busy.remove("vibe");
            self.toast(ToastKind::Error, "Connect at least one music service so Medley can find the songs");
            self.focus_panel(PanelKind::Accounts);
            return;
        }
        self.studio.vibe = Loadable::Loading;
        self.studio.vibe_progress = (0, 0);
        let count = self.studio.vibe_count;
        let ai = self.ai.clone();
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.spawn(async move {
            let result = async {
                let idea = ai.vibe_mix(&prompt, count).await?;
                let total = idea.songs.len();
                let _ = tx.send(Msg::VibeProgress(0, total));
                let tracks = medley_ai::resolve_ideas(&idea.songs, &providers, |done, total| {
                    let _ = tx.send(Msg::VibeProgress(done, total));
                    ctx.request_repaint();
                })
                .await;
                Ok::<_, anyhow::Error>(VibeResult { idea, prompt, tracks })
            }
            .await
            .map_err(|e| e.to_string());
            Msg::Vibe(result)
        });
    }

    /// Save the current vibe preview as a mix and open it.
    pub fn save_vibe_mix(&mut self) -> Option<String> {
        let result = self.studio.vibe.ready()?.clone();
        let tracks: Vec<Track> = result.tracks.iter().flatten().cloned().collect();
        let id = self.create_mix(result.idea.name.clone(), tracks);
        self.edit_mix(&id, |m| {
            m.description = result.idea.description.clone();
            m.ai_prompt = Some(result.prompt.clone());
        });
        self.open_view(View::Mix(id.clone()));
        self.toast(ToastKind::Success, format!("Saved “{}”", result.idea.name));
        Some(id)
    }

    /// Ask for liner notes for the current song.
    pub fn ai_liner_notes(&mut self) {
        let Some(now) = &mut self.now else { return };
        if now.notes.is_loading() {
            return;
        }
        now.notes = Loadable::Loading;
        let track = now.track.clone();
        let ai = self.ai.clone();
        self.spawn(async move {
            let result = ai.liner_notes(&track).await.map_err(|e| e.to_string());
            Msg::LinerNotes { key: track.key(), result }
        });
    }

    /// Append AI picks that continue the current vibe to Up Next.
    pub fn ai_keep_vibe_now(&mut self) {
        if !self.busy.insert("keep_vibe".into()) {
            return;
        }
        self.last_keep_vibe = Some(Instant::now());
        let mut recent: Vec<Track> = self.player.history().iter().rev().take(9).rev().cloned().collect();
        recent.extend(self.player.current().cloned());
        let avoid = self.player.queue.clone();
        let providers = self.connected_in_order();
        let ai = self.ai.clone();
        self.spawn(async move {
            let result = async {
                let ideas = ai.keep_the_vibe(&recent, &avoid, 8).await?;
                let tracks = medley_ai::resolve_ideas(&ideas, &providers, |_, _| {}).await;
                Ok::<_, anyhow::Error>(tracks.into_iter().flatten().collect::<Vec<_>>())
            }
            .await
            .map_err(|e| e.to_string());
            Msg::KeepVibe(result)
        });
    }

    fn maybe_keep_vibe(&mut self) {
        if !self.keep_vibe
            || self.is_busy("keep_vibe")
            || self.player.current().is_none()
            || self.player.upcoming().len() >= 3
            || self.last_keep_vibe.is_some_and(|t| t.elapsed() < Duration::from_secs(45))
            || self.ai_unavailable().is_some()
        {
            return;
        }
        self.ai_keep_vibe_now();
    }

    pub fn ai_name_mix(&mut self, mix_id: &str) {
        let Some(mix) = self.mix(mix_id) else { return };
        let tracks = mix.tracks.clone();
        let key = format!("name:{mix_id}");
        if !self.busy.insert(key) {
            return;
        }
        let ai = self.ai.clone();
        let mix = mix_id.to_owned();
        self.spawn(async move { Msg::MixNamed { mix, result: ai.name_mix(&tracks).await.map_err(|e| e.to_string()) } });
    }

    pub fn ai_smooth_flow(&mut self, mix_id: &str) {
        let Some(mix) = self.mix(mix_id) else { return };
        let tracks = mix.tracks.clone();
        let key = format!("flow:{mix_id}");
        if !self.busy.insert(key) {
            return;
        }
        let ai = self.ai.clone();
        let mix = mix_id.to_owned();
        self.spawn(
            async move { Msg::MixFlow { mix, result: ai.smooth_flow(&tracks).await.map_err(|e| e.to_string()) } },
        );
    }

    /// Turn `studio.layout_prompt` into a layout preview.
    pub fn ai_layout(&mut self) {
        let prompt = self.studio.layout_prompt.trim().to_owned();
        if prompt.is_empty() || !self.busy.insert("layout".into()) {
            return;
        }
        self.studio.layout = Loadable::Loading;
        let ai = self.ai.clone();
        self.spawn(async move { Msg::AiLayout(ai.layout_from_prompt(&prompt).await.map_err(|e| e.to_string())) });
    }

    // -- layouts -------------------------------------------------------------

    /// Apply a layout (and its accent/theme, if it carries them).
    pub fn apply_layout(&mut self, spec: LayoutSpec) {
        if spec.accent.is_some() || spec.theme.is_some() {
            self.edit_settings(|s| {
                if let Some(a) = spec.accent {
                    s.appearance.accent = a;
                }
                if let Some(t) = spec.theme {
                    s.appearance.theme = t;
                }
            });
        }
        self.dock_ops.push(DockOp::Apply(spec));
    }

    /// Parse pasted layout JSON.
    pub fn import_layout(&mut self, json: &str) -> Result<LayoutSpec, String> {
        LayoutSpec::from_json(json.trim()).map_err(|e| format!("That isn't a Medley layout: {e}"))
    }

    /// Save a layout spec into the user's layouts folder.
    pub fn save_layout(&mut self, mut spec: LayoutSpec) {
        spec = spec.sanitized();
        let file = self.paths.layouts_dir().join(format!("{}.json", slugify(&spec.name)));
        match medley_core::store::write_atomic(&file, spec.to_json().as_bytes(), false) {
            Ok(()) => {
                self.layouts.retain(|l| l.name != spec.name || is_builtin(&l.name));
                let name = spec.name.clone();
                self.layouts.push(spec);
                self.toast(ToastKind::Success, format!("Saved layout “{name}”"));
            }
            Err(e) => self.toast(ToastKind::Error, format!("Couldn't save layout: {e}")),
        }
    }

    pub fn delete_layout(&mut self, name: &str) {
        if is_builtin(name) {
            return;
        }
        let file = self.paths.layouts_dir().join(format!("{}.json", slugify(name)));
        let _ = std::fs::remove_file(file);
        self.layouts.retain(|l| l.name != name || is_builtin(&l.name));
    }

    // -- message handling ----------------------------------------------------

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Account(kind, result) => match result {
                Ok(Some(account)) => {
                    let was_connecting = matches!(self.accounts.get(&kind), Some(AccountState::Connecting));
                    if was_connecting {
                        self.toast(ToastKind::Success, format!("Connected to {kind} as {}", account.display_name));
                    }
                    self.accounts.insert(kind, AccountState::Connected(account));
                    self.refresh_library(kind);
                    if !self.search.submitted.is_empty() {
                        self.search.submitted.clear();
                        self.run_search();
                    }
                }
                Ok(None) => {
                    self.accounts.insert(kind, AccountState::Disconnected);
                }
                Err(e) => {
                    let restoring = matches!(self.accounts.get(&kind), Some(AccountState::Restoring));
                    if !restoring {
                        self.toast(ToastKind::Error, format!("{kind}: {e}"));
                    }
                    self.accounts.insert(kind, AccountState::Failed(e));
                }
            },
            Msg::Playlists(kind, result) => {
                if !self.is_connected(kind) {
                    return;
                }
                match result {
                    Ok(list) => {
                        self.playlists.insert(kind, Loadable::Ready(list));
                    }
                    Err(e) => {
                        self.playlists.insert(kind, Loadable::Failed(e));
                    }
                }
            }
            Msg::Tracks(view, result) => {
                let value = match result {
                    Ok(t) => Loadable::Ready(t),
                    Err(e) => Loadable::Failed(e),
                };
                self.collections.insert(view, value);
            }
            Msg::Search { query, provider, result } => {
                if query != self.search.submitted {
                    return;
                }
                let value = match result {
                    Ok(t) => Loadable::Ready(t),
                    Err(e) => Loadable::Failed(e),
                };
                self.search.results.insert(provider, value);
            }
            Msg::PlayStarted { request, result } => {
                if request != self.play_request {
                    return;
                }
                match result {
                    Ok(generation) => {
                        self.failures_in_a_row = 0;
                        self.status = Status::Playing;
                        if let Some(now) = &mut self.now {
                            now.generation = Some(generation);
                        }
                    }
                    Err(e) if e == "superseded" => {}
                    Err(e) => {
                        if self.spotify_playback == SpotifyPlayback::Connecting {
                            self.spotify_playback = SpotifyPlayback::Failed(e.clone());
                        }
                        self.on_play_failed(e)
                    }
                }
            }
            Msg::Audio(ev) => self.handle_audio(ev),
            Msg::LinerNotes { key, result } => {
                if let Some(now) = &mut self.now
                    && now.track.key() == key
                {
                    now.notes = match result {
                        Ok(n) => Loadable::Ready(n),
                        Err(e) => Loadable::Failed(e),
                    };
                }
            }
            Msg::VibeProgress(done, total) => {
                if self.studio.vibe.is_loading() {
                    self.studio.vibe_progress = (done, total);
                }
            }
            Msg::Vibe(result) => {
                self.busy.remove("vibe");
                self.studio.vibe = match result {
                    Ok(r) => Loadable::Ready(r),
                    Err(e) => Loadable::Failed(e),
                };
            }
            Msg::KeepVibe(result) => {
                self.busy.remove("keep_vibe");
                match result {
                    Ok(tracks) if tracks.is_empty() => {
                        self.toast(ToastKind::Info, "Couldn't find more songs for this vibe right now")
                    }
                    Ok(tracks) => {
                        let n = tracks.len();
                        self.player.enqueue(tracks);
                        self.toast(ToastKind::Success, format!("Keep the Vibe added {n} songs"));
                    }
                    Err(e) => {
                        self.toast(ToastKind::Error, format!("Keep the Vibe: {e}"));
                    }
                }
            }
            Msg::MixNamed { mix, result } => {
                self.busy.remove(&format!("name:{mix}"));
                match result {
                    Ok(name) => {
                        let title = name.name.clone();
                        self.edit_mix(&mix, |m| {
                            m.name = name.name;
                            m.description = name.description;
                        });
                        self.toast(ToastKind::Success, format!("Named “{title}”"));
                    }
                    Err(e) => self.toast(ToastKind::Error, format!("Naming failed: {e}")),
                }
            }
            Msg::MixFlow { mix, result } => {
                self.busy.remove(&format!("flow:{mix}"));
                match result {
                    Ok(order) => {
                        self.edit_mix(&mix, |m| {
                            if order.len() == m.tracks.len() {
                                let old = std::mem::take(&mut m.tracks);
                                m.tracks = order.iter().map(|&i| old[i].clone()).collect();
                            }
                        });
                        self.toast(ToastKind::Success, "Reordered for a smoother flow");
                    }
                    Err(e) => self.toast(ToastKind::Error, format!("Smooth the Flow: {e}")),
                }
            }
            Msg::AiLayout(result) => {
                self.busy.remove("layout");
                self.studio.layout = match result {
                    Ok(l) => Loadable::Ready(l),
                    Err(e) => Loadable::Failed(e),
                };
            }
            Msg::Exported { busy_key, provider, result } => {
                self.busy.remove(&busy_key);
                match result {
                    Ok(text) => {
                        self.toast(ToastKind::Success, text);
                        self.refresh_library(provider);
                    }
                    Err(e) => self.toast(ToastKind::Error, format!("Export to {provider} failed: {e}")),
                }
            }
            Msg::Toast(kind, text) => {
                if !text.is_empty() {
                    self.toast(kind, text);
                }
            }
        }
    }

    fn handle_audio(&mut self, ev: AudioEvent) {
        let current = self.now.as_ref().and_then(|n| n.generation);
        match ev {
            AudioEvent::Ended { generation } if current == Some(generation) => self.on_track_finished(),
            AudioEvent::Failed { generation, message } if current == Some(generation) => self.on_play_failed(message),
            AudioEvent::Buffering { generation, buffering } if current == Some(generation) => {
                if buffering && self.status == Status::Playing {
                    self.status = Status::Loading;
                } else if !buffering && self.status == Status::Loading {
                    self.status = Status::Playing;
                }
            }
            AudioEvent::SpotifySession { connected, message } => {
                self.spotify_playback = if connected {
                    SpotifyPlayback::Ready
                } else {
                    SpotifyPlayback::Failed(message.unwrap_or_else(|| "Spotify playback unavailable".into()))
                };
            }
            _ => {}
        }
    }
}

fn load_user_layouts(paths: &Paths) -> Vec<LayoutSpec> {
    let Ok(entries) = std::fs::read_dir(paths.layouts_dir()) else { return Vec::new() };
    let mut out: Vec<LayoutSpec> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| LayoutSpec::from_json(&s).ok())
        .filter(|l| !is_builtin(&l.name))
        .collect();
    out.sort_by_cached_key(|l| l.name.to_lowercase());
    out
}

pub fn is_builtin(name: &str) -> bool {
    static NAMES: std::sync::LazyLock<Vec<String>> =
        std::sync::LazyLock::new(|| builtin_layouts().into_iter().map(|l| l.name).collect());
    NAMES.iter().any(|n| n == name)
}

fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() { "layout".into() } else { s }
}

// ---------------------------------------------------------------------------
// The eframe app
// ---------------------------------------------------------------------------

pub struct MedleyApp {
    pub state: AppState,
    pub dock: Dock,
    applied_appearance: u64,
    last_dock_save: Instant,
    last_sweep: Instant,
    last_frame: Instant,
    /// Keeps the runtime alive for the app's lifetime.
    /// `MEDLEY_SCREENSHOT`: where to save, and frames drawn so far.
    snapshot: Option<(std::path::PathBuf, u32)>,
    _runtime: Arc<tokio::runtime::Runtime>,
}

impl MedleyApp {
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: Arc<tokio::runtime::Runtime>, paths: Paths) -> Self {
        let ctx = cc.egui_ctx.clone();
        egui_extras::install_image_loaders(&ctx);
        theme::install_fonts(&ctx);
        let mut state = AppState::new(ctx.clone(), runtime.handle().clone(), paths);
        if state.demo {
            crate::demo::seed(&mut state);
        }
        crate::demo::apply_overrides(&mut state);
        theme::apply(&ctx, &state.settings().appearance.clone());
        let saved = if state.demo { None } else { dock::load(&state.paths.dock_file()) };
        let dock = saved.unwrap_or_else(|| {
            let spec = state.layouts.first().cloned().unwrap_or_else(|| builtin_layouts().remove(0));
            dock::from_spec(&spec)
        });
        let rev = state.appearance_rev;
        Self {
            state,
            dock,
            applied_appearance: rev,
            last_dock_save: Instant::now(),
            last_sweep: Instant::now(),
            last_frame: Instant::now(),
            snapshot: crate::demo::screenshot_path().map(|p| (p, 0)),
            _runtime: runtime,
        }
    }

    fn apply_dock_ops(&mut self) {
        let ops = std::mem::take(&mut self.state.dock_ops);
        for op in ops {
            match op {
                DockOp::Apply(spec) => {
                    self.dock = dock::from_spec(&spec);
                    if !self.state.demo {
                        dock::save(&self.state.paths.dock_file(), &self.dock);
                    }
                }
                DockOp::Focus(panel) => dock::focus(&mut self.dock, panel),
                DockOp::SaveCurrent(name) => {
                    let spec = dock::to_spec(&self.dock, &name);
                    self.state.save_layout(spec);
                }
                DockOp::CopyCurrent => {
                    let mut spec = dock::to_spec(&self.dock, "My Layout");
                    spec.author = self
                        .state
                        .accounts
                        .values()
                        .find_map(|a| a.account().map(|a| a.display_name.clone()))
                        .unwrap_or_default();
                    self.state.ctx.copy_text(spec.to_json());
                    self.state.toast(
                        ToastKind::Success,
                        "Layout copied as JSON — paste it to a friend or add it to community/layouts",
                    );
                }
            }
        }
    }
}

impl eframe::App for MedleyApp {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Frame cap (vsync is off, see main.rs): animations request repaints
        // back to back, which would otherwise spin as fast as the GPU allows.
        let since = self.last_frame.elapsed();
        if since < MIN_FRAME_TIME {
            std::thread::sleep(MIN_FRAME_TIME - since);
        }
        self.last_frame = Instant::now();
        if let Some((path, _)) = &self.snapshot {
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                let [w, h] = image.size;
                match image::save_buffer(path, &rgba, w as u32, h as u32, image::ColorType::Rgba8) {
                    Ok(()) => tracing::info!("screenshot saved to {}", path.display()),
                    Err(e) => tracing::error!("saving screenshot: {e}"),
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.state.tick();
        if self.state.appearance_rev != self.applied_appearance {
            self.applied_appearance = self.state.appearance_rev;
            let appearance = self.state.settings().appearance.clone();
            theme::apply(ctx, &appearance);
        }
        // Keep the clock moving while something plays — slowly when unfocused.
        if matches!(self.state.status, Status::Playing | Status::Loading) {
            let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
            ctx.request_repaint_after(if focused { Duration::from_millis(250) } else { Duration::from_secs(1) });
        }
        if self.last_sweep.elapsed() > Duration::from_secs(30) {
            self.last_sweep = Instant::now();
            crate::widgets::sweep_artwork(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::chrome::toolbar(ui, &mut self.state, &mut self.dock);

        let pal = theme::palette(ui.ctx());
        let locked = self.state.settings().layout_locked;
        egui::CentralPanel::no_frame()
            .frame(egui::Frame::NONE.fill(pal.window).inner_margin(egui::Margin {
                left: 8,
                right: 8,
                top: 0,
                bottom: 8,
            }))
            .show(ui, |ui| {
                let style = theme::dock_style(ui.ctx(), locked);
                let mut viewer = crate::panels::Viewer { state: &mut self.state };
                egui_dock::DockArea::new(&mut self.dock)
                    .id(egui::Id::new("medley.dock"))
                    .style(style)
                    .show_close_buttons(false)
                    .show_add_buttons(false)
                    .draggable_tabs(!locked)
                    .tab_context_menus(!locked)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(ui, &mut viewer);
            });

        crate::chrome::toasts(ui.ctx(), &mut self.state);
        self.apply_dock_ops();

        if let Some((_, frames)) = &mut self.snapshot {
            *frames += 1;
            // Let layouts settle and fonts/animations finish before capturing.
            if *frames == 45 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ui.ctx().request_repaint();
        }

        // Persist the arrangement every few seconds (cheap; only a small JSON).
        if !self.state.demo
            && self.last_dock_save.elapsed() > Duration::from_secs(5)
            && !ui.ctx().input(|i| i.pointer.any_down())
        {
            self.last_dock_save = Instant::now();
            dock::save(&self.state.paths.dock_file(), &self.dock);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.state.save_all();
        if !self.state.demo {
            dock::save(&self.state.paths.dock_file(), &self.dock);
        }
        self.state.audio.stop();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if self.state.settings().appearance.translucent {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            visuals.panel_fill.to_normalized_gamma_f32()
        }
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slugify("DJ Booth!"), "dj-booth");
        assert_eq!(slugify("  "), "layout");
    }
}
