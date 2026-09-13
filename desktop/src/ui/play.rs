//! What sits around the picture while a game is open: a title bar with two
//! buttons, the time controls, one amber notice, and a chrome pill in full
//! screen that fades when the hands are still.
//!
//! Nothing here touches the engine. The scrubber and the skip keys push an
//! `Action` and the shell applies it before the next tick, so a rewind that
//! was asked for on this frame happens on this frame.

use super::theme::*;
use super::time::{self, scrubber, skip_back, ScrubEvent};
use super::widgets;
use super::Action;
use crate::scrub;
use crate::shell::{App, TIME_ROW, TITLE_BAR};
use egui::{Align2, CornerRadius, Margin, RichText, Vec2};
use std::time::Instant;

/// The two steps back the time row offers, in seconds.
const SKIPS: [u32; 2] = [5, 15];

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    // Everything the play view reads off the session is taken here, so the
    // rest of the frame can push actions without the borrow in the way.
    let Some(session) = &app.session else {
        return;
    };
    let title = session.game.title.clone();
    // How much tape each skip key needs, asked of the engine rather than
    // worked out here: how many frames a second is has a region in it.
    let skips = SKIPS.map(|seconds| (seconds, session.engine.frames_for(seconds)));

    if !app.fullscreen {
        title_bar(ui, app, &title);
    } else if chrome_up(app) {
        chrome(ui.ctx(), app);
    }
    // The time row while the game is running, which is not the same as while
    // no panel is up: a dialog stops the engine too, and a scrubber that moves
    // a frozen picture is a control that lies.
    if chrome_up(app) && app.playing() {
        time_row(ui, app, skips);
    }
    if let Some(text) = hud(app) {
        notice(ui.ctx(), app.fullscreen, &text);
    }
}

/// Whether the chrome is up. In a window it always is; in full screen it
/// stands down once nothing has happened for a while, and the shell hides the
/// pointer on the same clock.
fn chrome_up(app: &App) -> bool {
    !app.fullscreen || app.playback_paused || Instant::now() < app.chrome_until
}

/// What the pill says: a notice while there is one, otherwise which way time
/// is running, and nothing at all while it runs at its ordinary speed.
fn hud(app: &App) -> Option<String> {
    if let Some((text, _)) = &app.notice {
        return Some(text.clone());
    }
    let speed = app.time_speed();
    if speed != 0 && app.playing() {
        return Some(scrub::label(speed));
    }
    None
}

/// The game's name and the two things to do to the window it is in.
fn title_bar(ui: &mut egui::Ui, app: &mut App, title: &str) {
    egui::Panel::top("title")
        .exact_size(TITLE_BAR)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(BACKGROUND)
                .inner_margin(Margin::symmetric(16, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                // The buttons take their room first and the name gets what is
                // left: a ROM filename is long, and a title allowed to run on
                // would push "Full screen" off the edge of the window.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::compact(ui, "Menu", true).clicked() {
                        app.actions.push(Action::Pause);
                    }
                    if widgets::compact(ui, "Full screen", true).clicked() {
                        app.actions.push(Action::ToggleFullscreen);
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = RichText::new(title).size(17.0).strong().color(ON_SURFACE);
                        ui.add(egui::Label::new(text).truncate());
                    });
                });
            });
        });
}

/// In full screen there is no title bar to put the buttons in, so they float
/// in the corner on a dark pill and go away with the pointer.
fn chrome(ctx: &egui::Context, app: &mut App) {
    egui::Area::new(egui::Id::new("chrome"))
        .order(egui::Order::Foreground)
        .anchor(Align2::RIGHT_TOP, Vec2::new(-16.0, 16.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(CHROME)
                .corner_radius(CornerRadius::same(24))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if widgets::compact(ui, "Menu", true).clicked() {
                            app.actions.push(Action::Pause);
                        }
                        if widgets::compact(ui, "Exit full screen", true).clicked() {
                            app.actions.push(Action::ToggleFullscreen);
                        }
                    });
                });
        });
}

/// The two skip keys and the track, centred under the picture. In full screen
/// it floats over the picture instead, so it brings no background with it.
fn time_row(ui: &mut egui::Ui, app: &mut App, skips: [(u32, usize); 2]) {
    let fill = if app.fullscreen {
        egui::Color32::TRANSPARENT
    } else {
        BACKGROUND
    };
    egui::Panel::bottom("time")
        .exact_size(TIME_ROW)
        .show_separator_line(false)
        .frame(
            egui::Frame::new()
                .fill(fill)
                .inner_margin(Margin::symmetric(16, 12)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let room = ui.available_width();
                let keys = time::SKIP * 2.0 + ui.spacing().item_spacing.x * 2.0;
                // The track gives way first in a narrow window: the skip keys
                // are fixed targets, and a squeezed one is a target you miss.
                let track = time::TRACK.min((room - keys).max(time::MIN_TRACK));
                ui.add_space((room - keys - track).max(0.0) / 2.0);
                for (seconds, frames) in skips {
                    if skip_back(ui, seconds, app.rewind_depth >= frames) {
                        app.actions.push(Action::JumpBack(seconds));
                    }
                }
                match scrubber(ui, app.scrub_fraction, track, app.playback_paused) {
                    ScrubEvent::Dragged(fraction) => app.actions.push(Action::Scrub(fraction)),
                    ScrubEvent::Released => app.actions.push(Action::ScrubReleased),
                    ScrubEvent::Tapped => app.actions.push(Action::TogglePlayback),
                    ScrubEvent::None => {}
                }
            });
        });
}

/// One short sentence over the top of the picture, clear of the title bar.
/// Never in the way of a click: it is something to read, not something to do.
fn notice(ctx: &egui::Context, fullscreen: bool, text: &str) {
    let top = if fullscreen { 16.0 } else { TITLE_BAR + 12.0 };
    egui::Area::new(egui::Id::new("hud"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_TOP, Vec2::new(0.0, top))
        .interactable(false)
        .show(ctx, |ui| {
            widgets::pill(ui, text, NOTICE_BACK, AMBER);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::test_rom;
    use crate::session::Session;
    use crate::shell::{CHROME_IDLE, END_OF_TAPE};
    use crate::ui::Panel;
    use egui::Rect;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real engine on a built ROM, with no audio and no window: the play
    /// view reads a title and a frame count off it and nothing else.
    fn open(app: &mut App) {
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, warnings) = Session::open(&app.library, game, None).unwrap();
        assert!(warnings.is_empty());
        app.session = Some(session);
    }

    /// One frame of the play view at a given window size. The texture delta
    /// has to be taken or dropping it panics.
    fn pass(app: &mut App, ctx: &egui::Context, size: Vec2, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::pos2(0.0, 0.0), size)),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| show(ui, app)).textures_delta.clear();
    }

    /// A press and a release in the same place, which is what egui calls a
    /// click. Two frames, because the release is the frame it lands on.
    fn click(app: &mut App, ctx: &egui::Context, size: Vec2, at: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        pass(
            app,
            ctx,
            size,
            vec![egui::Event::PointerMoved(at), button(true)],
        );
        pass(app, ctx, size, vec![button(false)]);
    }

    /// Every state the play view has that no click can reach from a test:
    /// docked and full screen, with and without a panel in front of it, at
    /// three window widths, then the notice and the speed pill.
    /// Twice each, because egui settles a layout over two frames and the
    /// second is the one that has to hold up.
    #[test]
    fn every_play_view_draws_and_a_frame_nobody_touched_asks_for_nothing() {
        let dir = temp_dir("play-draw");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = App::blank(&dir);
        // No game open: the play view is not the screen that is up.
        pass(&mut app, &ctx, Vec2::new(1024.0, 768.0), Vec::new());
        open(&mut app);
        app.chrome_until = Instant::now() + CHROME_IDLE;
        for fullscreen in [false, true] {
            app.fullscreen = fullscreen;
            for panel in [Panel::None, Panel::Pause] {
                app.panel = panel;
                // Roomy, tight, and narrower than the row wants to be.
                for width in [1440.0, 640.0, 220.0] {
                    for _ in 0..2 {
                        pass(&mut app, &ctx, Vec2::new(width, 600.0), Vec::new());
                    }
                }
            }
        }
        app.fullscreen = false;
        app.panel = Panel::None;
        app.notice(END_OF_TAPE);
        pass(&mut app, &ctx, Vec2::new(1024.0, 768.0), Vec::new());
        app.notice = None;
        app.scrub_fraction = -1.0;
        pass(&mut app, &ctx, Vec2::new(1024.0, 768.0), Vec::new());
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The title bar's own buttons stay where they are whatever the game is
    /// called. A shelf of ROMs is a shelf of filenames, and a name allowed to
    /// take the whole bar would push "Full screen" off the edge of the window
    /// — so the test is a click in the corner, which has to still land.
    #[test]
    fn a_long_title_does_not_push_the_buttons_off_the_bar() {
        let dir = temp_dir("play-title");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = App::blank(&dir);
        open(&mut app);
        let window = Vec2::new(700.0, 600.0);
        // 16 points of margin and 20 into the rightmost button, which is the
        // first thing the right-to-left row lays out.
        let menu = egui::pos2(window.x - 36.0, TITLE_BAR / 2.0);
        for title in ["Test", &"a filename nobody would read twice, ".repeat(4)] {
            if let Some(session) = &mut app.session {
                session.game.title = title.to_string();
            }
            pass(&mut app, &ctx, window, Vec::new());
            click(&mut app, &ctx, window, menu);
            assert_eq!(
                std::mem::take(&mut app.actions),
                vec![Action::Pause],
                "{title}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clicking_the_time_handle_toggles_playback() {
        let dir = temp_dir("play-pause");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = App::blank(&dir);
        open(&mut app);
        let window = Vec2::new(700.0, 600.0);
        let keys = time::SKIP * 2.0 + ctx.style_of(egui::Theme::Dark).spacing.item_spacing.x * 2.0;
        let handle = egui::pos2((window.x + keys) / 2.0, window.y - TIME_ROW / 2.0);
        for paused in [false, true] {
            app.playback_paused = paused;
            pass(&mut app, &ctx, window, Vec::new());
            click(&mut app, &ctx, window, handle);
            assert_eq!(
                std::mem::take(&mut app.actions),
                vec![Action::TogglePlayback]
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The skip keys are dim until the tape holds that many seconds, and a
    /// click on a live one asks to go back exactly that far.
    #[test]
    fn a_skip_key_waits_for_enough_tape_and_then_jumps() {
        let dir = temp_dir("play-skip");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = App::blank(&dir);
        open(&mut app);
        let window = Vec2::new(700.0, 600.0);
        // The row is centred, and the first key is where it starts.
        let keys = time::SKIP * 2.0 + ctx.style_of(egui::Theme::Dark).spacing.item_spacing.x * 2.0;
        let left = 16.0 + (window.x - 32.0 - keys - time::TRACK) / 2.0;
        let five = egui::pos2(left + time::SKIP / 2.0, window.y - TIME_ROW / 2.0);
        app.rewind_depth = 0;
        // egui hit-tests a press against the frame before it, so each state
        // is drawn once before it is clicked — which is also the only way
        // anybody clicks anything.
        pass(&mut app, &ctx, window, Vec::new());
        click(&mut app, &ctx, window, five);
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        app.rewind_depth = app.session.as_ref().unwrap().engine.frames_for(5);
        pass(&mut app, &ctx, window, Vec::new());
        click(&mut app, &ctx, window, five);
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::JumpBack(5)]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// In a window the chrome is always up. In full screen it stands down when
    /// nothing has happened for a while, and comes back the moment it does.
    #[test]
    fn the_chrome_stands_down_only_in_full_screen_and_only_when_idle() {
        let dir = temp_dir("play-chrome");
        let mut app = App::blank(&dir);
        app.chrome_until = Instant::now() - Duration::from_secs(1);
        assert!(chrome_up(&app), "a window always shows its own chrome");
        app.fullscreen = true;
        assert!(!chrome_up(&app));
        app.chrome_until = Instant::now() + CHROME_IDLE;
        assert!(chrome_up(&app));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The pill says one thing at a time, and a notice is the thing it says.
    #[test]
    fn the_pill_says_the_notice_first_and_the_speed_otherwise() {
        let dir = temp_dir("play-hud");
        let mut app = App::blank(&dir);
        open(&mut app);
        assert_eq!(hud(&app), None);
        app.scrub_fraction = -1.0;
        assert_eq!(hud(&app).as_deref(), Some("Rewinding 8×"));
        app.scrub_fraction = 1.0;
        assert_eq!(hud(&app).as_deref(), Some("Fast-forward 8×"));
        // Nothing about speed while anything is in front of the game: it is
        // not running, whatever the track was left at.
        for stopped in [Panel::Pause, Panel::Slots] {
            app.panel = stopped;
            assert_eq!(hud(&app), None, "{stopped:?}");
        }
        app.panel = Panel::None;
        app.dialog = Some(crate::ui::Dialog::ConfirmReset);
        assert_eq!(hud(&app), None);
        app.dialog = None;
        // A notice is said wherever the track is and whatever is in front.
        app.notice(END_OF_TAPE);
        assert_eq!(hud(&app).as_deref(), Some(END_OF_TAPE));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
