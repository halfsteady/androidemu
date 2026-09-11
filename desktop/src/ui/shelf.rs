//! The shelf: what you have, most recently played first, and the way in.
//!
//! One header row rather than a stacked masthead, so the games start near the
//! top of the window instead of below three paragraphs — the grid is the point
//! of this screen and it should look like it.
//!
//! Nothing here touches the engine or writes to disk. Every control pushes an
//! `Action` and the shell applies it once the frame is over.

use super::theme::*;
use super::widgets;
use super::Action;
use crate::library::{playtime, Game};
use crate::shell::App;
use egui::{pos2, CornerRadius, FontId, Rect, RichText, Sense, TextFormat, Vec2};

/// The same two measurements the Android shelf scales off. A window narrower
/// than `TIGHT` is a phone screen and gets smaller cards; one at least `ROOMY`
/// has room for the controls beside the heading rather than under it.
const TIGHT: f32 = 600.0;
const ROOMY: f32 = 820.0;

/// A ROM's filename is its title until somebody renames it, and filenames are
/// long. Two lines then an ellipsis, the same as Android's `maxLines = 2`, and
/// the box is always those two lines tall: a title allowed to run on makes one
/// card three times the height of the card beside it and the grid goes ragged.
const TITLE_LINES: usize = 2;

/// The card's "play this" line ends in an arrow. Not "→": the bundled
/// proportional font has no glyph for U+2192 and would draw a tofu box, the
/// same way it has none for the "←" that `widgets::BACK` avoids.
const ONWARD: &str = "›";
/// A card's menu button. Android draws a vertical-dots icon; this font has no
/// dots glyph either, so the ellipsis it does have stands in.
const MORE: &str = "…";

pub fn show(ui: &mut egui::Ui, app: &mut App) {
    let width = ui.available_width();
    let (tight, roomy) = (width < TIGHT, width >= ROOMY);
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(BACKGROUND)
                .inner_margin(if tight { 14.0 } else { 24.0 }),
        )
        .show(ui, |ui| {
            // The index is read once a frame, not once a widget: the shelf and
            // the archive are the same file.
            let games = if app.show_archive {
                app.library.archived()
            } else {
                app.library.games()
            };
            let archived = if app.show_archive {
                games.len()
            } else {
                app.library.archived().len()
            };
            // Beside the heading where there is room for it, underneath where
            // there is not. A narrow window should still not push a button off
            // the edge, so the row it lands in scrolls sideways.
            if roomy {
                ui.horizontal(|ui| {
                    heading(ui, app.show_archive);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        actions(ui, app, archived, !games.is_empty(), true)
                    });
                });
            } else {
                heading(ui, app.show_archive);
                egui::ScrollArea::horizontal()
                    .id_salt("shelf-actions")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| actions(ui, app, archived, !games.is_empty(), false));
                    });
            }
            ui.add_space(16.0);
            if games.is_empty() {
                empty(ui, app.show_archive);
                return;
            }
            let list = app.settings.shelf_list;
            egui::ScrollArea::vertical()
                .id_salt("shelf")
                .show(ui, |ui| {
                    if list {
                        for game in &games {
                            row(ui, app, game);
                        }
                        return;
                    }
                    // 220 points is a desktop's cell. In a phone-shaped window it
                    // would be one column, which makes a single game fill the whole
                    // shelf and turns it into a slideshow.
                    let room = ui.available_width().max(1.0);
                    // Never wider than the panel: a card that sets a width the
                    // window does not have is a card with its menu off the edge.
                    let card_width: f32 = if tight { 150.0 } else { 220.0 };
                    let card_width = card_width.min(room);
                    let gap = ui.spacing().item_spacing.x;
                    let columns = ((room + gap) / (card_width + gap)).floor();
                    let columns = columns.max(1.0) as usize;
                    for chunk in games.chunks(columns) {
                        ui.horizontal_top(|ui| {
                            for game in chunk {
                                card(ui, app, game, card_width, tight);
                            }
                        });
                    }
                });
        });
}

/// Whose shelf this is. The one thing a masthead is for.
fn heading(ui: &mut egui::Ui, archive: bool) {
    let text = if archive {
        RichText::new("Put away").size(22.0).color(ON_SURFACE)
    } else {
        RichText::new("EMULIA")
            .size(20.0)
            .extra_letter_spacing(3.0)
            .color(LEAF)
    };
    ui.label(text.strong());
}

/// The controls beside the heading, in the order the Android shelf draws them:
/// add, settings, the archive, and the view toggle last, because it changes how
/// this screen looks rather than what is on it.
///
/// A right-to-left row lays its contents out from the right edge, so it walks
/// the same list backwards and both layouts read the same way.
fn actions(ui: &mut egui::Ui, app: &mut App, archived: usize, any_games: bool, reversed: bool) {
    if app.show_archive {
        if widgets::compact(ui, "Back to the shelf", true).clicked() {
            app.actions.push(Action::ShowArchive(false));
        }
        return;
    }
    let mut row = vec![
        ("Add a game".to_string(), Action::Import),
        ("Settings".to_string(), Action::OpenSettings),
    ];
    // Only offered when there is something in it, so an empty shelf does not
    // advertise an empty cupboard.
    if archived > 0 {
        row.push((format!("Put away ({archived})"), Action::ShowArchive(true)));
    }
    if any_games {
        let label = if app.settings.shelf_list {
            "Cards"
        } else {
            "List"
        };
        row.push((label.to_string(), Action::ToggleShelfList));
    }
    if reversed {
        row.reverse();
    }
    for (label, action) in row {
        if widgets::compact(ui, &label, true).clicked() {
            app.actions.push(action);
        }
    }
}

fn empty(ui: &mut egui::Ui, archive: bool) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.25);
        ui.label(
            RichText::new("EMULIA")
                .size(44.0)
                .strong()
                .extra_letter_spacing(6.0)
                .color(LEAF),
        );
        ui.add_space(12.0);
        if archive {
            ui.label(
                RichText::new("Nothing is put away.")
                    .size(20.0)
                    .color(ON_SURFACE),
            );
            return;
        }
        ui.label(
            RichText::new("A shelf full of possibilities")
                .size(20.0)
                .strong()
                .color(ON_SURFACE),
        );
        ui.add_space(8.0);
        widgets::note(ui, "Add a game file (.nes) from your computer to begin.");
        widgets::note(ui, "Games stay on this device. No account needed.");
    });
}

/// The part of a picture that fills a box without squashing it: the largest
/// centred rectangle of the box's shape, in texture coordinates.
///
/// Cropped rather than fitted, because a shelf of letterboxed covers is a shelf
/// of grey bars. A picture with no size at all is drawn whole rather than
/// dividing by zero.
fn crop_uv(image: Vec2, box_size: Vec2) -> Rect {
    let whole = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if image.x <= 0.0 || image.y <= 0.0 || box_size.x <= 0.0 || box_size.y <= 0.0 {
        return whole;
    }
    let scale = (box_size.x / image.x).max(box_size.y / image.y);
    let shown = image * scale;
    let u = ((shown.x - box_size.x) / shown.x) / 2.0;
    let v = ((shown.y - box_size.y) / shown.y) / 2.0;
    Rect::from_min_max(pos2(u, v), pos2(1.0 - u, 1.0 - v))
}

/// A game's name, wrapped to at most `TITLE_LINES` lines with an ellipsis, in a
/// box exactly that many lines tall whatever it says.
fn title(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    let width = ui.available_width();
    let font = FontId::proportional(size);
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        TextFormat {
            font_id: font.clone(),
            color: ON_SURFACE,
            ..Default::default()
        },
    );
    job.wrap.max_width = width;
    job.wrap.max_rows = TITLE_LINES;
    // Broken between words, not through them: half a filename on each line
    // reads as neither.
    job.wrap.break_anywhere = false;
    let galley = ui.painter().layout_job(job);
    let lines = ui.ctx().fonts_mut(|f| f.row_height(&font)) * TITLE_LINES as f32;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, lines.max(galley.size().y)), Sense::click());
    ui.painter().galley(rect.min, galley, ON_SURFACE);
    response
}

fn cover(ui: &mut egui::Ui, app: &mut App, game: &Game, rect: Rect) {
    match app.cover_texture(ui.ctx(), game) {
        // Rounded like the placeholder it stands in for: one shape, whether or
        // not a game has a picture yet.
        Some(texture) => {
            let sized = egui::load::SizedTexture::new(texture.id(), texture.size_vec2());
            egui::Image::from_texture(sized)
                .uv(crop_uv(texture.size_vec2(), rect.size()))
                .corner_radius(CornerRadius::same(CORNER_SMALL as u8))
                .paint_at(ui, rect);
        }
        None => widgets::cover_placeholder(ui, rect, &game.title),
    }
}

/// Where you got to. A game with an autosave is one you are in the middle of.
fn status(app: &App, game: &Game) -> &'static str {
    if app.library.has_autosave(&game.id) {
        "Resume"
    } else {
        "Ready to play"
    }
}

/// The line under the title: where you got to, and how long you have been
/// there. A put-away game says only the second — it is not something you are in
/// the middle of until it is back on the shelf.
fn subtitle(app: &App, game: &Game) -> String {
    let mut parts = Vec::new();
    if !app.show_archive {
        parts.push(status(app, game).to_string());
    }
    if let Some(time) = playtime(game.seconds) {
        parts.push(time);
    }
    parts.join(" · ")
}

/// One line of a card's menu. Every item says what colour it is: egui would
/// otherwise resolve the label through the style, which paints a pressed item
/// in the chosen-chip colour.
fn item(ui: &mut egui::Ui, label: &str, danger: bool) -> egui::Response {
    let colour = if danger { ERROR } else { ON_SURFACE };
    ui.button(RichText::new(label).size(15.0).color(colour))
}

/// A card's menu is named after its game rather than after where its button
/// landed, so a shelf that reorders under an open menu — the game just played
/// moving to the front — does not leave it pointing at the neighbour. A game is
/// on the shelf once, so the name is its own.
fn menu_id(game: &str) -> egui::Id {
    egui::Id::new(("game-menu", game))
}

/// The card's and the row's menu, so the two can never drift apart.
///
/// Box art and putting a game away used to be text buttons on the face of every
/// card, one of which took the game off the shelf from directly under the
/// pointer aiming to start it. They live here now: still one click away, no
/// longer in the way of the game.
fn menu(ui: &mut egui::Ui, app: &mut App, game: &Game) {
    let response = widgets::compact(ui, MORE, true);
    egui::Popup::menu(&response)
        .id(menu_id(&game.id))
        .show(|ui| {
            ui.set_min_width(180.0);
            if app.show_archive {
                if item(ui, "Bring back", false).clicked() {
                    app.actions.push(Action::SetArchived(game.clone(), false));
                }
                if item(ui, "Delete", true).clicked() {
                    app.actions.push(Action::DeleteRequested(game.clone()));
                }
                return;
            }
            if item(ui, "Choose box art", false).clicked() {
                app.actions.push(Action::ChooseArt(game.clone()));
            }
            if app.library.art_path(&game.id).exists() && item(ui, "Clear box art", false).clicked()
            {
                app.actions.push(Action::ClearArt(game.clone()));
            }
            if item(ui, "Put this away", false).clicked() {
                app.actions.push(Action::SetArchived(game.clone(), true));
            }
        });
}

/// A game on the shelf. The whole cover is one thing to click, and what it does
/// is play — which is the only reason to be on this screen.
///
/// A put-away game does not open on a click: the card would otherwise be a trap
/// next to "Bring back".
fn card(ui: &mut egui::Ui, app: &mut App, game: &Game, width: f32, tight: bool) -> Rect {
    egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(CORNER_LARGE as u8))
        .inner_margin(12.0)
        .show(ui, |ui| {
            // A card is a column even when the shelf holding it is a row. A
            // frame inherits the layout it was dropped into, and in the grid
            // that is left to right, which would stand the cover beside the
            // title rather than above it.
            ui.vertical(|ui| {
                let inner = (width - 24.0).max(0.0);
                ui.set_width(inner);
                let (rect, picture) =
                    ui.allocate_exact_size(Vec2::new(inner, inner * 0.75), Sense::click());
                cover(ui, app, game, rect);
                let mut open = picture.clicked();
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        // The menu button's share of the row, kept back so a
                        // long title wraps instead of pushing the menu off the
                        // card.
                        ui.set_width((inner - 50.0).max(0.0));
                        open |= title(ui, &game.title, if tight { 15.0 } else { 19.0 }).clicked();
                        if !app.show_archive {
                            ui.label(
                                RichText::new(format!("{}  {ONWARD}", status(app, game)))
                                    .size(if tight { 13.0 } else { 15.0 })
                                    .color(LEAF),
                            );
                        }
                        if let Some(time) = playtime(game.seconds) {
                            widgets::note(ui, &time);
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        menu(ui, app, game)
                    });
                });
                if open && !app.show_archive {
                    app.actions.push(Action::OpenGame(game.clone()));
                }
            });
        })
        .response
        .rect
}

/// A game as one row: a small piece of box art, the title, and where you got
/// to. The same click target and the same menu as the card — the only thing
/// that changes is how much of the shelf one game is allowed to take.
fn row(ui: &mut egui::Ui, app: &mut App, game: &Game) {
    egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(CORNER_MEDIUM as u8))
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (rect, picture) =
                    ui.allocate_exact_size(Vec2::new(76.0 * 4.0 / 3.0, 76.0), Sense::click());
                cover(ui, app, game, rect);
                let mut open = picture.clicked();
                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 50.0).max(0.0));
                    open |= title(ui, &game.title, 16.0).clicked();
                    let note = subtitle(app, game);
                    if !note.is_empty() {
                        widgets::note(ui, &note);
                    }
                });
                if open && !app.show_archive {
                    app.actions.push(Action::OpenGame(game.clone()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    menu(ui, app, game)
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;
    use std::path::{Path, PathBuf};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Everything the shelf reads, and nothing else: no window, no engine, no
    /// audio. `show` is a pure function of this and the `Ui` it is handed.
    fn app(dir: &Path) -> App {
        App {
            data_dir: dir.to_path_buf(),
            library: Library::open(dir).unwrap(),
            settings: crate::settings::Settings::load(dir),
            settings_dirty: false,
            input: crate::input::Input::new(crate::settings::Profiles::load(dir)),
            session: None,
            panel: crate::ui::Panel::None,
            panel_before: crate::ui::Panel::None,
            dialog: None,
            show_archive: false,
            notice: None,
            message: None,
            busy: false,
            fullscreen: false,
            chrome_until: std::time::Instant::now(),
            wizard: None,
            scrub_fraction: 0.0,
            rewind_depth: 0,
            audio_ms: 0.0,
            frame_dirty: false,
            covers: std::collections::HashMap::new(),
            thumbs: std::collections::HashMap::new(),
            preview: None,
            preview_dirty: false,
            actions: Vec::new(),
            quit: false,
        }
    }

    /// One frame of the shelf at a given window size. The texture delta has to
    /// be taken or dropping it panics.
    fn pass(app: &mut App, ctx: &egui::Context, size: Vec2) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), size)),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| show(ui, app)).textures_delta.clear();
    }

    /// The shelf has states no click can reach from a test — the archive, the
    /// list, an empty cupboard, a window narrower than a card. Every one of
    /// them is drawn here, twice, because egui settles a layout over two
    /// frames and the second is the one that has to hold up.
    #[test]
    fn every_shelf_draws_and_a_frame_nobody_touched_asks_for_nothing() {
        let dir = temp_dir("shelf-draw");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = app(&dir);
        // An empty shelf, then an empty archive.
        for archive in [false, true] {
            app.show_archive = archive;
            pass(&mut app, &ctx, Vec2::new(1024.0, 768.0));
        }
        app.show_archive = false;
        app.library.add("aaaa", "Game A", b"rom a").unwrap();
        let long = "a filename nobody would choose to read twice, and yet";
        app.library.add("bbbb", long, b"rom b").unwrap();
        app.library.set_archived("bbbb", true).unwrap();
        // A cover to crop: the autosave thumbnail of a game half-played.
        let thumbnail = app
            .library
            .thumbnail_path("aaaa", crate::library::Slot::Auto);
        let pixels = vec![0x7f; 256 * 240 * 4];
        crate::files::write_png(&thumbnail, 256, 240, &pixels).unwrap();
        for archive in [false, true] {
            app.show_archive = archive;
            // Both menus, drawn rather than left to a click nobody can make
            // from here: "Bring back" and "Delete" in the archive, box art and
            // "Put this away" on the shelf.
            let open = if archive { "bbbb" } else { "aaaa" };
            for list in [false, true] {
                app.settings.shelf_list = list;
                // Roomy, tight, and narrower than one card.
                for width in [1440.0, 520.0, 120.0] {
                    pass(&mut app, &ctx, Vec2::new(width, 600.0));
                    egui::Popup::open_id(&ctx, menu_id(open));
                    pass(&mut app, &ctx, Vec2::new(width, 600.0));
                    assert!(egui::Popup::is_id_open(&ctx, menu_id(open)));
                    egui::Popup::close_id(&ctx, menu_id(open));
                }
            }
        }
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Two cards side by side are the same height whatever their games are
    /// called. A shelf of ROMs is a shelf of filenames, and one of them is
    /// always long enough to have made its card three times its neighbour's.
    #[test]
    fn a_long_title_does_not_make_a_taller_card() {
        let dir = temp_dir("shelf-title");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = app(&dir);
        let short = app.library.add("aaaa", "Game A", b"rom a").unwrap();
        let long = "a filename nobody would choose to read twice, and yet";
        let long = app.library.add("bbbb", long, b"rom b").unwrap();
        assert_eq!(long.title.chars().count(), 53);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                pos2(0.0, 0.0),
                Vec2::new(1024.0, 768.0),
            )),
            ..Default::default()
        };
        let mut heights = Vec::new();
        ctx.run_ui(input, |ui| {
            ui.horizontal_top(|ui| {
                for game in [&short, &long] {
                    heights.push(card(ui, &mut app, game, 220.0, false).height());
                }
            });
        })
        .textures_delta
        .clear();
        // The title has to be one that would have needed clamping, or the
        // test would pass with no clamp at all.
        let column = 220.0 - 24.0 - 50.0;
        let rows = ctx.fonts_mut(|f| {
            f.layout(
                long.title.clone(),
                FontId::proportional(19.0),
                ON_SURFACE,
                column,
            )
            .rows
            .len()
        });
        assert!(rows > TITLE_LINES, "nothing to clamp: {rows} rows");
        assert_eq!(heights.len(), 2);
        assert!((heights[0] - heights[1]).abs() < 0.5, "{heights:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A box the same shape as the picture shows all of it.
    #[test]
    fn a_cover_the_shape_of_its_box_is_not_cropped() {
        let uv = crop_uv(Vec2::new(256.0, 192.0), Vec2::new(128.0, 96.0));
        assert!((uv.min.x - 0.0).abs() < 1e-6, "{uv:?}");
        assert!((uv.min.y - 0.0).abs() < 1e-6, "{uv:?}");
        assert!((uv.max.x - 1.0).abs() < 1e-6, "{uv:?}");
        assert!((uv.max.y - 1.0).abs() < 1e-6, "{uv:?}");
    }

    /// A wide picture in a 4:3 box loses the sides, evenly, and keeps all of
    /// its height. A NES frame is 256x240, which is exactly this case.
    #[test]
    fn a_wide_cover_loses_its_sides_and_keeps_its_height() {
        let uv = crop_uv(Vec2::new(400.0, 200.0), Vec2::new(200.0, 150.0));
        assert!(uv.min.x > 0.0 && uv.min.x < 0.5, "{uv:?}");
        assert!(
            (uv.min.x + uv.max.x - 1.0).abs() < 1e-6,
            "not centred: {uv:?}"
        );
        assert!((uv.min.y - 0.0).abs() < 1e-6, "{uv:?}");
        assert!((uv.max.y - 1.0).abs() < 1e-6, "{uv:?}");
        // 200 tall scaled to 150 makes it 300 wide; 200 of those are shown.
        assert!((uv.width() * 300.0 - 200.0).abs() < 1e-3, "{uv:?}");
    }

    /// And a tall one loses its top and bottom instead.
    #[test]
    fn a_tall_cover_loses_its_top_and_bottom() {
        let uv = crop_uv(Vec2::new(200.0, 400.0), Vec2::new(200.0, 150.0));
        assert!((uv.min.x - 0.0).abs() < 1e-6, "{uv:?}");
        assert!((uv.max.x - 1.0).abs() < 1e-6, "{uv:?}");
        assert!(uv.min.y > 0.0 && uv.min.y < 0.5, "{uv:?}");
        assert!(
            (uv.min.y + uv.max.y - 1.0).abs() < 1e-6,
            "not centred: {uv:?}"
        );
    }

    /// A texture that reports no size must not become a rectangle of NaN.
    #[test]
    fn a_cover_with_no_size_is_drawn_whole() {
        let uv = crop_uv(Vec2::ZERO, Vec2::new(200.0, 150.0));
        assert_eq!(uv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
        let uv = crop_uv(Vec2::new(200.0, 150.0), Vec2::ZERO);
        assert_eq!(uv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)));
    }

    /// The shelf's two glyphs have to be ones the bundled font can draw. It has
    /// neither "→" nor "⋯", and a tofu box on the line that says "play this" is
    /// worse than no arrow at all.
    #[test]
    fn the_shelf_glyphs_are_ones_the_font_has() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let font = egui::FontId::proportional(14.0);
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, ONWARD)));
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, MORE)));
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, " · ")));
        assert!(!ctx.fonts_mut(|f| f.has_glyphs(&font, "→")));
        assert!(!ctx.fonts_mut(|f| f.has_glyphs(&font, "⋯")));
    }
}
