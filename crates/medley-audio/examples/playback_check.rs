//! Manual, muted Spotify playback check. Opens browser approval if needed.
use medley_audio::{AudioEngine, AudioEvent};
use medley_core::{StreamSource, store::Paths};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn,medley_audio::spotify=info,symphonia_bundle_mp3=error").init();
    let paths = Paths::discover()?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let audio = AudioEngine::new(tx, paths.cache, 0.0);
    println!("Checking Spotify playback approval (audio is muted).");
    audio.connect_spotify(None, librespot_core::SessionConfig::default().client_id, 320).await?;
    println!("Playback login5 authentication succeeded.");
    let uri = std::env::args().nth(1).unwrap_or_else(|| "spotify:track:398wIEUnHVOnTEMnxOHiY9".into());
    let generation = audio.play(StreamSource::Spotify { uri }, 240_000).await?;
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        while let Some(event) = rx.recv().await {
            match event {
                AudioEvent::Buffering { generation: g, buffering: false } if g == generation => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    anyhow::ensure!(audio.position_ms() > 500, "playback position did not advance");
                    println!("Real Spotify playback confirmed: position advanced beyond 500 ms.");
                    return Ok(());
                }
                AudioEvent::Failed { generation: g, message } if g == generation => anyhow::bail!(message),
                _ => {}
            }
        }
        anyhow::bail!("audio event stream closed")
    })
    .await;
    audio.stop();
    result??;
    Ok(())
}
