//! Medley's AI features.
//!
//! Every feature is one structured request: the model gets a short brief and
//! must answer with JSON matching a schema, so the app never parses prose.
//! Two backends:
//!
//! * **Ollama** (default) — runs locally, nothing leaves the machine.
//! * **Claude** — Anthropic's Messages API over plain HTTP (there is no
//!   official Rust SDK), with structured outputs and server-side refusal
//!   fallback.
//!
//! Song ideas come back as title/artist pairs and are resolved to playable
//! tracks with [`resolve_ideas`], which searches the user's connected services.

use anyhow::{Context, Result, anyhow, bail};
use futures::StreamExt;
use medley_core::layout::{ColumnsLayout, LayoutSpec, PanelKind};
use medley_core::matching::best_match;
use medley_core::store::{Accent, AiBackend, CredentialStore, SettingsHandle, ThemeMode};
use medley_core::{Playability, Provider, ProviderKind, Track};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2026-10-21";
/// Server-side refusal fallback, `fallbacks: "default"` form.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SongIdea {
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixIdea {
    pub name: String,
    pub description: String,
    pub songs: Vec<SongIdea>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinerNotes {
    /// One punchy line.
    pub headline: String,
    /// Two or three short paragraphs.
    pub notes: String,
    /// One concrete thing to listen for.
    pub listen_for: String,
    pub mood: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixName {
    pub name: String,
    pub description: String,
}

/// Cheap to clone; all clones share the HTTP pool.
#[derive(Clone)]
pub struct Ai {
    http: reqwest::Client,
    settings: SettingsHandle,
    creds: CredentialStore,
}

struct Brief {
    system: &'static str,
    user: String,
    schema: Value,
}

impl Ai {
    pub fn new(settings: SettingsHandle, creds: CredentialStore) -> Self {
        // Reasoning models can take a while; no total timeout below a couple of minutes.
        let http = reqwest::Client::builder()
            .user_agent(medley_core::USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .pool_max_idle_per_host(1)
            .build()
            .expect("TLS backend available");
        Self { http, settings, creds }
    }

    /// `None` when AI features can run, else why not (shown in the UI).
    pub fn unavailable_reason(&self) -> Option<String> {
        let backend = self.settings.read().expect("settings lock").ai.backend;
        match backend {
            AiBackend::Off => Some("AI features are turned off in Settings.".into()),
            AiBackend::Ollama => None,
            AiBackend::Claude => {
                let has_key = self.creds.get().anthropic_api_key.is_some_and(|k| !k.trim().is_empty())
                    || std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty());
                (!has_key).then(|| "Add an Anthropic API key in Settings to use Claude.".into())
            }
        }
    }

    pub fn backend_label(&self) -> String {
        let s = self.settings.read().expect("settings lock");
        match s.ai.backend {
            AiBackend::Off => "Off".into(),
            AiBackend::Ollama => format!("Ollama · {}", s.ai.ollama_model),
            AiBackend::Claude => format!("Claude · {}", s.ai.claude_model),
        }
    }

    // -----------------------------------------------------------------------
    // Features
    // -----------------------------------------------------------------------

    /// "Rainy Sunday, slow jazz but nothing sleepy" → a named mix of song ideas.
    pub async fn vibe_mix(&self, prompt: &str, count: usize) -> Result<MixIdea> {
        let count = count.clamp(5, 50);
        let mut idea: MixIdea = self
            .ask(Brief {
                system: "You are a music curator with deep, eclectic taste. You turn a listener's description \
                         into a playlist of real, released songs that exist on major streaming services. \
                         Order the songs so the mix flows: think about energy, key and tempo transitions. \
                         Mix well-known songs with deeper cuts, avoid more than two songs by one artist, \
                         and never invent songs. Name the mix like a human would (2-5 words, no emoji) \
                         and describe it in one sentence.",
                user: format!("Make a mix of {count} songs for this vibe:\n\n{}", prompt.trim()),
                schema: json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "songs": song_list_schema(),
                    },
                    "required": ["name", "description", "songs"],
                    "additionalProperties": false
                }),
            })
            .await?;
        idea.songs = dedupe(idea.songs);
        idea.songs.truncate(count);
        if idea.songs.is_empty() {
            bail!("the model didn't suggest any songs — try describing the vibe differently");
        }
        Ok(idea)
    }

    /// Continue a listening session: songs that fit after `recent`.
    pub async fn keep_the_vibe(&self, recent: &[Track], avoid: &[Track], count: usize) -> Result<Vec<SongIdea>> {
        if recent.is_empty() {
            bail!("play a few songs first so there's a vibe to keep");
        }
        let played = recent.iter().rev().take(10).rev().map(describe).collect::<Vec<_>>().join("\n");
        let skip = avoid.iter().take(80).map(describe).collect::<Vec<_>>().join("\n");
        let out: SongsOnly = self
            .ask(Brief {
                system: "You are a DJ keeping a listening session going. Given the songs that just played, \
                         suggest real, released songs that continue the same feel — similar energy, era and \
                         texture, without simply repeating the same artists (at most one song per artist). \
                         Never suggest a song from the 'already queued' list. Never invent songs.",
                user: format!(
                    "Just played, oldest first:\n{played}\n\nAlready queued or played (don't repeat):\n{skip}\n\nSuggest the next {count} songs."
                ),
                schema: json!({
                    "type": "object",
                    "properties": {"songs": song_list_schema()},
                    "required": ["songs"],
                    "additionalProperties": false
                }),
            })
            .await?;
        let mut songs = dedupe(out.songs);
        songs.retain(|s| !avoid.iter().chain(recent).any(|t| same_song(s, t)));
        songs.truncate(count);
        Ok(songs)
    }

    /// Short, honest liner notes for the song that's playing.
    pub async fn liner_notes(&self, track: &Track) -> Result<LinerNotes> {
        self.ask(Brief {
            system: "You write liner notes for a music player, like a knowledgeable friend leaning over to \
                     tell you about the song. Be warm and specific, about 80-120 words in 'notes'. \
                     Accuracy matters more than color: if you are not confident about facts for this exact \
                     recording, write about how it sounds and feels instead, and never invent dates, chart \
                     positions, samples, awards or anecdotes. 'listen_for' is one concrete musical detail \
                     (an instrument entrance, a production trick, a lyric turn). 'mood' is 3 short lowercase tags.",
            user: format!("Song: {}", describe(track)),
            schema: json!({
                "type": "object",
                "properties": {
                    "headline": {"type": "string"},
                    "notes": {"type": "string"},
                    "listen_for": {"type": "string"},
                    "mood": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["headline", "notes", "listen_for", "mood"],
                "additionalProperties": false
            }),
        })
        .await
    }

    /// Give a hand-made mix a name and a one-line description.
    pub async fn name_mix(&self, tracks: &[Track]) -> Result<MixName> {
        if tracks.is_empty() {
            bail!("add some songs first");
        }
        let list = tracks.iter().take(60).map(describe).collect::<Vec<_>>().join("\n");
        self.ask(Brief {
            system: "You name playlists. Read the songs, find what ties them together (mood, scene, era, \
                     a moment in the day) and give the playlist an evocative 2-5 word name a person would \
                     actually type — no emoji, no quotes, no generic words like 'Vibes' or 'Mix' — and a \
                     one-sentence description.",
            user: format!("The playlist:\n{list}"),
            schema: json!({
                "type": "object",
                "properties": {"name": {"type": "string"}, "description": {"type": "string"}},
                "required": ["name", "description"],
                "additionalProperties": false
            }),
        })
        .await
    }

    /// Reorder a mix so it flows. Returns a permutation of `0..tracks.len()`.
    pub async fn smooth_flow(&self, tracks: &[Track]) -> Result<Vec<usize>> {
        if tracks.len() < 3 {
            bail!("a mix needs at least three songs to reorder");
        }
        if tracks.len() > 150 {
            bail!("mixes over 150 songs are too long to reorder in one go");
        }
        let list =
            tracks.iter().enumerate().map(|(i, t)| format!("{i}. {}", describe(t))).collect::<Vec<_>>().join("\n");
        let out: Order = self
            .ask(Brief {
                system: "You sequence playlists like a DJ. Reorder the numbered songs so the playlist flows: \
                         a strong opener, gradual energy changes, no jarring tempo or genre jumps, a satisfying \
                         closer. Return every index exactly once.",
                user: format!("Songs:\n{list}"),
                schema: json!({
                    "type": "object",
                    "properties": {"order": {"type": "array", "items": {"type": "integer"}}},
                    "required": ["order"],
                    "additionalProperties": false
                }),
            })
            .await?;
        Ok(repair_permutation(out.order, tracks.len()))
    }

    /// "Big artwork on the left, queue under the track list" → a layout.
    pub async fn layout_from_prompt(&self, prompt: &str) -> Result<LayoutSpec> {
        let panels =
            PanelKind::ALL.iter().map(|p| format!("- {}: {}", p.slug(), p.purpose())).collect::<Vec<_>>().join("\n");
        let accents: Vec<String> = Accent::ALL.iter().map(|a| a.name().to_ascii_lowercase()).collect();
        let panel_slugs: Vec<&str> = PanelKind::ALL.iter().map(|p| p.slug()).collect();
        let out: AiLayout = self
            .ask(Brief {
                system: "You design window layouts for Medley, a desktop music player whose panels can be \
                         docked side by side, stacked, or grouped as tabs. A layout is a list of columns \
                         (left to right); each column is a list of rows (top to bottom); each row holds one \
                         or more panels shown as tabs. Widths and heights are relative weights. Use each panel \
                         at most once, always include 'collection' (the main track list) in a large area, and \
                         put rarely used panels (accounts, appearance, settings) as extra tabs rather than \
                         giving them their own area. Pick an accent color and theme only if the request \
                         implies one, otherwise use 'pink' and 'system'.",
                user: format!("Available panels:\n{panels}\n\nThe user wants:\n{}", prompt.trim()),
                schema: json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "accent": {"type": "string", "enum": accents},
                        "theme": {"type": "string", "enum": ["system", "light", "dark"]},
                        "columns": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "width": {"type": "number"},
                                    "rows": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "height": {"type": "number"},
                                                "panels": {"type": "array", "items": {"type": "string", "enum": panel_slugs}}
                                            },
                                            "required": ["height", "panels"],
                                            "additionalProperties": false
                                        }
                                    }
                                },
                                "required": ["width", "rows"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["name", "description", "accent", "theme", "columns"],
                    "additionalProperties": false
                }),
            })
            .await?;
        let mut spec = ColumnsLayout { name: out.name, description: out.description, columns: out.columns }.into_spec();
        spec.accent = Accent::from_name(&out.accent);
        spec.theme = match out.theme.as_str() {
            "light" => Some(ThemeMode::Light),
            "dark" => Some(ThemeMode::Dark),
            _ => None,
        };
        Ok(spec)
    }

    // -----------------------------------------------------------------------
    // Transport
    // -----------------------------------------------------------------------

    async fn ask<T: DeserializeOwned>(&self, brief: Brief) -> Result<T> {
        let ai = self.settings.read().expect("settings lock").ai.clone();
        let text = match ai.backend {
            AiBackend::Off => bail!("AI features are turned off in Settings"),
            AiBackend::Ollama => self.ollama(&ai.ollama_url, &ai.ollama_model, &brief).await?,
            AiBackend::Claude => self.claude(&ai.claude_model, &ai.claude_effort, &brief).await?,
        };
        parse_json(&text)
    }

    async fn ollama(&self, base: &str, model: &str, brief: &Brief) -> Result<String> {
        let url = format!("{}/api/chat", base.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "stream": false,
            "format": brief.schema,
            "options": {"temperature": 0.8},
            "messages": [
                {"role": "system", "content": brief.system},
                {"role": "user", "content": brief.user}
            ]
        });
        let resp = self.http.post(&url).json(&body).send().await.map_err(|e| {
            if e.is_connect() {
                anyhow!("Ollama isn't reachable at {base} — start it with `ollama serve`, or pick another AI backend in Settings")
            } else {
                anyhow!("Ollama request failed: {e}")
            }
        })?;
        let status = resp.status();
        let value: Value = resp.json().await.context("Ollama sent an unreadable response")?;
        if !status.is_success() {
            let msg = value.get("error").and_then(Value::as_str).unwrap_or("unknown error");
            if status.as_u16() == 404 {
                bail!("Ollama: {msg} — run `ollama pull {model}` or choose an installed model in Settings");
            }
            bail!("Ollama: {msg}");
        }
        value
            .pointer("/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("Ollama returned no message"))
    }

    async fn claude(&self, model: &str, effort: &str, brief: &Brief) -> Result<String> {
        let key = self
            .creds
            .get()
            .anthropic_api_key
            .filter(|k| !k.trim().is_empty())
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .context("add an Anthropic API key in Settings")?;
        let effort = match effort {
            "low" | "medium" | "high" | "xhigh" | "max" => effort,
            _ => "medium",
        };
        let fallback = supports_default_fallback(model);
        let mut body = json!({
            "model": model,
            "max_tokens": 16000,
            "system": brief.system,
            "messages": [{"role": "user", "content": brief.user}],
            "output_config": {
                "effort": effort,
                "format": {"type": "json_schema", "schema": brief.schema}
            }
        });
        let mut req = self
            .http
            .post(ANTHROPIC_URL)
            .header("x-api-key", key.trim())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");
        if fallback {
            body["fallbacks"] = json!("default");
            req = req.header("anthropic-beta", FALLBACK_BETA);
        }

        let resp = req.json(&body).send().await.map_err(|e| anyhow!("couldn't reach Claude: {e}"))?;
        let status = resp.status();
        let value: Value = resp.json().await.context("Claude sent an unreadable response")?;
        if !status.is_success() {
            let msg = value.pointer("/error/message").and_then(Value::as_str).unwrap_or("request failed");
            let hint = match status.as_u16() {
                401 => " — check the API key in Settings",
                429 => " — rate limited, try again in a moment",
                529 => " — Claude is overloaded, try again shortly",
                _ => "",
            };
            bail!("Claude: {msg}{hint}");
        }
        match value.get("stop_reason").and_then(Value::as_str) {
            Some("refusal") => bail!("Claude declined this request — try rephrasing it"),
            Some("max_tokens") => bail!("Claude's answer was cut off — try asking for fewer songs"),
            _ => {}
        }
        let text: String = value
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect()
            })
            .unwrap_or_default();
        if text.trim().is_empty() {
            bail!("Claude returned an empty answer");
        }
        Ok(text)
    }
}

/// Models that accept `fallbacks: "default"` on the Claude API.
fn supports_default_fallback(model: &str) -> bool {
    matches!(model, "claude-fable-5-1" | "claude-opus-5-5" | "claude-opus-5" | "claude-sonnet-5-5")
}

#[derive(Deserialize)]
struct SongsOnly {
    songs: Vec<SongIdea>,
}

#[derive(Deserialize)]
struct Order {
    order: Vec<i64>,
}

#[derive(Deserialize)]
struct AiLayout {
    name: String,
    description: String,
    #[serde(default)]
    accent: String,
    #[serde(default)]
    theme: String,
    columns: Vec<medley_core::layout::Column>,
}

fn song_list_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {"title": {"type": "string"}, "artist": {"type": "string"}},
            "required": ["title", "artist"],
            "additionalProperties": false
        }
    })
}

fn describe(t: &Track) -> String {
    match &t.album {
        Some(album) if !album.is_empty() => format!("\"{}\" by {} (from {album})", t.title, t.artist_line()),
        _ => format!("\"{}\" by {}", t.title, t.artist_line()),
    }
}

fn same_song(idea: &SongIdea, track: &Track) -> bool {
    use medley_core::matching::{normalize_artist, normalize_title};
    normalize_title(&idea.title) == normalize_title(&track.title)
        && track.artists.iter().any(|a| normalize_artist(a) == normalize_artist(&idea.artist))
}

fn dedupe(songs: Vec<SongIdea>) -> Vec<SongIdea> {
    use medley_core::matching::{normalize_artist, normalize_title};
    let mut seen = std::collections::HashSet::new();
    songs
        .into_iter()
        .filter(|s| !s.title.trim().is_empty())
        .filter(|s| seen.insert((normalize_title(&s.title), normalize_artist(&s.artist))))
        .collect()
}

/// Keep valid, unique indices in the model's order, then append any it forgot.
fn repair_permutation(order: Vec<i64>, len: usize) -> Vec<usize> {
    let mut used = vec![false; len];
    let mut out = Vec::with_capacity(len);
    for i in order {
        if let Ok(i) = usize::try_from(i)
            && i < len
            && !used[i]
        {
            used[i] = true;
            out.push(i);
        }
    }
    out.extend((0..len).filter(|i| !used[*i]));
    out
}

/// Parse model output, tolerating code fences or chatter around the JSON
/// (small local models sometimes add them despite the schema).
fn parse_json<T: DeserializeOwned>(text: &str) -> Result<T> {
    if let Ok(v) = serde_json::from_str(text.trim()) {
        return Ok(v);
    }
    let start = text.find('{');
    let end = text.rfind('}');
    if let (Some(s), Some(e)) = (start, end)
        && s < e
        && let Ok(v) = serde_json::from_str(&text[s..=e])
    {
        return Ok(v);
    }
    bail!("the AI answered in an unexpected format — try again or switch models in Settings")
}

// ---------------------------------------------------------------------------
// Resolving ideas to tracks
// ---------------------------------------------------------------------------

/// Find each idea on the user's services, trying `providers` in order and
/// preferring full-length streams over previews. Keeps the input order;
/// `None` marks songs that couldn't be found anywhere.
pub async fn resolve_ideas(
    ideas: &[SongIdea],
    providers: &[Arc<dyn Provider>],
    progress: impl Fn(usize, usize) + Send + Sync,
) -> Vec<Option<Track>> {
    let total = ideas.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    futures::stream::iter(ideas.iter().cloned())
        .map(|idea| {
            let providers = providers.to_vec();
            let done = &done;
            let progress = &progress;
            async move {
                let found = resolve_one(&idea, &providers).await;
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                progress(n, total);
                found
            }
        })
        .buffered(4)
        .collect()
        .await
}

async fn resolve_one(idea: &SongIdea, providers: &[Arc<dyn Provider>]) -> Option<Track> {
    let query = format!("{} {}", idea.title, idea.artist);
    let mut preview: Option<(Track, f32)> = None;
    for provider in providers {
        let results = match provider.search(&query, 8).await {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("resolve on {}: {e}", provider.kind());
                continue;
            }
        };
        if let Some((track, score)) = best_match(&idea.title, &idea.artist, None, &results) {
            match track.playability {
                Playability::Full => return Some(track.clone()),
                Playability::Preview if preview.as_ref().is_none_or(|(_, s)| score > *s) => {
                    preview = Some((track.clone(), score));
                }
                _ => {}
            }
        }
    }
    preview.map(|(t, _)| t)
}

/// Move tracks to `target`: same-provider tracks pass through, others are
/// looked up by ISRC first and fuzzy search second.
pub async fn match_on(target: &Arc<dyn Provider>, tracks: &[Track]) -> Vec<Option<Track>> {
    let kind: ProviderKind = target.kind();
    futures::stream::iter(tracks.iter().cloned())
        .map(|t| {
            let target = target.clone();
            async move {
                if t.provider == kind {
                    return Some(t);
                }
                if let Some(isrc) = t.isrc.as_deref()
                    && let Ok(Some(found)) = target.find_by_isrc(isrc).await
                {
                    return Some(found);
                }
                let query = format!("{} {}", t.title, t.primary_artist());
                let results = target.search(&query, 8).await.ok()?;
                let dur = (t.duration_ms > 0).then_some(t.duration_ms);
                best_match(&t.title, t.primary_artist(), dur, &results).map(|(m, _)| m.clone())
            }
        })
        .buffered(4)
        .collect()
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permutation_repair() {
        assert_eq!(repair_permutation(vec![2, 0, 2, 9, -1], 4), vec![2, 0, 1, 3]);
        assert_eq!(repair_permutation(vec![], 3), vec![0, 1, 2]);
    }

    #[test]
    fn tolerant_json() {
        #[derive(Deserialize)]
        struct X {
            a: u8,
        }
        assert_eq!(parse_json::<X>("{\"a\":1}").unwrap().a, 1);
        assert_eq!(parse_json::<X>("Sure!\n```json\n{\"a\": 2}\n```").unwrap().a, 2);
        assert!(parse_json::<X>("no json here").is_err());
    }

    #[test]
    fn dedupes_songs() {
        let s = |t: &str, a: &str| SongIdea { title: t.into(), artist: a.into() };
        let out = dedupe(vec![
            s("Holocene", "Bon Iver"),
            s("Holocene (Live)", "Bon Iver"),
            s("holocene", "bon iver"),
            s("", "x"),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn fallback_models() {
        assert!(supports_default_fallback("claude-opus-5-5"));
        assert!(!supports_default_fallback("claude-haiku-5-5"));
    }
}
