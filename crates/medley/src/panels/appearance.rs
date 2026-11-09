//! Appearance: theme, accent, motion and size, plus the layout gallery,
//! saving, sharing and importing layouts.

use crate::app::{AppState, DockOp};
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
    pos2, vec2,
};
use medley_core::layout::{LayoutSpec, PanelKind, builtin_layouts};
use medley_core::store::{Accent, ThemeMode};
use std::sync::LazyLock;

const TILE_W: f32 = 200.0;
const TILE_PREVIEW_H: f32 = 125.0;

static BUILTIN_NAMES: LazyLock<Vec<String>> = LazyLock::new(|| builtin_layouts().into_iter().map(|l| l.name).collect());

fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.iter().any(|n| n == name)
}

enum Intent {
    Apply(LayoutSpec),
    Save(LayoutSpec),
    Delete(String),
}

pub fn ui(ui: &mut Ui, state: &mut AppState) {
    let mut intents = Vec::new();
    crate::panels::scroll(ui, "appearance", |ui| {
        widgets::large_title(ui, "Appearance");
        ui.add_space(4.0);
        look_section(ui, state);
        layout_section(ui, state);
        gallery_section(ui, state, &mut intents);
        import_section(ui, state, &mut intents);
        ui.add_space(12.0);
    });
    for intent in intents {
        match intent {
            Intent::Apply(spec) => {
                let name = spec.name.clone();
                state.apply_layout(spec);
                state.toast(crate::app::ToastKind::Success, format!("Applied “{name}”"));
            }
            Intent::Save(spec) => state.save_layout(spec),
            Intent::Delete(name) => state.delete_layout(&name),
        }
    }
}

// ---------------------------------------------------------------------------
// Look
// ---------------------------------------------------------------------------

fn look_section(ui: &mut Ui, state: &mut AppState) {
    let appearance = state.settings().appearance.clone();
    widgets::section_header(ui, "Look");
    widgets::card(ui, |ui| {
        row(ui, "Theme", |ui| {
            let mut mode = appearance.theme;
            if widgets::segmented(
                ui,
                &mut mode,
                &[(ThemeMode::System, "Auto"), (ThemeMode::Light, "Light"), (ThemeMode::Dark, "Dark")],
            ) {
                state.edit_settings(|s| s.appearance.theme = mode);
            }
        });
        widgets::hairline(ui);
        row(ui, "Accent color", |ui| {
            // Right-to-left layout: draw in reverse so the swatches read left to right.
            for accent in Accent::ALL.iter().rev() {
                if accent_swatch(ui, *accent, *accent == appearance.accent).clicked() && *accent != appearance.accent {
                    let a = *accent;
                    state.edit_settings(|s| s.appearance.accent = a);
                }
            }
        });
        widgets::hairline(ui);
        row(ui, "Interface size", |ui| {
            const SIZES: [(u8, &str); 4] = [(90, "90%"), (100, "100%"), (110, "110%"), (125, "125%")];
            let current = (appearance.ui_scale * 100.0).round();
            let mut pick = SIZES
                .iter()
                .min_by(|a, b| (a.0 as f32 - current).abs().total_cmp(&(b.0 as f32 - current).abs()))
                .map(|s| s.0)
                .unwrap_or(100);
            if widgets::segmented(ui, &mut pick, &SIZES) {
                state.edit_settings(|s| s.appearance.ui_scale = pick as f32 / 100.0);
            }
        });
        widgets::hairline(ui);
        let mut translucent = appearance.translucent;
        if widgets::switch_row(
            ui,
            "Translucent window",
            Some("Lets your desktop's blur show through. Takes effect after restart."),
            &mut translucent,
        ) {
            state.edit_settings(|s| s.appearance.translucent = translucent);
        }
        widgets::hairline(ui);
        let mut reduce = appearance.reduce_motion;
        if widgets::switch_row(ui, "Reduce motion", Some("Turns off springs and animated transitions."), &mut reduce) {
            state.edit_settings(|s| s.appearance.reduce_motion = reduce);
        }
    });
}

fn accent_swatch(ui: &mut Ui, accent: Accent, selected: bool) -> Response {
    let pal = theme::palette(ui.ctx());
    let [r, g, b] = accent.rgb(pal.dark);
    let color = Color32::from_rgb(r, g, b);
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered());
        let c = rect.center();
        if selected {
            ui.painter().circle_stroke(c, 11.0, Stroke::new(2.0, color));
        }
        ui.painter().circle_filled(c, 8.0 + hover, color);
        if selected {
            ui.painter().circle_filled(c, 3.0, Color32::WHITE);
        }
    }
    response.on_hover_text(accent.name())
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

fn layout_section(ui: &mut Ui, state: &mut AppState) {
    let locked = state.settings().layout_locked;
    widgets::section_header(ui, "Layout");
    widgets::card(ui, |ui| {
        let mut lock = locked;
        if widgets::switch_row(ui, "Lock layout", Some("Prevents dragging panels by accident."), &mut lock) {
            state.edit_settings(|s| s.layout_locked = lock);
        }
        widgets::hairline(ui);

        let open_id = ui.id().with("save_layout_open");
        let name_id = ui.id().with("save_layout_name");
        let open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if widgets::pill_button(ui, "Save Current Layout…", false).clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(open_id, !open));
            }
            if widgets::pill_button(ui, "Share Layout", false)
                .on_hover_text("Copies the current arrangement as JSON")
                .clicked()
            {
                state.dock_ops.push(DockOp::CopyCurrent);
            }
        });
        if open {
            let pal = theme::palette(ui.ctx());
            let mut name = ui.ctx().data(|d| d.get_temp::<String>(name_id)).unwrap_or_default();
            let mut submit = false;
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text(RichText::new("Layout name").color(pal.tertiary))
                        .desired_width(220.0),
                );
                if resp.changed() {
                    ui.ctx().data_mut(|d| d.insert_temp(name_id, name.clone()));
                }
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let valid = !name.trim().is_empty();
                let clicked = ui.add_enabled_ui(valid, |ui| widgets::pill_button(ui, "Save", true)).inner.clicked();
                submit = valid && (clicked || enter);
            });
            if submit {
                state.dock_ops.push(DockOp::SaveCurrent(name.trim().to_owned()));
                ui.ctx().data_mut(|d| {
                    d.insert_temp(open_id, false);
                    d.remove::<String>(name_id);
                });
            }
        }
        widgets::caption(
            ui,
            "Drag a panel's tab to move it; drop it on another panel to group them as tabs. Closed panels come back from the layout menu in the toolbar.",
        );
    });
}

fn gallery_section(ui: &mut Ui, state: &AppState, intents: &mut Vec<Intent>) {
    widgets::section_header(ui, "Gallery");
    let spacing = 14.0;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(spacing, spacing);
        for spec in &state.layouts {
            ui.allocate_ui_with_layout(vec2(TILE_W, TILE_PREVIEW_H + 96.0), Layout::top_down(Align::Min), |ui| {
                ui.set_width(TILE_W);
                gallery_tile(ui, spec, intents);
            });
        }
    });
    widgets::caption(ui, "Copy your layout and open a pull request adding it to community/layouts on GitHub.");
}

fn gallery_tile(ui: &mut Ui, spec: &LayoutSpec, intents: &mut Vec<Intent>) {
    let pal = theme::palette(ui.ctx());
    ui.spacing_mut().item_spacing.y = 4.0;
    let preview = layout_preview(ui, spec, vec2(TILE_W, TILE_PREVIEW_H));
    if preview.on_hover_text("Click to apply").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        intents.push(Intent::Apply(spec.clone()));
    }
    ui.add(egui::Label::new(RichText::new(&spec.name).font(theme::semibold(13.5)).color(pal.label)).truncate());
    let meta = match (spec.author.is_empty(), spec.description.is_empty()) {
        (false, false) => format!("{} · {}", spec.author, spec.description),
        (false, true) => format!("by {}", spec.author),
        (true, false) => spec.description.clone(),
        (true, true) => String::new(),
    };
    if !meta.is_empty() {
        let resp =
            ui.add(egui::Label::new(RichText::new(&meta).font(theme::regular(11.5)).color(pal.secondary)).truncate());
        resp.on_hover_text(&meta);
    }
    ui.horizontal(|ui| {
        if widgets::pill_button(ui, "Apply", false).clicked() {
            intents.push(Intent::Apply(spec.clone()));
        }
        if !is_builtin(&spec.name) && widgets::icon_button(ui, Icon::Trash, 26.0, "Delete layout").clicked() {
            intents.push(Intent::Delete(spec.name.clone()));
        }
    });
}

fn import_section(ui: &mut Ui, state: &mut AppState, intents: &mut Vec<Intent>) {
    widgets::section_header(ui, "Import");
    let text_id = ui.id().with("import_text");
    let result_id = ui.id().with("import_result");
    widgets::card(ui, |ui| {
        let pal = theme::palette(ui.ctx());
        widgets::caption(ui, "Paste a layout someone shared with you.");
        let mut text = ui.ctx().data(|d| d.get_temp::<String>(text_id)).unwrap_or_default();
        let resp = ui.add(
            egui::TextEdit::multiline(&mut text)
                .hint_text(RichText::new("{ \"format\": 1, \"name\": … }").color(pal.tertiary))
                .font(egui::TextStyle::Monospace)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        if resp.changed() {
            ui.ctx().data_mut(|d| {
                d.insert_temp(text_id, text.clone());
                d.remove::<Result<LayoutSpec, String>>(result_id);
            });
        }
        ui.add_space(2.0);
        let preview_clicked =
            ui.add_enabled_ui(!text.trim().is_empty(), |ui| widgets::pill_button(ui, "Preview", false)).inner.clicked();
        if preview_clicked {
            let result = state.import_layout(&text);
            ui.ctx().data_mut(|d| d.insert_temp(result_id, result));
        }
        match ui.ctx().data(|d| d.get_temp::<Result<LayoutSpec, String>>(result_id)) {
            Some(Ok(spec)) => {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    layout_preview(ui, &spec, vec2(TILE_W, TILE_PREVIEW_H));
                    ui.vertical(|ui| {
                        widgets::title(ui, &spec.name);
                        if !spec.author.is_empty() {
                            widgets::secondary(ui, &format!("by {}", spec.author));
                        }
                        if !spec.description.is_empty() {
                            widgets::caption(ui, &spec.description);
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if widgets::pill_button(ui, "Apply", true).clicked() {
                                intents.push(Intent::Apply(spec.clone()));
                            }
                            if widgets::pill_button(ui, "Save to Gallery", false).clicked() {
                                intents.push(Intent::Save(spec.clone()));
                            }
                        });
                    });
                });
            }
            Some(Err(e)) => {
                ui.add(egui::Label::new(RichText::new(e).font(theme::regular(11.5)).color(pal.danger)).wrap());
            }
            None => {}
        }
    });
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn row(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.label(label);
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
}

/// A miniature of a layout: each area as a rounded card labeled with its
/// panels, the main content area tinted with the layout's accent.
pub fn layout_preview(ui: &mut Ui, spec: &LayoutSpec, size: Vec2) -> Response {
    let pal = theme::palette(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let accent = spec
        .accent
        .map(|a| {
            let [r, g, b] = a.rgb(pal.dark);
            Color32::from_rgb(r, g, b)
        })
        .unwrap_or(pal.accent);
    let painter = ui.painter().with_clip_rect(rect);
    let frame_r = CornerRadius::same(theme::R_CONTROL + 2);
    let window = if pal.dark { Color32::from_rgb(0x16, 0x16, 0x18) } else { Color32::from_rgb(0xE9, 0xE9, 0xEE) };
    let card = if pal.dark { Color32::from_rgb(0x2A, 0x2A, 0x2E) } else { Color32::WHITE };
    painter.rect_filled(rect, frame_r, window);

    // A thin toolbar strip, like the real window.
    let bar = Rect::from_min_size(rect.min, vec2(rect.width(), (rect.height() * 0.1).max(8.0)));
    painter.circle_filled(pos2(bar.center().x, bar.center().y), 2.2, pal.tertiary);
    painter.rect_filled(
        Rect::from_center_size(bar.center(), vec2(rect.width() * 0.32, bar.height() * 0.5)),
        CornerRadius::same(3),
        if pal.dark { Color32::from_rgb(0x26, 0x26, 0x29) } else { Color32::from_rgb(0xF7, 0xF7, 0xF9) },
    );

    let area = Rect::from_min_max(pos2(rect.left() + 4.0, bar.bottom()), pos2(rect.right() - 4.0, rect.bottom() - 4.0));
    let gap = 3.0;
    let font_size = (size.y / 125.0 * 9.5).clamp(7.5, 11.0);
    for ([x, y, w, h], panels) in spec.root.leaf_rects() {
        let r = Rect::from_min_size(
            pos2(area.left() + x * area.width(), area.top() + y * area.height()),
            vec2(w * area.width(), h * area.height()),
        )
        .shrink(gap / 2.0);
        if r.width() < 2.0 || r.height() < 2.0 {
            continue;
        }
        let main = panels.contains(&PanelKind::Collection);
        let cr = CornerRadius::same(4);
        painter.rect_filled(r, cr, card);
        if main {
            painter.rect_filled(r, cr, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 34));
        }
        // Tab dots for grouped panels.
        if panels.len() > 1 {
            for i in 0..panels.len().min(6) {
                let c = pos2(r.left() + 6.0 + i as f32 * 5.0, r.top() + 5.0);
                painter.circle_filled(c, 1.6, if i == 0 { accent } else { pal.tertiary });
            }
        }
        let label = match panels.len() {
            0 => String::new(),
            1 => panels[0].title().to_owned(),
            n => format!("{} +{}", panels[0].title(), n - 1),
        };
        let color = if main { accent } else { pal.secondary };
        let mut job = egui::text::LayoutJob::simple_singleline(label, theme::semibold(font_size), color);
        job.wrap = egui::text::TextWrapping::truncate_at_width((r.width() - 6.0).max(4.0));
        let galley = painter.layout_job(job);
        let pos = Align2::CENTER_CENTER.anchor_size(r.center(), galley.size()).min;
        painter.with_clip_rect(r).galley(pos, galley, color);
    }

    let hover = theme::fade(ui.ctx(), response.id.with("h"), response.hovered());
    let edge = if hover > 0.0 {
        Stroke::new(1.5, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), (255.0 * hover) as u8))
    } else {
        Stroke::new(1.0, pal.separator)
    };
    painter.rect_stroke(rect, frame_r, edge, StrokeKind::Inside);
    response
}
