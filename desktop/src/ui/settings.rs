//! Picture settings with the real pipeline previewing them, the palette
//! import, the controller wizard, and the read-only rows.
//!
//! The preview is not a mock-up. The sample frame goes through the same
//! shaders the game does, into an off-screen buffer that is read back into a
//! texture, so what this panel promises cannot drift from what the game will
//! look like. That read-back is the one thing here that needs the GL context,
//! and it is kept in its own step: everything below it is drawn from `App`
//! alone, the way the rest of the screens are, and pushes an `Action` rather
//! than changing anything itself.

use super::theme::*;
use super::widgets;
use super::{Action, Panel};
use crate::input::NesButton;
use crate::picture::{Aspect, Look, PaletteChoice};
use crate::shell::App;
use crate::video::Video;
use egui::{Rect, RichText, Sense, Vec2};

/// How big the off-screen preview is drawn: 4:3, and wide enough that a look
/// made of scanlines or of a dot grid has rows of them to show.
pub const PREVIEW_WIDTH: i32 = 640;
pub const PREVIEW_HEIGHT: i32 = 480;

/// What stands in the preview's place until the first one has been read back.
const PREPARING: &str = "Preparing a preview…";

/// The mark against a button that has been learned. Android's "✓" is not a
/// glyph the bundled font has — nor are "✔", "✅" or "☑" — and a tofu box
/// beside a button reads as the one that went wrong. The bullet is one it
/// has, and `the_settings_glyphs_are_ones_the_font_has` keeps it that way.
const DONE: &str = "•";

pub fn show(ui: &mut egui::Ui, app: &mut App, video: &mut Video) {
    if app.panel == Panel::Settings {
        refresh_preview(ui.ctx(), app, video);
    }
    panels(ui.ctx(), app);
}

/// Whichever of the two is up. Everything `show` does apart from the preview,
/// which is the one step that needs a GL context: the tests draw through here,
/// so a panel wired to the wrong screen fails one of them.
fn panels(ctx: &egui::Context, app: &mut App) {
    match app.panel {
        Panel::Settings => settings(ctx, app),
        Panel::Mapping => mapping(ctx, app),
        _ => {}
    }
}

/// Draws the sample through the real pipeline and keeps the result, when
/// something has changed that it would look different for.
fn refresh_preview(ctx: &egui::Context, app: &mut App, video: &mut Video) {
    if !app.preview_dirty {
        return;
    }
    let source = app.sample_source();
    let drawn = video.preview(
        &source,
        app.settings.look,
        app.settings.aspect,
        app.settings.trim_edges,
        PREVIEW_WIDTH,
        PREVIEW_HEIGHT,
    );
    refreshed(app);
    match drawn {
        Ok(rgba) => {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [PREVIEW_WIDTH as usize, PREVIEW_HEIGHT as usize],
                &rgba,
            );
            match &mut app.preview {
                Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
                None => {
                    app.preview =
                        Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR))
                }
            }
        }
        // Said in the log rather than on screen: the panel keeps whatever it
        // had, or says it is preparing one, and the settings still work.
        Err(e) => app.library.log_problem("Preview", &e),
    }
}

/// What an attempt at a preview costs, whichever way it went.
///
/// The preview borrows the one texture the game's frames go into, so the
/// picture behind this panel is now the sample and has to be asked for again;
/// it costs one upload, and only when something changed. And the request is
/// spent either way: a preview that cannot be drawn cannot be drawn sixty
/// times a second either, and every attempt would be another line in the log.
fn refreshed(app: &mut App) {
    app.frame_dirty |= app.session.is_some();
    app.preview_dirty = false;
}

/// Every choice that changes what the screen looks like, with the picture
/// above them showing what they do.
fn settings(ctx: &egui::Context, app: &mut App) {
    // Read before the closure takes the rest of `app`: the rows say what the
    // settings are and push actions, and the two cannot borrow at once.
    let settings = app.settings.clone();
    let audio_ms = app.audio_ms;
    let has_file = app.has_palette_file;
    widgets::panel(ctx, "settings", "Settings", None, false, |ui| {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, width * 0.75), Sense::hover());
        match &app.preview {
            Some(texture) => {
                ui.painter().image(
                    texture.id(),
                    rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(CORNER_MEDIUM as u8), WELL);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    PREPARING,
                    egui::FontId::proportional(14.0),
                    ON_SURFACE_VARIANT,
                );
            }
        }

        widgets::section(ui, "Shape");
        if let Some(a) =
            widgets::choice_row(ui, "shape", &Aspect::ALL, settings.aspect, Aspect::label)
        {
            app.actions.push(Action::SetAspect(a));
        }
        widgets::note(ui, "Pixel-perfect drops to the next whole multiple");

        widgets::section(ui, "Look");
        if let Some(l) = widgets::choice_row(ui, "look", &Look::ALL, settings.look, Look::label) {
            app.actions.push(Action::SetLook(l));
        }
        widgets::note(ui, settings.look.note());

        widgets::section(ui, "Colours");
        if let Some(p) = widgets::choice_row(
            ui,
            "colours",
            &PaletteChoice::ALL,
            settings.palette,
            PaletteChoice::label,
        ) {
            // Choosing a file with no file imported asks for one rather than
            // quietly selecting a palette that would fall back to Standard.
            app.actions.push(if p == PaletteChoice::File && !has_file {
                Action::ImportPalette
            } else {
                Action::SetPalette(p)
            });
        }
        widgets::note(ui, settings.palette.note());
        if settings.palette == PaletteChoice::File
            && widgets::value_row(ui, "Palette file", "Load a different .pal file", "Replace")
                .clicked()
        {
            app.actions.push(Action::ImportPalette);
        }

        widgets::section(ui, "Picture");
        let mut trim = settings.trim_edges;
        if widgets::switch_row(
            ui,
            "Trim the edges",
            "Hides the 8 rows a television lost to overscan",
            &mut trim,
        ) {
            app.actions.push(Action::SetTrim(trim));
        }

        widgets::section(ui, "Controls");
        if widgets::value_row(
            ui,
            "Controller buttons",
            "Map A, B, Select and Start for a controller or the keyboard",
            "Set up",
        )
        .clicked()
        {
            app.actions.push(Action::StartWizard);
        }

        widgets::section(ui, "About");
        widgets::info_row(
            ui,
            "Audio delay",
            &format!("{audio_ms:.1} ms"),
            "Sound queued ahead of the speaker",
        );
        widgets::info_row(
            ui,
            "Version",
            env!("CARGO_PKG_VERSION"),
            "Emulia, on this computer",
        );
        if widgets::value_row(ui, "Problem log", "What went wrong, and why", "Open").clicked() {
            app.actions.push(Action::OpenProblems);
        }
        ui.add_space(12.0);
        if widgets::primary(ui, "Done").clicked() {
            app.actions.push(Action::CloseSettings);
        }
    });
}

/// The four presses that teach a controller — or the keyboard — which of its
/// buttons are A, B, Select and Start. The shell feeds it the presses; this
/// only says which one it is waiting for.
fn mapping(ctx: &egui::Context, app: &mut App) {
    let Some(wizard) = &app.wizard else {
        return;
    };
    let step = wizard.step();
    let device = wizard.device_name().map(str::to_string);
    widgets::panel(
        ctx,
        "mapping",
        "Controller buttons",
        Some("Press each button on the controller or keyboard you want to use."),
        false,
        |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                // The last step's name is held once all four are in: the
                // wizard is finished on the same press, and a panel that
                // blanks for the frame in between reads as a mistake.
                let waiting = NesButton::ORDER[step.min(NesButton::ORDER.len() - 1)].label();
                // One space, where Android has two: at 44 points a doubled
                // space is a hole in the biggest line on the screen.
                ui.label(
                    RichText::new(format!("Press {waiting}"))
                        .size(44.0)
                        .strong()
                        .color(LEAF),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    for (i, b) in NesButton::ORDER.iter().enumerate() {
                        let done = i < step;
                        let text = if done {
                            format!("{DONE} {}", b.label())
                        } else {
                            b.label().to_string()
                        };
                        ui.label(RichText::new(text).size(15.0).color(if done {
                            LEAF
                        } else {
                            ON_SURFACE_VARIANT
                        }));
                    }
                });
                // Held at the last step for the same reason the name above
                // it is: all four are in for the frame before the wizard is
                // put away, and "Step 5 of 4" is nobody's fourth step.
                let of = NesButton::ORDER.len();
                widgets::note(ui, &format!("Step {} of {of}", (step + 1).min(of)));
                // Only once something has pressed: until then there is no
                // device, and the four steps all belong to whichever presses
                // first.
                if let Some(name) = device {
                    widgets::note(ui, &name);
                }
            });
            ui.add_space(16.0);
            if widgets::secondary(ui, "Cancel").clicked() {
                app.actions.push(Action::CancelWizard);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::test_rom;
    use crate::picture::model;
    use egui::epaint::ClippedShape;
    use std::path::PathBuf;

    /// The window every one of these is drawn in unless it says otherwise.
    const WINDOW: Vec2 = Vec2::new(1024.0, 1800.0);

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

    /// One frame of the panel, through the dispatch `show` itself uses.
    /// Everything but the preview, which needs a live GL context and so cannot
    /// be run in a test; without one the panel says it is preparing a preview,
    /// which is the other half of what it has to draw properly anyway.
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
        let mut output = ctx.run_ui(input, |ui| panels(ui.ctx(), app));
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
    /// the button.
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

    /// Where the one switch on the panel is. It has no label to be found by,
    /// so it is found by its shape: the only 52-by-30 rounded rect drawn.
    fn switch(shapes: &[ClippedShape]) -> egui::Pos2 {
        fn walk(shape: &egui::Shape, found: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Rect(r) if (r.rect.size() - Vec2::new(52.0, 30.0)).length() < 0.5 => {
                    found.push(r.rect.center())
                }
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut found);
        }
        assert_eq!(found.len(), 1, "{} switches drawn", found.len());
        found[0]
    }

    fn at(shapes: &[ClippedShape], label: &str) -> egui::Pos2 {
        let found = spots(shapes, label);
        assert_eq!(found.len(), 1, "{label} appears {} times", found.len());
        found[0]
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

    /// Both panels at three widths, with each palette chosen — the row for
    /// replacing a file only exists for one of them — and twice each, because
    /// egui settles a layout over two frames and the second is the one that
    /// has to hold up. None of them may ask for anything nobody touched.
    #[test]
    fn the_panels_draw_and_a_frame_nobody_touched_asks_for_nothing() {
        let dir = temp_dir("settings-draw");
        let ctx = context();
        let mut app = App::blank(&dir);
        app.wizard = Some(crate::input::Wizard::new());
        for palette in PaletteChoice::ALL {
            app.settings.palette = palette;
            app.has_palette_file = palette == PaletteChoice::File;
            for panel in [Panel::Settings, Panel::Mapping] {
                app.panel = panel;
                // Roomy, tight, and narrower than the card asks to be.
                for width in [1440.0, 640.0, 160.0] {
                    for _ in 0..2 {
                        pass(&mut app, &ctx, Vec2::new(width, 600.0), Vec::new());
                    }
                }
            }
        }
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Every chip asks for the thing it is named after. Three rows of them
    /// share one panel, and a row that raised another row's action would
    /// change a setting nobody touched.
    #[test]
    fn each_row_of_chips_asks_for_what_it_says() {
        let dir = temp_dir("settings-chips");
        let ctx = context();
        let mut app = App::blank(&dir);
        app.panel = Panel::Settings;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        let said = texts(&shapes);
        for line in [
            "SHAPE",
            "LOOK",
            "COLOURS",
            "PICTURE",
            "CONTROLS",
            "ABOUT",
            PREPARING,
            "Pixel-perfect drops to the next whole multiple",
            "Trim the edges",
            "Controller buttons",
            "Audio delay",
            "Version",
            "Problem log",
            "Done",
        ] {
            assert!(said.iter().any(|t| t == line), "no {line:?} in {said:?}");
        }
        click(&mut app, &ctx, at(&shapes, Aspect::Pixels.label()));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SetAspect(Aspect::Pixels)]
        );
        click(&mut app, &ctx, at(&shapes, Look::Scanlines.label()));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SetLook(Look::Scanlines)]
        );
        click(&mut app, &ctx, at(&shapes, PaletteChoice::Vivid.label()));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SetPalette(PaletteChoice::Vivid)]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// "From a file" with no file imported asks for one: selecting it would
    /// otherwise be a palette that quietly falls back to Standard. Once there
    /// is a file it is a choice like any other, with a row for replacing it.
    #[test]
    fn choosing_a_file_palette_asks_for_a_file_until_there_is_one() {
        let dir = temp_dir("settings-palette");
        let ctx = context();
        let mut app = App::blank(&dir);
        app.panel = Panel::Settings;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        assert!(!texts(&shapes).iter().any(|t| t == "Palette file"));
        click(&mut app, &ctx, at(&shapes, PaletteChoice::File.label()));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::ImportPalette]
        );
        // What `Action::OpenSettings` asks the disk once, so that the panel
        // does not ask it sixty times a second.
        crate::settings::store_palette(&dir, &model::bytes(&model::standard())).unwrap();
        app.has_palette_file = crate::settings::imported_palette(&dir).is_some();
        assert!(app.has_palette_file);
        let shapes = drawn(&mut app, &ctx, WINDOW);
        click(&mut app, &ctx, at(&shapes, PaletteChoice::File.label()));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SetPalette(PaletteChoice::File)]
        );
        // The row for replacing it belongs to the choice, not to the file:
        // it is there once the imported palette is the one in use.
        assert!(!texts(&shapes).iter().any(|t| t == "Palette file"));
        app.settings.palette = PaletteChoice::File;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        assert!(texts(&shapes).iter().any(|t| t == "Palette file"));
        click(&mut app, &ctx, at(&shapes, "Replace"));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::ImportPalette]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The rows under the chips, each of which leaves the panel or changes
    /// something that is not a picture.
    #[test]
    fn the_rows_open_what_they_name_and_done_closes_the_panel() {
        let dir = temp_dir("settings-rows");
        let ctx = context();
        let mut app = App::blank(&dir);
        app.panel = Panel::Settings;
        let shapes = drawn(&mut app, &ctx, WINDOW);
        click(&mut app, &ctx, at(&shapes, "Set up"));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::StartWizard]);
        click(&mut app, &ctx, at(&shapes, "Open"));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::OpenProblems]);
        click(&mut app, &ctx, at(&shapes, "Done"));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::CloseSettings]
        );
        click(&mut app, &ctx, switch(&shapes));
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::SetTrim(true)]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The wizard says which button it is waiting for, how far through it is,
    /// and offers the way out. Four presses is four steps, and the fourth one
    /// still names a button rather than running off the end of the four.
    #[test]
    fn the_mapping_panel_counts_the_four_presses() {
        let dir = temp_dir("settings-mapping");
        let ctx = context();
        let mut app = App::blank(&dir);
        app.panel = Panel::Mapping;
        // No wizard is no panel: the shell only ever raises the two together.
        assert!(texts(&drawn(&mut app, &ctx, WINDOW)).is_empty());
        app.wizard = Some(crate::input::Wizard::new());
        let shapes = drawn(&mut app, &ctx, WINDOW);
        let said = texts(&shapes);
        for line in [
            "Controller buttons",
            "Press each button on the controller or keyboard you want to use.",
            "Press A",
            "Step 1 of 4",
            "Cancel",
        ] {
            assert!(said.iter().any(|t| t == line), "no {line:?} in {said:?}");
        }
        for (step, waiting) in ["B", "Select", "Start", "Start"].iter().enumerate() {
            // The four presses, the last of which the shell answers by
            // putting the wizard away; the panel holds still for that frame.
            app.wizard.as_mut().unwrap().press(
                crate::settings::Profiles::KEYBOARD,
                "Keyboard",
                &format!("key{step}"),
            );
            let said = texts(&drawn(&mut app, &ctx, WINDOW));
            let step_line = format!("Step {} of 4", (step + 2).min(4));
            assert!(
                said.iter().any(|t| t == &format!("Press {waiting}")),
                "{said:?}"
            );
            assert!(said.contains(&step_line), "no {step_line:?} in {said:?}");
            // The device that pressed first owns the rest of the steps, and
            // the panel says which one it is.
            assert!(said.iter().any(|t| t == "Keyboard"), "{said:?}");
            // A button already learned is shown with its mark against it.
            assert!(
                said.iter()
                    .any(|t| t == &format!("{DONE} {}", NesButton::ORDER[step].label())),
                "{said:?}"
            );
        }
        let shapes = drawn(&mut app, &ctx, WINDOW);
        click(&mut app, &ctx, at(&shapes, "Cancel"));
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::CancelWizard]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// An attempt at a preview is spent whether or not it worked, and it
    /// leaves the game's own frame needing to be uploaded again, because the
    /// two of them share one texture. Sixty failed attempts a second would be
    /// sixty lines in the problem log.
    #[test]
    fn an_attempt_at_a_preview_is_spent_and_costs_the_game_its_frame() {
        let dir = temp_dir("settings-refresh");
        let mut app = App::blank(&dir);
        app.preview_dirty = true;
        refreshed(&mut app);
        assert!(!app.preview_dirty);
        // Nothing is showing the frame texture, so nothing has to be put back.
        assert!(!app.frame_dirty);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = crate::session::Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        app.preview_dirty = true;
        refreshed(&mut app);
        assert!(!app.preview_dirty);
        assert!(app.frame_dirty);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A glyph the bundled font cannot draw comes out as a tofu box. The mark
    /// beside a learned button and the ellipsis in the placeholder are the two
    /// this panel borrows from outside the Latin alphabet.
    #[test]
    fn the_settings_glyphs_are_ones_the_font_has() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let font = egui::FontId::proportional(14.0);
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, DONE)));
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, PREPARING)));
    }
}
