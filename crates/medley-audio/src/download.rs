//! Feeds a [`StreamBuffer`] from a progressive HTTP file or an HLS playlist.

use crate::buffer::StreamBuffer;
use anyhow::{Context, Result, bail};
use futures::StreamExt;
use medley_core::StreamSource;

pub async fn fill(http: reqwest::Client, source: StreamSource, buf: StreamBuffer) {
    let result = match &source {
        StreamSource::Http { url, headers, .. } => http_into(&http, url, headers, &buf, true).await,
        StreamSource::Hls { url, headers, .. } => hls_into(&http, url, headers, &buf).await,
        StreamSource::Spotify { .. } => Ok(()),
    };
    match result {
        Ok(()) => buf.finish(),
        Err(e) if buf.is_cancelled() => tracing::debug!("download stopped: {e}"),
        Err(e) => {
            tracing::warn!("stream download failed: {e:#}");
            buf.fail(format!("{e:#}"));
        }
    }
}

async fn http_into(
    http: &reqwest::Client,
    url: &str,
    headers: &[(String, String)],
    buf: &StreamBuffer,
    set_total: bool,
) -> Result<()> {
    let mut req = http.get(url);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req.send().await?.error_for_status().context("audio request failed")?;
    if set_total {
        buf.set_ready(resp.content_length());
    }
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        if buf.is_cancelled() {
            return Ok(());
        }
        buf.push(&chunk?)?;
    }
    Ok(())
}

async fn get_text(http: &reqwest::Client, url: &url::Url, headers: &[(String, String)]) -> Result<(url::Url, String)> {
    let mut req = http.get(url.clone());
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let resp = req.send().await?.error_for_status().context("playlist request failed")?;
    // Segments resolve against the final (post-redirect) playlist URL.
    let final_url = resp.url().clone();
    Ok((final_url, resp.text().await?))
}

async fn hls_into(http: &reqwest::Client, url: &str, headers: &[(String, String)], buf: &StreamBuffer) -> Result<()> {
    let (mut base, mut text) = get_text(http, &url::Url::parse(url)?, headers).await?;

    // Master playlist: follow the first variant.
    if text.contains("#EXT-X-STREAM-INF") {
        let variant = text
            .lines()
            .skip_while(|l| !l.starts_with("#EXT-X-STREAM-INF"))
            .nth(1)
            .context("empty HLS master playlist")?
            .trim()
            .to_owned();
        let next = base.join(&variant)?;
        (base, text) = get_text(http, &next, &[]).await?;
    }
    if text.lines().any(|l| l.starts_with("#EXT-X-KEY") && !l.contains("METHOD=NONE")) {
        bail!("encrypted HLS streams aren't supported");
    }
    buf.set_ready(None);

    // Segments live on signed CDN URLs: never forward the API auth header to them.
    for line in text.lines().map(str::trim) {
        if buf.is_cancelled() {
            return Ok(());
        }
        if let Some(map) = line.strip_prefix("#EXT-X-MAP:") {
            if let Some(uri) = attr(map, "URI") {
                http_into(http, base.join(&uri)?.as_str(), &[], buf, false).await?;
            }
        } else if !line.is_empty() && !line.starts_with('#') {
            http_into(http, base.join(line)?.as_str(), &[], buf, false).await?;
        }
    }
    Ok(())
}

/// Read `KEY="value"` from an HLS attribute list.
fn attr(list: &str, key: &str) -> Option<String> {
    let start = list.find(&format!("{key}=\""))? + key.len() + 2;
    let end = list[start..].find('"')? + start;
    Some(list[start..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hls_attributes() {
        assert_eq!(attr(r#"URI="init.mp4",BYTERANGE="10@0""#, "URI").as_deref(), Some("init.mp4"));
        assert_eq!(attr("METHOD=NONE", "URI"), None);
    }
}
