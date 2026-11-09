//! OAuth 2 PKCE helpers and a minimal loopback HTTP server.
//!
//! Desktop apps can't keep a client secret safe and have no public redirect
//! target, so every provider logs in through the system browser and redirects
//! back to `http://127.0.0.1:<port>/…`. The server binds to loopback only,
//! checks the `state` nonce, and shuts down after the first valid callback.

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// How long we wait for the user to finish logging in.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        let verifier = random_string(64);
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        Self { verifier, challenge }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// URL-safe random string from the RFC 7636 unreserved alphabet.
pub fn random_string(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    (0..len).map(|_| ALPHABET[rand::random_range(0..ALPHABET.len())] as char).collect()
}

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub body: String,
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    pub fn html(status: u16, body: String) -> Self {
        Self { status, content_type: "text/html; charset=utf-8", body }
    }
    pub fn not_found() -> Self {
        Self { status: 404, content_type: "text/plain", body: "Not found".into() }
    }
}

/// What a route handler wants the server to do after answering.
pub enum Flow<T> {
    Continue,
    Done(Result<T>),
}

/// Serve loopback requests until the handler returns [`Flow::Done`] or we time out.
pub async fn serve_loopback<T, F>(port: u16, timeout: Duration, mut handler: F) -> Result<T>
where
    F: FnMut(&Request) -> (Response, Flow<T>),
{
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("port {port} on 127.0.0.1 is busy — close whatever uses it and retry"))?;

    let work = async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let request = match read_request(&mut stream).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::debug!("loopback: bad request: {e}");
                    continue;
                }
            };
            let (response, flow) = handler(&request);
            let head = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                response.status,
                reason(response.status),
                response.content_type,
                response.body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(response.body.as_bytes()).await;
            let _ = stream.shutdown().await;
            if let Flow::Done(result) = flow {
                return result;
            }
        }
    };

    match tokio::time::timeout(timeout, work).await {
        Ok(result) => result,
        Err(_) => bail!("login timed out — no response from the browser within {} minutes", timeout.as_secs() / 60),
    }
}

/// Wait for a standard `?code=…&state=…` OAuth redirect on `path`.
pub async fn wait_for_code(port: u16, path: &str, expected_state: &str, service: &str) -> Result<String> {
    let path = path.to_owned();
    let expected_state = expected_state.to_owned();
    let service = service.to_owned();
    serve_loopback(port, LOGIN_TIMEOUT, move |req| {
        if req.path != path {
            return (Response::not_found(), Flow::Continue);
        }
        if let Some(err) = req.query.get("error") {
            let msg = req.query.get("error_description").cloned().unwrap_or_else(|| err.clone());
            return (
                Response::html(400, result_page(&service, false, &msg)),
                Flow::Done(Err(anyhow!("{service} login failed: {msg}"))),
            );
        }
        if req.query.get("state").map(String::as_str) != Some(expected_state.as_str()) {
            return (
                Response::html(400, result_page(&service, false, "The login response didn't match this request.")),
                Flow::Continue,
            );
        }
        match req.query.get("code") {
            Some(code) => (
                Response::html(200, result_page(&service, true, "You can close this tab and return to Medley.")),
                Flow::Done(Ok(code.clone())),
            ),
            None => {
                (Response::html(400, result_page(&service, false, "No authorization code received.")), Flow::Continue)
            }
        }
    })
    .await
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Result<Request> {
    const MAX: usize = 64 * 1024;
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let header_end = loop {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await??;
        if n == 0 {
            bail!("connection closed");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX {
            bail!("request too large");
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let first = lines.next().unwrap_or_default();
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or("/").to_owned();

    let content_length = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0)
        .min(MAX);
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await??;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    let url = url::Url::parse(&format!("http://127.0.0.1{target}")).context("bad request target")?;
    Ok(Request {
        method,
        path: url.path().to_owned(),
        query: url.query_pairs().into_owned().collect(),
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    }
}

/// Escape text for safe interpolation into HTML.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// Shared stylesheet for the browser pages Medley shows during login.
pub const PAGE_STYLE: &str = r#"
:root { color-scheme: light dark; --bg:#f5f5f7; --card:#fff; --text:#1d1d1f; --sub:#6e6e73; --accent:#ff2d55; }
@media (prefers-color-scheme: dark) { :root { --bg:#000; --card:#1c1c1e; --text:#f5f5f7; --sub:#98989d; --accent:#ff375f; } }
* { box-sizing: border-box; }
body { margin:0; min-height:100vh; display:grid; place-items:center; background:var(--bg); color:var(--text);
  font: 15px/1.5 -apple-system, BlinkMacSystemFont, "SF Pro Text", Inter, system-ui, sans-serif; padding:16px; }
main { background:var(--card); border-radius:20px; padding:40px 36px; max-width:420px; width:100%; text-align:center;
  box-shadow: 0 1px 2px rgba(0,0,0,.06), 0 12px 40px rgba(0,0,0,.10); }
.badge { width:64px; height:64px; border-radius:16px; margin:0 auto 20px; display:grid; place-items:center; font-size:30px; color:#fff;
  background: linear-gradient(135deg,#ff9f0a,#ff2d55 50%,#5e5ce6); }
.badge.err { background:#8e8e93; }
h1 { font-size:22px; letter-spacing:-0.02em; margin:0 0 8px; }
p { color:var(--sub); margin:0 0 4px; }
button { margin-top:24px; font:inherit; font-weight:600; color:#fff; background:var(--accent); border:0; border-radius:980px;
  padding:11px 22px; cursor:pointer; transition: transform .1s ease-out, opacity .2s; }
button:active { transform: scale(.97); }
button:disabled { opacity:.4; cursor:default; }
"#;

pub fn result_page(service: &str, ok: bool, message: &str) -> String {
    let (badge, class, title) = if ok {
        ("✓", "badge", format!("Connected to {}", html_escape(service)))
    } else {
        ("!", "badge err", format!("{} login didn't finish", html_escape(service)))
    };
    format!(
        "<!doctype html><html lang=en><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>\
<title>Medley</title><style>{PAGE_STYLE}</style></head><body><main><div class='{class}'>{badge}</div><h1>{title}</h1><p>{}</p></main></body></html>",
        html_escape(message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_shape() {
        let p = Pkce::new();
        assert_eq!(p.verifier.len(), 64);
        assert_eq!(p.challenge.len(), 43); // base64url(sha256) without padding
        assert!(!p.challenge.contains('='));
    }

    #[test]
    fn escape() {
        assert_eq!(html_escape("<a href=\"x\">'&'</a>"), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
    }

    #[tokio::test]
    async fn loopback_receives_code_and_rejects_bad_state() {
        let port = 18_000 + rand::random_range(0..2000u16);
        let server = tokio::spawn(async move { wait_for_code(port, "/cb", "good", "Test").await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let get = |path: &'static str| async move {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).await.unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).await.unwrap();
            out
        };
        assert!(get("/cb?code=abc&state=evil").await.starts_with("HTTP/1.1 400"));
        assert!(get("/favicon.ico").await.starts_with("HTTP/1.1 404"));
        assert!(get("/cb?code=abc&state=good").await.starts_with("HTTP/1.1 200"));
        assert_eq!(server.await.unwrap().unwrap(), "abc");
    }
}
