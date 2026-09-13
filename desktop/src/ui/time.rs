//! One horizontal track for both directions of time. Drag left to rewind,
//! right to fast-forward, further is faster; release and it springs back.
//! A click on the handle without a drag pauses.
//!
//! Every glyph here is one the bundled font can actually draw. Android's
//! ◀◀ ▶▶ and the ▮▮ on a stopped handle are all tofu boxes in egui's
//! proportional family, so the track ends are « and » and a stopped handle
//! says ⏸; `the_time_glyphs_are_ones_the_font_has` is what keeps them that way.

use super::theme::*;
use super::widgets;
use crate::scrub;
use egui::{Align2, CornerRadius, FontId, Rect, Sense, Stroke, Vec2};

/// How tall the track and the skip keys are.
pub const HEIGHT: f32 = 56.0;
/// How wide the track is drawn when there is room, and the narrowest it may be
/// squeezed to before the row it sits in stops giving way.
pub const TRACK: f32 = 240.0;
pub const MIN_TRACK: f32 = 104.0;
/// How wide one skip key is.
pub const SKIP: f32 = 56.0;

const HANDLE: f32 = 64.0;
/// The two ends of the track, and what the handle says when time is running at
/// its ordinary speed.
const BACKWARD: &str = "«";
const FORWARD: &str = "»";
const STOPPED: &str = "⏸";
/// The wind-back arrow on a skip key.
const REPLAY: &str = "↺";

#[derive(Debug, PartialEq)]
pub enum ScrubEvent {
    None,
    Dragged(f32),
    Released,
    Tapped,
}

pub fn scrubber(ui: &mut egui::Ui, fraction: f32, width: f32, paused: bool) -> ScrubEvent {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, HEIGHT), Sense::click_and_drag());
    // Never zero. A track squeezed narrower than its own handle would divide
    // the pointer's distance by nothing and hand the engine a NaN speed.
    let travel = ((width - HANDLE) / 2.0).max(1.0);
    let centre = rect.center() + Vec2::new(fraction.clamp(-1.0, 1.0) * travel, 0.0);
    let handle = Rect::from_center_size(centre, Vec2::new(HANDLE, HEIGHT - 8.0));
    let speed = if paused { 0 } else { scrub::speed(fraction) };
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect(
            rect,
            CornerRadius::same((HEIGHT / 2.0) as u8),
            WELL,
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        painter.text(
            rect.left_center() + Vec2::new(18.0, 0.0),
            Align2::CENTER_CENTER,
            BACKWARD,
            FontId::proportional(15.0),
            WELL_MARK,
        );
        painter.text(
            rect.right_center() - Vec2::new(18.0, 0.0),
            Align2::CENTER_CENTER,
            FORWARD,
            FontId::proportional(15.0),
            WELL_MARK,
        );
        // The notch in the middle is where "now" is, so the spring-back has
        // somewhere visible to spring to.
        painter.rect_filled(
            Rect::from_center_size(rect.center(), Vec2::new(2.0, HEIGHT - 16.0)),
            1.0,
            WELL_EDGE,
        );
        let (fill, fore) = match speed {
            s if s < 0 => (AMBER, ON_TIME),
            s if s > 0 => (LEAF, ON_TIME),
            _ => (RAISED, ON_RAISED),
        };
        painter.rect(
            handle,
            CornerRadius::same(((HEIGHT - 8.0) / 2.0) as u8),
            fill,
            Stroke::NONE,
            egui::StrokeKind::Inside,
        );
        let glyph = if speed == 0 {
            if paused {
                "Play".to_string()
            } else {
                STOPPED.to_string()
            }
        } else {
            format!("{}×", speed.abs())
        };
        painter.text(
            handle.center(),
            Align2::CENTER_CENTER,
            glyph,
            FontId::proportional(15.0),
            fore,
        );
    }

    if let Some(pos) = response.interact_pointer_pos() {
        if response.dragged() || response.drag_started() {
            return ScrubEvent::Dragged(((pos.x - rect.center().x) / travel).clamp(-1.0, 1.0));
        }
        if response.clicked() {
            // The handle is the pause button. A press anywhere else on the
            // track scrubs from that point; with no drag it is a momentary
            // nudge that the release cancels.
            return if handle.contains(pos) {
                ScrubEvent::Tapped
            } else {
                ScrubEvent::Released
            };
        }
    }
    if response.drag_stopped() {
        return ScrubEvent::Released;
    }
    ScrubEvent::None
}

/// The two fixed steps back, which are the whole of rewinding for anyone who
/// would rather press a button than hold a drag.
pub fn skip_back(ui: &mut egui::Ui, seconds: u32, enabled: bool) -> bool {
    let label = format!("{REPLAY}{seconds}");
    widgets::key(ui, &label, Vec2::new(SKIP, HEIGHT), enabled).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, Modifiers, PointerButton};

    /// One pass with the track at the top left of the window, where a pointer
    /// can be aimed at it: the root `Ui` covers the viewport and starts at its
    /// corner, so the track is (0, 0) to (`width`, `HEIGHT`).
    fn pass(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        fraction: f32,
        width: f32,
    ) -> ScrubEvent {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), Vec2::new(640.0, 480.0))),
            events,
            ..Default::default()
        };
        let mut event = ScrubEvent::None;
        ctx.run_ui(input, |ui| event = scrubber(ui, fraction, width, false))
            .textures_delta
            .clear();
        event
    }

    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::default(),
        }
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        ctx
    }

    /// A frame nobody touched asks for nothing, wherever the handle is
    /// sitting and whether or not a skip key is live.
    #[test]
    fn an_untouched_track_reports_nothing() {
        let ctx = context();
        for fraction in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            assert_eq!(pass(&ctx, Vec::new(), fraction, TRACK), ScrubEvent::None);
        }
        let mut clicked = true;
        egui::__run_test_ui(|ui| clicked = skip_back(ui, 5, false));
        assert!(!clicked);
        let mut clicked = true;
        egui::__run_test_ui(|ui| clicked = skip_back(ui, 15, true));
        assert!(!clicked);
    }

    /// Left of the middle is backwards and right of it is forwards, in the
    /// fraction `scrub::speed` reads. This is the feel of the whole control:
    /// how far the pointer is from the middle is how fast time runs.
    #[test]
    fn dragging_the_handle_runs_time_both_ways() {
        let ctx = context();
        let middle = pos2(TRACK / 2.0, HEIGHT / 2.0);
        // The travel each way, which is what the drag is measured against.
        let travel = (TRACK - HANDLE) / 2.0;
        for (to, want) in [(-0.5, -0.5), (0.75, 0.75)] {
            pass(&ctx, vec![egui::Event::PointerMoved(middle)], 0.0, TRACK);
            pass(&ctx, vec![press(middle, true)], 0.0, TRACK);
            let at = middle + Vec2::new(to * travel, 0.0);
            let dragged = pass(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, TRACK);
            assert_eq!(dragged, ScrubEvent::Dragged(want));
            assert_eq!(
                pass(&ctx, vec![press(at, false)], want, TRACK),
                ScrubEvent::Released
            );
        }
    }

    /// Past the end of the track is still the end of the track, and a track
    /// narrower than its own handle must not hand back a NaN.
    #[test]
    fn a_drag_past_the_end_or_off_a_squeezed_track_stays_a_number() {
        let ctx = context();
        for width in [TRACK, MIN_TRACK, HANDLE, 8.0] {
            let middle = pos2(width / 2.0, HEIGHT / 2.0);
            pass(&ctx, vec![egui::Event::PointerMoved(middle)], 0.0, width);
            pass(&ctx, vec![press(middle, true)], 0.0, width);
            let far = pos2(-4000.0, HEIGHT / 2.0);
            let dragged = pass(&ctx, vec![egui::Event::PointerMoved(far)], 0.0, width);
            assert_eq!(dragged, ScrubEvent::Dragged(-1.0), "{width} wide");
            pass(&ctx, vec![press(far, false)], 0.0, width);
        }
    }

    /// A click on the handle is the pause; a click elsewhere on the track is a
    /// nudge the release cancels, so it must not pause too.
    #[test]
    fn a_click_pauses_only_on_the_handle() {
        let ctx = context();
        for (x, want) in [
            (TRACK / 2.0, ScrubEvent::Tapped),
            (12.0, ScrubEvent::Released),
        ] {
            let at = pos2(x, HEIGHT / 2.0);
            pass(&ctx, vec![egui::Event::PointerMoved(at)], 0.0, TRACK);
            pass(&ctx, vec![press(at, true)], 0.0, TRACK);
            assert_eq!(pass(&ctx, vec![press(at, false)], 0.0, TRACK), want);
        }
    }

    /// Android's time controls are drawn with glyphs this font does not have,
    /// and a tofu box on the control that runs time is worse than a plain
    /// letter. These are the ones that stood in for them.
    #[test]
    fn the_time_glyphs_are_ones_the_font_has() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let font = FontId::proportional(15.0);
        for glyph in [BACKWARD, FORWARD, STOPPED, REPLAY, "×"] {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, glyph)), "{glyph}");
        }
        for missing in ["◀", "▶", "▮"] {
            assert!(
                !ctx.fonts_mut(|f| f.has_glyphs(&font, missing)),
                "{missing}"
            );
        }
    }
}
