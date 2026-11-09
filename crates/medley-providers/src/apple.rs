//! Apple Music API.
//!
//! Requests need two tokens: a *developer token* (an ES256 JWT minted from a
//! MusicKit `.p8` key; Apple Developer Program membership required) and a
//! *Music User Token*, which only MusicKit JS can obtain. Medley serves a tiny
//! page on 127.0.0.1 that loads Apple's MusicKit JS, asks the user to sign in,
//! and posts the user token back to the app.
//!
//! Full-length Apple Music audio is FairPlay-protected and only plays inside
//! Apple's own players, so Medley streams the 30-second previews natively and
//! offers "Open in Apple Music" for the full song. Library and playlist
//! management work fully.

use crate::http;
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use medley_core::oauth::{Flow, LOGIN_TIMEOUT, PAGE_STYLE, Response, random_string, result_page, serve_loopback};
use medley_core::store::{CredentialStore, SettingsHandle};
use medley_core::{Account, Playability, PlaylistSummary, Provider, ProviderKind, StreamSource, Track, now_unix};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Mutex;

const API: &str = "https://api.music.apple.com";
/// Apple allows at most ~6 months; mint for 150 days.
const DEV_TOKEN_TTL: u64 = 150 * 24 * 3600;

pub struct AppleMusic {
    http: reqwest::Client,
    settings: SettingsHandle,
    creds: CredentialStore,
    storefront: Mutex<Option<String>>,
    minted: Mutex<Option<(String, u64)>>,
}

impl AppleMusic {
    pub fn new(http: reqwest::Client, settings: SettingsHandle, creds: CredentialStore) -> Self {
        Self { http, settings, creds, storefront: Mutex::new(None), minted: Mutex::new(None) }
    }

    /// A pasted developer token wins; otherwise mint one from the configured `.p8` key.
    fn developer_token(&self) -> Result<String> {
        if let Some(t) = self.creds.get().apple_developer_token.filter(|t| !t.trim().is_empty()) {
            return Ok(t.trim().to_owned());
        }
        if let Some((token, exp)) = self.minted.lock().expect("mint lock").clone()
            && exp > now_unix() + 3600
        {
            return Ok(token);
        }
        let (team, key, path) = {
            let s = self.settings.read().expect("settings lock");
            (
                s.apple.team_id.trim().to_owned(),
                s.apple.key_id.trim().to_owned(),
                s.apple.private_key_path.trim().to_owned(),
            )
        };
        if team.is_empty() || key.is_empty() || path.is_empty() {
            bail!("Apple Music needs a MusicKit developer token or key (Accounts panel)");
        }
        let pem = std::fs::read(shellexpand_home(&path)).with_context(|| format!("reading MusicKit key {path}"))?;
        let (token, exp) = mint_developer_token(&team, &key, &pem)?;
        *self.minted.lock().expect("mint lock") = Some((token.clone(), exp));
        Ok(token)
    }

    fn user_token(&self) -> Result<String> {
        self.creds.get().apple_music_user_token.ok_or_else(|| anyhow!("Apple Music isn't connected"))
    }

    async fn request(&self, method: Method, path: &str, body: Option<&Value>) -> Result<reqwest::Response> {
        let url = if path.starts_with("https://") { path.to_owned() } else { format!("{API}{path}") };
        let mut req = self
            .http
            .request(method, &url)
            .bearer_auth(self.developer_token()?)
            .header("Music-User-Token", self.user_token()?);
        if let Some(body) = body {
            req = req.json(body);
        }
        http::send(req).await
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        http::json(self.request(Method::GET, path, None).await?, "Apple Music").await
    }

    async fn storefront(&self) -> Result<String> {
        if let Some(sf) = self.storefront.lock().expect("storefront lock").clone() {
            return Ok(sf);
        }
        let resp: Data<Resource> = self.get("/v1/me/storefront").await?;
        let sf = resp.data.into_iter().next().map(|r| r.id).unwrap_or_else(|| "us".into());
        *self.storefront.lock().expect("storefront lock") = Some(sf.clone());
        Ok(sf)
    }

    async fn songs_paged(&self, first: &str, max: usize) -> Result<Vec<Track>> {
        let mut out = Vec::new();
        let mut next = Some(first.to_owned());
        while let Some(path) = next.take() {
            let page: Data<Resource> = self.get(&path).await?;
            out.extend(page.data.into_iter().filter_map(map_song));
            if out.len() < max {
                next = page.next;
            }
        }
        out.truncate(max);
        Ok(out)
    }

    fn account(&self, storefront: &str) -> Account {
        Account {
            provider: ProviderKind::AppleMusic,
            user_id: "me".into(),
            display_name: format!("Apple Music · {}", storefront.to_uppercase()),
            avatar: None,
        }
    }
}

#[async_trait]
impl Provider for AppleMusic {
    fn kind(&self) -> ProviderKind {
        ProviderKind::AppleMusic
    }

    fn setup_hint(&self) -> Option<String> {
        self.developer_token().err().map(|e| e.to_string())
    }

    async fn restore(&self) -> Result<Option<Account>> {
        if self.creds.get().apple_music_user_token.is_none() {
            return Ok(None);
        }
        let sf = self.storefront().await?;
        Ok(Some(self.account(&sf)))
    }

    async fn connect(&self) -> Result<Account> {
        let developer_token = self.developer_token()?;
        let port = self.settings.read().expect("settings lock").apple.redirect_port;
        let nonce = random_string(24);
        let page = musickit_page(&developer_token, &nonce);

        let waiter = tokio::spawn(async move {
            serve_loopback(port, LOGIN_TIMEOUT, move |req| match (req.method.as_str(), req.path.as_str()) {
                ("GET", "/apple") => (Response::html(200, page.clone()), Flow::Continue),
                ("POST", "/apple/token") => {
                    let token = req.body.trim().to_owned();
                    if req.query.get("nonce") != Some(&nonce) || token.is_empty() || token.len() > 4096 {
                        return (
                            Response::html(400, result_page("Apple Music", false, "Invalid request.")),
                            Flow::Continue,
                        );
                    }
                    (Response::html(200, "ok".into()), Flow::Done(Ok(token)))
                }
                _ => (Response::not_found(), Flow::Continue),
            })
            .await
        });
        tokio::task::yield_now().await;
        open::that_detached(format!("http://127.0.0.1:{port}/apple")).context("couldn't open the browser")?;
        let user_token = waiter.await??;
        self.creds.update(|c| c.apple_music_user_token = Some(user_token))?;
        *self.storefront.lock().expect("storefront lock") = None;
        let sf = self.storefront().await?;
        Ok(self.account(&sf))
    }

    async fn disconnect(&self) -> Result<()> {
        *self.storefront.lock().expect("storefront lock") = None;
        self.creds.update(|c| c.apple_music_user_token = None)
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>> {
        let sf = self.storefront().await?;
        let mut url = url::Url::parse(&format!("{API}/v1/catalog/{sf}/search"))?;
        url.query_pairs_mut()
            .append_pair("term", query)
            .append_pair("types", "songs")
            .append_pair("limit", &limit.clamp(1, 25).to_string());
        let resp: SearchResponse = self.get(url.as_str()).await?;
        Ok(resp.results.songs.map(|s| s.data).unwrap_or_default().into_iter().filter_map(map_song).collect())
    }

    async fn find_by_isrc(&self, isrc: &str) -> Result<Option<Track>> {
        let sf = self.storefront().await?;
        let resp: Data<Resource> = self.get(&format!("/v1/catalog/{sf}/songs?filter[isrc]={isrc}")).await?;
        Ok(resp.data.into_iter().find_map(map_song))
    }

    async fn liked_tracks(&self, limit: usize) -> Result<Vec<Track>> {
        self.songs_paged("/v1/me/library/songs?limit=100&include=catalog", limit).await
    }

    async fn playlists(&self) -> Result<Vec<PlaylistSummary>> {
        let mut out = Vec::new();
        let mut next = Some("/v1/me/library/playlists?limit=100".to_owned());
        while let Some(path) = next.take() {
            let page: Data<Resource> = self.get(&path).await?;
            next = page.next.filter(|_| out.len() < 500);
            out.extend(page.data.into_iter().filter_map(|r| {
                let a = r.attributes?;
                Some(PlaylistSummary {
                    provider: ProviderKind::AppleMusic,
                    id: r.id,
                    name: a.name,
                    description: a.description.and_then(|d| d.standard).filter(|d| !d.is_empty()),
                    owner: None,
                    track_count: None,
                    artwork: a.artwork.map(|art| artwork_url(&art.url, 100)),
                    editable: a.can_edit.unwrap_or(false),
                })
            }));
        }
        Ok(out)
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        self.songs_paged(&format!("/v1/me/library/playlists/{playlist_id}/tracks?limit=100&include=catalog"), 2000)
            .await
    }

    async fn create_playlist(&self, name: &str, description: &str, tracks: &[Track]) -> Result<PlaylistSummary> {
        let data: Vec<Value> = tracks
            .iter()
            .filter(|t| t.provider == ProviderKind::AppleMusic)
            .map(|t| json!({ "id": t.id, "type": if t.id.starts_with("i.") { "library-songs" } else { "songs" } }))
            .collect();
        let count = data.len();
        let body = json!({
            "attributes": { "name": name, "description": description },
            "relationships": { "tracks": { "data": data } }
        });
        let created: Data<Resource> =
            http::json(self.request(Method::POST, "/v1/me/library/playlists", Some(&body)).await?, "Apple Music")
                .await?;
        let r = created.data.into_iter().next().context("Apple Music didn't return the new playlist")?;
        Ok(PlaylistSummary {
            provider: ProviderKind::AppleMusic,
            id: r.id,
            name: name.to_owned(),
            description: Some(description.to_owned()),
            owner: None,
            track_count: Some(count as u32),
            artwork: None,
            editable: true,
        })
    }

    async fn stream(&self, track: &Track) -> Result<StreamSource> {
        let preview = match &track.preview_url {
            Some(u) => Some(u.clone()),
            None if !track.id.starts_with("i.") => {
                let sf = self.storefront().await?;
                let resp: Data<Resource> = self.get(&format!("/v1/catalog/{sf}/songs/{}", track.id)).await?;
                resp.data.into_iter().find_map(map_song).and_then(|t| t.preview_url)
            }
            None => None,
        };
        match preview {
            Some(url) => Ok(StreamSource::Http { url, headers: Vec::new(), hint: "m4a".into() }),
            None => bail!("No preview available — open it in Apple Music to hear the full song"),
        }
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    iat: u64,
    exp: u64,
}

/// Mint a MusicKit developer token (ES256 JWT) from a `.p8` private key.
pub fn mint_developer_token(team_id: &str, key_id: &str, pem: &[u8]) -> Result<(String, u64)> {
    let iat = now_unix();
    let exp = iat + DEV_TOKEN_TTL;
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    header.kid = Some(key_id.to_owned());
    let key = jsonwebtoken::EncodingKey::from_ec_pem(pem).context("the MusicKit key isn't a valid .p8 file")?;
    let token = jsonwebtoken::encode(&header, &Claims { iss: team_id, iat, exp }, &key)?;
    Ok((token, exp))
}

fn shellexpand_home(path: &str) -> std::path::PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => std::path::PathBuf::from(home).join(rest),
        _ => std::path::PathBuf::from(path),
    }
}

/// Replace the `{w}x{h}` template Apple uses for artwork URLs.
fn artwork_url(template: &str, size: u32) -> String {
    template
        .replace("{w}", &size.to_string())
        .replace("{h}", &size.to_string())
        .replace("{c}", "bb")
        .replace("{f}", "jpg")
}

/// Library songs come with their catalog twin under `relationships.catalog`;
/// prefer it because it carries previews and the ISRC.
fn map_song(r: Resource) -> Option<Track> {
    if let Some(catalog) = r.relationships.as_ref().and_then(|rel| rel.catalog.as_ref()).and_then(|c| c.data.first())
        && let Some(t) = map_song(catalog.clone())
    {
        return Some(t);
    }
    if !(r.kind == "songs" || r.kind == "library-songs") {
        return None;
    }
    let a = r.attributes?;
    let preview_url = a.previews.and_then(|p| p.into_iter().next()).map(|p| p.url);
    Some(Track {
        provider: ProviderKind::AppleMusic,
        title: a.name,
        artists: a.artist_name.map(|n| vec![n]).unwrap_or_default(),
        album: a.album_name,
        duration_ms: a.duration_in_millis.unwrap_or(0),
        artwork: a.artwork.as_ref().map(|art| artwork_url(&art.url, 100)),
        artwork_large: a.artwork.as_ref().map(|art| artwork_url(&art.url, 600)),
        isrc: a.isrc,
        permalink: a.url,
        playability: if preview_url.is_some() || r.kind == "songs" {
            Playability::Preview
        } else {
            Playability::Unavailable
        },
        preview_url,
        id: r.id,
    })
}

fn musickit_page(developer_token: &str, nonce: &str) -> String {
    // JSON-encode and neutralise `<` so nothing can break out of the script tag.
    let token_js = serde_json::to_string(developer_token).unwrap_or_default().replace('<', "\\u003c");
    let nonce_js = serde_json::to_string(nonce).unwrap_or_default().replace('<', "\\u003c");
    format!(
        r#"<!doctype html><html lang=en><head><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1">
<title>Connect Apple Music · Medley</title><style>{PAGE_STYLE}</style>
<script src="https://js-cdn.music.apple.com/musickit/v3/musickit.js" data-web-components async></script></head>
<body><main><div class=badge>♪</div><h1>Connect Apple Music</h1>
<p id=status>Loading MusicKit…</p><button id=go disabled>Continue with Apple Music</button></main>
<script>
const DEV_TOKEN = {token_js};
const NONCE = {nonce_js};
const statusEl = document.getElementById('status');
const go = document.getElementById('go');
document.addEventListener('musickitloaded', async () => {{
  try {{
    await MusicKit.configure({{ developerToken: DEV_TOKEN, app: {{ name: 'Medley', build: '0.1.0' }} }});
    statusEl.textContent = 'Sign in with your Apple Account to let Medley read and edit your library.';
    go.disabled = false;
  }} catch (e) {{ statusEl.textContent = 'MusicKit could not start: ' + e; }}
}});
go.addEventListener('click', async () => {{
  go.disabled = true;
  try {{
    const token = await MusicKit.getInstance().authorize();
    const res = await fetch('/apple/token?nonce=' + encodeURIComponent(NONCE), {{ method: 'POST', headers: {{ 'Content-Type': 'text/plain' }}, body: token }});
    if (!res.ok) throw new Error('Medley rejected the token');
    document.querySelector('h1').textContent = 'Connected to Apple Music';
    statusEl.textContent = 'You can close this tab and return to Medley.';
    go.remove();
  }} catch (e) {{ statusEl.textContent = 'Sign-in did not finish: ' + e; go.disabled = false; }}
}});
</script></body></html>"#
    )
}

#[derive(Deserialize)]
struct Data<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Deserialize, Clone)]
struct Resource {
    id: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    attributes: Option<Attributes>,
    #[serde(default)]
    relationships: Option<Relationships>,
}

#[derive(Deserialize, Clone)]
struct Relationships {
    #[serde(default)]
    catalog: Option<CatalogRel>,
}

#[derive(Deserialize, Clone)]
struct CatalogRel {
    #[serde(default)]
    data: Vec<Resource>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Attributes {
    #[serde(default)]
    name: String,
    #[serde(default)]
    artist_name: Option<String>,
    #[serde(default)]
    album_name: Option<String>,
    #[serde(default)]
    duration_in_millis: Option<u64>,
    #[serde(default)]
    artwork: Option<Artwork>,
    #[serde(default)]
    isrc: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    previews: Option<Vec<Preview>>,
    #[serde(default)]
    description: Option<Description>,
    #[serde(default)]
    can_edit: Option<bool>,
}

#[derive(Deserialize, Clone)]
struct Artwork {
    url: String,
}

#[derive(Deserialize, Clone)]
struct Preview {
    url: String,
}

#[derive(Deserialize, Clone)]
struct Description {
    #[serde(default)]
    standard: Option<String>,
}

#[derive(Deserialize)]
struct SearchResponse {
    results: SearchResults,
}

#[derive(Deserialize)]
struct SearchResults {
    #[serde(default)]
    songs: Option<Data<Resource>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_song_prefers_catalog_twin() {
        let raw = r#"{"data":[{"id":"i.abc","type":"library-songs","attributes":{"name":"Lib Name"},
            "relationships":{"catalog":{"data":[{"id":"1440","type":"songs","attributes":{"name":"Cat Name",
            "artistName":"Artist","durationInMillis":180000,"isrc":"USABC","url":"https://music.apple.com/x",
            "artwork":{"url":"https://is1/{w}x{h}bb.jpg"},"previews":[{"url":"https://audio/p.m4a"}]}}]}}}]}"#;
        let page: Data<Resource> = serde_json::from_str(raw).unwrap();
        let t = page.data.into_iter().find_map(map_song).unwrap();
        assert_eq!(t.id, "1440");
        assert_eq!(t.title, "Cat Name");
        assert_eq!(t.preview_url.as_deref(), Some("https://audio/p.m4a"));
        assert_eq!(t.artwork_large.as_deref(), Some("https://is1/600x600bb.jpg"));
        assert_eq!(t.playability, Playability::Preview);
    }

    #[test]
    fn page_never_breaks_out_of_script() {
        let page = musickit_page("</script><script>alert(1)</script>", "n");
        assert!(!page.contains("</script><script>alert(1)"));
    }

    #[test]
    fn mints_es256_tokens() {
        // Throwaway P-256 key generated for this test only.
        let pem = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgfuwab74JzUU3kMsw
NK75dsmB1UlqRJUNxDR+XKdw8t+hRANCAATThCcRTxFowqO+hv3sUKqYvE16QJCb
SjsLvbT534tj3oV1Bq1Iem0pPhTmsAU7IGh6h8Pgbg4QIY+/DXj8qxq1
-----END PRIVATE KEY-----";
        let (token, exp) = mint_developer_token("TEAM123", "KEY456", pem).unwrap();
        assert_eq!(token.split('.').count(), 3);
        assert!(exp > now_unix());
        let header = jsonwebtoken::decode_header(&token).unwrap();
        assert_eq!(header.kid.as_deref(), Some("KEY456"));
        assert_eq!(header.alg, jsonwebtoken::Algorithm::ES256);
    }
}
