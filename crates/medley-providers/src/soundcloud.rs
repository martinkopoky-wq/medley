//! SoundCloud public API (OAuth 2.1 + PKCE, confidential client).
//!
//! Notes from the current docs: tokens last ~1 h, refresh tokens are
//! single-use (so every refresh must persist the new one), requests carry
//! `Authorization: OAuth <token>`, and streams come from `/tracks/{urn}/streams`.
//! Custom players must credit the uploader and link back to SoundCloud — the
//! UI shows "on SoundCloud" with the permalink for every SoundCloud track.

use crate::http;
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use medley_core::oauth::{Pkce, random_string, wait_for_code};
use medley_core::store::{CredentialStore, OAuthTokens, SettingsHandle};
use medley_core::{Account, Playability, PlaylistSummary, Provider, ProviderKind, StreamSource, Track};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Mutex;

const AUTH: &str = "https://secure.soundcloud.com";
const API: &str = "https://api.soundcloud.com";
const CALLBACK_PATH: &str = "/soundcloud/callback";

pub struct SoundCloud {
    http: reqwest::Client,
    settings: SettingsHandle,
    creds: CredentialStore,
    refresh_lock: tokio::sync::Mutex<()>,
    me: Mutex<Option<String>>,
}

impl SoundCloud {
    pub fn new(http: reqwest::Client, settings: SettingsHandle, creds: CredentialStore) -> Self {
        Self { http, settings, creds, refresh_lock: tokio::sync::Mutex::new(()), me: Mutex::new(None) }
    }

    pub fn redirect_uri(port: u16) -> String {
        format!("http://127.0.0.1:{port}{CALLBACK_PATH}")
    }

    fn app(&self) -> Result<(String, String, u16)> {
        let (id, port) = {
            let s = self.settings.read().expect("settings lock");
            (s.soundcloud.client_id.trim().to_owned(), s.soundcloud.redirect_port)
        };
        let secret = self.creds.get().soundcloud_client_secret.unwrap_or_default();
        if id.is_empty() || secret.is_empty() {
            bail!("SoundCloud needs your app's client ID and secret (Accounts panel)");
        }
        Ok((id, secret, port))
    }

    async fn token(&self) -> Result<String> {
        let _guard = self.refresh_lock.lock().await;
        let tokens = self.creds.get().soundcloud.ok_or_else(|| anyhow!("SoundCloud isn't connected"))?;
        if !tokens.is_expired() {
            return Ok(tokens.access_token);
        }
        let (id, secret, _) = self.app()?;
        let refresh = tokens.refresh_token.context("SoundCloud session expired — please reconnect")?;
        let resp = http::send(
            self.http.post(format!("{AUTH}/oauth/token")).header("accept", "application/json; charset=utf-8").form(&[
                ("grant_type", "refresh_token"),
                ("client_id", id.as_str()),
                ("client_secret", secret.as_str()),
                ("refresh_token", refresh.as_str()),
            ]),
        )
        .await?;
        let t: TokenResponse = http::json(resp, "SoundCloud").await?;
        // Refresh tokens are single-use: persist the new one before anything else.
        let fresh = OAuthTokens::from_response(t.access_token, t.refresh_token, t.expires_in, t.scope, Some(id));
        self.creds.update(|c| c.soundcloud = Some(fresh.clone()))?;
        Ok(fresh.access_token)
    }

    async fn request(&self, method: Method, url: &str, body: Option<&Value>) -> Result<reqwest::Response> {
        let url = if url.starts_with("https://") { url.to_owned() } else { format!("{API}{url}") };
        let token = self.token().await?;
        let mut req = self
            .http
            .request(method, &url)
            .header("Authorization", format!("OAuth {token}"))
            .header("accept", "application/json; charset=utf-8");
        if let Some(body) = body {
            req = req.json(body);
        }
        http::send(req).await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        http::json(self.request(Method::GET, url, None).await?, "SoundCloud").await
    }

    async fn collection(&self, first: &str, max: usize) -> Result<Vec<ScTrack>> {
        let mut out = Vec::new();
        let mut next = Some(first.to_owned());
        while let Some(url) = next.take() {
            let page: Collection<ScTrack> = self.get(&url).await?;
            out.extend(page.collection);
            if out.len() < max {
                next = page.next_href;
            }
        }
        out.truncate(max);
        Ok(out)
    }

    async fn me(&self) -> Result<Account> {
        let me: ScUser = self.get("/me").await?;
        let id = me.key();
        *self.me.lock().expect("me lock") = Some(id.clone());
        Ok(Account {
            provider: ProviderKind::SoundCloud,
            display_name: me.full_name.filter(|n| !n.trim().is_empty()).unwrap_or(me.username),
            avatar: me.avatar_url,
            user_id: id,
        })
    }
}

#[async_trait]
impl Provider for SoundCloud {
    fn kind(&self) -> ProviderKind {
        ProviderKind::SoundCloud
    }

    fn setup_hint(&self) -> Option<String> {
        self.app().err().map(|e| e.to_string())
    }

    async fn restore(&self) -> Result<Option<Account>> {
        if self.creds.get().soundcloud.is_none() {
            return Ok(None);
        }
        self.me().await.map(Some)
    }

    async fn connect(&self) -> Result<Account> {
        let (id, secret, port) = self.app()?;
        let redirect = Self::redirect_uri(port);
        let pkce = Pkce::new();
        let state = random_string(24);
        let mut url = url::Url::parse(&format!("{AUTH}/authorize"))?;
        url.query_pairs_mut()
            .append_pair("client_id", &id)
            .append_pair("redirect_uri", &redirect)
            .append_pair("response_type", "code")
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);

        let waiter = tokio::spawn(async move { wait_for_code(port, CALLBACK_PATH, &state, "SoundCloud").await });
        tokio::task::yield_now().await;
        open::that_detached(url.as_str()).context("couldn't open the browser")?;
        let code = waiter.await??;

        let resp = http::send(
            self.http.post(format!("{AUTH}/oauth/token")).header("accept", "application/json; charset=utf-8").form(&[
                ("grant_type", "authorization_code"),
                ("client_id", id.as_str()),
                ("client_secret", secret.as_str()),
                ("redirect_uri", redirect.as_str()),
                ("code_verifier", pkce.verifier.as_str()),
                ("code", code.as_str()),
            ]),
        )
        .await?;
        let t: TokenResponse = http::json(resp, "SoundCloud").await?;
        let tokens = OAuthTokens::from_response(t.access_token, t.refresh_token, t.expires_in, t.scope, Some(id));
        self.creds.update(|c| c.soundcloud = Some(tokens))?;
        self.me().await
    }

    async fn disconnect(&self) -> Result<()> {
        // Revoke on SoundCloud's side too (their terms require offering this).
        if self.creds.get().soundcloud.is_some()
            && let Ok(resp) = self.request(Method::POST, "/disconnect", None).await
        {
            tracing::info!("SoundCloud disconnect: {}", resp.status());
        }
        *self.me.lock().expect("me lock") = None;
        self.creds.update(|c| c.soundcloud = None)
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let mut url = url::Url::parse(&format!("{API}/tracks"))?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("limit", &limit.clamp(1, 50).to_string())
            .append_pair("access", "playable,preview")
            .append_pair("linked_partitioning", "true");
        let page: Collection<ScTrack> = self.get(url.as_str()).await?;
        Ok(page.collection.into_iter().filter_map(map_track).collect())
    }

    async fn liked_tracks(&self, limit: usize) -> Result<Vec<Track>> {
        let tracks = self
            .collection("/me/likes/tracks?limit=50&linked_partitioning=true&access=playable,preview,blocked", limit)
            .await?;
        Ok(tracks.into_iter().filter_map(map_track).collect())
    }

    async fn playlists(&self) -> Result<Vec<PlaylistSummary>> {
        let me = self.me.lock().expect("me lock").clone();
        let mut out = Vec::new();
        let mut next = Some("/me/playlists?show_tracks=false&limit=50&linked_partitioning=true".to_owned());
        while let Some(url) = next.take() {
            let page: Collection<ScPlaylist> = self.get(&url).await?;
            next = page.next_href.filter(|_| out.len() < 500);
            out.extend(page.collection.into_iter().map(|p| {
                let owner = p.user.as_ref().map(ScUser::key);
                PlaylistSummary {
                    provider: ProviderKind::SoundCloud,
                    id: p.urn.clone().unwrap_or_else(|| format!("soundcloud:playlists:{}", p.id.unwrap_or_default())),
                    name: p.title,
                    description: p.description.filter(|d| !d.trim().is_empty()),
                    owner: p.user.map(|u| u.username),
                    track_count: p.track_count,
                    artwork: p.artwork_url.map(|u| sized(&u, "large")),
                    editable: owner.is_some() && owner == me,
                }
            }));
        }
        Ok(out)
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let tracks = self
            .collection(
                &format!(
                    "/playlists/{playlist_id}/tracks?limit=200&linked_partitioning=true&access=playable,preview,blocked"
                ),
                2000,
            )
            .await?;
        Ok(tracks.into_iter().filter_map(map_track).collect())
    }

    async fn create_playlist(&self, name: &str, description: &str, tracks: &[Track]) -> Result<PlaylistSummary> {
        let ids: Vec<Value> = tracks
            .iter()
            .filter(|t| t.provider == ProviderKind::SoundCloud)
            .filter_map(|t| t.id.rsplit(':').next()?.parse::<u64>().ok())
            .map(|id| json!({ "id": id }))
            .collect();
        let count = ids.len();
        let body =
            json!({ "playlist": { "title": name, "description": description, "sharing": "private", "tracks": ids } });
        let created: ScPlaylist =
            http::json(self.request(Method::POST, "/playlists", Some(&body)).await?, "SoundCloud").await?;
        Ok(PlaylistSummary {
            provider: ProviderKind::SoundCloud,
            id: created.urn.unwrap_or_else(|| format!("soundcloud:playlists:{}", created.id.unwrap_or_default())),
            name: created.title,
            description: Some(description.to_owned()),
            owner: None,
            track_count: Some(count as u32),
            artwork: None,
            editable: true,
        })
    }

    async fn stream(&self, track: &Track) -> Result<StreamSource> {
        let streams: Streams = self.get(&format!("/tracks/{}/streams", track.id)).await?;
        let token = self.token().await?;
        let headers = vec![("Authorization".to_owned(), format!("OAuth {token}"))];
        let pick = if track.playability == Playability::Preview {
            streams.preview_mp3_128_url.map(|u| (u, false, "mp3"))
        } else {
            streams
                .http_mp3_128_url
                .map(|u| (u, false, "mp3"))
                .or(streams.hls_mp3_128_url.map(|u| (u, true, "mp3")))
                .or(streams.hls_aac_160_url.map(|u| (u, true, "mp4")))
                .or(streams.preview_mp3_128_url.map(|u| (u, false, "mp3")))
        };
        match pick {
            Some((url, false, hint)) => Ok(StreamSource::Http { url, headers, hint: hint.into() }),
            Some((url, true, hint)) => Ok(StreamSource::Hls { url, headers, hint: hint.into() }),
            None => bail!("SoundCloud doesn't allow off-platform playback of this track"),
        }
    }
}

/// SoundCloud artwork URLs encode the size: `…-large.jpg` → `…-t500x500.jpg`.
fn sized(url: &str, size: &str) -> String {
    url.replace("-large.", &format!("-{size}."))
}

fn map_track(t: ScTrack) -> Option<Track> {
    if t.kind.as_deref().is_some_and(|k| k != "track") {
        return None;
    }
    let id = t.urn.clone().or_else(|| t.id.map(|i| format!("soundcloud:tracks:{i}")))?;
    let publisher = t.publisher_metadata.unwrap_or_default();
    let uploader = t.user.as_ref().map(|u| u.username.clone());
    let artist = t
        .metadata_artist
        .filter(|a| !a.trim().is_empty())
        .or(publisher.artist.filter(|a| !a.trim().is_empty()))
        .or(uploader.clone())
        .unwrap_or_default();
    let artwork = t.artwork_url.or_else(|| t.user.and_then(|u| u.avatar_url));
    Some(Track {
        provider: ProviderKind::SoundCloud,
        id,
        title: t.title.unwrap_or_else(|| "Untitled".into()),
        artists: vec![artist],
        album: publisher.album_title,
        duration_ms: t.duration.unwrap_or(0),
        artwork: artwork.as_deref().map(|u| sized(u, "large")),
        artwork_large: artwork.as_deref().map(|u| sized(u, "t500x500")),
        isrc: publisher.isrc,
        permalink: t.permalink_url,
        preview_url: None,
        playability: match t.access.as_deref() {
            Some("preview") => Playability::Preview,
            Some("blocked") => Playability::Unavailable,
            _ => Playability::Full,
        },
    })
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Deserialize)]
struct ScUser {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    urn: Option<String>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    full_name: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
}

impl ScUser {
    fn key(&self) -> String {
        self.urn.clone().unwrap_or_else(|| format!("soundcloud:users:{}", self.id.unwrap_or_default()))
    }
}

#[derive(Deserialize, Default)]
struct PublisherMeta {
    #[serde(default)]
    isrc: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    album_title: Option<String>,
}

#[derive(Deserialize)]
struct ScTrack {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    urn: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    user: Option<ScUser>,
    #[serde(default)]
    metadata_artist: Option<String>,
    #[serde(default)]
    duration: Option<u64>,
    #[serde(default)]
    artwork_url: Option<String>,
    #[serde(default)]
    permalink_url: Option<String>,
    #[serde(default)]
    access: Option<String>,
    #[serde(default)]
    publisher_metadata: Option<PublisherMeta>,
}

#[derive(Deserialize)]
struct ScPlaylist {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    urn: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    user: Option<ScUser>,
    #[serde(default)]
    track_count: Option<u32>,
    #[serde(default)]
    artwork_url: Option<String>,
}

#[derive(Deserialize)]
struct Collection<T> {
    #[serde(default = "Vec::new")]
    collection: Vec<T>,
    #[serde(default)]
    next_href: Option<String>,
}

#[derive(Deserialize)]
struct Streams {
    #[serde(default)]
    http_mp3_128_url: Option<String>,
    #[serde(default)]
    hls_mp3_128_url: Option<String>,
    #[serde(default)]
    hls_aac_160_url: Option<String>,
    #[serde(default)]
    preview_mp3_128_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_tracks_and_artwork_sizes() {
        let raw = r#"{"kind":"track","id":42,"urn":"soundcloud:tracks:42","title":"Night Drive",
            "user":{"username":"uploader","urn":"soundcloud:users:7"},"metadata_artist":"Real Artist",
            "duration":201000,"artwork_url":"https://i1.sndcdn.com/artworks-abc-large.jpg",
            "permalink_url":"https://soundcloud.com/u/night-drive","access":"preview",
            "publisher_metadata":{"isrc":"GBXYZ"}}"#;
        let t = map_track(serde_json::from_str(raw).unwrap()).unwrap();
        assert_eq!(t.id, "soundcloud:tracks:42");
        assert_eq!(t.artists, vec!["Real Artist"]);
        assert_eq!(t.playability, Playability::Preview);
        assert_eq!(t.artwork_large.as_deref(), Some("https://i1.sndcdn.com/artworks-abc-t500x500.jpg"));
        assert_eq!(t.isrc.as_deref(), Some("GBXYZ"));
    }

    #[test]
    fn falls_back_to_uploader() {
        let raw = r#"{"id":1,"title":"x","user":{"username":"dj"},"metadata_artist":""}"#;
        let t = map_track(serde_json::from_str(raw).unwrap()).unwrap();
        assert_eq!(t.artists, vec!["dj"]);
        assert_eq!(t.id, "soundcloud:tracks:1");
    }
}
