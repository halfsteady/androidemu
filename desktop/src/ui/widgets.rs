//! The shell's handful of controls, drawn to the Android tokens. Each is a
//! plain function that allocates a rect, paints it and returns the response,
//! so every screen is built from the same few shapes.
//!
//! Nothing here keeps state between frames. A widget is given what it needs
//! and hands back what happened — a `Response`, a chosen value, a bool — and
//! the screen above it decides what that means. That is what lets the panels
//! be pure functions of `App`.

use super::theme::*;
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Response, Sense, Stroke, Ui, Vec2};

/// How wide a line of text is, for the controls that hug their label.
fn text_width(ui: &Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), FontId::proportional(size), ON_RAISED)
        .size()
        .x
}

/// Every button in the shell: a rounded rect, a centred label, and a fill that
/// lifts under the pointer. A disabled one is drawn dim rather than removed,
/// so a row of controls does not change shape as things become available.
#[allow(clippy::too_many_arguments)]
fn label_button(
    ui: &mut Ui,
    label: &str,
    size: Vec2,
    fill: Color32,
    fore: Color32,
    stroke: Stroke,
    font: f32,
    radius: f32,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let fill = if !enabled {
            KEY_DIM
        } else if response.is_pointer_button_down_on() {
            fill.gamma_multiply(1.15)
        } else if response.hovered() {
            fill.gamma_multiply(1.08)
        } else {
            fill
        };
        let fore = if enabled { fore } else { KEY_DIM_GLYPH };
        ui.painter().rect(
            rect,
            CornerRadius::same(radius as u8),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(font),
            fore,
        );
    }
    response
}

pub fn primary(ui: &mut Ui, label: &str) -> Response {
    let size = Vec2::new(ui.available_width(), PRIMARY_HEIGHT);
    label_button(
        ui,
        label,
        size,
        LEAF,
        ON_LEAF,
        Stroke::NONE,
        17.0,
        CORNER_MEDIUM,
        true,
    )
}

pub fn secondary(ui: &mut Ui, label: &str) -> Response {
    let size = Vec2::new(ui.available_width(), SECONDARY_HEIGHT);
    label_button(
        ui,
        label,
        size,
        RAISED,
        ON_RAISED,
        Stroke::NONE,
        15.0,
        CORNER_MEDIUM,
        true,
    )
}

pub fn quiet(ui: &mut Ui, label: &str, danger: bool) -> Response {
    let size = Vec2::new(ui.available_width(), QUIET_HEIGHT);
    let fore = if danger { ERROR } else { ON_RAISED };
    label_button(
        ui,
        label,
        size,
        Color32::TRANSPARENT,
        fore,
        Stroke::new(1.0, OUTLINE),
        15.0,
        CORNER_SMALL,
        true,
    )
}

pub fn compact(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let width = text_width(ui, label, 14.0) + 28.0;
    label_button(
        ui,
        label,
        Vec2::new(width, 36.0),
        RAISED,
        ON_RAISED,
        Stroke::NONE,
        14.0,
        CORNER_SMALL,
        enabled,
    )
}

pub fn tile(ui: &mut Ui, label: &str, width: f32) -> Response {
    label_button(
        ui,
        label,
        Vec2::new(width, 96.0),
        SURFACE_HIGH,
        ON_SURFACE,
        Stroke::NONE,
        15.0,
        CORNER_MEDIUM,
        true,
    )
}

pub fn section(ui: &mut Ui, label: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(label.to_uppercase())
            .size(12.0)
            .strong()
            .extra_letter_spacing(2.0)
            .color(LEAF),
    );
}

pub fn note(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .color(ON_SURFACE_VARIANT),
    );
}

/// A row of chips, one of them chosen. Returns the one just clicked, if any.
pub fn choice_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    items: &[T],
    selected: T,
    label: impl Fn(T) -> &'static str,
) -> Option<T> {
    let mut chosen = None;
    // The salt has to be this call site, not the scroll area's own auto id:
    // two choice rows in one panel would otherwise share a scroll offset.
    let salt = ui.next_auto_id().value();
    egui::ScrollArea::horizontal().id_salt(salt).show(ui, |ui| {
        ui.horizontal(|ui| {
            for &item in items {
                let on = item == selected;
                let (fill, fore) = if on {
                    (LEAF, ON_LEAF)
                } else {
                    (SURFACE_HIGH, ON_RAISED)
                };
                let text = label(item);
                let width = text_width(ui, text, 14.0) + 28.0;
                if label_button(
                    ui,
                    text,
                    Vec2::new(width, 36.0),
                    fill,
                    fore,
                    Stroke::NONE,
                    14.0,
                    CORNER_SMALL,
                    true,
                )
                .clicked()
                {
                    chosen = Some(item);
                }
            }
        });
    });
    chosen
}

/// The shared left-hand side of the three settings rows: what it is, and one
/// line saying what it does.
fn row_body(ui: &mut Ui, title: &str, hint: &str) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(title).size(16.0));
        if !hint.is_empty() {
            note(ui, hint);
        }
    });
}

fn row_frame(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(SURFACE_HIGH)
        .corner_radius(CornerRadius::same(CORNER_SMALL as u8))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| add(ui));
        });
}

pub fn value_row(ui: &mut Ui, title: &str, hint: &str, action: &str) -> Response {
    let mut response = None;
    row_frame(ui, |ui| {
        row_body(ui, title, hint);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            response = Some(compact(ui, action, true));
        });
    });
    response.expect("the row's button is always drawn")
}

pub fn info_row(ui: &mut Ui, title: &str, value: &str, hint: &str) {
    row_frame(ui, |ui| {
        row_body(ui, title, hint);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value).size(15.0).color(ON_RAISED));
        });
    });
}

/// Returns true when the switch was just flipped.
pub fn switch_row(ui: &mut Ui, title: &str, hint: &str, on: &mut bool) -> bool {
    let mut changed = false;
    row_frame(ui, |ui| {
        row_body(ui, title, hint);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(52.0, 30.0), Sense::click());
            if response.clicked() {
                *on = !*on;
                changed = true;
            }
            let fill = if *on { LEAF } else { WELL };
            ui.painter().rect_filled(rect, CornerRadius::same(15), fill);
            let knob = if *on {
                rect.right_center() - Vec2::new(15.0, 0.0)
            } else {
                rect.left_center() + Vec2::new(15.0, 0.0)
            };
            ui.painter()
                .circle_filled(knob, 11.0, if *on { ON_LEAF } else { ON_RAISED });
        });
    });
    changed
}

/// A rounded notice: a short sentence in its own colour.
pub fn pill(ui: &mut Ui, text: &str, back: Color32, fore: Color32) {
    egui::Frame::new()
        .fill(back)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(egui::Margin::symmetric(16, 8))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(14.0).strong().color(fore));
        });
}

/// A scrim and one card in the middle of the window. Returns true when the
/// back arrow was pressed.
pub fn panel(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    subtitle: Option<&str>,
    back: bool,
    add: impl FnOnce(&mut Ui),
) -> bool {
    // The safe area, not the whole viewport: a notch or a status bar must
    // not be allowed to eat the panel's title.
    let screen = ctx.content_rect();
    // The scrim is its own interactable area so a click meant for the game
    // underneath stops at the panel.
    egui::Area::new(egui::Id::new((id, "scrim")))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, CornerRadius::ZERO, SCRIM);
        });
    let mut went_back = false;
    let width = PANEL_WIDTH.min(screen.width() - 32.0);
    let max_height = screen.height() * 0.9;
    egui::Area::new(egui::Id::new((id, "card")))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(CORNER_LARGE as u8))
                .inner_margin(egui::Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(width - 48.0);
                    ui.set_max_height(max_height - 48.0);
                    ui.horizontal(|ui| {
                        if back && compact(ui, "←", true).clicked() {
                            went_back = true;
                        }
                        ui.label(egui::RichText::new(title).size(27.0).strong());
                    });
                    if let Some(subtitle) = subtitle {
                        note(ui, subtitle);
                    }
                    ui.add_space(8.0);
                    // The title and the subtitle stay put; only the body
                    // scrolls, so a long list never hides what it belongs to.
                    egui::ScrollArea::vertical()
                        .max_height(max_height - 140.0)
                        .show(ui, |ui| {
                            ui.set_width(width - 64.0);
                            add(ui);
                        });
                });
        });
    went_back
}

/// The plain sentence, bottom centre, until dismissed.
pub fn message_bar(ctx: &egui::Context, text: &str) -> bool {
    let mut dismissed = false;
    egui::Area::new(egui::Id::new("message"))
        .order(egui::Order::Tooltip)
        .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -24.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(SURFACE_HIGH)
                .corner_radius(CornerRadius::same(CORNER_MEDIUM as u8))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(text).size(15.0));
                        if compact(ui, "OK", true).clicked() {
                            dismissed = true;
                        }
                    });
                });
        });
    dismissed
}

/// What a game with no cover art shows: a two-tone card with its initial, so
/// the shelf is a wall of distinguishable shapes rather than of blanks.
pub fn cover_placeholder(ui: &mut Ui, rect: Rect, title: &str) {
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(CORNER_SMALL as u8), COVER_BOTTOM);
    painter.rect_filled(
        Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.center().y)),
        CornerRadius {
            nw: CORNER_SMALL as u8,
            ne: CORNER_SMALL as u8,
            sw: 0,
            se: 0,
        },
        COVER_TOP,
    );
    let initial: String = title
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        initial,
        FontId::proportional(rect.height() * 0.5),
        ON_SURFACE,
    );
}
