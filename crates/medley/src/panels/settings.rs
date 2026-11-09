//! Settings: AI backend, playback, storage locations and about.
//!
//! Also hosts the small form helpers (label/control rows, settings-backed text
//! fields, secret fields) shared with the Accounts panel.

use crate::app::{AppState, ToastKind};
use crate::theme;
use crate::widgets;
use eframe::egui::{self, Align, Layout, RichText, Ui};
use medley_core::store::{AiBackend, CredentialStore, Credentials};

const MODELS: [(&str, &str); 3] =
    [("claude-opus-5-5", "Opus 5.5"), ("claude-sonnet-5-5", "Sonnet 5.5"), ("claude-haiku-5-5", "Haiku 5.5")];
const CUSTOM: &str = "custom";

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    crate::panels::scroll(ui, "settings", |ui| {
        widgets::large_title(ui, "Settings");
        ui.add_space(4.0);
        ai_section(ui, state);
        playback_section(ui, state);
        storage_section(ui, state);
        about_section(ui, state);
        ui.add_space(12.0);
    });
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

fn ai_section(ui: &mut Ui, state: &mut AppState) {
    let ai = state.settings().ai.clone();
    widgets::section_header(ui, "AI");
    widgets::card(ui, |ui| {
        row(ui, "Backend", |ui| {
            let mut backend = ai.backend;
            if widgets::segmented(
                ui,
                &mut backend,
                &[(AiBackend::Off, "Off"), (AiBackend::Ollama, "Ollama"), (AiBackend::Claude, "Claude")],
            ) {
                state.edit_settings(|s| s.ai.backend = backend);
            }
        });
        match ai.backend {
            AiBackend::Off => {
                widgets::caption(ui, "Vibe → Mix, Keep the Vibe, liner notes and layout design are turned off.");
            }
            AiBackend::Ollama => {
                widgets::hairline(ui);
                if let Some(v) = text_setting(ui, "Server", &ai.ollama_url, "http://127.0.0.1:11434", 240.0) {
                    state.edit_settings(|s| s.ai.ollama_url = v);
                }
                if let Some(v) = text_setting(ui, "Model", &ai.ollama_model, "llama3.2", 240.0) {
                    state.edit_settings(|s| s.ai.ollama_model = v);
                }
                widgets::caption(ui, "Runs on your computer. Install from ollama.com, then `ollama pull llama3.2`.");
            }
            AiBackend::Claude => {
                widgets::hairline(ui);
                let creds = state.creds.clone();
                row(ui, "API key", |ui| {
                    if let Err(e) = secret_field(
                        ui,
                        &creds,
                        "anthropic_key",
                        "sk-ant-…",
                        |c| c.anthropic_api_key.clone(),
                        |c, v| c.anthropic_api_key = v,
                    ) {
                        state.toast(ToastKind::Error, format!("Couldn't save the API key: {e}"));
                    }
                });
                widgets::caption(
                    ui,
                    "Stored in your config folder with owner-only permissions. Or set ANTHROPIC_API_KEY.",
                );
                widgets::hairline(ui);
                model_picker(ui, state, &ai.claude_model);
                widgets::hairline(ui);
                row(ui, "Effort", |ui| {
                    let mut effort: &str = match ai.claude_effort.as_str() {
                        "low" => "low",
                        "high" => "high",
                        _ => "medium",
                    };
                    if widgets::segmented(ui, &mut effort, &[("low", "Low"), ("medium", "Medium"), ("high", "High")]) {
                        let e = effort.to_owned();
                        state.edit_settings(|s| s.ai.claude_effort = e);
                    }
                });
                widgets::caption(ui, "Higher effort thinks longer and costs more.");
                widgets::caption(ui, "Song titles and your prompts are sent to Anthropic.");
            }
        }
        widgets::hairline(ui);
        let label = state.ai.backend_label();
        let status = state.ai_unavailable();
        let pal = theme::palette(ui.ctx());
        row(ui, "Status", |ui| match &status {
            None => {
                ui.label(RichText::new(format!("Ready · {label}")).color(pal.success));
            }
            Some(_) => {
                ui.label(RichText::new("Not available").color(pal.secondary));
            }
        });
        if let Some(reason) = status {
            widgets::caption(ui, &reason);
        }
    });
}

fn model_picker(ui: &mut Ui, state: &mut AppState, model: &str) {
    let custom_id = ui.id().with("claude_custom_model");
    let forced_custom = ui.ctx().data(|d| d.get_temp::<bool>(custom_id)).unwrap_or(false);
    let preset = MODELS.iter().find(|(id, _)| *id == model).map(|(id, _)| *id);
    let mut choice: &str = match preset {
        Some(id) if !forced_custom => id,
        _ => CUSTOM,
    };
    row(ui, "Model", |ui| {
        let options: Vec<(&str, &str)> = MODELS.iter().copied().chain([(CUSTOM, "Custom")]).collect();
        if widgets::segmented(ui, &mut choice, &options) {
            if choice == CUSTOM {
                ui.ctx().data_mut(|d| d.insert_temp(custom_id, true));
            } else {
                ui.ctx().data_mut(|d| d.insert_temp(custom_id, false));
                let m = choice.to_owned();
                state.edit_settings(|s| s.ai.claude_model = m);
            }
        }
    });
    if choice == CUSTOM
        && let Some(v) = text_setting(ui, "Model ID", model, "claude-opus-5-5", 240.0)
    {
        state.edit_settings(|s| s.ai.claude_model = v.trim().to_owned());
    }
}

fn playback_section(ui: &mut Ui, state: &mut AppState) {
    widgets::section_header(ui, "Playback");
    widgets::card(ui, |ui| {
        row(ui, "Volume", |ui| {
            let pal = theme::palette(ui.ctx());
            let mut volume = state.volume();
            ui.label(RichText::new(format!("{:.0}%", volume * 100.0)).color(pal.secondary));
            ui.allocate_ui_with_layout(egui::vec2(150.0, 26.0), Layout::left_to_right(Align::Center), |ui| {
                if widgets::volume_slider(ui, &mut volume, 110.0) {
                    state.set_volume(volume);
                }
            });
        });
        widgets::caption(
            ui,
            "The audio device is released after 30 seconds of silence, so an idle Medley uses no audio CPU.",
        );
    });
}

fn storage_section(ui: &mut Ui, state: &mut AppState) {
    widgets::section_header(ui, "Storage");
    let paths = [
        ("Settings & credentials", state.paths.config.clone()),
        ("Mixes & layouts", state.paths.data.clone()),
        ("Cache", state.paths.cache.clone()),
    ];
    widgets::card(ui, |ui| {
        let pal = theme::palette(ui.ctx());
        for (i, (label, path)) in paths.iter().enumerate() {
            if i > 0 {
                widgets::hairline(ui);
            }
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(*label);
                    ui.add(
                        egui::Label::new(
                            RichText::new(path.display().to_string()).monospace().size(11.5).color(pal.secondary),
                        )
                        .selectable(true)
                        .truncate(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::link_button(ui, "Open").clicked() {
                        state.open_url(&format!("file://{}", path.display()));
                    }
                });
            });
        }
        widgets::caption(ui, "Credentials are written with owner-only permissions. Deleting the cache is always safe.");
    });
}

fn about_section(ui: &mut Ui, state: &mut AppState) {
    widgets::section_header(ui, "About");
    widgets::card(ui, |ui| {
        let pal = theme::palette(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Medley").font(theme::display(20.0)).color(pal.label));
            ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(pal.secondary));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if widgets::link_button(ui, "GitHub").clicked() {
                    state.open_url("https://github.com/martinkopoky-wq/medley");
                }
            });
        });
        widgets::caption(ui, "One fast, native library for Spotify, SoundCloud and Apple Music.");
        widgets::hairline(ui);
        ui.label(RichText::new("Keyboard shortcuts").font(theme::semibold(13.0)));
        let cmd = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" };
        let shortcuts = [
            ("Space".to_owned(), "Play / Pause"),
            (format!("{cmd} + ← / →"), "Previous / Next"),
            (format!("{cmd} + ↑ / ↓"), "Volume up / down"),
            (format!("{cmd} + F"), "Search"),
        ];
        egui::Grid::new("shortcuts").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
            for (keys, action) in shortcuts {
                ui.label(RichText::new(keys).monospace().size(12.0).color(pal.secondary));
                ui.label(action);
                ui.end_row();
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Form helpers (shared with the Accounts panel)
// ---------------------------------------------------------------------------

/// A full-width row: label on the left, controls laid out right-to-left.
pub(super) fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.label(label);
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
}

/// A labeled single-line text field. Returns the new value when edited.
pub(super) fn text_setting(ui: &mut Ui, label: &str, value: &str, hint: &str, width: f32) -> Option<String> {
    let mut text = value.to_owned();
    let mut changed = false;
    row(ui, label, |ui| {
        let pal = theme::palette(ui.ctx());
        let resp = ui.add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(RichText::new(hint).color(pal.tertiary))
                .desired_width(width),
        );
        changed = resp.changed();
    });
    changed.then_some(text)
}

/// A masked field backed by the credential store. Edits are kept locally and
/// written (0600) when the field loses focus, not on every keystroke.
pub(super) fn secret_field(
    ui: &mut Ui,
    creds: &CredentialStore,
    id_salt: &str,
    hint: &str,
    get: impl Fn(&Credentials) -> Option<String>,
    set: impl FnOnce(&mut Credentials, Option<String>),
) -> anyhow::Result<()> {
    let pal = theme::palette(ui.ctx());
    let id = ui.id().with(("secret", id_salt));
    let stored = get(&creds.get()).unwrap_or_default();
    let mut buf = ui.ctx().data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| stored.clone());
    if !stored.trim().is_empty() {
        let resp = ui.label(RichText::new("Saved").font(theme::regular(11.5)).color(pal.success));
        resp.on_hover_text("Stored in credentials.json");
    }
    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .password(true)
            .hint_text(RichText::new(hint).color(pal.tertiary))
            .desired_width(220.0),
    );
    if resp.changed() || resp.has_focus() {
        ui.ctx().data_mut(|d| d.insert_temp(id, buf.clone()));
    }
    if resp.lost_focus() {
        ui.ctx().data_mut(|d| d.remove::<String>(id));
        let trimmed = buf.trim().to_owned();
        if trimmed != stored.trim() {
            let value = (!trimmed.is_empty()).then_some(trimmed);
            creds.update(|c| set(c, value))?;
        }
    }
    Ok(())
}
