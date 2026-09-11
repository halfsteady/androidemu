//! The window, the GL context, the main loop, and the App the panels draw
//! from. One thread: events, UI, emulation, picture, panels, swap.
//!
//! The order inside the loop is the whole design. Events arrive first so a
//! keypress is acted on in the frame it happened. The UI is built next and
//! ends its pass without painting, because the things it asked for have to be
//! applied before the engine runs — a pause has to stop this frame, not the
//! next one. Then the emulator runs, the picture is drawn, and only then are
//! the panels painted over it. Housekeeping saves what the frame changed.

use crate::bridge::{key_name, Bridge};
use crate::input::{Input, Wizard, WizardEvent};
use crate::library::{Game, Library, SaveSlot, Slot};
use crate::picture::{self, Viewport};
use crate::session::Session;
use crate::settings::{Profiles, Settings};
use crate::ui::{Action, Dialog, Panel};
use crate::video::Video;
use crate::Options;
use sdl2::event::{Event, WindowEvent};
use sdl2::keyboard::Scancode;
use sdl2::video::{FullscreenType, GLProfile, SwapInterval};
use sdl2::{GameControllerSubsystem, JoystickSubsystem};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the pointer and the full-screen chrome stay up after the last
/// sign of life.
pub const CHROME_IDLE: Duration = Duration::from_secs(5);
pub const NOTICE_TIME: Duration = Duration::from_millis(2200);
/// The bar across the top of the play screen, in points. The picture is fitted
/// below it so the chrome never covers the game.
pub const TITLE_BAR: f32 = 52.0;
pub const TIME_ROW: f32 = 84.0;
pub const END_OF_TAPE: &str = "That's as far back as this goes.";

/// Everything a drawn frame can read and everything the shell acts on. One
/// value, passed to the panels by `&mut`, so there is no state hiding in the
/// widgets between frames.
#[allow(dead_code)] // The panels that read these arrive in Tasks 11 to 14.
pub struct App {
    pub data_dir: PathBuf,
    pub library: Library,
    pub settings: Settings,
    pub settings_dirty: bool,
    pub input: Input,
    pub session: Option<Session>,
    pub panel: Panel,
    /// Where `ClosePanel` returns to when the problem log was opened from
    /// somewhere other than the shelf.
    pub panel_before: Panel,
    pub dialog: Option<Dialog>,
    pub show_archive: bool,
    pub notice: Option<(String, Instant)>,
    /// The plain sentence shown until it is dismissed.
    pub message: Option<String>,
    pub busy: bool,
    pub fullscreen: bool,
    pub chrome_until: Instant,
    pub wizard: Option<Wizard>,
    /// Where the time handle is, -1 to 1.
    pub scrub_fraction: f32,
    pub rewind_depth: usize,
    pub audio_ms: f32,
    /// Whether the framebuffer has changed since it was last handed to the
    /// GPU. A paused game shows the same 245 KB every tick; uploading it
    /// sixty times a second buys nothing.
    pub frame_dirty: bool,
    /// Cover art by game id, keyed on the file's mtime so replacing the
    /// picture replaces the texture.
    pub covers: HashMap<String, (i64, egui::TextureHandle)>,
    /// Slot thumbnails for the open game, keyed on the save's time.
    pub thumbs: HashMap<u8, (i64, egui::TextureHandle)>,
    pub preview: Option<egui::TextureHandle>,
    pub preview_dirty: bool,
    pub actions: Vec<Action>,
    pub quit: bool,
}

impl App {
    /// Says what went wrong in one plain sentence and keeps the detail in the
    /// problem log, where someone helping can find it.
    pub fn report(&mut self, label: &str, detail: &str) {
        self.library.log_problem(label, detail);
        self.message = Some(label.to_string());
    }

    pub fn notice(&mut self, text: &str) {
        self.notice = Some((text.to_string(), Instant::now()));
    }

    /// A game is running and nothing is in front of it.
    pub fn playing(&self) -> bool {
        self.session.is_some()
            && self.panel == Panel::None
            && self.dialog.is_none()
            && !self.busy
            && self.wizard.is_none()
    }

    fn texture(
        ctx: &egui::Context,
        name: &str,
        path: &std::path::Path,
    ) -> Option<egui::TextureHandle> {
        let (w, h, rgba) = crate::files::read_png(path).ok()?;
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        Some(ctx.load_texture(name, image, egui::TextureOptions::LINEAR))
    }

    fn mtime(path: &std::path::Path) -> i64 {
        std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64)
    }

    pub fn cover_texture(
        &mut self,
        ctx: &egui::Context,
        game: &Game,
    ) -> Option<egui::TextureHandle> {
        let path = self.library.cover(&game.id)?;
        let key = Self::mtime(&path);
        if let Some((k, t)) = self.covers.get(&game.id) {
            if *k == key {
                return Some(t.clone());
            }
        }
        let texture = Self::texture(ctx, &format!("cover-{}", game.id), &path)?;
        self.covers.insert(game.id.clone(), (key, texture.clone()));
        Some(texture)
    }

    #[allow(dead_code)] // The slots panel in Task 13.
    pub fn slot_texture(
        &mut self,
        ctx: &egui::Context,
        slot: &SaveSlot,
    ) -> Option<egui::TextureHandle> {
        let key = slot.time?;
        if let Some((k, t)) = self.thumbs.get(&slot.number) {
            if *k == key {
                return Some(t.clone());
            }
        }
        let texture = Self::texture(ctx, &format!("slot-{}", slot.number), &slot.thumbnail)?;
        self.thumbs.insert(slot.number, (key, texture.clone()));
        Some(texture)
    }

    /// The paused frame, else the newest saved moment on the shelf, else the
    /// built pattern; always the pattern when a palette other than Standard is
    /// chosen, because a saved PNG holds finished colour and cannot be
    /// restained.
    #[allow(dead_code)] // The settings preview in Task 14.
    pub fn sample_source(&self) -> Vec<u8> {
        let standard = self.settings.palette == picture::PaletteChoice::Standard;
        if let Some(session) = &self.session {
            if standard {
                return session.engine.frame().to_vec();
            }
        } else if standard {
            for game in self.library.games() {
                if let Ok((w, h, rgba)) =
                    crate::files::read_png(&self.library.thumbnail_path(&game.id, Slot::Auto))
                {
                    if (w, h) == (picture::WIDTH as u32, picture::HEIGHT as u32) {
                        return rgba;
                    }
                }
            }
        }
        picture::sample_frame(&self.settings.preview_colours(&self.data_dir))
    }

    /// Everything a screen reads and nothing else: no window, no engine, no
    /// audio. Every `show` is a pure function of this and the `Ui` it is
    /// handed, which is what lets the screens be drawn in a test at all.
    #[cfg(test)]
    pub fn blank(data_dir: &std::path::Path) -> App {
        App {
            data_dir: data_dir.to_path_buf(),
            library: Library::open(data_dir).unwrap(),
            settings: Settings::load(data_dir),
            settings_dirty: false,
            input: Input::new(Profiles::load(data_dir)),
            session: None,
            panel: Panel::None,
            panel_before: Panel::None,
            dialog: None,
            show_archive: false,
            notice: None,
            message: None,
            busy: false,
            fullscreen: false,
            chrome_until: Instant::now(),
            wizard: None,
            scrub_fraction: 0.0,
            rewind_depth: 0,
            audio_ms: 0.0,
            frame_dirty: false,
            covers: HashMap::new(),
            thumbs: HashMap::new(),
            preview: None,
            preview_dirty: false,
            actions: Vec::new(),
            quit: false,
        }
    }
}

/// What the panels draw, into the root `Ui` of the pass. Each screen arrives
/// with its own task; until then a running game is the picture with nothing on
/// top of it.
fn draw(ui: &mut egui::Ui, app: &mut App, _video: &mut Video) {
    if app.session.is_none() {
        crate::ui::shelf::show(ui, app);
    } else {
        crate::ui::play::show(ui, app);
    }
    // Task 13 draws the panels and Task 14 the settings preview, which is what
    // `_video` is for. The message is drawn last and over everything, because
    // it is the one thing that has to be read before anything else is worth
    // doing.
    if let Some(text) = app.message.clone() {
        if crate::ui::widgets::message_bar(ui.ctx(), &text) {
            app.actions.push(crate::ui::Action::CloseDialog);
        }
    }
}

/// Escape means "back one step", and what that step is depends on what is in
/// front of the game.
fn escape(app: &App) -> Action {
    match (
        app.dialog.is_some(),
        app.panel,
        app.wizard.is_some(),
        app.session.is_some(),
        app.fullscreen,
    ) {
        (true, ..) => Action::CloseDialog,
        (_, _, true, _, _) => Action::CancelWizard,
        (_, Panel::Problems, ..)
        | (_, Panel::Settings, ..)
        | (_, Panel::Slots, ..)
        | (_, Panel::Mapping, ..) => Action::ClosePanel,
        (_, Panel::Pause, ..) => Action::Resume,
        (_, Panel::None, _, true, _) => Action::Pause,
        (_, Panel::None, _, false, true) => Action::ToggleFullscreen,
        _ => Action::CloseDialog,
    }
}

/// A picture dropped on an open game is that game's cover; anything else is a
/// ROM to shelve.
fn dropped(app: &App, path: PathBuf) -> Action {
    let image = path.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "png" | "jpg" | "jpeg"
        )
    });
    match (&app.session, image) {
        (Some(session), true) => Action::ChooseArtFrom(session.game.clone(), path),
        _ => Action::ImportFrom(path),
    }
}

fn finish_wizard(app: &mut App, key: String, profile: crate::settings::Profile) {
    let name = profile.name.clone();
    app.input.set_profile(key, profile);
    app.wizard = None;
    app.panel = Panel::None;
    app.message = Some(format!(
        "Buttons saved for {name}. The directional pad and stick work automatically."
    ));
}

fn open_game(app: &mut App, game: Game, sdl: Option<&sdl2::Sdl>) {
    // Whatever is open is being put away, not abandoned: its autosave and its
    // playtime are written before the window belongs to something else. A
    // .nes dropped on a running game used to cost both.
    if let Some(mut open) = app.session.take() {
        open.close();
    }
    app.thumbs.clear();
    app.scrub_fraction = 0.0;
    app.input.clear();
    let audio = match sdl {
        Some(sdl) => match crate::open_audio(sdl) {
            Ok(a) => Some(a),
            Err(e) => {
                app.report("Sound couldn't be started.", &e);
                None
            }
        },
        None => None,
    };
    match Session::open(&app.library, game, audio) {
        Ok((mut session, warnings)) => {
            session
                .engine
                .set_palette(app.settings.colours(&app.data_dir));
            app.session = Some(session);
            app.frame_dirty = true;
            app.panel = if warnings.is_empty() {
                Panel::None
            } else {
                Panel::Pause
            };
            if let Some(w) = warnings.first() {
                app.message = Some(w.clone());
            }
        }
        Err(e) => app.report("That game couldn't be opened.", &e),
    }
}

/// The last game brought back or deleted takes the archive with it: a screen
/// whose whole subject is gone is not a screen to leave somebody standing on.
fn leave_empty_archive(app: &mut App) {
    if app.library.archived().is_empty() {
        app.show_archive = false;
    }
}

fn apply(app: &mut App, action: Action, window: &mut sdl2::video::Window, sdl: Option<&sdl2::Sdl>) {
    // Working the time controls is a sign of life even when the pointer has
    // not moved, and a drag the full-screen chrome fades out from under is a
    // drag that ends by accident.
    if matches!(
        action,
        Action::Scrub(_) | Action::ScrubReleased | Action::JumpBack(_)
    ) {
        app.chrome_until = Instant::now() + CHROME_IDLE;
    }
    match action {
        Action::OpenGame(game) => open_game(app, game, sdl),
        // The file dialogs are modal and block this thread, which is the one
        // drawing the window: the loop stops for as long as the dialog is up
        // and picks up again with the answer. Nothing is running behind it —
        // the shelf is the only screen that raises either of these.
        Action::Import => {
            let picked = rfd::FileDialog::new()
                .add_filter("NES game", &["nes"])
                .set_title("Add a game")
                .pick_file();
            if let Some(path) = picked {
                apply(app, Action::ImportFrom(path), window, sdl);
            }
        }
        Action::ImportFrom(path) => match crate::import(&app.library, &path) {
            Ok(game) => open_game(app, game, sdl),
            Err(e) => app.report("That game file didn't work.", &e),
        },
        Action::ChooseArt(game) => {
            let picked = rfd::FileDialog::new()
                .add_filter("Picture", &["png", "jpg", "jpeg"])
                .set_title("Choose box art")
                .pick_file();
            if let Some(path) = picked {
                apply(app, Action::ChooseArtFrom(game, path), window, sdl);
            }
        }
        // The cached texture goes with the file it was made from: a new
        // picture under the same name would otherwise keep showing the old one.
        Action::ChooseArtFrom(game, path) => match app.library.set_art(&game.id, &path) {
            Ok(()) => {
                app.covers.remove(&game.id);
            }
            Err(e) => app.report("That picture didn't work as box art.", &e),
        },
        Action::ClearArt(game) => {
            match app.library.clear_art(&game.id) {
                Ok(()) => {
                    app.message = Some(format!("{} is back to its last saved moment.", game.title))
                }
                Err(e) => app.report("The box art couldn't be cleared.", &e),
            }
            app.covers.remove(&game.id);
        }
        // Said out loud, in the Android shell's words: a card that vanishes
        // from the shelf should say where it went, and one that comes back
        // should say that nothing was lost.
        Action::SetArchived(game, archived) => match app.library.set_archived(&game.id, archived) {
            Ok(()) if archived => {
                app.message = Some(format!(
                    "{} is put away. Tap Put away on the shelf to bring it back.",
                    game.title
                ))
            }
            Ok(()) => {
                app.message = Some(format!(
                    "{} is back on the shelf, exactly where you left it.",
                    game.title
                ));
                leave_empty_archive(app);
            }
            Err(e) => app.report("The shelf couldn't be updated.", &e),
        },
        // Asked before done: deleting a game takes its saves with it. Task 13
        // draws the question; until then the dialog is raised and answered by
        // Escape, and nothing is lost.
        Action::DeleteRequested(game) => app.dialog = Some(Dialog::ConfirmDelete(game)),
        Action::DeleteConfirmed(game) => {
            match app.library.forget(&game.id) {
                Ok(()) => {
                    app.message = Some(format!(
                        "{} and its saves are gone from this device.",
                        game.title
                    ));
                    leave_empty_archive(app);
                }
                Err(e) => app.report("The game couldn't be deleted.", &e),
            }
            app.covers.remove(&game.id);
            app.dialog = None;
        }
        Action::ShowArchive(show) => app.show_archive = show,
        Action::ToggleShelfList => {
            app.settings.shelf_list = !app.settings.shelf_list;
            app.settings_dirty = true;
        }
        Action::Pause => {
            if let Some(session) = &mut app.session {
                session.paused = true;
                session.record_playtime();
                if let Err(e) = session.save(Slot::Auto) {
                    let title = session.game.title.clone();
                    app.report(
                        "Progress couldn't be saved automatically.",
                        &format!("{title}: {e}"),
                    );
                }
                app.thumbs.clear();
                app.panel = Panel::Pause;
                app.input.clear();
            }
        }
        Action::Resume => {
            app.panel = Panel::None;
            app.message = None;
        }
        Action::ClosePanel => {
            app.panel = if app.session.is_some() {
                Panel::Pause
            } else {
                Panel::None
            }
        }
        Action::CloseDialog => {
            app.dialog = None;
            app.message = None;
        }
        Action::ToggleFullscreen => {
            // Remembered only once the window has actually gone there: a
            // refused change that is saved anyway comes back wrong next time.
            let wanted = !app.fullscreen;
            let state = if wanted {
                FullscreenType::Desktop
            } else {
                FullscreenType::Off
            };
            match window.set_fullscreen(state) {
                Ok(()) => {
                    app.fullscreen = wanted;
                    app.settings.fullscreen = wanted;
                    app.settings_dirty = true;
                }
                Err(e) => app.report("Full screen didn't work.", &e),
            }
        }
        Action::SaveConfirmed(n) => {
            if let Some(session) = &mut app.session {
                if let Err(e) = session.save(Slot::Number(n)) {
                    app.report("That save didn't work.", &e);
                }
                app.thumbs.remove(&n);
            }
            app.dialog = None;
        }
        Action::Load(n) => {
            if let Some(session) = &mut app.session {
                match session.load(Slot::Number(n)) {
                    Ok(()) => {
                        app.frame_dirty = true;
                        app.message = Some("Save loaded. Press Resume when you're ready.".into());
                    }
                    Err(e) => app.report("That save couldn't be loaded.", &e),
                }
            }
        }
        Action::JumpBack(seconds) => {
            if let Some(session) = &mut app.session {
                let frames = session.engine.frames_for(seconds);
                let mut done = 0;
                while done < frames && session.engine.rewind_step() {
                    done += 1;
                }
                app.frame_dirty |= done > 0;
                if done < frames {
                    app.notice(END_OF_TAPE);
                }
            }
        }
        Action::Scrub(fraction) => app.scrub_fraction = fraction,
        Action::ScrubReleased => app.scrub_fraction = 0.0,
        Action::Screenshot => {
            if let Some(session) = &app.session {
                let saved = crate::files::pictures_dir()
                    .and_then(|dir| session.screenshot(&dir).map(|_| dir));
                match saved {
                    Ok(dir) => {
                        app.message = Some(format!("Screenshot saved to {}.", dir.display()))
                    }
                    Err(e) => app.report("The screenshot couldn't be saved.", &e),
                }
            }
        }
        Action::BackToShelf => {
            if let Some(mut session) = app.session.take() {
                session.close();
            }
            app.panel = Panel::None;
            app.dialog = None;
            app.scrub_fraction = 0.0;
            app.input.clear();
        }
        // Tasks 12 to 14 bring the panels that raise the rest. Saying so out
        // loud beats a silent no-op while the shell is half built.
        other => eprintln!("unhandled {other:?}"),
    }
}

/// One SDL event, turned into whatever it means here: a held button, a raised
/// action, or a key the wizard is learning. Lifted out of the loop so that the
/// loop stays six numbered steps long.
fn handle_event(
    app: &mut App,
    event: &Event,
    bridge: &Bridge,
    controllers: &GameControllerSubsystem,
    joysticks: &JoystickSubsystem,
) {
    match event {
        Event::Quit { .. } => app.quit = true,
        Event::Window {
            win_event: WindowEvent::FocusLost,
            ..
        } => {
            // Walking away from a running game should not cost progress, and a
            // pause that is already up stays up.
            let playing = app.session.is_some() && app.panel == Panel::None && app.wizard.is_none();
            if playing {
                app.actions.push(Action::Pause);
            }
        }
        Event::DropFile { filename, .. } => {
            let action = dropped(app, PathBuf::from(filename));
            app.actions.push(action);
        }
        Event::MouseMotion { .. } | Event::MouseButtonDown { .. } => {
            app.chrome_until = Instant::now() + CHROME_IDLE;
        }
        Event::KeyDown {
            scancode: Some(scancode),
            keymod,
            repeat: false,
            ..
        } => {
            app.chrome_until = Instant::now() + CHROME_IDLE;
            let shift =
                keymod.intersects(sdl2::keyboard::Mod::LSHIFTMOD | sdl2::keyboard::Mod::RSHIFTMOD);
            if app.wizard.is_some() {
                // The wizard has the keyboard: every key is a button it is
                // trying to learn. Every key but Escape, which is the way out
                // of anything, and would otherwise become someone's A button.
                if *scancode == Scancode::Escape {
                    let action = escape(app);
                    app.actions.push(action);
                } else if let Some(wizard) = app.wizard.as_mut() {
                    let pressed =
                        wizard.press(Profiles::KEYBOARD, "Keyboard", &key_name(*scancode));
                    if let WizardEvent::Done(key, profile) = pressed {
                        finish_wizard(app, key, profile);
                    }
                }
                return;
            }
            // A text field, once there is one, gets the key instead.
            if bridge.ctx.egui_wants_keyboard_input() {
                return;
            }
            match scancode {
                Scancode::Escape => {
                    let action = escape(app);
                    app.actions.push(action);
                }
                Scancode::Space => app.actions.push(if app.panel == Panel::Pause {
                    Action::Resume
                } else if app.session.is_some() {
                    Action::Pause
                } else {
                    Action::CloseDialog
                }),
                Scancode::F11 => app.actions.push(Action::ToggleFullscreen),
                Scancode::F5 if app.playing() => app.actions.push(Action::SaveConfirmed(0)),
                Scancode::F8 if app.playing() => app.actions.push(Action::Load(0)),
                _ => {
                    let jump = app.input.jump_back(*scancode, shift);
                    if let Some(seconds) = jump {
                        if app.playing() {
                            app.actions.push(Action::JumpBack(seconds));
                        }
                    } else if app.panel == Panel::Pause
                        && app.input.is_start(None, &key_name(*scancode))
                    {
                        app.actions.push(Action::Resume);
                    } else {
                        app.input.key(*scancode, true);
                    }
                }
            }
        }
        Event::KeyUp {
            scancode: Some(scancode),
            ..
        } => app.input.key(*scancode, false),
        Event::ControllerDeviceAdded { which, .. } => {
            if let (Ok(controller), Ok(guid)) =
                (controllers.open(*which), joysticks.device_guid(*which))
            {
                app.input.pad_added(controller, guid.string());
            }
        }
        Event::ControllerDeviceRemoved { which, .. } => {
            let was_ours = app.input.pad_removed(*which).is_some();
            if was_ours && app.session.is_some() {
                // Always say so. Pause only when the game is the thing in
                // front of you: a pause raised over a dialog or the wizard
                // would close what you were in the middle of.
                app.message = Some("Controller disconnected. Your game is paused.".into());
                if app.panel == Panel::None && app.dialog.is_none() && app.wizard.is_none() {
                    app.actions.push(Action::Pause);
                }
            }
        }
        Event::ControllerButtonDown { which, button, .. } => {
            app.chrome_until = Instant::now() + CHROME_IDLE;
            let physical = button.string();
            if app.wizard.is_some() {
                let device = app
                    .input
                    .pad(*which)
                    .map(|pad| (pad.guid.clone(), pad.name.clone()));
                let pressed = match (device, app.wizard.as_mut()) {
                    (Some((guid, name)), Some(wizard)) => {
                        Some(wizard.press(&guid, &name, &physical))
                    }
                    _ => None,
                };
                if let Some(WizardEvent::Done(key, profile)) = pressed {
                    finish_wizard(app, key, profile);
                }
                return;
            }
            if app.panel == Panel::Pause && app.input.is_start(Some(*which), &physical) {
                app.actions.push(Action::Resume);
            }
            app.input.pad_button(*which, *button, true);
        }
        Event::ControllerButtonUp { which, button, .. } => {
            app.input.pad_button(*which, *button, false)
        }
        Event::ControllerAxisMotion {
            which, axis, value, ..
        } => app.input.pad_axis(*which, *axis, *value),
        _ => {}
    }
}

pub fn run(options: Options) -> Result<(), String> {
    std::fs::create_dir_all(&options.data_dir)
        .map_err(|e| format!("{}: {e}", options.data_dir.display()))?;
    let library = Library::open(&options.data_dir)?;
    let settings = Settings::load(&options.data_dir);
    let sdl = sdl2::init()?;
    let video_subsystem = sdl.video()?;
    let controllers = sdl.game_controller()?;
    let joysticks = sdl.joystick()?;
    {
        let attr = video_subsystem.gl_attr();
        attr.set_context_profile(GLProfile::Core);
        attr.set_context_version(3, 3);
        attr.set_double_buffer(true);
    }
    let mut window = video_subsystem
        .window("Emulia", 1024, 768)
        .position_centered()
        .resizable()
        .allow_highdpi()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;
    let gl_context = window.gl_create_context()?;
    window.gl_make_current(&gl_context)?;
    let gl = Arc::new(unsafe {
        glow::Context::from_loader_function(|s| video_subsystem.gl_get_proc_address(s) as *const _)
    });
    let vsync = video_subsystem
        .gl_set_swap_interval(SwapInterval::VSync)
        .is_ok();
    if !vsync {
        eprintln!("vsync unavailable; pacing by sleep");
    }
    // Declared after the context so they are dropped before it: both hold GL
    // objects that can only be given back while the context is current.
    let mut video = Video::new(gl.clone()).map_err(|e| {
        format!(
            "OpenGL 3.3 is needed for the picture ({e}). Driver: {}",
            video_subsystem.current_video_driver()
        )
    })?;
    let mut bridge = Bridge::new(gl)?;
    crate::ui::theme::apply(&bridge.ctx);
    let mut events = sdl.event_pump()?;
    let now = Instant::now();
    let mut app = App {
        data_dir: options.data_dir.clone(),
        library,
        fullscreen: settings.fullscreen,
        settings,
        settings_dirty: false,
        input: Input::new(Profiles::load(&options.data_dir)),
        session: None,
        panel: Panel::None,
        panel_before: Panel::None,
        dialog: None,
        show_archive: false,
        notice: None,
        message: None,
        busy: false,
        chrome_until: now + CHROME_IDLE,
        wizard: None,
        scrub_fraction: 0.0,
        rewind_depth: 0,
        audio_ms: 0.0,
        frame_dirty: true,
        covers: HashMap::new(),
        thumbs: HashMap::new(),
        preview: None,
        preview_dirty: true,
        actions: Vec::new(),
        quit: false,
    };
    if app.fullscreen {
        // A refused full screen must not leave the layout believing it got
        // one; the saved preference is left alone, so the next run tries again.
        if let Err(e) = window.set_fullscreen(FullscreenType::Desktop) {
            app.fullscreen = false;
            app.report("Full screen didn't work.", &e);
        }
    }
    if let Some(rom) = &options.rom {
        match crate::import(&app.library, rom) {
            Ok(game) => app.actions.push(Action::OpenGame(game)),
            Err(e) => app.report("That game file didn't work.", &e),
        }
    }
    let audio_sdl = if options.mute {
        None
    } else {
        Some(sdl.clone())
    };
    let mut upload_reported = false;
    while !app.quit {
        // 1. Events.
        for event in events.poll_iter() {
            bridge.handle(&event);
            handle_event(&mut app, &event, &bridge, &controllers, &joysticks);
        }
        // 2. UI. The pass ends here without painting: the actions it raised
        // have to be applied before the engine runs.
        let (_platform, primitives, mut textures) =
            bridge.frame(&window, |ui| draw(ui, &mut app, &mut video));
        // 3. Actions.
        let actions = std::mem::take(&mut app.actions);
        for action in actions {
            apply(&mut app, action, &mut window, audio_sdl.as_ref());
        }
        // 4. Emulation. What the tick has to say is collected here and said
        // below, once the borrow of the session has been given back.
        let mut hit_end = false;
        let mut trouble = None;
        if let Some(session) = &mut app.session {
            let held = app.input.time_speed();
            session.scrub = if app.scrub_fraction != 0.0 {
                crate::scrub::speed(app.scrub_fraction)
            } else {
                held
            };
            session.paused = app.panel != Panel::None
                || app.dialog.is_some()
                || app.busy
                || app.wizard.is_some();
            let (p1, p2) = if session.paused {
                (nes_core::Buttons(0), nes_core::Buttons(0))
            } else {
                app.input.buttons()
            };
            let advanced = session.advance(p1, p2);
            hit_end = advanced.hit_end;
            trouble = session
                .take_error()
                .map(|e| (session.game.title.clone(), e));
            app.rewind_depth = session.engine.rewind_depth();
            app.audio_ms = session.audio_ms();
            app.frame_dirty |= advanced.frames > 0;
        }
        if hit_end {
            app.notice(END_OF_TAPE);
        }
        if let Some((title, detail)) = trouble {
            app.library.log_problem(&title, &detail);
        }
        // 5. Picture and panels.
        let (dw, dh) = window.drawable_size();
        unsafe {
            use glow::HasContext;
            video.gl().viewport(0, 0, dw as i32, dh as i32);
            video.gl().clear_color(0.067, 0.094, 0.075, 1.0);
            video.gl().clear(glow::COLOR_BUFFER_BIT);
        }
        if app.session.is_some() {
            let uploaded = match (&app.session, app.frame_dirty) {
                (Some(session), true) => video.upload(session.engine.frame()),
                _ => Ok(()),
            };
            match uploaded {
                Ok(()) => {
                    app.frame_dirty = false;
                    let scale = Bridge::pixels_per_point(&window);
                    let top = if app.fullscreen {
                        0.0
                    } else {
                        TITLE_BAR * scale
                    };
                    let bottom = if app.fullscreen {
                        0.0
                    } else {
                        TIME_ROW * scale
                    };
                    let area = Viewport {
                        x: 0,
                        y: bottom as i32,
                        width: dw as i32,
                        height: (dh as f32 - top - bottom) as i32,
                    };
                    video.draw(
                        area,
                        app.settings.look,
                        app.settings.aspect,
                        app.settings.trim_edges,
                    );
                }
                // Once: a frame that cannot be uploaded cannot be uploaded
                // sixty times a second either, and the log is for people.
                Err(e) => {
                    if !upload_reported {
                        upload_reported = true;
                        app.report("The picture couldn't be shown.", &e);
                    }
                }
            }
        }
        bridge.paint(&window, &primitives, &mut textures);
        window.gl_swap_window();
        // 6. Housekeeping.
        if app.settings_dirty {
            if let Err(e) = app.settings.save(&app.data_dir) {
                app.report("Settings couldn't be saved.", &e);
            }
            app.settings_dirty = false;
        }
        if app.input.take_dirty() {
            if let Err(e) = app.input.profiles().save(&app.data_dir) {
                app.report("Controller buttons couldn't be saved.", &e);
            }
        }
        if let Some((_, since)) = &app.notice {
            if since.elapsed() > NOTICE_TIME {
                app.notice = None;
            }
        }
        if app.fullscreen && app.session.is_some() {
            sdl.mouse().show_cursor(Instant::now() < app.chrome_until);
        } else {
            sdl.mouse().show_cursor(true);
        }
        // With vsync the swap paces the loop. Without it nothing does, and a
        // spinning loop would take a whole core to show the same frame twice.
        if !vsync {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    if let Some(mut session) = app.session.take() {
        session.close();
    }
    video.destroy();
    bridge.destroy();
    Ok(())
}
