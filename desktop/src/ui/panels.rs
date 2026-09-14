//! The pause menu, the ten slots, the confirmations and the problem log.
//! Every string here is the Android one.
//!
//! These sit over a running game rather than beside it, so each is a scrim and
//! one card: what is underneath stays where it was and stays out of reach.
//! Nothing here changes anything: the slots and the log are read from the
//! library every frame, but no control here writes to it, to the disk or to
//! the engine. Each pushes an `Action` and the shell applies it once the frame
//! is over.

use super::theme::*;
use super::widgets;
use super::{Action, Dialog, Panel};
use crate::library::SaveSlot;
use crate::shell::App;
use egui::{Align2, CornerRadius, Margin, Rect, RichText, Sense, Vec2};

/// How wide a slot tile likes to be, and how tall its picture is for that
/// width. A NES frame is 4:3 once it has been fitted, which is the 0.75.
///
/// 172 rather than a rounder number because of what it buys: the card is 620
/// wide and leaves 556 inside, which is three of these and not three of
/// anything much wider. Three to a row is four rows of slots instead of five.
const TILE: f32 = 172.0;
const THUMBNAIL: f32 = 0.75;

/// The widest a confirmation gets. A question laid across a 1440-point window
/// is a question nobody reads to the end of.
const DIALOG: f32 = 420.0;

/// What a slot with nothing in it says where the others say when they were
/// written. The "·" is one the bundled font has; "⋯" and the rest are not.
const EMPTY_SLOT: &str = "Empty · ready for a moment";

/// What an empty problem log says, which is the happy answer.
const NOTHING_WRONG: &str = "Nothing has gone wrong yet.";

/// How far back the log is shown. The file keeps hundreds of lines; what went
/// wrong just now is what somebody looking at this is looking for.
const RECENT: usize = 40;

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    match app.panel {
        Panel::Pause => pause(ui.ctx(), app),
        Panel::Slots => slots(ui.ctx(), app),
        Panel::Problems => problems(ui.ctx(), app),
        // Settings and the button wizard are their own file: they are the two
        // panels that need more than `App` to draw.
        _ => {}
    }
    // Over whichever panel raised it: a question has to be answered before the
    // panel behind it can be used again. The message bar is drawn by the shell
    // after this, so it stays over the top of even a dialog.
    if let Some(dialog) = app.dialog.clone() {
        confirm(ui.ctx(), app, dialog);
    }
}

/// Everything there is to do with the game that is not playing it.
fn pause(ctx: &egui::Context, app: &mut App) {
    // Read before the closure borrows the rest of `app` mutably.
    let fullscreen = app.fullscreen;
    widgets::panel(
        ctx,
        "pause",
        "Take your time",
        Some("Progress saves automatically when you pause or leave."),
        false,
        |ui| {
            if widgets::primary(ui, "Resume game").clicked() {
                app.actions.push(Action::Resume);
            }
            ui.add_space(8.0);
            // Two to a row, sharing the card between them: a tile given the
            // whole width would make the four of them a list, and this is a
            // grid of four equal things to do.
            let gap = ui.spacing().item_spacing.x;
            let half = ((ui.available_width() - gap) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                if widgets::tile(ui, "Save states", half).clicked() {
                    app.actions.push(Action::OpenSlots);
                }
                if widgets::tile(ui, "Screenshot", half).clicked() {
                    app.actions.push(Action::Screenshot);
                }
            });
            ui.horizontal(|ui| {
                let full = if fullscreen {
                    "Exit full screen"
                } else {
                    "Full screen"
                };
                if widgets::tile(ui, full, half).clicked() {
                    app.actions.push(Action::ToggleFullscreen);
                }
                if widgets::tile(ui, "Settings", half).clicked() {
                    app.actions.push(Action::OpenSettings);
                }
            });
            ui.add_space(8.0);
            if widgets::secondary(ui, "Reset game").clicked() {
                app.actions.push(Action::ResetRequested);
            }
            if widgets::secondary(ui, "Back to your shelf").clicked() {
                app.actions.push(Action::BackToShelf);
            }
        },
    );
}

/// The ten slots, each with the picture of the moment it holds.
fn slots(ctx: &egui::Context, app: &mut App) {
    let Some(session) = &app.session else {
        return;
    };
    let id = session.game.id.clone();
    // Read once a frame rather than once a tile: ten slots are ten looks at
    // the disk, and all ten have to agree about what is on it.
    let slots = app.library.slots(&id);
    let back = widgets::panel(
        ctx,
        "slots",
        "Save states",
        Some("Ten slots, plus a separate automatic save."),
        true,
        |ui| {
            let room = ui.available_width().max(1.0);
            // Never wider than the card: a tile that sets a width the panel
            // does not have is a tile with its buttons off the edge.
            let width = TILE.min(room);
            let gap = ui.spacing().item_spacing.x;
            let columns = ((room + gap) / (width + gap)).floor().max(1.0) as usize;
            for chunk in slots.chunks(columns) {
                ui.horizontal_top(|ui| {
                    for slot in chunk {
                        tile(ui, app, slot, width);
                    }
                });
            }
            ui.add_space(8.0);
            if widgets::primary(ui, "Resume game").clicked() {
                app.actions.push(Action::Resume);
            }
        },
    );
    if back {
        app.actions.push(Action::ClosePanel);
    }
}

/// One slot: what it looks like, when it was written, and the two things to do
/// with it.
fn tile(ui: &mut egui::Ui, app: &mut App, slot: &SaveSlot, width: f32) {
    egui::Frame::new()
        .fill(SURFACE_HIGH)
        .corner_radius(CornerRadius::same(CORNER_MEDIUM as u8))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            // A tile is a column even though the row of tiles holding it is a
            // row. A frame inherits the layout it was dropped into, which here
            // is left to right, and that would stand the picture beside the
            // buttons rather than above them — the same trap the shelf's cards
            // are laid out around.
            ui.vertical(|ui| {
                let inner = (width - 20.0).max(0.0);
                ui.set_width(inner);
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(inner, inner * THUMBNAIL), Sense::hover());
                match app.slot_texture(ui.ctx(), slot) {
                    Some(texture) => {
                        ui.painter().image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                    // An empty slot is a hole in the card rather than a blank
                    // one: the row should read as ten places, some filled.
                    None => {
                        ui.painter().rect_filled(
                            rect,
                            CornerRadius::same(CORNER_SMALL as u8),
                            WELL,
                        );
                    }
                }
                ui.label(
                    RichText::new(format!("Slot {}", slot.number + 1))
                        .size(15.0)
                        .strong()
                        .color(ON_SURFACE),
                );
                widgets::note(ui, &written(slot));
                ui.horizontal(|ui| {
                    if widgets::compact(ui, "Save", true).clicked() {
                        // A slot with something in it is asked about first; an
                        // empty one is written straight away, because there is
                        // nothing there to lose.
                        app.actions.push(if slot.time.is_some() {
                            Action::SaveRequested(slot.number)
                        } else {
                            Action::SaveConfirmed(slot.number)
                        });
                    }
                    if widgets::compact(ui, "Load", slot.time.is_some()).clicked() {
                        app.actions.push(Action::Load(slot.number));
                    }
                });
            });
        });
}

/// When a slot was written, in this computer's own way of saying a date, or
/// what it says instead when there is nothing in it.
///
/// A time that cannot be read shows no time rather than the empty line: a full
/// slot that says it is empty is a slot somebody writes over by mistake.
fn written(slot: &SaveSlot) -> String {
    match slot.time {
        Some(ms) => chrono::DateTime::from_timestamp_millis(ms)
            .map(|t| t.with_timezone(&chrono::Local).format("%x %R").to_string())
            .unwrap_or_default(),
        None => EMPTY_SLOT.to_string(),
    }
}

/// What has gone wrong, newest first. The one screen in the shell that is not
/// for the person playing but for the person helping them.
fn problems(ctx: &egui::Context, app: &mut App) {
    let lines = app.library.problems();
    widgets::panel(ctx, "problems", "Problem log", None, false, |ui| {
        if lines.is_empty() {
            widgets::note(ui, NOTHING_WRONG);
        }
        for line in lines.iter().take(RECENT) {
            ui.label(
                RichText::new(line)
                    .size(12.0)
                    .monospace()
                    .color(ON_SURFACE_VARIANT),
            );
        }
        ui.add_space(8.0);
        if widgets::secondary(ui, "Close").clicked() {
            app.actions.push(Action::ClosePanel);
        }
    });
}

/// The one question at a time the shell ever asks. Every one of them is worded
/// so that the button says what it does rather than "Yes".
fn confirm(ctx: &egui::Context, app: &mut App, dialog: Dialog) {
    let (title, body, yes, no, danger, yes_action) = match &dialog {
        Dialog::ConfirmReset => {
            let title = app
                .session
                .as_ref()
                .map(|s| s.game.title.clone())
                .unwrap_or_default();
            (
                "Reset game?".to_string(),
                format!(
                    "Restart {title} from the beginning. In-game saves and manual save slots are kept. The current session and rewind history will be replaced."
                ),
                "Reset",
                "Cancel",
                false,
                Action::ResetConfirmed,
            )
        }
        Dialog::ConfirmReplace(n) => (
            format!("Replace slot {}?", n + 1),
            "This replaces the progress saved in this slot. Your other slots stay available."
                .to_string(),
            "Replace save",
            "Keep it",
            false,
            Action::SaveConfirmed(*n),
        ),
        Dialog::ConfirmDelete(game) => (
            format!("Delete {}?", game.title),
            "This removes the game, its battery save and all ten of its save states from this computer. It cannot be undone."
                .to_string(),
            "Delete forever",
            "Keep it",
            true,
            Action::DeleteConfirmed(game.clone()),
        ),
    };
    // The safe area, the same one the panels cover: the scrim has to reach the
    // edges of what can be clicked.
    let screen = ctx.content_rect();
    // Above every panel by being in a higher order than any of them, not by
    // being drawn after them. Within one order egui sorts whatever appeared
    // this frame to the top, so a panel raised while the question was already
    // up — by a stray Space, say — would otherwise come out over the scrim and
    // take the clicks the question was waiting for.
    egui::Area::new(egui::Id::new("dialog-scrim"))
        .order(egui::Order::Tooltip)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ctx, |ui| {
            // The question has to be answered: a click meant for the panel or
            // the game behind it stops here.
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, CornerRadius::ZERO, SCRIM);
        });
    // And the card above its own scrim, for the same reason and by the same
    // means: `Order::TOP`, because there is nothing the question should be
    // underneath while it is being asked.
    egui::Area::new(egui::Id::new("dialog"))
        .order(egui::Order::TOP)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(CORNER_LARGE as u8))
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(DIALOG.min(screen.width() - 48.0).max(0.0));
                    ui.label(RichText::new(&title).size(22.0).strong().color(ON_SURFACE));
                    ui.label(RichText::new(&body).size(15.0).color(ON_SURFACE_VARIANT));
                    ui.add_space(12.0);
                    if widgets::quiet(ui, yes, danger).clicked() {
                        app.actions.push(yes_action);
                    }
                    if widgets::quiet(ui, no, false).clicked() {
                        app.actions.push(Action::CloseDialog);
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::test_rom;
    use crate::library::Slot;
    use crate::session::Session;
    use egui::epaint::ClippedShape;
    use std::path::PathBuf;

    /// The window every one of these is drawn in unless it says otherwise.
    const WINDOW: Vec2 = Vec2::new(1024.0, 768.0);

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        ctx
    }

    /// A real engine on a built ROM, with no audio and no window: the panels
    /// read a title, an id and the odd save state off it and nothing else.
    fn open(app: &mut App) {
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, warnings) = Session::open(&app.library, game, None).unwrap();
        assert!(warnings.is_empty());
        app.session = Some(session);
    }

    /// One frame of whatever is in front of the game, at a given window size.
    /// The texture delta has to be taken or dropping it panics; the shapes are
    /// handed back because they are how a test finds anything on screen.
    fn pass(
        app: &mut App,
        ctx: &egui::Context,
        size: Vec2,
        events: Vec<egui::Event>,
    ) -> Vec<ClippedShape> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::pos2(0.0, 0.0), size)),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| show(ui, app));
        output.textures_delta.clear();
        output.shapes
    }

    /// A panel that has settled. An `Area` spends its first frame working out
    /// how big it is and paints nothing at all, so the second frame is the one
    /// with anything on it — and the one a click can be aimed at.
    fn drawn(app: &mut App, ctx: &egui::Context, size: Vec2) -> Vec<ClippedShape> {
        pass(app, ctx, size, Vec::new());
        pass(app, ctx, size, Vec::new())
    }

    /// Every piece of text a frame painted, in the order it was painted.
    fn texts(shapes: &[ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, found: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => found.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut found);
        }
        found
    }

    /// Where a piece of text was painted. Every control in `widgets` centres
    /// its label in its own rect, so the middle of the label is the middle of
    /// the button — which beats working out where the panel put it.
    fn spots(shapes: &[ClippedShape], label: &str) -> Vec<egui::Pos2> {
        fn walk(shape: &egui::Shape, label: &str, found: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    found.push(text.pos + text.galley.size() / 2.0);
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, label, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, label, &mut found);
        }
        found
    }

    fn at(shapes: &[ClippedShape], label: &str) -> egui::Pos2 {
        let found = spots(shapes, label);
        assert_eq!(found.len(), 1, "{label} appears {} times", found.len());
        found[0]
    }

    /// The control labelled `label` belonging to the tile headed `slot`: ten
    /// tiles carry the same two buttons, and the one nearest the heading is
    /// the one under it.
    fn in_tile(shapes: &[ClippedShape], slot: &str, label: &str) -> egui::Pos2 {
        let heading = at(shapes, slot);
        spots(shapes, label)
            .into_iter()
            .min_by(|a, b| a.distance(heading).total_cmp(&b.distance(heading)))
            .unwrap_or_else(|| panic!("no {label} under {slot}"))
    }

    /// A press and a release in the same place, which is what egui calls a
    /// click. Two frames, because the release is the frame it lands on.
    fn click(app: &mut App, ctx: &egui::Context, at: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        pass(
            app,
            ctx,
            WINDOW,
            vec![egui::Event::PointerMoved(at), button(true)],
        );
        pass(app, ctx, WINDOW, vec![button(false)]);
    }

    /// Every panel and every question, at three window widths, twice each —
    /// egui settles a layout over two frames and the second is the one that
    /// has to hold up. None of them may ask for anything nobody touched.
    #[test]
    fn every_panel_draws_and_a_frame_nobody_touched_asks_for_nothing() {
        let dir = temp_dir("panels-draw");
        let ctx = context();
        let mut app = App::blank(&dir);
        // The delete question is raised from the shelf, where there is no
        // game open at all.
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        app.dialog = Some(Dialog::ConfirmDelete(game));
        pass(&mut app, &ctx, WINDOW, Vec::new());
        app.dialog = None;
        open(&mut app);
        app.session.as_mut().unwrap().save(Slot::Number(0)).unwrap();
        app.library
            .log_problem("Something went wrong", "the detail");
        for panel in [Panel::Pause, Panel::Slots, Panel::Problems, Panel::None] {
            app.panel = panel;
            for dialog in [
                None,
                Some(Dialog::ConfirmReset),
                Some(Dialog::ConfirmReplace(3)),
            ] {
                app.dialog = dialog;
                for fullscreen in [false, true] {
                    app.fullscreen = fullscreen;
                    // Roomy, tight, and narrower than one slot tile.
                    for width in [1440.0, 640.0, 160.0] {
                        for _ in 0..2 {
                            pass(&mut app, &ctx, Vec2::new(width, 600.0), Vec::new());
                        }
                    }
                }
            }
        }
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The pause panel says what it is for and offers the way back into the
    /// game as the first and biggest thing on it.
    #[test]
    fn the_pause_panel_resumes_the_game() {
        let dir = temp_dir("panels-pause");
        let ctx = context();
        let mut app = App::blank(&dir);
        open(&mut app);
        app.panel = Panel::Pause;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        let said = texts(&shapes);
        for line in [
            "Take your time",
            "Progress saves automatically when you pause or leave.",
            "Resume game",
            "Save states",
            "Screenshot",
            "Full screen",
            "Settings",
            "Reset game",
            "Back to your shelf",
        ] {
            assert!(said.iter().any(|t| t == line), "no {line:?} in {said:?}");
        }
        click(&mut app, &ctx, at(&shapes, "Resume game"));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::Resume]);
        // The tile says what the button would do, not where you are.
        app.fullscreen = true;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        click(&mut app, &ctx, at(&shapes, "Exit full screen"));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::ToggleFullscreen]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Ten tiles, each saying whether there is anything in it. Saving over a
    /// slot with something in it asks first; an empty one is written straight
    /// away, and its Load is dim until there is something to load.
    #[test]
    fn a_full_slot_asks_before_it_is_replaced_and_an_empty_one_does_not() {
        let dir = temp_dir("panels-slots");
        let ctx = context();
        let mut app = App::blank(&dir);
        open(&mut app);
        app.session.as_mut().unwrap().save(Slot::Number(0)).unwrap();
        app.panel = Panel::Slots;
        // Tall enough for all ten at once: three to a row is four rows, and
        // the panel scrolls rather than shrinking them in a shorter window.
        let said = texts(&drawn(&mut app, &ctx, Vec2::new(1024.0, 1800.0)));
        for slot in 1..=10 {
            let heading = format!("Slot {slot}");
            assert!(said.contains(&heading), "no {heading:?}");
        }
        // Nine of the ten are empty, and slot 1 says a date instead.
        assert_eq!(said.iter().filter(|t| *t == EMPTY_SLOT).count(), 9);
        // Written over only after the question has been asked.
        let shapes = drawn(&mut app, &ctx, WINDOW);
        click(&mut app, &ctx, in_tile(&shapes, "Slot 1", "Save"));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SaveRequested(0)]
        );
        click(&mut app, &ctx, in_tile(&shapes, "Slot 2", "Save"));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SaveConfirmed(1)]
        );
        // And loaded only where there is something to load.
        click(&mut app, &ctx, in_tile(&shapes, "Slot 2", "Load"));
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        click(&mut app, &ctx, in_tile(&shapes, "Slot 1", "Load"));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::Load(0)]);
        // The back chevron leaves the panel rather than the game.
        click(&mut app, &ctx, at(&shapes, widgets::BACK));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::ClosePanel]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Three tiles across the card, each of them a column. A frame takes the
    /// layout it was dropped into, so a tile built straight into the row of
    /// tiles stands its picture beside its own buttons instead of above them —
    /// which makes every tile twice as wide as it asked to be and carries the
    /// whole card, back chevron and all, off the side of the window.
    #[test]
    fn three_slot_tiles_fit_across_the_card_and_each_is_a_column() {
        let dir = temp_dir("panels-tiles");
        let ctx = context();
        let mut app = App::blank(&dir);
        open(&mut app);
        app.panel = Panel::Slots;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        let gap = ctx.style_of(egui::Theme::Dark).spacing.item_spacing.x;
        let first = at(&shapes, "Slot 1");
        // Three to a row, one tile plus one gap apart...
        assert!(
            (at(&shapes, "Slot 3").x - first.x - 2.0 * (TILE + gap)).abs() < 0.5,
            "{first:?} to {:?}",
            at(&shapes, "Slot 3")
        );
        // ...and the fourth back under the first, on the next row down.
        let fourth = at(&shapes, "Slot 4");
        assert!((fourth.x - first.x).abs() < 0.5, "{first:?} to {fourth:?}");
        assert!(fourth.y > first.y, "{first:?} to {fourth:?}");
        // Within a tile the buttons are under the heading, not beside it,
        // and the card they are on starts inside the window.
        let save = in_tile(&shapes, "Slot 1", "Save");
        assert!(save.y > first.y, "{first:?} to {save:?}");
        assert!(save.x < first.x + TILE, "{first:?} to {save:?}");
        assert!(at(&shapes, widgets::BACK).x > 0.0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The log is the one screen written for the person helping, so an empty
    /// one has to say that it is empty rather than look broken.
    #[test]
    fn the_problem_log_says_so_when_there_is_nothing_in_it() {
        let dir = temp_dir("panels-problems");
        let ctx = context();
        let mut app = App::blank(&dir);
        app.panel = Panel::Problems;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        assert!(texts(&shapes).iter().any(|t| t == NOTHING_WRONG));
        app.library.log_problem("A thing went wrong", "the detail");
        let shapes = drawn(&mut app, &ctx, WINDOW);
        let said = texts(&shapes);
        assert!(!said.iter().any(|t| t == NOTHING_WRONG), "{said:?}");
        assert!(
            said.iter().any(|t| t.contains("A thing went wrong")),
            "{said:?}"
        );
        click(&mut app, &ctx, at(&shapes, "Close"));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::ClosePanel]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Every question says what it is about and what the button will do, and
    /// the button does it. "Yes" would be four buttons that all say the same
    /// thing about four different things you cannot undo.
    #[test]
    fn every_question_names_what_its_answer_does() {
        let dir = temp_dir("panels-dialogs");
        let ctx = context();
        let mut app = App::blank(&dir);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        open(&mut app);
        for (dialog, yes, wanted) in [
            (Dialog::ConfirmReset, "Reset", Action::ResetConfirmed),
            (
                Dialog::ConfirmReplace(2),
                "Replace save",
                Action::SaveConfirmed(2),
            ),
            (
                Dialog::ConfirmDelete(game.clone()),
                "Delete forever",
                Action::DeleteConfirmed(game.clone()),
            ),
        ] {
            app.dialog = Some(dialog.clone());
            let shapes = drawn(&mut app, &ctx, WINDOW);
            click(&mut app, &ctx, at(&shapes, yes));
            assert_eq!(std::mem::take(&mut app.actions), vec![wanted], "{dialog:?}");
        }
        // And every one of them can be backed out of.
        for (dialog, no) in [
            (Dialog::ConfirmReset, "Cancel"),
            (Dialog::ConfirmReplace(2), "Keep it"),
            (Dialog::ConfirmDelete(game), "Keep it"),
        ] {
            app.dialog = Some(dialog.clone());
            let shapes = drawn(&mut app, &ctx, WINDOW);
            click(&mut app, &ctx, at(&shapes, no));
            assert_eq!(
                std::mem::take(&mut app.actions),
                vec![Action::CloseDialog],
                "{dialog:?}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A question covers the panel it was raised from: the panel is still
    /// there to be read, and none of it can be pressed until the question is
    /// answered. Either order — and the second one matters, because a panel
    /// that appears while the scrim is already up is a new layer, and a new
    /// layer is one egui sorts to the top of its own order.
    #[test]
    fn a_question_swallows_the_clicks_meant_for_the_panel_behind_it() {
        let dir = temp_dir("panels-scrim");
        let ctx = context();
        let mut app = App::blank(&dir);
        open(&mut app);
        for panel_first in [true, false] {
            app.panel = Panel::None;
            app.dialog = None;
            drawn(&mut app, &ctx, WINDOW);
            if panel_first {
                app.panel = Panel::Pause;
                drawn(&mut app, &ctx, WINDOW);
                app.dialog = Some(Dialog::ConfirmReset);
            } else {
                app.dialog = Some(Dialog::ConfirmReset);
                drawn(&mut app, &ctx, WINDOW);
                app.panel = Panel::Pause;
            }
            let shapes = drawn(&mut app, &ctx, WINDOW);
            // Still readable, and no longer reachable.
            assert!(texts(&shapes).iter().any(|t| t == "Take your time"));
            click(&mut app, &ctx, at(&shapes, "Resume game"));
            assert!(
                app.actions.is_empty(),
                "panel first: {panel_first}, {:?}",
                app.actions
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A slot with a time nobody can make sense of still has something in it,
    /// and the one line it has must not be the one that says it is empty.
    #[test]
    fn a_slot_with_an_unreadable_time_does_not_claim_to_be_empty() {
        let mut slot = SaveSlot {
            number: 0,
            time: Some(i64::MAX),
            thumbnail: PathBuf::new(),
        };
        assert_eq!(written(&slot), "");
        slot.time = None;
        assert_eq!(written(&slot), EMPTY_SLOT);
        slot.time = Some(0);
        assert!(!written(&slot).is_empty());
    }

    /// The slots panel borrows Android's wording, and the middle dot in it is
    /// not a glyph the bundled font can be assumed to have — a tofu box where
    /// a date belongs reads as a broken save rather than an empty slot.
    #[test]
    fn the_panel_glyphs_are_ones_the_font_has() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let font = egui::FontId::proportional(12.0);
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, EMPTY_SLOT)));
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, NOTHING_WRONG)));
    }
}
