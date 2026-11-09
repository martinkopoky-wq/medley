//! Apple-style look: system colors, Inter as the SF stand-in, continuous
//! corners, hairline separators, floating cards, and spring motion.

use eframe::egui::{
    self, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Id, Margin, Shadow, Stroke,
    TextStyle, Theme, Vec2,
};
use medley_core::ProviderKind;
use medley_core::store::{Accent, Appearance, ThemeMode};
use std::sync::Arc;

pub const SEMIBOLD: &str = "semibold";
pub const DISPLAY: &str = "display";

/// Corner radii, in points.
pub const R_SMALL: u8 = 6;
pub const R_CONTROL: u8 = 8;
pub const R_CARD: u8 = 12;
pub const R_ART: u8 = 6;

/// Semantic colors for the current theme. Read it with [`palette`].
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Behind the cards (the window itself).
    pub window: Color32,
    /// Panel bodies.
    pub card: Color32,
    /// Popovers, menus, the toolbar "LCD".
    pub elevated: Color32,
    pub label: Color32,
    pub secondary: Color32,
    pub tertiary: Color32,
    pub separator: Color32,
    /// Control fill (buttons, fields).
    pub fill: Color32,
    pub fill_strong: Color32,
    /// Row hover.
    pub hover: Color32,
    pub accent: Color32,
    /// Accent at low alpha, for selected rows.
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub success: Color32,
}

#[derive(Clone, Copy)]
struct ThemeConfig {
    accent: Accent,
    translucent: bool,
}

fn config_id() -> Id {
    Id::new("medley.theme.config")
}

/// The palette for whatever theme egui is showing this frame.
pub fn palette(ctx: &Context) -> Palette {
    let cfg = ctx
        .data(|d| d.get_temp::<ThemeConfig>(config_id()))
        .unwrap_or(ThemeConfig { accent: Accent::Pink, translucent: false });
    build_palette(ctx.theme() == Theme::Dark, cfg.accent, cfg.translucent)
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn build_palette(dark: bool, accent: Accent, translucent: bool) -> Palette {
    let accent_c = rgb(accent.rgb(dark));
    let alpha = |c: Color32, a: u8| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    let (mut window, mut card) = if dark {
        (Color32::from_rgb(0x16, 0x16, 0x18), Color32::from_rgb(0x1F, 0x1F, 0x22))
    } else {
        (Color32::from_rgb(0xE9, 0xE9, 0xEE), Color32::from_rgb(0xFF, 0xFF, 0xFF))
    };
    if translucent {
        window = alpha(window, 150);
        card = alpha(card, 225);
    }
    if dark {
        Palette {
            dark,
            window,
            card,
            elevated: Color32::from_rgb(0x2C, 0x2C, 0x2E),
            label: Color32::from_rgb(0xF5, 0xF5, 0xF7),
            secondary: Color32::from_rgb(0x98, 0x98, 0x9D),
            tertiary: Color32::from_rgb(0x63, 0x63, 0x66),
            separator: Color32::from_rgb(0x38, 0x38, 0x3A),
            fill: Color32::from_rgba_unmultiplied(118, 118, 128, 60),
            fill_strong: Color32::from_rgba_unmultiplied(118, 118, 128, 95),
            hover: Color32::from_rgba_unmultiplied(255, 255, 255, 14),
            accent: accent_c,
            accent_soft: alpha(accent_c, 48),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(0xFF, 0x45, 0x3A),
            success: Color32::from_rgb(0x30, 0xD1, 0x58),
        }
    } else {
        Palette {
            dark,
            window,
            card,
            elevated: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            label: Color32::from_rgb(0x1D, 0x1D, 0x1F),
            secondary: Color32::from_rgb(0x6E, 0x6E, 0x73),
            tertiary: Color32::from_rgb(0xAE, 0xAE, 0xB2),
            separator: Color32::from_rgb(0xD8, 0xD8, 0xDC),
            fill: Color32::from_rgba_unmultiplied(118, 118, 128, 30),
            fill_strong: Color32::from_rgba_unmultiplied(118, 118, 128, 55),
            hover: Color32::from_rgba_unmultiplied(0, 0, 0, 10),
            accent: accent_c,
            accent_soft: alpha(accent_c, 36),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(0xFF, 0x3B, 0x30),
            success: Color32::from_rgb(0x34, 0xC7, 0x59),
        }
    }
}

/// Brand dot colors, used sparingly next to tracks and in the library.
pub fn provider_color(kind: ProviderKind) -> Color32 {
    match kind {
        ProviderKind::Spotify => Color32::from_rgb(0x1E, 0xD7, 0x60),
        ProviderKind::SoundCloud => Color32::from_rgb(0xFF, 0x55, 0x00),
        ProviderKind::AppleMusic => Color32::from_rgb(0xFA, 0x2D, 0x48),
    }
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "Inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "Inter-SemiBold", include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "InterDisplay-Bold", include_bytes!("../../../assets/fonts/InterDisplay-Bold.ttf"));

    // Keep egui's bundled fonts behind Inter for emoji / symbol fallback.
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "Inter".to_owned());
    let mut semibold = vec!["Inter-SemiBold".to_owned()];
    semibold.extend(fallbacks.iter().cloned());
    let mut display = vec!["InterDisplay-Bold".to_owned()];
    display.extend(fallbacks);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semibold);
    fonts.families.insert(FontFamily::Name(DISPLAY.into()), display);
    ctx.set_fonts(fonts);
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}

// ---------------------------------------------------------------------------
// Applying the theme
// ---------------------------------------------------------------------------

/// Push appearance settings into egui. Cheap; call whenever they change.
pub fn apply(ctx: &Context, appearance: &Appearance) {
    ctx.data_mut(|d| {
        d.insert_temp(config_id(), ThemeConfig { accent: appearance.accent, translucent: appearance.translucent })
    });
    ctx.set_theme(match appearance.theme {
        ThemeMode::System => egui::ThemePreference::System,
        ThemeMode::Light => egui::ThemePreference::Light,
        ThemeMode::Dark => egui::ThemePreference::Dark,
    });
    let scale = if appearance.ui_scale.is_finite() { appearance.ui_scale.clamp(0.75, 1.75) } else { 1.0 };
    ctx.set_zoom_factor(scale);
    for theme in [Theme::Light, Theme::Dark] {
        let pal = build_palette(theme == Theme::Dark, appearance.accent, appearance.translucent);
        ctx.style_mut_of(theme, |style| style_for(style, &pal, appearance.reduce_motion));
    }
}

fn style_for(style: &mut egui::Style, pal: &Palette, reduce_motion: bool) {
    use egui::style::ScrollStyle;
    style.text_styles = [
        (TextStyle::Small, regular(11.0)),
        (TextStyle::Body, regular(13.5)),
        (TextStyle::Button, regular(13.5)),
        (TextStyle::Heading, display(22.0)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();
    style.animation_time = if reduce_motion { 0.0 } else { 0.18 };

    let sp = &mut style.spacing;
    sp.item_spacing = Vec2::new(8.0, 6.0);
    sp.button_padding = Vec2::new(12.0, 5.0);
    sp.interact_size.y = 26.0;
    sp.menu_margin = Margin::same(6);
    sp.window_margin = Margin::same(12);
    sp.icon_spacing = 6.0;
    sp.slider_rail_height = 4.0;
    sp.combo_height = 300.0;
    sp.scroll = ScrollStyle::floating();
    sp.scroll.bar_width = 8.0;

    let v = &mut style.visuals;
    v.dark_mode = pal.dark;
    v.panel_fill = pal.window;
    v.window_fill = pal.elevated;
    v.window_stroke = Stroke::new(1.0, pal.separator);
    v.window_corner_radius = CornerRadius::same(R_CARD);
    v.menu_corner_radius = CornerRadius::same(R_CONTROL + 2);
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 34,
        spread: 0,
        color: Color32::from_black_alpha(if pal.dark { 120 } else { 40 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 22,
        spread: 0,
        color: Color32::from_black_alpha(if pal.dark { 110 } else { 32 }),
    };
    v.faint_bg_color = pal.hover;
    v.extreme_bg_color = pal.fill;
    v.text_edit_bg_color = Some(pal.fill);
    v.code_bg_color = pal.fill;
    v.hyperlink_color = pal.accent;
    v.warn_fg_color = Color32::from_rgb(0xFF, 0x9F, 0x0A);
    v.error_fg_color = pal.danger;
    v.weak_text_color = Some(pal.secondary);
    v.override_text_color = None;
    v.button_frame = true;
    v.collapsing_header_frame = false;
    v.indent_has_left_vline = false;
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.image_loading_spinners = false;

    v.selection.bg_fill = Color32::from_rgba_unmultiplied(pal.accent.r(), pal.accent.g(), pal.accent.b(), 90);
    v.selection.stroke = Stroke::new(1.5, pal.accent);

    let radius = CornerRadius::same(R_CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = pal.card;
    w.noninteractive.weak_bg_fill = pal.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0, pal.separator);
    w.noninteractive.fg_stroke = Stroke::new(1.0, pal.label);
    w.noninteractive.corner_radius = radius;
    for (wv, fill, fg) in [
        (&mut w.inactive, pal.fill, pal.label),
        (&mut w.hovered, pal.fill_strong, pal.label),
        (&mut w.active, pal.fill_strong, pal.label),
        (&mut w.open, pal.fill_strong, pal.label),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.bg_stroke = Stroke::NONE;
        wv.fg_stroke = Stroke::new(1.5, fg);
        wv.corner_radius = radius;
        wv.expansion = 0.0;
    }
}

/// Floating rounded cards with gaps between them, segmented-control tabs.
pub fn dock_style(ctx: &Context, locked: bool) -> egui_dock::Style {
    let pal = palette(ctx);
    let mut s = egui_dock::Style::from_egui(&ctx.global_style());
    let card_r = R_CARD;
    s.dock_area_padding = Some(Margin::same(0));
    s.main_surface_border_stroke = Stroke::NONE;
    s.main_surface_border_rounding = CornerRadius::ZERO;

    s.separator.width = 8.0;
    s.separator.extra_interact_width = 2.0;
    s.separator.extra = 120.0;
    s.separator.color_idle = Color32::TRANSPARENT;
    s.separator.color_hovered = if locked { Color32::TRANSPARENT } else { pal.fill_strong };
    s.separator.color_dragged = pal.accent_soft;

    s.tab_bar.bg_fill = pal.card;
    s.tab_bar.height = 34.0;
    s.tab_bar.inner_margin = Margin { left: 6, right: 6, top: 4, bottom: 2 };
    s.tab_bar.corner_radius = CornerRadius { nw: card_r, ne: card_r, sw: 0, se: 0 };
    s.tab_bar.hline_color = Color32::TRANSPARENT;
    s.tab_bar.fill_tab_bar = true;
    s.tab_bar.show_scroll_bar_on_overflow = false;

    let tab = |bg: Color32, text: Color32| egui_dock::TabInteractionStyle {
        outline_color: Color32::TRANSPARENT,
        corner_radius: CornerRadius::same(R_SMALL + 1),
        bg_fill: bg,
        text_color: text,
    };
    s.tab.active = tab(pal.fill, pal.label);
    s.tab.focused = tab(pal.fill, pal.label);
    s.tab.active_with_kb_focus = tab(pal.fill, pal.label);
    s.tab.focused_with_kb_focus = tab(pal.fill, pal.label);
    s.tab.inactive = tab(Color32::TRANSPARENT, pal.secondary);
    s.tab.inactive_with_kb_focus = tab(Color32::TRANSPARENT, pal.secondary);
    s.tab.hovered = tab(pal.hover, pal.label);
    s.tab.hline_below_active_tab_name = false;
    s.tab.spacing = 2.0;
    s.tab.minimum_width = Some(40.0);
    s.tab.tab_body.bg_fill = pal.card;
    s.tab.tab_body.stroke = Stroke::NONE;
    s.tab.tab_body.corner_radius = CornerRadius { nw: 0, ne: 0, sw: card_r, se: card_r };
    s.tab.tab_body.inner_margin = Margin { left: 14, right: 14, top: 8, bottom: 12 };

    s.buttons.close_tab_color = pal.tertiary;
    s.buttons.close_tab_active_color = pal.label;
    s.buttons.close_tab_bg_fill = pal.fill;
    s.buttons.add_tab_color = pal.secondary;
    s.buttons.add_tab_active_color = pal.label;
    s.buttons.add_tab_bg_fill = pal.fill;

    s.overlay.selection_color = pal.accent_soft;
    s.overlay.button_color = pal.accent;
    s.overlay.button_border_stroke = Stroke::new(1.0, pal.accent);
    s.overlay.hovered_leaf_highlight.color = pal.accent_soft;
    s.overlay.hovered_leaf_highlight.corner_radius = CornerRadius::same(card_r);
    s.overlay.hovered_leaf_highlight.stroke = Stroke::new(1.5, pal.accent);
    s
}

// ---------------------------------------------------------------------------
// Motion
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct SpringState {
    value: f32,
    velocity: f32,
    init: bool,
}

fn reduce_motion(ctx: &Context) -> bool {
    ctx.global_style().animation_time <= 0.0
}

/// A critically-damped-ish spring toward `target` (Apple's default feel:
/// ~0.35 s response, slight overshoot only on large moves). Interruptible:
/// a new target simply continues from the current value and velocity.
pub fn spring(ctx: &Context, id: Id, target: f32) -> f32 {
    spring_with(ctx, id, target, 0.35, 0.86)
}

pub fn spring_with(ctx: &Context, id: Id, target: f32, response: f32, damping_ratio: f32) -> f32 {
    if reduce_motion(ctx) {
        ctx.data_mut(|d| d.insert_temp(id, SpringState { value: target, velocity: 0.0, init: true }));
        return target;
    }
    let dt = ctx.input(|i| i.stable_dt).clamp(1.0 / 240.0, 1.0 / 30.0);
    let mut st = ctx.data_mut(|d| *d.get_temp_mut_or_default::<SpringState>(id));
    if !st.init {
        st = SpringState { value: target, velocity: 0.0, init: true };
    }
    let omega = std::f32::consts::TAU / response.max(0.05);
    let k = omega * omega;
    let c = 2.0 * damping_ratio * omega;
    // Semi-implicit Euler in a few sub-steps for stability.
    let steps = 4;
    let h = dt / steps as f32;
    for _ in 0..steps {
        let force = -k * (st.value - target) - c * st.velocity;
        st.velocity += force * h;
        st.value += st.velocity * h;
    }
    let settled = (st.value - target).abs() < 0.001 && st.velocity.abs() < 0.01;
    if settled {
        st.value = target;
        st.velocity = 0.0;
    } else {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(id, st));
    st.value
}

/// Ease 0→1 for hover/visibility (no overshoot).
pub fn fade(ctx: &Context, id: Id, on: bool) -> f32 {
    ctx.animate_bool_with_time(id, on, if reduce_motion(ctx) { 0.0 } else { 0.15 })
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_differ_by_theme() {
        let l = build_palette(false, Accent::Blue, false);
        let d = build_palette(true, Accent::Blue, false);
        assert_ne!(l.card, d.card);
        assert_eq!(l.accent, Color32::from_rgb(0x00, 0x7A, 0xFF));
        assert_eq!(d.accent, Color32::from_rgb(0x0A, 0x84, 0xFF));
    }

    #[test]
    fn translucent_window_has_alpha() {
        assert!(build_palette(true, Accent::Pink, true).window.a() < 255);
    }
}
