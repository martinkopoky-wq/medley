//! Medley — one fast, native library for Spotify, SoundCloud and Apple Music.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod chrome;
mod demo;
mod dock;
mod panels;
mod player;
mod theme;
mod widgets;

use anyhow::Context as _;
use eframe::egui;
use medley_core::store::{Paths, Settings, load_json};
use std::sync::Arc;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,medley=info,medley_core=info,medley_providers=info,medley_audio=info".into()),
        )
        .init();

    let paths = Paths::discover()?;
    // Two workers are plenty for a music player and keep idle threads (and RAM) low.
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(8)
            .thread_name("medley-rt")
            .enable_all()
            .build()
            .context("starting the async runtime")?,
    );

    let translucent = load_json::<Settings>(&paths.settings_file()).appearance.translucent;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Medley")
        .with_app_id("app.medley.Medley")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([760.0, 480.0])
        .with_transparent(translucent);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    #[cfg(target_os = "macos")]
    {
        viewport = viewport.with_fullsize_content_view(true).with_title_shown(false).with_titlebar_shown(false);
    }

    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // Never block in swap. On Wayland a hidden or locked window gets no frame
    // callbacks, so a vsync'd swap would freeze the UI thread — and with it the
    // logic that starts the next song. The app caps its own frame rate instead.
    options.glow_options.vsync = false;
    eframe::run_native("Medley", options, Box::new(move |cc| Ok(Box::new(app::MedleyApp::new(cc, runtime, paths)))))
        .map_err(|e| anyhow::anyhow!("window system error: {e}"))
}

fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../../assets/icon-256.png");
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}
