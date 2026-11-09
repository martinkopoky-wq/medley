//! Window chrome: the Music.app-style toolbar (transport, "LCD", volume,
//! layout menu), keyboard shortcuts, and toast notifications.

use crate::app::{AppState, DockOp, ToastKind};
use crate::dock::{self, Dock};
use crate::player::{Repeat, Status};
use crate::theme;
use crate::widgets::{self, Icon};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Key, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use medley_core::format_duration;
use medley_core::layout::PanelKind;

const TOOLBAR_HEIGHT: f32 = 64.0;

pub fn toolbar(ui: &mut Ui, state: &mut AppState, dock: &mut Dock) {
    shortcuts(ui.ctx(), state);
    let pal = theme::palette(ui.ctx());
    egui::Panel::top("medley.toolbar")
        .exact_size(TOOLBAR_HEIGHT)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(pal.window).inner_margin(egui::Margin::symmetric(16, 0)))
        .show(ui, |ui| {
            let full = ui.max_rect();

            // Left: transport.
            let left = Rect::from_min_size(full.min, vec2(220.0, full.height()));
            let mut l = ui.new_child(
                egui::UiBuilder::new().max_rect(left).layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            transport(&mut l, state);

            // Right: volume + layout menu.
            let right = Rect::from_min_max(pos2(full.right() - 200.0, full.top()), full.max);
            let mut r = ui.new_child(
                egui::UiBuilder::new().max_rect(right).layout(egui::Layout::right_to_left(egui::Align::Center)),
            );
            layout_menu(&mut r, state, dock);
            r.add_space(6.0);
            let mut volume = state.volume();
            r.allocate_ui_with_layout(vec2(130.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if widgets::volume_slider(ui, &mut volume, 90.0) {
                    state.set_volume(volume);
                }
            });

            // Center: the LCD.
            let space = (full.width() - 2.0 * 230.0).max(220.0);
            let lcd_w = space.min(560.0);
            let lcd = Rect::from_center_size(full.center(), vec2(lcd_w, 50.0));
            lcd_display(ui, state, lcd);
        });
}

fn transport(ui: &mut Ui, state: &mut AppState) {
    let pal = theme::palette(ui.ctx());
    ui.spacing_mut().item_spacing.x = 6.0;
    let shuffle_tint = state.player.shuffle.then_some(pal.accent);
    if widgets::icon_button_colored(ui, Icon::Shuffle, 28.0, "Shuffle", shuffle_tint).clicked() {
        let on = !state.player.shuffle;
        state.player.set_shuffle(on);
    }
    let has_queue = state.player.current().is_some();
    ui.add_enabled_ui(has_queue, |ui| {
        if widgets::icon_button(ui, Icon::Previous, 32.0, "Previous").clicked() {
            state.previous();
        }
    });
    let playing = state.is_playing();
    let loading = state.status == Status::Loading;
    if widgets::play_button(ui, playing, loading, 38.0).clicked() {
        state.toggle_play();
    }
    ui.add_enabled_ui(has_queue, |ui| {
        if widgets::icon_button(ui, Icon::Next, 32.0, "Next").clicked() {
            state.next();
        }
    });
    let (icon, tint, tip) = match state.player.repeat {
        Repeat::Off => (Icon::Repeat, None, "Repeat"),
        Repeat::All => (Icon::Repeat, Some(pal.accent), "Repeat All"),
        Repeat::One => (Icon::RepeatOne, Some(pal.accent), "Repeat One"),
    };
    if widgets::icon_button_colored(ui, icon, 28.0, tip, tint).clicked() {
        state.player.repeat = state.player.repeat.cycle();
    }
}

fn lcd_display(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let pal = theme::palette(ui.ctx());
    let fill = if pal.dark { Color32::from_rgb(0x26, 0x26, 0x29) } else { Color32::from_rgb(0xF7, 0xF7, 0xF9) };
    ui.painter().rect_filled(rect, CornerRadius::same(theme::R_CONTROL + 2), fill);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(theme::R_CONTROL + 2),
        Stroke::new(1.0, pal.separator),
        StrokeKind::Inside,
    );

    let Some(now) = &state.now else {
        let (r, _) = (rect.shrink(6.0), ());
        widgets::paint_icon(
            ui.painter(),
            Rect::from_center_size(r.center(), vec2(22.0, 22.0)),
            Icon::Note,
            pal.tertiary,
        );
        return;
    };
    let track = now.track.clone();
    let art = Rect::from_min_size(rect.min, vec2(rect.height(), rect.height()));
    widgets::paint_artwork(ui, art, track.artwork.as_deref(), theme::R_CONTROL + 2, &track.key());

    let text_left = art.right() + 10.0;
    let text_right = rect.right() - 10.0;
    let center_x = (text_left + text_right) / 2.0;
    let max_w = text_right - text_left;
    let title = ui.painter().layout(track.title.clone(), theme::semibold(12.5), pal.label, f32::INFINITY);
    let title = if title.size().x > max_w {
        ui.painter().layout_job(single_line(&track.title, theme::semibold(12.5), pal.label, max_w))
    } else {
        title
    };
    ui.painter().galley(pos2(center_x - title.size().x / 2.0, rect.top() + 6.0), title, pal.label);

    let mut sub = track.artist_line();
    if let Some(album) = track.album.as_deref().filter(|a| !a.is_empty()) {
        sub = format!("{sub} — {album}");
    }
    let sub_g = ui.painter().layout_job(single_line(&sub, theme::regular(11.5), pal.secondary, max_w - 20.0));
    let sub_y = rect.top() + 22.0;
    let dot_x = center_x - sub_g.size().x / 2.0 - 8.0;
    ui.painter().circle_filled(pos2(dot_x, sub_y + sub_g.size().y / 2.0), 3.0, theme::provider_color(track.provider));
    ui.painter().galley(pos2(center_x - sub_g.size().x / 2.0, sub_y), sub_g, pal.secondary);

    // Progress: elapsed · bar · remaining.
    let pos = state.position_ms();
    let dur = state.duration_ms();
    let bar_rect =
        Rect::from_min_max(pos2(text_left + 36.0, rect.bottom() - 15.0), pos2(text_right - 36.0, rect.bottom() - 1.0));
    let font = theme::regular(10.0);
    ui.painter().text(
        pos2(text_left, bar_rect.center().y),
        Align2::LEFT_CENTER,
        format_duration(pos),
        font.clone(),
        pal.secondary,
    );
    let remaining = format!("-{}", format_duration(dur.saturating_sub(pos)));
    ui.painter().text(pos2(text_right, bar_rect.center().y), Align2::RIGHT_CENTER, remaining, font, pal.secondary);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
    if let Some(target) = widgets::scrubber(&mut child, Id::new("medley.lcd.scrub"), pos, dur, bar_rect.width()) {
        state.seek(target);
    }

    let hit = ui.interact(
        Rect::from_min_max(art.min, pos2(rect.right(), rect.bottom() - 16.0)),
        Id::new("medley.lcd.open"),
        Sense::click(),
    );
    if hit.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        state.focus_panel(PanelKind::NowPlaying);
    }
}

fn single_line(text: &str, font: egui::FontId, color: Color32, max_width: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(10.0));
    job
}

fn layout_menu(ui: &mut Ui, state: &mut AppState, dock: &mut Dock) {
    let locked = state.settings().layout_locked;
    let resp = widgets::icon_button(ui, Icon::Layout, 30.0, "Layout");
    egui::Popup::menu(&resp).width(220.0).show(|ui| {
        let pal = theme::palette(ui.ctx());
        ui.label(RichText::new("Panels").font(theme::semibold(11.5)).color(pal.secondary));
        for panel in PanelKind::ALL {
            let open = dock::contains(dock, panel);
            let label = if open { format!("✓  {}", panel.title()) } else { format!("    {}", panel.title()) };
            if ui.button(label).clicked() {
                state.dock_ops.push(DockOp::Focus(panel));
                ui.close();
            }
        }
        ui.separator();
        let lock_label = if locked { "Unlock Layout" } else { "Lock Layout" };
        if ui.button(lock_label).clicked() {
            state.edit_settings(|s| s.layout_locked = !locked);
            ui.close();
        }
        if ui.button("Layout Gallery…").clicked() {
            state.dock_ops.push(DockOp::Focus(PanelKind::Appearance));
            ui.close();
        }
        if ui.button("Copy Layout as JSON").clicked() {
            state.dock_ops.push(DockOp::CopyCurrent);
            ui.close();
        }
        if ui.button("Reset to Classic").clicked() {
            if let Some(spec) = state.layouts.first().cloned() {
                state.dock_ops.push(DockOp::Apply(spec));
            }
            ui.close();
        }
    });
}

fn shortcuts(ctx: &egui::Context, state: &mut AppState) {
    let typing = ctx.memory(|m| m.focused().is_some());
    let (space, next, prev, find, up, down) = ctx.input(|i| {
        let cmd = i.modifiers.command;
        (
            i.key_pressed(Key::Space) && !typing,
            cmd && i.key_pressed(Key::ArrowRight),
            cmd && i.key_pressed(Key::ArrowLeft),
            cmd && i.key_pressed(Key::F),
            cmd && i.key_pressed(Key::ArrowUp),
            cmd && i.key_pressed(Key::ArrowDown),
        )
    });
    if space {
        state.toggle_play();
    }
    if next {
        state.next();
    }
    if prev {
        state.previous();
    }
    if find {
        state.focus_panel(PanelKind::Search);
    }
    if up || down {
        let v = (state.volume() + if up { 0.06 } else { -0.06 }).clamp(0.0, 1.0);
        state.set_volume(v);
    }
}

/// HUD-style toasts at the bottom center.
pub fn toasts(ctx: &egui::Context, state: &mut AppState) {
    if state.toasts.is_empty() {
        return;
    }
    let pal = theme::palette(ctx);
    let screen = ctx.content_rect();
    let mut y = screen.bottom() - 24.0;
    let mut dismiss = None;
    for toast in state.toasts.iter().rev() {
        let age = toast.born.elapsed().as_secs_f32();
        let left = toast.lifetime().as_secs_f32() - age;
        let appear = theme::spring(ctx, Id::new(("toast", toast.id)), 1.0);
        let fade = (left / 0.3).clamp(0.0, 1.0);
        let opacity = (appear.clamp(0.0, 1.0) * fade).clamp(0.0, 1.0);
        let font = theme::regular(13.0);
        let galley = ctx.fonts_mut(|f| f.layout(toast.text.clone(), font, pal.label, 460.0));
        let size = vec2(galley.size().x + 52.0, galley.size().y + 20.0);
        let offset = (1.0 - appear) * 16.0;
        let rect = Rect::from_center_size(pos2(screen.center().x, y - size.y / 2.0 + offset), size);
        y -= size.y + 8.0;
        let area = egui::Area::new(Id::new(("toast-area", toast.id)))
            .order(egui::Order::Tooltip)
            .fixed_pos(rect.min)
            .interactable(true);
        area.show(ctx, |ui| {
            ui.set_opacity(opacity);
            let (r, resp) = ui.allocate_exact_size(size, Sense::click());
            let shadow = egui::Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(if pal.dark { 120 } else { 40 }),
            };
            ui.painter().add(shadow.as_shape(r, CornerRadius::same(255)));
            ui.painter().rect_filled(r, CornerRadius::same(255), pal.elevated);
            ui.painter().rect_stroke(r, CornerRadius::same(255), Stroke::new(1.0, pal.separator), StrokeKind::Inside);
            let dot = match toast.kind {
                ToastKind::Info => pal.secondary,
                ToastKind::Success => pal.success,
                ToastKind::Error => pal.danger,
            };
            let icon = match toast.kind {
                ToastKind::Error => Icon::Close,
                _ => Icon::Check,
            };
            let c = pos2(r.left() + 20.0, r.center().y);
            ui.painter().circle_filled(c, 9.0, dot);
            widgets::paint_icon(ui.painter(), Rect::from_center_size(c, vec2(11.0, 11.0)), icon, Color32::WHITE);
            ui.painter().galley(pos2(r.left() + 36.0, r.center().y - galley.size().y / 2.0), galley, pal.label);
            if resp.clicked() {
                dismiss = Some(toast.id);
            }
        });
    }
    if let Some(id) = dismiss {
        state.toasts.retain(|t| t.id != id);
    }
}
