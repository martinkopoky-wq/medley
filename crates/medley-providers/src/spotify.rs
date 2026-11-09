//! Spotify Web API (metadata, library, playlists). Audio goes through librespot
//! in `medley-audio`, using the same OAuth token.
//!
//! Endpoints follow the February 2026 Web API changes: playlist contents live
//! under `/playlists/{id}/items` (field `item`), playlists are created with
//! `POST /me/playlists`, and search pages are capped at 10 results.

use crate::http::{self, unescape_html};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use medley_core::oauth::{Pkce, random_string, wait_for_code};
use medley_core::store::{CredentialStore, OAuthTokens, SettingsHandle};
use medley_core::{Account, Playability, PlaylistSummary, Provider, ProviderKind, StreamSource, Track};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Mutex;

/// Client id librespot (and thus spotifyd, ncspot, …) uses. Its registered
/// redirect is `http://127.0.0.1:8898/login`.
pub const LIBRESPOT_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";

const ACCOUNTS: &str = "https://accounts.spotify.com";
const API: &str = "https://api.spotify.com/v1";
const SEARCH_PAGE: usize = 10;

pub const SCOPES: &[&str] = &[
    "streaming",
    "user-read-private",
    "user-library-read",
    "user-library-modify",
    "playlist-read-private",
    "playlist-read-collaborative",
    "playlist-modify-private",
    "playlist-modify-public",
    "user-top-read",
    "user-read-recently-played",
    "user-read-playback-state",
    "user-modify-playback-state",
];

pub struct Spotify {
    http: reqwest::Client,
    settings: SettingsHandle,
    creds: CredentialStore,
    refresh_lock: tokio::sync::Mutex<()>,
    me: Mutex<Option<String>>,
}

impl Spotify {
    pub fn new(http: reqwest::Client, settings: SettingsHandle, creds: CredentialStore) -> Self {
        Self { http, settings, creds, refresh_lock: tokio::sync::Mutex::new(()), me: Mutex::new(None) }
    }

    fn client_id(&self) -> String {
        let s = self.settings.read().expect("settings lock");
        let id = s.spotify.client_id.trim();
        if id.is_empty() { LIBRESPOT_CLIENT_ID.to_owned() } else { id.to_owned() }
    }

    fn redirect(&self) -> (String, u16) {
        let port = self.settings.read().expect("settings lock").spotify.redirect_port;
        (format!("http://127.0.0.1:{port}/login"), port)
    }

    /// Client id the stored token belongs to (librespot's session must match it).
    pub fn token_client_id(&self) -> String {
        self.creds.get().spotify.and_then(|t| t.client_id).unwrap_or_else(|| self.client_id())
    }

    async fn token(&self) -> Result<String> {
        let _guard = self.refresh_lock.lock().await;
        let tokens = self.creds.get().spotify.ok_or_else(|| anyhow!("Spotify isn't connected"))?;
        if !tokens.is_expired() {
            return Ok(tokens.access_token);
        }
        let refresh = tokens.refresh_token.clone().context("Spotify session expired — please reconnect")?;
        let client_id = tokens.client_id.clone().unwrap_or_else(|| self.client_id());
        let resp = http::send(self.http.post(format!("{ACCOUNTS}/api/token")).form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", client_id.as_str()),
        ]))
        .await?;
        let t: TokenResponse = http::json(resp, "Spotify").await?;
        let fresh = OAuthTokens::from_response(
            t.access_token,
            t.refresh_token.or(Some(refresh)),
            t.expires_in,
            t.scope,
            Some(client_id),
        );
        self.creds.update(|c| c.spotify = Some(fresh.clone()))?;
        Ok(fresh.access_token)
    }

    async fn request(&self, method: Method, url: &str, body: Option<&Value>) -> Result<reqwest::Response> {
        let url = if url.starts_with("https://") { url.to_owned() } else { format!("{API}{url}") };
        for attempt in 0..2 {
            let token = self.token().await?;
            let mut req = self.http.request(method.clone(), &url).bearer_auth(&token);
            if let Some(body) = body {
                req = req.json(body);
            }
            let resp = http::send(req).await?;
            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && attempt == 0 {
                // Token revoked or expired early: force one refresh and retry.
                self.creds.update(|c| {
                    if let Some(t) = c.spotify.as_mut() {
                        t.expires_at = 0;
                    }
                })?;
                continue;
            }
            return Ok(resp);
        }
        unreachable!("loop returns on the second attempt")
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        http::json(self.request(Method::GET, url, None).await?, "Spotify").await
    }

    async fn me(&self) -> Result<Account> {
        let me: Me = self.get("/me").await?;
        *self.me.lock().expect("me lock") = Some(me.id.clone());
        Ok(Account {
            provider: ProviderKind::Spotify,
            display_name: me.display_name.filter(|n| !n.is_empty()).unwrap_or_else(|| me.id.clone()),
            avatar: me.images.unwrap_or_default().into_iter().next().map(|i| i.url),
            user_id: me.id,
        })
    }

    async fn paged<T: serde::de::DeserializeOwned>(&self, first: &str, max: usize) -> Result<Vec<T>> {
        let mut out = Vec::new();
        let mut next = Some(first.to_owned());
        while let Some(url) = next.take() {
            let page: Paging<T> = self.get(&url).await?;
            out.extend(page.items);
            if out.len() < max {
                next = page.next;
            }
        }
        out.truncate(max);
        Ok(out)
    }
}

#[async_trait]
impl Provider for Spotify {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Spotify
    }

    fn setup_hint(&self) -> Option<String> {
        None // works out of the box with librespot's client id
    }

    async fn restore(&self) -> Result<Option<Account>> {
        if self.creds.get().spotify.is_none() {
            return Ok(None);
        }
        self.me().await.map(Some)
    }

    async fn connect(&self) -> Result<Account> {
        let client_id = self.client_id();
        let (redirect, port) = self.redirect();
        let pkce = Pkce::new();
        let state = random_string(24);
        let mut url = url::Url::parse(&format!("{ACCOUNTS}/authorize"))?;
        url.query_pairs_mut()
            .append_pair("client_id", &client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &redirect)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("state", &state)
            .append_pair("scope", &SCOPES.join(" "));

        let waiter = tokio::spawn(async move { wait_for_code(port, "/login", &state, "Spotify").await });
        tokio::task::yield_now().await;
        open::that_detached(url.as_str()).context("couldn't open the browser")?;
        let code = waiter.await??;

        let resp = http::send(self.http.post(format!("{ACCOUNTS}/api/token")).form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("client_id", client_id.as_str()),
            ("code_verifier", pkce.verifier.as_str()),
        ]))
        .await?;
        let t: TokenResponse = http::json(resp, "Spotify").await?;
        let tokens =
            OAuthTokens::from_response(t.access_token, t.refresh_token, t.expires_in, t.scope, Some(client_id));
        self.creds.update(|c| c.spotify = Some(tokens))?;
        self.me().await
    }

    async fn disconnect(&self) -> Result<()> {
        *self.me.lock().expect("me lock") = None;
        self.creds.update(|c| c.spotify = None)
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let mut out = Vec::new();
        let mut offset = 0;
        while out.len() < limit {
            let page = SEARCH_PAGE.min(limit - out.len());
            let mut url = url::Url::parse(&format!("{API}/search"))?;
            url.query_pairs_mut()
                .append_pair("q", query)
                .append_pair("type", "track")
                .append_pair("limit", &page.to_string())
                .append_pair("offset", &offset.to_string());
            let resp: SearchResponse = self.get(url.as_str()).await?;
            let Some(tracks) = resp.tracks else { break };
            let got = tracks.items.len();
            out.extend(tracks.items.into_iter().filter_map(map_track));
            if got < page || tracks.next.is_none() {
                break;
            }
            offset += got;
        }
        Ok(out)
    }

    async fn find_by_isrc(&self, isrc: &str) -> Result<Option<Track>> {
        let found = self.search(&format!("isrc:{isrc}"), 1).await?;
        Ok(found.into_iter().next())
    }

    async fn liked_tracks(&self, limit: usize) -> Result<Vec<Track>> {
        let items: Vec<SavedTrack> = self.paged("/me/tracks?limit=50", limit).await?;
        Ok(items.into_iter().filter_map(|i| i.track).filter_map(map_track).collect())
    }

    async fn playlists(&self) -> Result<Vec<PlaylistSummary>> {
        let me = self.me.lock().expect("me lock").clone();
        let lists: Vec<SpPlaylist> = self.paged("/me/playlists?limit=50", 500).await?;
        Ok(lists
            .into_iter()
            .map(|p| {
                let owner_id = p.owner.as_ref().map(|o| o.id.clone());
                let images = p.images.unwrap_or_default();
                PlaylistSummary {
                    provider: ProviderKind::Spotify,
                    editable: p.collaborative || (owner_id.is_some() && owner_id == me),
                    owner: p.owner.and_then(|o| o.display_name),
                    track_count: p.items.or(p.tracks).and_then(|t| t.total),
                    artwork: pick_image(&images, 64).or_else(|| images.first().map(|i| i.url.clone())),
                    description: p.description.filter(|d| !d.is_empty()).map(|d| unescape_html(&d)),
                    name: p.name,
                    id: p.id,
                }
            })
            .collect())
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let items: Vec<PlaylistItem> = self
            .paged(&format!("/playlists/{playlist_id}/items?limit=50"), 2000)
            .await
            .map_err(|e| anyhow!("{e}. Spotify only shares the contents of playlists you own or collaborate on."))?;
        Ok(items
            .into_iter()
            .filter_map(|i| i.item.or(i.track))
            .filter_map(|v| serde_json::from_value::<SpTrack>(v).ok())
            .filter_map(map_track)
            .collect())
    }

    async fn create_playlist(&self, name: &str, description: &str, tracks: &[Track]) -> Result<PlaylistSummary> {
        let created: SpPlaylist = http::json(
            self.request(
                Method::POST,
                "/me/playlists",
                Some(&json!({ "name": name, "description": description, "public": false })),
            )
            .await?,
            "Spotify",
        )
        .await?;
        let uris: Vec<String> = tracks
            .iter()
            .filter(|t| t.provider == ProviderKind::Spotify)
            .map(|t| format!("spotify:track:{}", t.id))
            .collect();
        for chunk in uris.chunks(100) {
            http::ok(
                self.request(
                    Method::POST,
                    &format!("/playlists/{}/items", created.id),
                    Some(&json!({ "uris": chunk })),
                )
                .await?,
                "Spotify",
            )
            .await?;
        }
        Ok(PlaylistSummary {
            provider: ProviderKind::Spotify,
            id: created.id,
            name: created.name,
            description: Some(description.to_owned()),
            owner: None,
            track_count: Some(uris.len() as u32),
            artwork: None,
            editable: true,
        })
    }

    async fn stream(&self, track: &Track) -> Result<StreamSource> {
        if track.provider != ProviderKind::Spotify {
            bail!("not a Spotify track");
        }
        Ok(StreamSource::Spotify { uri: format!("spotify:track:{}", track.id) })
    }

    async fn playback_token(&self) -> Result<Option<String>> {
        self.token().await.map(Some)
    }
}

fn pick_image(images: &[SpImage], min_width: u32) -> Option<String> {
    let mut sorted: Vec<&SpImage> = images.iter().collect();
    sorted.sort_by_key(|i| i.width.unwrap_or(0));
    sorted.iter().find(|i| i.width.unwrap_or(0) >= min_width).or(sorted.last()).map(|i| i.url.clone())
}

fn map_track(t: SpTrack) -> Option<Track> {
    if t.is_local || t.kind.as_deref().is_some_and(|k| k != "track") {
        return None;
    }
    let id = t.id?;
    let images = t.album.as_ref().map(|a| a.images.as_slice()).unwrap_or_default();
    Some(Track {
        provider: ProviderKind::Spotify,
        title: t.name,
        artists: t.artists.into_iter().map(|a| a.name).collect(),
        album: t.album.as_ref().map(|a| a.name.clone()),
        duration_ms: t.duration_ms,
        artwork: pick_image(images, 60),
        artwork_large: pick_image(images, 600),
        isrc: t.external_ids.and_then(|e| e.isrc),
        permalink: t
            .external_urls
            .and_then(|u| u.spotify)
            .or_else(|| Some(format!("https://open.spotify.com/track/{id}"))),
        preview_url: None,
        playability: if t.is_playable == Some(false) { Playability::Unavailable } else { Playability::Full },
        id,
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
struct Me {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    images: Option<Vec<SpImage>>,
}

#[derive(Deserialize)]
struct SpImage {
    url: String,
    #[serde(default)]
    width: Option<u32>,
}

#[derive(Deserialize)]
struct SpArtist {
    name: String,
}

#[derive(Deserialize)]
struct SpAlbum {
    name: String,
    #[serde(default)]
    images: Vec<SpImage>,
}

#[derive(Deserialize)]
struct ExternalIds {
    #[serde(default)]
    isrc: Option<String>,
}

#[derive(Deserialize)]
struct ExternalUrls {
    #[serde(default)]
    spotify: Option<String>,
}

#[derive(Deserialize)]
struct SpTrack {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    artists: Vec<SpArtist>,
    #[serde(default)]
    album: Option<SpAlbum>,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    external_ids: Option<ExternalIds>,
    #[serde(default)]
    external_urls: Option<ExternalUrls>,
    #[serde(default)]
    is_local: bool,
    #[serde(default)]
    is_playable: Option<bool>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Paging<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    tracks: Option<Paging<SpTrack>>,
}

#[derive(Deserialize)]
struct SavedTrack {
    #[serde(default)]
    track: Option<SpTrack>,
}

/// Playlist rows: `item` since Feb 2026, `track` before. Parsed lazily so a
/// podcast episode or malformed row never fails the whole page.
#[derive(Deserialize)]
struct PlaylistItem {
    #[serde(default)]
    item: Option<Value>,
    #[serde(default)]
    track: Option<Value>,
}

#[derive(Deserialize)]
struct SpOwner {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct Total {
    #[serde(default)]
    total: Option<u32>,
}

#[derive(Deserialize)]
struct SpPlaylist {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    owner: Option<SpOwner>,
    #[serde(default)]
    images: Option<Vec<SpImage>>,
    #[serde(default)]
    collaborative: bool,
    #[serde(default)]
    items: Option<Total>,
    #[serde(default)]
    tracks: Option<Total>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_feb_2026_playlist_items() {
        let page = r#"{"items":[
            {"item":{"id":"abc","name":"Song","type":"track","artists":[{"name":"A"}],
              "album":{"name":"Al","images":[{"url":"big","width":640},{"url":"mid","width":300},{"url":"small","width":64}]},
              "duration_ms":1000,"external_ids":{"isrc":"USX"}}},
            {"item":{"id":"ep","name":"Pod","type":"episode"}},
            {"track":{"id":"old","name":"Legacy","type":"track"}}
        ],"next":null}"#;
        let parsed: Paging<PlaylistItem> = serde_json::from_str(page).unwrap();
        let tracks: Vec<Track> = parsed
            .items
            .into_iter()
            .filter_map(|i| i.item.or(i.track))
            .filter_map(|v| serde_json::from_value::<SpTrack>(v).ok())
            .filter_map(map_track)
            .collect();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].artwork.as_deref(), Some("small"));
        assert_eq!(tracks[0].artwork_large.as_deref(), Some("big"));
        assert_eq!(tracks[0].isrc.as_deref(), Some("USX"));
        assert_eq!(tracks[1].id, "old");
    }
}
