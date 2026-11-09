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

/// Retry once for short rate limits. Never retry before the server's deadline;
/// longer cooldowns are returned to the UI rather than keeping an action pending.
pub async fn send(req: RequestBuilder) -> Result<Response> {
    let retry = req.try_clone();
    let resp = req.send().await?;
    if resp.status() == StatusCode::TOO_MANY_REQUESTS
        && let Some(retry) = retry
        && let Some(wait) = retry_after(&resp)
        && wait <= 10
    {
        tracing::warn!("rate limited, retrying in {wait}s");
        tokio::time::sleep(Duration::from_secs(wait)).await;
        return Ok(retry.send().await?);
    }
    Ok(resp)
}

fn retry_after(resp: &Response) -> Option<u64> {
    resp.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.parse().ok()
}

/// Decode a JSON body or turn the error payload into a readable message.
pub async fn json<T: DeserializeOwned>(resp: Response, service: &str) -> Result<T> {
    let status = resp.status();
    let wait = retry_after(&resp);
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        bail!("{service}: {}", error_message(status, &bytes, service, wait));
    }
    serde_json::from_slice(&bytes).map_err(|e| anyhow::anyhow!("{service}: unexpected response ({e})"))
}

/// Like [`json`] but for endpoints whose body we don't need.
pub async fn ok(resp: Response, service: &str) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let wait = retry_after(&resp);
        let bytes = resp.bytes().await.unwrap_or_default();
        bail!("{service}: {}", error_message(status, &bytes, service, wait));
    }
    Ok(())
}

fn error_message(status: StatusCode, body: &[u8], service: &str, wait: Option<u64>) -> String {
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
        StatusCode::TOO_MANY_REQUESTS => " — API limit reached",
        _ => "",
    };
    let mut message = match detail {
        Some(d) => format!("{d} ({}){hint}", status.as_u16()),
        None => format!("HTTP {}{hint}", status.as_u16()),
    };
    if status == StatusCode::TOO_MANY_REQUESTS {
        if parsed.as_ref().and_then(|v| v.pointer("/error/reason")).and_then(|v| v.as_str()) == Some("QUOTA_EXCEEDED") {
            message.push_str(". The app's API quota is exhausted; reconnecting will not reset it");
        }
        if let Some(wait) = wait {
            message.push_str(&format!(". Wait at least {wait} seconds before trying again"));
        }
        if service == "Spotify" {
            message.push_str(
                ". If using the built-in Client ID, set your own Spotify app's Client ID in Accounts → Spotify, then reconnect",
            );
        }
    }
    message
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn read_headers(socket: &mut tokio::net::TcpStream) {
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            headers.push(socket.read_u8().await.unwrap());
            assert!(headers.len() <= 8192);
        }
    }

    async fn rate_limited_server(header: &str) -> (reqwest::Client, String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let header = header.to_owned();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            read_headers(&mut socket).await;
            let body = r#"{"error":{"message":"API rate limit exceeded"}}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 429 Too Many Requests\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        // Ignore proxy environment variables for these loopback-only tests.
        (reqwest::Client::builder().no_proxy().build().unwrap(), url, server)
    }

    #[tokio::test]
    async fn long_cooldown_is_not_retried_early_and_is_shown_to_user() {
        let (client, url, server) = rate_limited_server("Retry-After: 3600\r\n").await;
        let resp = tokio::time::timeout(Duration::from_secs(5), send(client.get(url))).await.unwrap().unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        let message = json::<serde_json::Value>(resp, "Spotify").await.unwrap_err().to_string();
        assert!(message.contains("3600 seconds"), "{message}");
        assert!(message.contains("Accounts → Spotify"), "{message}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn missing_retry_after_does_not_guess_a_cooldown() {
        let (client, url, server) = rate_limited_server("").await;
        let resp = send(client.get(url)).await.unwrap();
        let message = ok(resp, "SoundCloud").await.unwrap_err().to_string();
        assert!(message.contains("429"), "{message}");
        assert!(!message.contains("Spotify"), "{message}");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn short_cooldown_retries_once() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for status in ["429 Too Many Requests\r\nRetry-After: 0", "200 OK"] {
                let (mut socket, _) = listener.accept().await.unwrap();
                read_headers(&mut socket).await;
                socket
                    .write_all(
                        format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let resp = tokio::time::timeout(Duration::from_secs(5), send(client.get(url))).await.unwrap().unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        server.await.unwrap();
    }

    #[test]
    fn exhausted_quota_is_distinct_from_temporary_rate_limit() {
        let message = error_message(
            StatusCode::TOO_MANY_REQUESTS,
            br#"{"error":{"message":"Too many requests","reason":"QUOTA_EXCEEDED"}}"#,
            "Spotify",
            None,
        );
        assert!(message.contains("quota is exhausted"), "{message}");
        assert!(!message.contains("in a moment"), "{message}");
    }
}
