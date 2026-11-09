use anyhow::{Result, bail};
use reqwest::{RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::time::Duration;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(medley_core::USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(2)
        .build()
        .expect("TLS backend available")
}

/// Send, honouring one `429 Retry-After` (capped at 10 s).
pub async fn send(req: RequestBuilder) -> Result<Response> {
    let retry = req.try_clone();
    let resp = req.send().await?;
    if resp.status() == StatusCode::TOO_MANY_REQUESTS
        && let Some(retry) = retry
    {
        let wait = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(2)
            .min(10);
        tracing::warn!("rate limited, retrying in {wait}s");
        tokio::time::sleep(Duration::from_secs(wait)).await;
        return Ok(retry.send().await?);
    }
    Ok(resp)
}

/// Decode a JSON body or turn the error payload into a readable message.
pub async fn json<T: DeserializeOwned>(resp: Response, service: &str) -> Result<T> {
    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        bail!("{service}: {}", error_message(status, &bytes));
    }
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("{service}: unexpected response ({e})"))
}

/// Like [`json`] but for endpoints whose body we don't need.
pub async fn ok(resp: Response, service: &str) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let bytes = resp.bytes().await.unwrap_or_default();
        bail!("{service}: {}", error_message(status, &bytes));
    }
    Ok(())
}

fn error_message(status: StatusCode, body: &[u8]) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_slice(body).ok();
    let detail = parsed.as_ref().and_then(|v| {
        let candidates = [
            v.pointer("/error/message"),
            v.pointer("/error_description"),
            v.pointer("/message"),
            v.pointer("/errors/0/detail"),
            v.pointer("/errors/0/title"),
            v.pointer("/errors/0/error_message"),
            v.pointer("/error"),
        ];
        candidates.into_iter().flatten().find_map(|x| x.as_str().map(str::to_owned))
    });
    let hint = match status {
        StatusCode::UNAUTHORIZED => " — please reconnect this account",
        StatusCode::FORBIDDEN => " — the service refused access",
        StatusCode::TOO_MANY_REQUESTS => " — rate limited, try again in a moment",
        _ => "",
    };
    match detail {
        Some(d) => format!("{d} ({}){hint}", status.as_u16()),
        None => format!("HTTP {}{hint}", status.as_u16()),
    }
}

/// Minimal HTML entity decoding for text fields some APIs return escaped.
pub fn unescape_html(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#x2F;", "/")
}
