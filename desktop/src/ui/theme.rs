//! The Android design system's tokens, applied to egui. Three greens and
//! everything descends from them; three radii; three button heights.
//!
//! These are the same numbers as `android/.../Ui.kt`, in the same order, so a
//! colour can be changed on both sides by looking at one line in each. The
//! widgets in `widgets.rs` read them directly rather than going through
//! egui's style, which only decides what the handful of stock widgets — the
//! scroll bars, the text cursor — look like.

use egui::{Color32, CornerRadius, Stroke};

pub const BACKGROUND: Color32 = Color32::from_rgb(0x11, 0x18, 0x13);
pub const SURFACE: Color32 = Color32::from_rgb(0x1d, 0x28, 0x20);
pub const SURFACE_HIGH: Color32 = Color32::from_rgb(0x24, 0x31, 0x28);
pub const ON_SURFACE: Color32 = Color32::from_rgb(0xed, 0xf4, 0xe9);
pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(0xb3, 0xc4, 0xad);
pub const OUTLINE: Color32 = Color32::from_rgb(0x6d, 0x7f, 0x68);
pub const LEAF: Color32 = Color32::from_rgb(0xb9, 0xe3, 0x8c);
pub const ON_LEAF: Color32 = Color32::from_rgb(0x17, 0x30, 0x0c);
pub const RAISED: Color32 = Color32::from_rgb(0x2a, 0x3a, 0x2e);
pub const ON_RAISED: Color32 = Color32::from_rgb(0xc3, 0xd1, 0xbd);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0xb4, 0xa6);
pub const ON_ERROR: Color32 = Color32::from_rgb(0x5f, 0x14, 0x09);
pub const AMBER: Color32 = Color32::from_rgb(0xf0, 0xd4, 0x9a);
pub const ON_TIME: Color32 = Color32::from_rgb(0x15, 0x22, 0x10);
pub const NOTICE_BACK: Color32 = Color32::from_rgb(0x2b, 0x24, 0x10);
pub const WELL: Color32 = Color32::from_rgb(0x14, 0x1d, 0x17);
pub const WELL_EDGE: Color32 = Color32::from_rgb(0x3b, 0x4d, 0x3e);
pub const WELL_MARK: Color32 = Color32::from_rgb(0x5f, 0x73, 0x59);
pub const KEY_DIM: Color32 = Color32::from_rgb(0x1b, 0x24, 0x1d);
pub const KEY_DIM_GLYPH: Color32 = Color32::from_rgb(0x54, 0x65, 0x4f);
pub const CHROME: Color32 = Color32::from_rgba_premultiplied(0x0a, 0x12, 0x0c, 0xcc);
pub const SCRIM: Color32 = Color32::from_rgba_premultiplied(0x0a, 0x12, 0x0c, 0xd9);
pub const COVER_TOP: Color32 = Color32::from_rgb(0x3b, 0x5a, 0x43);
pub const COVER_BOTTOM: Color32 = Color32::from_rgb(0x21, 0x30, 0x1f);

pub const CORNER_SMALL: f32 = 14.0;
pub const CORNER_MEDIUM: f32 = 20.0;
pub const CORNER_LARGE: f32 = 28.0;
pub const PRIMARY_HEIGHT: f32 = 60.0;
pub const SECONDARY_HEIGHT: f32 = 52.0;
pub const QUIET_HEIGHT: f32 = 46.0;
pub const PANEL_WIDTH: f32 = 620.0;

/// Paints egui in the app's colours. egui 0.36 keeps a style per theme and
/// picks between them from the system preference, so both are written and the
/// preference is pinned to dark: this shell is one palette, and a light
/// desktop must not repaint half of it.
pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(style);
}

fn style(style: &mut egui::Style) {
    let v = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(ON_SURFACE);
    v.panel_fill = BACKGROUND;
    v.window_fill = SURFACE;
    v.extreme_bg_color = WELL;
    v.faint_bg_color = SURFACE_HIGH;
    v.window_corner_radius = CornerRadius::same(CORNER_LARGE as u8);
    v.window_stroke = Stroke::NONE;
    v.selection.bg_fill = LEAF;
    v.selection.stroke = Stroke::new(1.0, ON_LEAF);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.bg_fill = RAISED;
        w.weak_bg_fill = SURFACE_HIGH;
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = CornerRadius::same(CORNER_SMALL as u8);
        w.fg_stroke = Stroke::new(1.0, ON_RAISED);
    }
    v.widgets.hovered.bg_fill = SURFACE_HIGH;
    v.widgets.active.bg_fill = LEAF;
    v.widgets.active.fg_stroke = Stroke::new(1.0, ON_LEAF);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
}
