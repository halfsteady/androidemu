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
use std::collections::{HashMap, HashSet, VecDeque};
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
/// Said when a file the shell keeps its choices in cannot be read. The file is
/// left exactly as it is either way; only the reason is taken.
pub const SETTINGS_UNREADABLE: &str =
    "Your settings couldn't be read, so the usual ones are in use.";
pub const CONTROLLERS_UNREADABLE: &str =
    "Your controller buttons couldn't be read, so the usual ones are in use.";
/// Said when the mapping wizard is offered a key the shell has already spoken
/// for. On the message bar rather than the amber pill: the pill is only drawn
/// over an open game, and the wizard is reached from the shelf as often as
/// from a game.
pub const RESERVED_KEY: &str = "That key already does something. Pick another.";
/// Said when `palette: 5` is chosen and `palette.pal` cannot be honoured. The
/// file's name and what is wrong with it go in the problem log beside it.
pub const PALETTE_FELL_BACK: &str =
    "Your palette file couldn't be used, so the standard colours are back.";

/// Everything a drawn frame can read and everything the shell acts on. One
/// value, passed to the panels by `&mut`, so there is no state hiding in the
/// widgets between frames.
pub struct App {
    pub external: Option<crate::dolphin::Running>,
    pub dolphin_label: String,
    pub data_dir: PathBuf,
    pub library: Library,
    pub settings: Settings,
    pub settings_dirty: bool,
    pub input: Input,
    pub session: Option<Session>,
    pub panel: Panel,
    pub playback_paused: bool,
    /// Where `ClosePanel` returns to when the problem log was opened from
    /// somewhere other than the shelf.
    pub panel_before: Panel,
    pub dialog: Option<Dialog>,
    pub show_archive: bool,
    pub notice: Option<(String, Instant)>,
    /// The plain sentence shown until it is dismissed.
    pub message: Option<String>,
    pub busy: bool,
    /// The long jobs the scrim is up for, in the order they were asked for,
    /// each waiting for a scrim to be painted before it runs. See `defer`: the
    /// window stops answering while one of these runs, and a window that stops
    /// answering with nothing on it looks broken. A queue rather than one
    /// slot, because a handful of files dropped at once is a handful of
    /// imports raised on the same frame, and the second one is no quicker than
    /// the first.
    pub pending: VecDeque<Action>,
    pub fullscreen: bool,
    pub chrome_until: Instant,
    pub wizard: Option<Wizard>,
    /// Where the time handle is, -1 to 1.
    pub scrub_fraction: f32,
    pub rewind_depth: usize,
    /// The audio delay the settings panel shows, and when it was last taken.
    /// Sampled rather than read: see `sample_audio`.
    pub audio_ms: f32,
    pub audio_sampled: Instant,
    /// Whether the framebuffer has changed since it was last handed to the
    /// GPU. A paused game shows the same 245 KB every tick; uploading it
    /// sixty times a second buys nothing.
    pub frame_dirty: bool,
    /// Cover art by game id, keyed on the file's mtime so replacing the
    /// picture replaces the texture.
    pub covers: HashMap<String, (i64, egui::TextureHandle)>,
    /// Which of them this frame actually drew. `sweep_covers` keeps these and
    /// lets the rest go at the end of the shelf's pass.
    covers_drawn: HashSet<String>,
    /// Slot thumbnails for the open game, keyed on the save's time.
    pub thumbs: HashMap<u8, (i64, egui::TextureHandle)>,
    pub preview: Option<egui::TextureHandle>,
    pub preview_dirty: bool,
    /// Whether there is an imported palette to choose. Answered when the
    /// settings panel opens rather than while it is drawn: the panel is drawn
    /// sixty times a second, and the answer is a file read and a parse.
    pub has_palette_file: bool,
    /// The 64 colours the chosen palette came to, resolved by `apply_palette`
    /// and kept. From a file is a file read and a parse, and the preview and
    /// the engine both want the answer — once each time it changes, not once a
    /// frame and not twice.
    pub colours: [u32; 64],
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

    /// Which way time is running and how fast: the track while it is held,
    /// otherwise whatever the keys and the triggers say. One answer, because
    /// the engine and the pill above it have to agree about it.
    pub fn time_speed(&self) -> i32 {
        if self.playback_paused {
            0
        } else if self.scrub_fraction != 0.0 {
            crate::scrub::speed(self.scrub_fraction)
        } else {
            self.input
                .game_time_speed(self.session.as_ref().is_some_and(|s| s.engine.is_snes()))
        }
    }

    fn emulation_paused(&self) -> bool {
        self.playback_paused
            || self.panel != Panel::None
            || self.dialog.is_some()
            || self.busy
            || self.wizard.is_some()
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

    /// The cover for one card, decoded and uploaded the first time it is
    /// asked for and kept by the file's mtime after that.
    ///
    /// Only ask for the ones on screen: see `shelf::cover`. A shelf of two
    /// hundred games is two hundred PNG decodes and two hundred uploads on the
    /// frame it opens, and the screen holds a dozen.
    pub fn cover_texture(
        &mut self,
        ctx: &egui::Context,
        game: &Game,
    ) -> Option<egui::TextureHandle> {
        let path = self.library.cover(&game.id)?;
        self.covers_drawn.insert(game.id.clone());
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

    /// Lets go of every cover that was not drawn this frame, and is called at
    /// the end of the one screen that draws any: see `shelf::show`.
    ///
    /// Kept by what is on screen rather than by a count. A count has to be
    /// bigger than any window anybody has, and one that is not turns into the
    /// worst of both: a shelf with more cards visible than the limit would
    /// empty the map every frame and decode every one of them again, sixty
    /// times a second, which is the thing the cache is for.
    pub fn sweep_covers(&mut self) {
        let drawn = std::mem::take(&mut self.covers_drawn);
        self.covers.retain(|id, _| drawn.contains(id));
    }

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

    /// Works out what the chosen palette comes to, hands it to the open game
    /// and asks for a fresh preview. The engine repaints the frame it is
    /// holding, so a palette changed over a paused game reaches the screen on
    /// this frame rather than whenever something next happens to move.
    ///
    /// This is also the one place the palette file is read, and the one place
    /// a choice that cannot be honoured is said out loud: everything else —
    /// the engine, the preview, the sample frame — uses what this resolved.
    /// Once per change rather than once per frame, which is what keeps a
    /// missing file from writing a line in the log sixty times a second.
    pub fn apply_palette(&mut self) {
        let (colours, why) = self.settings.colours(&self.data_dir);
        self.colours = colours;
        if let Some(session) = &mut self.session {
            session.engine.set_palette(colours);
            self.frame_dirty = true;
        }
        self.preview_dirty = true;
        if let Some(why) = why {
            self.report(PALETTE_FELL_BACK, &why);
        }
    }

    /// The table the sample frame is painted with: what the palette resolved
    /// to, with the model's closest match standing in for Standard so the
    /// built pattern has real colours to draw with rather than the core's
    /// finished ones.
    fn preview_colours(&self) -> [u32; 64] {
        match self.settings.palette {
            picture::PaletteChoice::Standard => picture::model::standard(),
            _ => self.colours,
        }
    }

    /// The paused frame, else the newest saved moment on the shelf, else the
    /// built pattern; always the pattern when a palette other than Standard is
    /// chosen, because a saved PNG holds finished colour and cannot be
    /// restained.
    pub fn sample_source(&self) -> Vec<u8> {
        let standard = self.settings.palette == picture::PaletteChoice::Standard;
        if let Some(session) = &self.session {
            if standard || session.engine.is_snes() {
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
        picture::sample_frame(&self.preview_colours())
    }

    /// Everything a screen reads and nothing else: no window, no engine, no
    /// audio. Every `show` is a pure function of this and the `Ui` it is
    /// handed, which is what lets the screens be drawn in a test at all.
    #[cfg(test)]
    pub fn blank(data_dir: &std::path::Path) -> App {
        App {
            data_dir: data_dir.to_path_buf(),
            library: Library::open(data_dir).unwrap(),
            settings: Settings::load(data_dir).0,
            settings_dirty: false,
            input: Input::new(Profiles::load(data_dir).0),
            session: None,
            external: None,
            dolphin_label: "Automatic discovery".into(),
            panel: Panel::None,
            playback_paused: false,
            panel_before: Panel::None,
            dialog: None,
            show_archive: false,
            notice: None,
            message: None,
            busy: false,
            pending: VecDeque::new(),
            fullscreen: false,
            chrome_until: Instant::now(),
            wizard: None,
            scrub_fraction: 0.0,
            rewind_depth: 0,
            audio_ms: 0.0,
            audio_sampled: Instant::now(),
            frame_dirty: false,
            covers: HashMap::new(),
            covers_drawn: HashSet::new(),
            thumbs: HashMap::new(),
            preview: None,
            preview_dirty: false,
            has_palette_file: false,
            colours: crate::palette::PALETTE,
            actions: Vec::new(),
            quit: false,
        }
    }
}

/// What the screens draw, into the root `Ui` of the pass, in the order they
/// sit in: the shelf or the game, then whatever panel is over it, then the
/// settings, then the scrim and the sentence that go over everything.
fn draw(ui: &mut egui::Ui, app: &mut App, video: &mut Video) {
    if app.session.is_none() {
        crate::ui::shelf::show(ui, app);
    } else {
        crate::ui::play::show(ui, app);
    }
    crate::ui::panels::show(ui, app);
    // The settings panel draws its preview through the real pipeline, which
    // is what it needs the video for.
    crate::ui::settings::show(ui, app, video);
    over_everything(ui.ctx(), app);
}

/// What goes over whichever screen is up, in the order it sits in.
///
/// Both of these belong to the shell rather than to a screen. The scrim used
/// to be drawn by the play view, which returns without drawing anything when
/// there is no game — and importing a game and choosing box art, the two
/// longest jobs there are, both happen on the shelf, where nothing was drawn
/// at all. The message is last because it is the one thing that has to be read
/// before anything else is worth doing; it puts away itself and nothing else,
/// so a question behind it is still waiting.
fn over_everything(ctx: &egui::Context, app: &mut App) {
    if app.busy {
        crate::ui::widgets::busy(ctx);
    }
    if let Some(text) = app.message.clone() {
        if crate::ui::widgets::message_bar(ctx, &text) {
            app.actions.push(crate::ui::Action::CloseMessage);
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
        // Nothing is in front of the shelf but, perhaps, a sentence at the
        // bottom of it, and Escape is how that is put away from the keyboard.
        // The archive is a screen of its own behind that sentence, and backing
        // out of it is the step Escape had no way of taking.
        _ if app.message.is_none() && app.show_archive => Action::ShowArchive(false),
        _ => Action::CloseMessage,
    }
}

/// What Space means, which like Escape depends on what is in front of the
/// game. A question is answered before anything behind it moves: a panel that
/// walks out from under a confirmation leaves the confirmation floating over a
/// running game with nothing to confirm.
fn space(app: &App) -> Action {
    if app.dialog.is_some() {
        Action::CloseDialog
    } else if app.panel == Panel::Pause {
        Action::Resume
    } else if app.session.is_some() {
        Action::Pause
    } else {
        Action::CloseMessage
    }
}

/// Whether a key belongs to egui rather than to the game: only while a text
/// field has the focus, of which this shell has none today.
///
/// Not "is anything focused at all", which is what `egui_wants_keyboard_input`
/// answers. egui moves the focus on Tab and leaves it on a button that has
/// been clicked, and from that moment every key the game wanted was swallowed
/// until somebody clicked somewhere else or pressed Escape.
fn typing(ctx: &egui::Context) -> bool {
    ctx.text_edit_focused()
}

/// What the window losing the keyboard means for the game behind it.
///
/// Pausing is the safe thing and what the shell has always done, and it is a
/// setting because it is not the only thing somebody might want: a game left
/// running in a window behind something else is a game that is still being
/// played. A pause that is already up stays up, and the presses the wizard is
/// waiting for are not interrupted by a click on another window.
fn on_focus_lost(app: &App) -> Option<Action> {
    let playing = app.session.is_some() && app.panel == Panel::None && app.wizard.is_none();
    (app.settings.pause_on_focus_loss && playing).then_some(Action::Pause)
}

/// The window has lost the keyboard.
///
/// Whatever was held is let go of whichever way this goes: a key released
/// while another window has the keyboard is never reported, so a direction
/// held on the way out would be held for ever — whether the game was paused,
/// or a panel was up over it, or it was left running on purpose.
///
/// Then, when it does not pause, the other half of a pause that is not about
/// stopping: nobody is going to pause this game now, so its battery RAM is
/// written here rather than waiting for the next flush to come round.
fn focus_lost(app: &mut App) {
    app.input.clear();
    if let Some(action) = on_focus_lost(app) {
        app.actions.push(action);
        return;
    }
    // Pausing is on and there was nothing to pause: a panel, the wizard or the
    // shelf is up, and the game behind it was stopped and saved when it went.
    if app.settings.pause_on_focus_loss {
        return;
    }
    if let Some(session) = &mut app.session {
        if let Err(e) = session.flush_battery() {
            let title = session.game.title.clone();
            app.report(
                "Progress couldn't be saved automatically.",
                &format!("{title}: {e}"),
            );
        }
    }
}

/// Whether the shell keeps this key for itself. Escape is the way out of
/// anything; Space opens and closes the pause menu, Backspace jumps back, and
/// the three function keys are full screen and the quick save and load.
///
/// `,` and `.` are not here: they are held rather than pressed, and they reach
/// the game through `Input` the way every other key does, so they are
/// somebody's to map if they want them.
fn reserved(scancode: Scancode) -> bool {
    matches!(
        scancode,
        Scancode::Escape
            | Scancode::Space
            | Scancode::Backspace
            | Scancode::F5
            | Scancode::F8
            | Scancode::F11
    )
}

/// Puts away a refusal and nothing else. Any press that is not a reserved key
/// answers one, whether the wizard took it or turned it down for coming from
/// the wrong device — but the message bar is shared, and a sentence somebody
/// else put there is somebody else's to take back.
fn spend_refusal(app: &mut App) {
    if app.message.as_deref() == Some(RESERVED_KEY) {
        app.message = None;
    }
}

/// One key while the wizard has the keyboard, where every key is a button it
/// is trying to learn.
///
/// Every key but the ones the shell has already spoken for. The wizard runs
/// before the hotkeys, so it would take Space happily — and the hotkey would
/// then swallow it in every game that key was pressed in, leaving a button
/// that does nothing and no way to tell why. Escape is the way out of the
/// wizard rather than a refusal, as it is the way out of anything.
///
/// A refusal is answered by the next press that is not one: leaving it up
/// while the panel has moved on to the next button is a sentence about a key
/// nobody pressed any more, waiting to be clicked away. Only a refusal —
/// anything else on the message bar, a controller that was unplugged mid-way
/// through, is somebody else's to put away.
fn wizard_key(app: &mut App, scancode: Scancode) {
    if scancode == Scancode::Escape {
        let action = escape(app);
        app.actions.push(action);
        return;
    }
    if reserved(scancode) {
        app.message = Some(RESERVED_KEY.to_string());
        return;
    }
    // Before the press, not after: the last one finishes the wizard, and what
    // it has to say is said by `finish_wizard`.
    spend_refusal(app);
    if let Some(wizard) = app.wizard.as_mut() {
        let pressed = wizard.press(Profiles::KEYBOARD, "Keyboard", &key_name(scancode));
        if let WizardEvent::Done(key, profile) = pressed {
            finish_wizard(app, key, *profile);
        }
    }
}

/// Whether Start — on a pad or on the keyboard — means "back to the game". It
/// does over the pause panel, and not while a question is in front of it.
fn start_resumes(app: &App) -> bool {
    app.panel == Panel::Pause && app.dialog.is_none()
}

/// A frame of a 60 Hz display, which is what the loop paces to when there is
/// no game open to ask.
const DISPLAY_FRAME: Duration = Duration::from_micros(16_667);

/// How long one frame of the open game's region lasts, or a sixtieth of a
/// second when the shelf is what is on screen.
fn frame_time(app: &App) -> Duration {
    app.session
        .as_ref()
        .map_or(DISPLAY_FRAME, Session::frame_time)
}

/// What is left of the frame once the loop has done its work, and never
/// nothing at all: a frame that overran still has to let the rest of the
/// machine have a turn before the next one.
fn pace(frame_time: Duration, elapsed: Duration) -> Duration {
    frame_time
        .saturating_sub(elapsed)
        .max(Duration::from_millis(1))
}

/// How often the audio delay is taken. It is a number somebody glances at
/// while a game plays, not one they do arithmetic with, and one that changes
/// sixty times a second cannot be read at all.
const AUDIO_SAMPLE: Duration = Duration::from_millis(500);

/// Takes the audio delay, twice a second at most and only while the game is
/// actually running.
///
/// Stopping the game empties the queue it is read from, and every panel stops
/// the game — including the settings panel that shows the number. Read every
/// frame it would say 0.0 ms there, always. So the last figure from while the
/// game was playing is kept, which is the one that answers the question the
/// row is asked: is sound running ahead of the picture.
fn sample_audio(app: &mut App, now: Instant, queued_ms: f32, playing: bool) {
    if playing && now.duration_since(app.audio_sampled) >= AUDIO_SAMPLE {
        app.audio_ms = queued_ms;
        app.audio_sampled = now;
    }
}

/// Where closing a panel leaves you. The problem log is reached from the
/// shelf, from the pause panel and from Settings, so it goes back to wherever
/// it was opened from; everything else sits over a game, or over the shelf.
fn closed(app: &App) -> Panel {
    match (app.panel, app.session.is_some()) {
        (Panel::Problems, _) => app.panel_before,
        (_, true) => Panel::Pause,
        (_, false) => Panel::None,
    }
}

/// Leaves whichever panel is up, and lets go of what it was holding.
///
/// Every way out of the settings panel comes through here: Done, Escape, and
/// the wizard it raised. The preview is a megabyte of texture for a panel
/// that is no longer on screen, and the next one to open is drawn fresh —
/// but the problem log is opened from Settings and goes back to it, so it is
/// only let go of when Settings is not where this lands.
fn leave_panel(app: &mut App) {
    app.panel = closed(app);
    if app.panel != Panel::Settings {
        app.preview = None;
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
    let keyboard = key == Profiles::KEYBOARD || key == "snes:keyboard";
    app.input.set_profile(key, profile);
    app.wizard = None;
    // The same way out as cancelling it: back to the pause panel over a game,
    // and to the shelf when there is none.
    leave_panel(app);
    // The keyboard was asked for its directions too, so it has nothing left
    // over to be reassured about; a controller steers from a d-pad and a
    // stick the wizard never mentioned, and saying so saves somebody looking
    // for the four steps that would have mapped them.
    app.message = Some(if keyboard {
        format!("Buttons saved for {name}.")
    } else {
        format!("Buttons saved for {name}. The directional pad and stick work automatically.")
    });
}

/// Putting the open game away: it writes its autosave and its playtime, and
/// the shell forgets everything that was only true while it was open. Its own
/// function so that leaving can be tested without a window.
fn close_session(app: &mut App) {
    app.playback_paused = false;
    if let Some(mut session) = app.session.take() {
        session.close();
    }
    app.panel = Panel::None;
    app.dialog = None;
    app.scrub_fraction = 0.0;
    // The shelf's Settings panel reads this: with no game open there is no
    // audio queue, and the last game's figure is not it.
    app.audio_ms = 0.0;
    app.input.clear();
}

fn poll_external(app: &mut App) -> bool {
    let exited = app.external.as_mut().and_then(|run| match run.poll() {
        Ok(None) => None,
        other => Some(other),
    });
    let Some(result) = exited else {
        return false;
    };
    if let Some(run) = app.external.take() {
        match result {
            Ok(Some(status)) if status.success() => {
                app.message = Some(format!("{}: Dolphin closed.", run.title))
            }
            other => app.report(
                "Dolphin stopped unexpectedly. See the problem log.",
                &format!("{other:?}\n{}", run.diagnostics()),
            ),
        }
    }
    true
}

fn open_game(app: &mut App, game: Game, sdl: Option<&sdl2::Sdl>) {
    if app.external.is_some() {
        app.report(
            "Close Dolphin before opening another game.",
            "The launched Dolphin process is still running.",
        );
        return;
    }
    if game.is_external() {
        let result = (|| -> Result<crate::dolphin::Running, String> {
            let profile = crate::dolphin::Profile::load(&app.data_dir)?;
            let executable = profile.resolve()?;
            let disc = app.library.disc(&game.id)?;
            if !disc.path.is_file() {
                return Err(format!(
                    "{} is missing. Relink the disc from its shelf menu.",
                    disc.path.display()
                ));
            }
            if let Some(session) = &mut app.session {
                session.save(Slot::Auto)?;
            }
            close_session(app);
            crate::dolphin::Running::launch(&executable, &disc, &game.title)
        })();
        match result {
            Ok(running) => {
                app.external = Some(running);
                if let Err(e) = app.library.record(&game.id, 0) {
                    app.library.log_problem(&game.title, &e);
                }
                app.message =
                    Some("Opened in Dolphin. Controls and saves are managed there.".into());
            }
            Err(e) => app.report(
                "Dolphin couldn't start. Check Settings or relink the disc.",
                &e,
            ),
        }
        return;
    }
    // Whatever is open is being put away, not abandoned: its autosave and its
    // playtime are written before the window belongs to something else. A
    // .nes dropped on a running game used to cost both.
    if let Some(mut open) = app.session.take() {
        open.close();
    }
    app.thumbs.clear();
    app.playback_paused = false;
    app.scrub_fraction = 0.0;
    app.input.clear();
    let audio = match sdl {
        Some(sdl) => match crate::open_audio(sdl, game.audio_channels()) {
            Ok(a) => Some(a),
            Err(e) => {
                app.report("Sound couldn't be started.", &e);
                None
            }
        },
        None => None,
    };
    match Session::open(&app.library, game, audio) {
        Ok((session, warnings)) => {
            app.session = Some(session);
            // A game opens in the colours that were chosen for it, and the
            // preview is stale the moment the sample frame changes.
            app.apply_palette();
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

/// Whether an action is one of the long ones: a file read and a validate, a
/// JPEG decoded and resized and written back out, a save with a thumbnail in
/// it, a screenshot. None of them is slow enough to need a thread, and all of
/// them are slow enough that the window would stop answering mid-frame with
/// no sign of why.
fn is_long(action: &Action) -> bool {
    matches!(
        action,
        Action::ImportFrom(_)
            | Action::RelinkDisc(_)
            | Action::ChooseArtFrom(..)
            | Action::SaveConfirmed(_)
            | Action::Screenshot
    )
}

/// Puts a long job off until the scrim it asks for has been painted, and
/// hands back whatever should run now.
///
/// The order inside the loop is what makes this necessary: the frame is built
/// before the actions it raised are applied, so a job that runs here runs
/// under a frame that was drawn without the scrim in it. Raising the scrim and
/// keeping the job until the next iteration has painted it costs a sixtieth of
/// a second and is the difference between a window that says it is working and
/// one that has died.
///
/// A second long job while one is waiting is run where it stands rather than
/// dropped: one scrim is up either way, and losing somebody's import because
/// they also asked for a screenshot would be worse than the freeze.
fn defer(app: &mut App, action: Action) -> Option<Action> {
    if !is_long(&action) {
        return Some(action);
    }
    app.busy = true;
    // A question that has been answered comes down with the click that
    // answered it. The confirmation's card is drawn above the scrim — it has
    // to be, so that a question asked while something else is working can
    // still be read — and leaving "Replace slot 1?" up while the save it asked
    // for was waiting meant a second click saved a second time.
    app.dialog = None;
    app.pending.push_back(action);
    // Nothing more is needed to clear the input: `busy` pauses the session in
    // step 4 and zeroes both pads, so no button reaches the game while the
    // scrim is up. What is physically held is deliberately kept — a quick-save
    // must not leave somebody who was holding right standing still.
    None
}

/// Takes back the next job the scrim went up for. The scrim stays up while
/// there are more behind it: each one runs under a scrim of its own, on the
/// iteration after that scrim was painted, so a window with four files dropped
/// on it answers between every one of them.
fn take_pending(app: &mut App) -> Option<Action> {
    let action = app.pending.pop_front()?;
    app.busy = !app.pending.is_empty();
    Some(action)
}

/// Applies an action, putting the long ones off for a frame. Everything that
/// raises an action goes through here; `apply` itself is what runs when the
/// waiting is over.
fn act(app: &mut App, action: Action, window: &mut sdl2::video::Window, sdl: Option<&sdl2::Sdl>) {
    if let Some(now) = defer(app, action) {
        apply(app, now, window, sdl);
    }
}

fn pause_game(app: &mut App, menu: bool) {
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
        if menu {
            app.panel = Panel::Pause;
        }
        app.input.clear();
    }
}

fn toggle_playback(app: &mut App) {
    if app.session.is_none() {
        return;
    }
    app.playback_paused = !app.playback_paused;
    app.scrub_fraction = 0.0;
    app.input.clear();
    if app.playback_paused {
        pause_game(app, false);
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
        Action::ChooseDolphin => {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Choose Dolphin.app or the Dolphin executable")
                .pick_file()
            {
                match crate::dolphin::executable(&path).and_then(|_| {
                    crate::dolphin::Profile {
                        executable: Some(path),
                    }
                    .save(&app.data_dir)
                }) {
                    Ok(()) => {
                        app.dolphin_label = crate::dolphin::Profile::load(&app.data_dir)
                            .and_then(|p| p.resolve())
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|e| e);
                        app.message = Some("Dolphin application saved.".into());
                    }
                    Err(e) => app.report("That Dolphin application couldn't be used.", &e),
                }
            }
        }
        Action::RelinkDisc(game) => {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Disc image", &["iso", "gcm"])
                .set_title("Locate this game's disc image")
                .pick_file()
            {
                match app.library.relink_disc(&game.id, &path) {
                    Ok(()) => app.message = Some("Disc location updated.".into()),
                    Err(e) => app.report("That disc couldn't be linked.", &e),
                }
            }
        }
        Action::Import => {
            let picked = rfd::FileDialog::new()
                .add_filter("Game image", &["nes", "sfc", "smc", "iso", "gcm"])
                .set_title("Add a game")
                .pick_file();
            if let Some(path) = picked {
                act(app, Action::ImportFrom(path), window, sdl);
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
                act(app, Action::ChooseArtFrom(game, path), window, sdl);
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
                    "{} is put away. Open Put away on the shelf to bring it back.",
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
        // Asked before done: deleting a game takes its saves with it, and
        // there is no getting them back. The question is a panel of its own.
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
        Action::Pause => pause_game(app, true),
        Action::TogglePlayback => toggle_playback(app),
        Action::Resume => {
            app.playback_paused = false;
            app.panel = Panel::None;
            app.message = None;
        }
        // Done on the settings panel is the back chevron said with a button,
        // so it is the same exit as Escape.
        Action::ClosePanel | Action::CloseSettings => {
            leave_panel(app);
            // Spent: the next panel to open records its own way back.
            app.panel_before = Panel::None;
        }
        // One each: both can be up at once, and a sentence put away while a
        // question is waiting must not answer the question.
        Action::CloseDialog => app.dialog = None,
        Action::CloseMessage => app.message = None,
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
                let mut failure = None;
                while done < frames {
                    match session.engine.rewind_step() {
                        Ok(true) => done += 1,
                        Ok(false) => break,
                        Err(e) => {
                            failure = Some(e);
                            break;
                        }
                    }
                }
                app.frame_dirty |= done > 0;
                if let Some(e) = failure {
                    app.report("Couldn’t rewind this game.", &e);
                } else if done < frames {
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
        Action::OpenSlots => app.panel = Panel::Slots,
        // Asked before done: a slot with something in it is somebody's
        // progress, and there is no getting it back once it is written over.
        Action::SaveRequested(n) => app.dialog = Some(Dialog::ConfirmReplace(n)),
        Action::ResetRequested => app.dialog = Some(Dialog::ConfirmReset),
        Action::ResetConfirmed => {
            app.dialog = None;
            if let Some(session) = &mut app.session {
                match session.reset() {
                    Ok(()) => {
                        app.panel = Panel::None;
                        app.scrub_fraction = 0.0;
                        // Back to the game, so the same clean slate `Resume`
                        // leaves: a sentence about what happened before the
                        // restart has nothing left to be about.
                        app.message = None;
                        // Nothing else will ask for the fresh frame: the tick
                        // that would have marked it runs after this, and a
                        // reset that is not uploaded leaves the old picture on
                        // screen until something else happens to move.
                        app.frame_dirty = true;
                    }
                    // A game that restarted but could not write its autosave
                    // is still a game that restarted, so the panel stays up
                    // and says so rather than pretending nothing happened.
                    Err(e) => {
                        app.panel = Panel::Pause;
                        let title = session.game.title.clone();
                        app.report(
                            "Game reset, but its automatic save couldn't be updated.",
                            &format!("{title}: {e}"),
                        );
                    }
                }
                // A reset rewrites the automatic save. The numbered slots
                // are untouched, but the cache is refilled from disk the
                // moment the slots panel is opened again, so letting it go
                // costs a frame and cannot show a picture from before.
                app.thumbs.clear();
            }
        }
        Action::OpenProblems => {
            // Recorded here rather than by whoever raised it: the log is the
            // one panel that is opened from three different places. Raised a
            // second time from the log itself it would record the log, and
            // Close would lead back to where it already was.
            if app.panel != Panel::Problems {
                app.panel_before = app.panel;
            }
            app.panel = Panel::Problems;
        }
        Action::BackToShelf => close_session(app),
        Action::OpenSettings => {
            app.dolphin_label = crate::dolphin::Profile::load(&app.data_dir)
                .and_then(|p| p.resolve())
                .map(|p| p.display().to_string())
                .unwrap_or_else(|e| e);
            app.panel = Panel::Settings;
            // Both halves of the preview — the settings and the frame it is
            // drawn from — can have moved since the panel was last up, and so
            // can the palette file the Colours row offers. Resolving it here
            // asks for the preview as well, and says so if the file somebody
            // chose has gone in the meantime.
            app.has_palette_file = crate::settings::imported_palette(&app.data_dir).is_some();
            app.apply_palette();
        }
        Action::SetLook(look) => {
            app.settings.look = look;
            app.settings_dirty = true;
            app.preview_dirty = true;
        }
        Action::SetAspect(aspect) => {
            app.settings.aspect = aspect;
            app.settings_dirty = true;
            app.preview_dirty = true;
        }
        Action::SetTrim(trim) => {
            app.settings.trim_edges = trim;
            app.settings_dirty = true;
            app.preview_dirty = true;
        }
        // Nothing about the picture, so nothing to preview: it is read by
        // `focus_lost` the next time the window loses the keyboard.
        Action::SetPauseOnFocusLoss(pause) => {
            app.settings.pause_on_focus_loss = pause;
            app.settings_dirty = true;
        }
        // The palette is the one picture choice the engine has to be told
        // about: the rest happen in the shader, and this one is baked into
        // the frame the console draws.
        Action::SetPalette(palette) => {
            app.settings.palette = palette;
            app.settings_dirty = true;
            app.apply_palette();
        }
        // Modal, like the other file dialogs: the loop stops while it is up
        // and picks up again with the answer.
        Action::ImportPalette => {
            let picked = rfd::FileDialog::new()
                .add_filter("Palette", &["pal"])
                .set_title("Choose a palette")
                .pick_file();
            if let Some(path) = picked {
                use_palette(app, &path);
            }
        }
        Action::StartWizard => {
            app.wizard = Some(Wizard::for_system(
                app.session.as_ref().is_some_and(|s| s.engine.is_snes()),
            ));
            app.panel = Panel::Mapping;
            // Whatever is held down belongs to the game behind the panel, not
            // to the four presses about to be read.
            app.input.clear();
        }
        Action::CancelWizard => {
            app.wizard = None;
            leave_panel(app);
        }
    }
}

/// What happens to a palette file once one has been chosen. Kept apart from
/// the dialog that found it: a file picker cannot be opened in a test, and
/// this is the half that can go wrong.
fn use_palette(app: &mut App, path: &std::path::Path) {
    let stored = std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| crate::settings::store_palette(&app.data_dir, &bytes));
    match stored {
        Ok(_) => {
            app.settings.palette = picture::PaletteChoice::File;
            app.settings_dirty = true;
            app.has_palette_file = true;
            app.apply_palette();
            app.message = Some("Palette loaded.".into());
        }
        // The file's name goes in the log beside the reason: the sentence on
        // its own is the one that is already on screen.
        Err(e) => app.report(
            "That palette file didn't work.",
            &format!("{}: {e}", path.display()),
        ),
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
        } => focus_lost(app),
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
                wizard_key(app, *scancode);
                return;
            }
            // A text field, once there is one, gets the key instead.
            if typing(&bridge.ctx) {
                return;
            }
            match scancode {
                Scancode::Escape => {
                    let action = escape(app);
                    app.actions.push(action);
                }
                Scancode::Space => {
                    let action = space(app);
                    app.actions.push(action);
                }
                Scancode::F11 => app.actions.push(Action::ToggleFullscreen),
                Scancode::F5 if app.playing() => app.actions.push(Action::SaveConfirmed(0)),
                Scancode::F8 if app.playing() => app.actions.push(Action::Load(0)),
                _ => {
                    let jump = app.input.jump_back(*scancode, shift);
                    if let Some(seconds) = jump {
                        if app.playing() {
                            app.actions.push(Action::JumpBack(seconds));
                        }
                    } else if start_resumes(app)
                        && app.input.game_is_start(
                            None,
                            &key_name(*scancode),
                            app.session.as_ref().is_some_and(|s| s.engine.is_snes()),
                        )
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
                // A key the keyboard was told off for is answered by the next
                // press on whichever device the wizard belongs to.
                spend_refusal(app);
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
                    finish_wizard(app, key, *profile);
                }
                return;
            }
            if start_resumes(app)
                && app.input.game_is_start(
                    Some(*which),
                    &physical,
                    app.session.as_ref().is_some_and(|s| s.engine.is_snes()),
                )
            {
                app.actions.push(Action::Resume);
            }
            app.input.pad_button(*which, *button, true);
        }
        Event::ControllerButtonUp { which, button, .. } => {
            app.input.pad_button(*which, *button, false)
        }
        Event::ControllerAxisMotion {
            which, axis, value, ..
        } => {
            // A stick pushed or a trigger pulled is somebody at the controls
            // as much as a button is, and the full-screen chrome fading out
            // from under a thumb that is holding right is chrome that went
            // away while the game was being played. The dead zone is what
            // keeps a resting stick's jitter from holding it up for ever.
            if crate::input::past_dead_zone(*axis, *value) {
                app.chrome_until = Instant::now() + CHROME_IDLE;
            }
            app.input.pad_axis(*which, *axis, *value)
        }
        _ => {}
    }
}

pub fn run(options: Options) -> Result<(), String> {
    std::fs::create_dir_all(&options.data_dir)
        .map_err(|e| format!("{}: {e}", options.data_dir.display()))?;
    let library = Library::open(&options.data_dir)?;
    // Both files are read before the window exists, and both can be somebody's
    // hand-edited JSON. Whatever they say goes in the problem log once the
    // library is in an `App` that has one.
    let (settings, settings_trouble) = Settings::load(&options.data_dir);
    let (profiles, profiles_trouble) = Profiles::load(&options.data_dir);
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
    let mut icon = image::load_from_memory(include_bytes!("../packaging/icons/Emulia.png"))
        .map_err(|e| format!("Cannot decode application icon: {e}"))?
        .into_rgba8();
    let surface = sdl2::surface::Surface::from_data(
        icon.as_mut(),
        512,
        512,
        512 * 4,
        sdl2::pixels::PixelFormatEnum::RGBA32,
    )?;
    window.set_icon(surface);
    // The one thing this shell cannot do without, and the message has to say
    // which driver refused: "OpenGL 3.3 is needed" means nothing on its own to
    // somebody running under a software renderer or a remote desktop.
    let gl_context = window.gl_create_context().map_err(|e| {
        format!(
            "OpenGL 3.3 is needed for the picture ({e}). Driver: {}",
            video_subsystem.current_video_driver()
        )
    })?;
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
        input: Input::new(profiles),
        session: None,
        external: None,
        dolphin_label: "Automatic discovery".into(),
        panel: Panel::None,
        playback_paused: false,
        panel_before: Panel::None,
        dialog: None,
        show_archive: false,
        notice: None,
        message: None,
        busy: false,
        pending: VecDeque::new(),
        chrome_until: now + CHROME_IDLE,
        wizard: None,
        scrub_fraction: 0.0,
        rewind_depth: 0,
        audio_ms: 0.0,
        audio_sampled: now,
        frame_dirty: true,
        covers: HashMap::new(),
        covers_drawn: HashSet::new(),
        thumbs: HashMap::new(),
        preview: None,
        preview_dirty: true,
        has_palette_file: false,
        colours: crate::palette::PALETTE,
        actions: Vec::new(),
        quit: false,
    };
    // Said now that there is somewhere to say it. Neither file has been
    // written over: nothing saves either of them until somebody changes a
    // setting or finishes the mapping wizard on purpose.
    for (label, trouble) in [
        (SETTINGS_UNREADABLE, settings_trouble),
        (CONTROLLERS_UNREADABLE, profiles_trouble),
    ] {
        if let Some(detail) = trouble {
            app.report(label, &detail);
        }
    }
    // The chosen palette, read once here rather than on every frame that wants
    // it, and said out loud now if the file it names has gone.
    app.apply_palette();
    if app.fullscreen {
        // A refused full screen must not leave the layout believing it got
        // one; the saved preference is left alone, so the next run tries again.
        if let Err(e) = window.set_fullscreen(FullscreenType::Desktop) {
            app.fullscreen = false;
            app.report("Full screen didn't work.", &e);
        }
    }
    #[cfg(all(target_os = "macos", feature = "dolphin-embed-probe"))]
    let mut probe = if crate::dolphin_embed_probe::Probe::requested() {
        let rom = options
            .rom
            .as_ref()
            .ok_or("The Dolphin probe needs a disc path")?;
        let game = crate::import(&app.library, rom)?;
        if !game.is_external() {
            return Err("The Dolphin probe needs a GameCube/Wii disc".into());
        }
        Some(crate::dolphin_embed_probe::Probe::start(&window, rom)?)
    } else {
        None
    };
    #[cfg(all(target_os = "macos", feature = "dolphin-embed-probe"))]
    let probe_requested = probe.is_some();
    #[cfg(not(all(target_os = "macos", feature = "dolphin-embed-probe")))]
    let probe_requested = false;
    if let Some(rom) = options.rom.as_ref().filter(|_| !probe_requested) {
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
    // Whether the frame that is about to be painted has the busy scrim in it.
    // The job that scrim belongs to waits for it: see `defer`.
    let mut scrim_painted = false;
    while !app.quit {
        let started = Instant::now();
        if poll_external(&mut app) {
            window.raise();
        }
        #[cfg(all(target_os = "macos", feature = "dolphin-embed-probe"))]
        if probe.as_ref().is_some_and(|p| !p.pump()) {
            probe = None;
        }
        // 0. The long job whose scrim is now on screen. Run through `apply`
        // rather than `act`, because this is what the waiting was for.
        if scrim_painted {
            if let Some(action) = take_pending(&mut app) {
                apply(&mut app, action, &mut window, audio_sdl.as_ref());
            }
        }
        // 1. Events.
        for event in events.poll_iter() {
            bridge.handle(&event);
            #[cfg(all(target_os = "macos", feature = "dolphin-embed-probe"))]
            if probe.is_some() {
                match event {
                    Event::Quit { .. } => app.quit = true,
                    Event::KeyDown {
                        scancode: Some(Scancode::Escape),
                        repeat: false,
                        ..
                    } => probe = None,
                    _ => {}
                }
                continue;
            }
            handle_event(&mut app, &event, &bridge, &controllers, &joysticks);
        }
        // 2. UI. The pass ends here without painting: the actions it raised
        // have to be applied before the engine runs.
        let (primitives, mut textures) = bridge.frame(&window, |ui| {
            #[cfg(all(target_os = "macos", feature = "dolphin-embed-probe"))]
            if let Some(p) = &probe {
                if p.draw(ui) {
                    probe = None;
                }
                return;
            }
            draw(ui, &mut app, &mut video)
        });
        // Taken before the actions below can raise a new scrim: what matters
        // is what the frame just built says, not what this iteration decides
        // afterwards.
        scrim_painted = app.busy;
        // 3. Actions.
        let actions = std::mem::take(&mut app.actions);
        for action in actions {
            act(&mut app, action, &mut window, audio_sdl.as_ref());
        }
        // 4. Emulation. What the tick has to say is collected here and said
        // below, once the borrow of the session has been given back.
        let mut hit_end = false;
        let mut trouble = None;
        let mut queued_ms = 0.0;
        let speed = app.time_speed();
        let paused = app.emulation_paused();
        if let Some(session) = &mut app.session {
            session.scrub = speed;
            session.paused = paused;
            let (p1, p2) = if session.paused {
                (emulation_api::Buttons(0), emulation_api::Buttons(0))
            } else {
                app.input.game_buttons(session.engine.is_snes())
            };
            let advanced = session.advance(p1, p2);
            hit_end = advanced.hit_end;
            trouble = session
                .take_error()
                .map(|e| (session.game.title.clone(), e, session.paused));
            app.rewind_depth = session.engine.rewind_depth();
            queued_ms = session.audio_ms();
            app.frame_dirty |= advanced.frames > 0;
        }
        // Taken here, while the queue still has what the tick put in it, and
        // only while the game is the thing on screen.
        let playing = app.playing();
        sample_audio(&mut app, Instant::now(), queued_ms, playing);
        if hit_end {
            app.notice(END_OF_TAPE);
        }
        if let Some((title, detail, stopped)) = trouble {
            if stopped {
                app.panel = Panel::Pause;
                app.report(&format!("{title}: playback stopped."), &detail);
            } else {
                app.library.log_problem(&title, &detail);
            }
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
                (Some(session), true) => video.upload_frame(
                    session.engine.frame(),
                    session.engine.video_descriptor(),
                    !session.engine.is_snes(),
                ),
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
            sdl.mouse()
                .show_cursor(app.playback_paused || Instant::now() < app.chrome_until);
        } else {
            sdl.mouse().show_cursor(true);
        }
        // With vsync the swap paces the loop. Without it nothing does, and a
        // loop that sleeps a millisecond runs a thousand times a second to
        // show the same frame sixty times — a whole core for nothing. What is
        // left of this frame is what there is to wait for.
        if !vsync {
            std::thread::sleep(pace(frame_time(&app), started.elapsed()));
        }
    }
    if let Some(mut session) = app.session.take() {
        session.close();
    }
    #[cfg(all(target_os = "macos", feature = "dolphin-embed-probe"))]
    drop(probe);
    video.destroy();
    bridge.destroy();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::test_rom;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("emulia-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slider_pause_freezes_both_cores_without_opening_a_panel() {
        let dir = temp_dir("slider-pause");
        for rom in [test_rom(), crate::engine::tests::snes_rom(false)] {
            let mut app = App::blank(&dir);
            let engine = crate::engine::Engine::new(&rom).unwrap();
            let game = app.library.add(&engine.id(), "Pause test", &rom).unwrap();
            open_game(&mut app, game, None);
            toggle_playback(&mut app);
            assert_eq!(app.panel, Panel::None);
            assert!(app.emulation_paused());
            assert_eq!(app.time_speed(), 0);
            let paused = app.emulation_paused();
            let session = app.session.as_mut().unwrap();
            session.paused = paused;
            let before = session.engine.save_state().unwrap();
            for _ in 0..3 {
                assert_eq!(
                    session
                        .advance(emulation_api::Buttons(0), emulation_api::Buttons(0))
                        .frames,
                    0
                );
            }
            assert_eq!(session.engine.save_state().unwrap(), before);
            assert!(app
                .library
                .has_autosave(&app.session.as_ref().unwrap().game.id));
            assert_eq!(escape(&app), Action::Pause);
            toggle_playback(&mut app);
            assert!(!app.emulation_paused());
            let session = app.session.as_mut().unwrap();
            session.paused = false;
            assert!(
                session
                    .advance(emulation_api::Buttons(0), emulation_api::Buttons(0))
                    .frames
                    > 0
            );
            pause_game(&mut app, true);
            assert_eq!(app.panel, Panel::Pause);
            close_session(&mut app);
            assert!(!app.playback_paused);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A texture handle to stand in for the preview. Nothing here looks at
    /// what is in it; what matters is whether it is still being held.
    fn fake_texture() -> egui::TextureHandle {
        egui::Context::default().load_texture(
            "test",
            egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 255]),
            egui::TextureOptions::LINEAR,
        )
    }

    /// The problem log is the one panel opened from three different places,
    /// and the way out of it has to lead back to the one you came in by.
    /// Everything else over a game goes back to the pause panel, and anything
    /// over the shelf goes back to the shelf.
    #[test]
    fn dolphin_handoff_saves_the_embedded_game_and_returns_to_the_shelf() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("dolphin-handoff");
        let mut app = App::blank(&dir);
        let rom = crate::engine::tests::test_rom();
        let nes = crate::engine::Engine::new(&rom).unwrap();
        let nes_game = app.library.add(&nes.id(), "NES", &rom).unwrap();
        open_game(&mut app, nes_game.clone(), None);
        let state = app.session.as_ref().unwrap().engine.save_state().unwrap();
        let disc_path = dir.join("disc.iso");
        let mut header = vec![0; 1024];
        header[28..32].copy_from_slice(&[0xc2, 0x33, 0x9f, 0x3d]);
        std::fs::write(&disc_path, header).unwrap();
        let game = crate::import(&app.library, &disc_path).unwrap();
        let executable = dir.join("fake-dolphin");
        std::fs::write(&executable, "#!/bin/sh\nsleep 0.1\nexit 0\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        crate::dolphin::Profile {
            executable: Some(executable),
        }
        .save(&dir)
        .unwrap();
        open_game(&mut app, game.clone(), None);
        assert!(app.session.is_none());
        assert!(app.external.is_some());
        assert_eq!(app.panel, Panel::None);
        assert_eq!(
            std::fs::read(app.library.state_path(&nes_game.id, Slot::Auto)).unwrap(),
            state
        );
        open_game(&mut app, nes_game, None);
        assert!(
            app.session.is_none(),
            "two emulators must not run audio together"
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !poll_external(&mut app) {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.external.is_none());
        assert_eq!(app.library.find(&game.id).unwrap().seconds, 0);
        assert!(app.message.as_deref().unwrap().contains("Dolphin closed"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn closing_a_panel_goes_back_to_wherever_it_was_opened_from() {
        let dir = temp_dir("shell-close");
        let mut app = App::blank(&dir);
        app.panel = Panel::Problems;
        assert_eq!(closed(&app), Panel::None);
        // Opened from Settings, which is one of the three screens that raise it.
        app.panel_before = Panel::Settings;
        assert_eq!(closed(&app), Panel::Settings);
        // Escape is the back chevron said with the keyboard, so it has to mean
        // the same thing rather than its own thing.
        assert_eq!(escape(&app), Action::ClosePanel);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        app.panel_before = Panel::None;
        for panel in [Panel::Slots, Panel::Settings, Panel::Mapping] {
            app.panel = panel;
            assert_eq!(closed(&app), Panel::Pause, "{panel:?}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Space and Start answer the question in front of the game before they
    /// move anything behind it, the same way Escape does. A panel that walks
    /// out from under a confirmation leaves the confirmation floating over a
    /// running game with its scrim across the controls.
    #[test]
    fn space_and_start_answer_the_question_in_front_of_the_game_first() {
        let dir = temp_dir("shell-space");
        let mut app = App::blank(&dir);
        // On the shelf, Space dismisses whatever is being said.
        assert_eq!(space(&app), Action::CloseMessage);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        assert_eq!(space(&app), Action::Pause);
        app.panel = Panel::Pause;
        assert_eq!(space(&app), Action::Resume);
        assert!(start_resumes(&app));
        for panel in [Panel::None, Panel::Pause, Panel::Slots] {
            app.panel = panel;
            app.dialog = Some(Dialog::ConfirmReset);
            assert_eq!(space(&app), escape(&app), "{panel:?}");
            assert_eq!(space(&app), Action::CloseDialog, "{panel:?}");
            assert!(!start_resumes(&app), "{panel:?}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A sentence at the bottom of the screen and a question in the middle of
    /// it are two different things. Putting the sentence away used to answer
    /// the question as well, which cancelled a save nobody had answered yet.
    #[test]
    fn a_sentence_is_put_away_without_answering_the_question_behind_it() {
        let dir = temp_dir("shell-message");
        let mut app = App::blank(&dir);
        app.message = Some("Palette loaded.".into());
        assert_eq!(escape(&app), Action::CloseMessage);
        assert_eq!(space(&app), Action::CloseMessage);
        // With a question up, both of them answer that instead, and the
        // sentence is left on screen to be read.
        app.dialog = Some(Dialog::ConfirmReset);
        assert_eq!(escape(&app), Action::CloseDialog);
        assert_eq!(space(&app), Action::CloseDialog);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The wizard leaves you where it found you, whether it finished or was
    /// given up on: the pause panel over a game, and the shelf when there is
    /// no game. It used to drop you into the game itself, which from the
    /// shelf was a screen with nothing on it.
    #[test]
    fn the_wizard_hands_back_to_whatever_raised_it() {
        let dir = temp_dir("shell-wizard");
        let mut app = App::blank(&dir);
        let profile = crate::settings::Profile {
            snes: Default::default(),
            name: "Keyboard".into(),
            a: "X".into(),
            b: "Z".into(),
            select: "Right Shift".into(),
            start: "Return".into(),
            up: "W".into(),
            down: "S".into(),
            left: "A".into(),
            right: "D".into(),
        };
        app.panel = Panel::Mapping;
        app.wizard = Some(Wizard::new());
        finish_wizard(&mut app, Profiles::KEYBOARD.to_string(), profile.clone());
        assert!(app.wizard.is_none());
        assert_eq!(app.panel, Panel::None);
        // The keyboard was asked for its directions too, so there is nothing
        // left over that works on its own to promise.
        assert_eq!(app.message.as_deref(), Some("Buttons saved for Keyboard."));
        assert_eq!(app.input.profiles().get(Profiles::KEYBOARD), Some(&profile));
        // A pad keeps the older sentence: its d-pad and stick steer without
        // anybody being asked to press them.
        app.panel = Panel::Mapping;
        finish_wizard(
            &mut app,
            "030000".to_string(),
            crate::settings::Profile {
                name: "Pad".into(),
                a: "b".into(),
                b: "a".into(),
                select: "back".into(),
                start: "start".into(),
                ..crate::settings::Profile::default()
            },
        );
        assert_eq!(
            app.message.as_deref(),
            Some("Buttons saved for Pad. The directional pad and stick work automatically.")
        );
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        app.panel = Panel::Mapping;
        finish_wizard(&mut app, Profiles::KEYBOARD.to_string(), profile);
        assert_eq!(app.panel, Panel::Pause);
        // Which is the same door cancelling it uses.
        assert_eq!(closed(&app), Panel::Pause);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The delay is taken while the game runs, held while it does not, and
    /// never more than twice a second. Every panel stops the game and empties
    /// the queue the figure is read from — including the settings panel that
    /// shows it — so reading it there would print 0.0 ms and nothing else.
    #[test]
    fn the_audio_delay_is_sampled_while_the_game_plays_and_held_while_it_is_not() {
        let dir = temp_dir("shell-audio");
        let mut app = App::blank(&dir);
        let start = Instant::now();
        sample_audio(&mut app, start, 31.0, false);
        assert_eq!(app.audio_ms, 0.0);
        sample_audio(&mut app, start + AUDIO_SAMPLE, 31.0, true);
        assert_eq!(app.audio_ms, 31.0);
        // The sixty frames in between are not sixty readings.
        sample_audio(
            &mut app,
            start + AUDIO_SAMPLE + Duration::from_millis(16),
            9.0,
            true,
        );
        assert_eq!(app.audio_ms, 31.0);
        sample_audio(&mut app, start + AUDIO_SAMPLE * 2, 44.0, true);
        assert_eq!(app.audio_ms, 44.0);
        // And held, not zeroed, once a panel is what is on screen.
        sample_audio(&mut app, start + AUDIO_SAMPLE * 3, 0.0, false);
        assert_eq!(app.audio_ms, 44.0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Going back to the shelf puts the game away and leaves nothing of it
    /// behind. The held audio delay is part of that: the Settings panel opened
    /// from the shelf would otherwise print the last game's figure, and there
    /// is no queue behind it to sample a new one from.
    #[test]
    fn leaving_a_game_puts_it_away_and_keeps_none_of_it() {
        let dir = temp_dir("shell-back-to-shelf");
        let mut app = App::blank(&dir);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        app.panel = Panel::Pause;
        app.dialog = Some(Dialog::ConfirmReset);
        app.scrub_fraction = -0.4;
        app.audio_ms = 44.0;
        close_session(&mut app);
        assert!(app.session.is_none());
        assert_eq!(app.panel, Panel::None);
        assert_eq!(app.dialog, None);
        assert_eq!(app.scrub_fraction, 0.0);
        assert_eq!(app.audio_ms, 0.0);
        // Put away, not dropped: the autosave it left is what the shelf shows
        // as its cover and what opening it again resumes from.
        assert!(app.library.has_autosave("aaaa"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The palette reaches the engine only when there is one to reach, and
    /// the frame it repainted has to be uploaded again. Marking the frame
    /// with no game open would upload whatever the last one left behind.
    #[test]
    fn a_palette_reaches_the_open_game_and_asks_for_its_frame_again() {
        let dir = temp_dir("shell-apply-palette");
        let mut app = App::blank(&dir);
        app.settings.palette = picture::PaletteChoice::Vivid;
        app.apply_palette();
        assert!(app.preview_dirty);
        assert!(!app.frame_dirty);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        app.preview_dirty = false;
        app.apply_palette();
        assert!(app.preview_dirty && app.frame_dirty);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Every way out of the settings panel lets go of the preview: Done,
    /// Escape, and the wizard it raised. A megabyte of texture for a panel
    /// nobody is looking at. The problem log is the exception, because it is
    /// opened from Settings and goes back to it.
    #[test]
    fn every_way_out_of_settings_lets_go_of_the_preview() {
        let dir = temp_dir("shell-leave");
        let mut app = App::blank(&dir);
        for panel in [Panel::Settings, Panel::Mapping] {
            app.panel = panel;
            app.preview = Some(fake_texture());
            leave_panel(&mut app);
            assert_eq!(app.panel, Panel::None, "{panel:?}");
            assert!(app.preview.is_none(), "{panel:?}");
        }
        // Back to Settings from the log it raised, with its preview intact.
        app.panel = Panel::Problems;
        app.panel_before = Panel::Settings;
        app.preview = Some(fake_texture());
        leave_panel(&mut app);
        assert_eq!(app.panel, Panel::Settings);
        assert!(app.preview.is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A palette file is kept and chosen when it reads, and says so with the
    /// file's name in the log when it does not. The picker that finds the
    /// file cannot be opened in a test; this is everything after it.
    #[test]
    fn a_palette_file_is_kept_when_it_reads_and_says_why_when_it_does_not() {
        let dir = temp_dir("shell-palette");
        let mut app = App::blank(&dir);
        let short = dir.join("short.pal");
        std::fs::write(&short, [0u8; 100]).unwrap();
        use_palette(&mut app, &short);
        assert_eq!(app.settings.palette, picture::PaletteChoice::Standard);
        assert!(!app.settings_dirty);
        assert_eq!(
            app.message.as_deref(),
            Some("That palette file didn't work.")
        );
        let logged = app.library.problems().remove(0);
        assert!(logged.contains("short.pal"), "{logged}");
        // A whole table is kept, chosen, and used from here on.
        let table = picture::model::build(1.1, 0.0, 1.0, 0.0, 1.0);
        let good = dir.join("good.pal");
        std::fs::write(&good, picture::model::bytes(&table)).unwrap();
        use_palette(&mut app, &good);
        assert_eq!(app.settings.palette, picture::PaletteChoice::File);
        assert!(app.settings_dirty && app.preview_dirty);
        assert_eq!(app.message.as_deref(), Some("Palette loaded."));
        assert_eq!(app.settings.colours(&app.data_dir), (table, None));
        assert_eq!(app.colours, table);
        // A file that is not there is a file that cannot be read, which is
        // the same sentence and a different reason.
        app.message = None;
        use_palette(&mut app, &dir.join("nothing.pal"));
        assert_eq!(
            app.message.as_deref(),
            Some("That palette file didn't work.")
        );
        let logged = app.library.problems().remove(0);
        assert!(logged.contains("nothing.pal"), "{logged}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A long job is put off for a frame so the scrim it raises is on screen
    /// before the window stops answering, and the same job is what comes back
    /// once it is. The loop is what cannot be tested here; the handshake it
    /// runs is this.
    #[test]
    fn a_long_job_waits_for_its_scrim_and_a_short_one_does_not() {
        let dir = temp_dir("shell-busy");
        let mut app = App::blank(&dir);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game.clone(), None).unwrap();
        app.session = Some(session);
        // Everything that reads a file, decodes a picture or writes one.
        for long in [
            Action::ImportFrom(dir.join("game.nes")),
            Action::ChooseArtFrom(game.clone(), dir.join("art.png")),
            Action::SaveConfirmed(3),
            Action::Screenshot,
        ] {
            assert!(is_long(&long), "{long:?}");
        }
        // And nothing else: a pause that waited a frame would be a pause that
        // let one more frame of the game through.
        for quick in [
            Action::Pause,
            Action::Resume,
            Action::Load(1),
            Action::OpenGame(game.clone()),
            Action::JumpBack(5),
        ] {
            assert!(!is_long(&quick), "{quick:?}");
            assert_eq!(defer(&mut app, quick.clone()), Some(quick));
            assert!(!app.busy && app.pending.is_empty());
        }
        // A long one goes into the queue and the scrim goes up.
        let first = Action::ImportFrom(dir.join("one.nes"));
        let second = Action::ImportFrom(dir.join("two.nes"));
        assert_eq!(defer(&mut app, first.clone()), None);
        assert!(app.busy);
        // Which is what stops the game: no button reaches it while it is up.
        assert!(!app.playing());
        // A second long job on the same frame — four files dropped at once, or
        // a second click on a button that is still there — queues behind it
        // rather than running inline with no scrim over it.
        assert_eq!(defer(&mut app, second.clone()), None);
        assert_eq!(defer(&mut app, Action::Screenshot), None);
        assert_eq!(
            app.pending,
            [first.clone(), second.clone(), Action::Screenshot]
        );
        // They come back in the order they were asked for, one per scrim, and
        // the scrim stays up until the last of them has been taken.
        assert_eq!(take_pending(&mut app), Some(first));
        assert!(app.busy, "two jobs left and no scrim over them");
        assert_eq!(take_pending(&mut app), Some(second));
        assert!(app.busy);
        assert_eq!(take_pending(&mut app), Some(Action::Screenshot));
        assert!(!app.busy && app.pending.is_empty());
        assert_eq!(take_pending(&mut app), None);
        assert!(!app.busy);
        // And an answered question comes down with the click that answered it.
        // Its card is drawn above the scrim, so leaving it up while the save it
        // asked for waited meant a second click saved a second time.
        app.dialog = Some(Dialog::ConfirmReplace(1));
        assert_eq!(defer(&mut app, Action::SaveConfirmed(1)), None);
        assert_eq!(app.dialog, None);
        assert_eq!(take_pending(&mut app), Some(Action::SaveConfirmed(1)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A job waits for a scrim that has actually been painted. The loop cannot
    /// be run in a test, so this is the handshake it keeps: the frame is built
    /// before the actions it raised are applied, so what the frame was built
    /// with is what decides whether the job may run.
    #[test]
    fn a_queued_job_runs_only_after_a_frame_that_had_the_scrim() {
        let dir = temp_dir("shell-scrim");
        let mut app = App::blank(&dir);
        // Iteration N builds its frame, and only then is the job asked for.
        let mut scrim_painted = app.busy;
        defer(&mut app, Action::Screenshot);
        assert!(
            !scrim_painted,
            "the frame that was built has no scrim in it"
        );
        // So the top of N+1 runs nothing, and N+1 builds the scrim and swaps it.
        scrim_painted = app.busy;
        assert!(scrim_painted);
        // Which makes the top of N+2 where the job runs.
        assert_eq!(take_pending(&mut app), Some(Action::Screenshot));
        assert!(!app.busy);
        // And the frame after that is built without one again.
        scrim_painted = app.busy;
        assert!(!scrim_painted);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The scrim and the sentence go over whichever screen is up, including
    /// the shelf — which is where importing a game and choosing box art, the
    /// two longest jobs there are, are asked for. The play view used to draw
    /// the scrim, and it draws nothing at all without a game.
    #[test]
    fn the_scrim_and_the_sentence_are_drawn_with_no_game_open() {
        let dir = temp_dir("shell-over");
        let ctx = egui::Context::default();
        crate::ui::theme::apply(&ctx);
        let mut app = App::blank(&dir);
        app.busy = true;
        app.message = Some("Palette loaded.".into());
        let pass = |app: &mut App, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::Vec2::new(600.0, 400.0),
                )),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| over_everything(ui.ctx(), app))
                .textures_delta
                .clear();
        };
        // Twice, because egui settles a layout over two frames and the second
        // is the one that has to hold up.
        for _ in 0..2 {
            pass(&mut app, Vec::new());
        }
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        // The scrim is over the shelf and under the sentence, so the one
        // control the sentence has is still the one a click lands on.
        let ok = egui::pos2(340.0, 400.0 - 24.0 - 34.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: ok,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        pass(&mut app, vec![egui::Event::PointerMoved(ok), button(true)]);
        pass(&mut app, vec![button(false)]);
        assert_eq!(
            std::mem::take(&mut app.actions),
            vec![Action::CloseMessage],
            "the OK button is under the scrim"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A palette file that cannot be honoured falls back to Standard, says so
    /// and names the file in the log. Once, where the palette is resolved —
    /// the engine, the preview and the sample frame all take what this worked
    /// out, so a file somebody deleted cannot write a line sixty times a
    /// second.
    #[test]
    fn a_palette_file_that_cannot_be_honoured_falls_back_and_says_so() {
        let dir = temp_dir("shell-palette-gone");
        let mut app = App::blank(&dir);
        app.settings.palette = picture::PaletteChoice::File;
        app.apply_palette();
        assert_eq!(app.colours, crate::palette::PALETTE);
        assert_eq!(app.preview_colours(), crate::palette::PALETTE);
        assert_eq!(app.message.as_deref(), Some(PALETTE_FELL_BACK));
        let logged = app.library.problems().remove(0);
        assert!(logged.contains("palette.pal"), "{logged}");
        // A file that reads is honoured, and says nothing at all.
        let table = picture::model::build(1.1, 0.0, 1.0, 0.0, 1.0);
        std::fs::write(dir.join("palette.pal"), picture::model::bytes(&table)).unwrap();
        app.message = None;
        app.apply_palette();
        assert_eq!(app.colours, table);
        assert_eq!(app.preview_colours(), table);
        assert_eq!(app.message, None);
        assert_eq!(app.library.problems().len(), 1, "said twice");
        // Standard is the core's table for the picture and the model's
        // closest match for the built sample frame, and neither is a read.
        app.settings.palette = picture::PaletteChoice::Standard;
        app.apply_palette();
        assert_eq!(app.colours, crate::palette::PALETTE);
        assert_eq!(app.preview_colours(), picture::model::standard());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Neither file the shell keeps its choices in is written over because it
    /// could not be read. Nothing is dirty on the way up, so the first
    /// housekeeping pass saves neither, and what is on disk is still there for
    /// somebody to fix.
    #[test]
    fn an_unreadable_choice_file_is_left_where_it_is() {
        let dir = temp_dir("shell-unreadable");
        std::fs::write(dir.join("settings.json"), "not json").unwrap();
        std::fs::write(dir.join("controllers.json"), "{oops").unwrap();
        let mut app = App::blank(&dir);
        assert_eq!(app.settings, Settings::default());
        assert!(!app.settings_dirty);
        assert!(!app.input.take_dirty());
        // Which is what the shell says, with the file named in the log.
        let (_, trouble) = Settings::load(&dir);
        app.report(SETTINGS_UNREADABLE, &trouble.unwrap());
        let (_, trouble) = Profiles::load(&dir);
        app.report(CONTROLLERS_UNREADABLE, &trouble.unwrap());
        assert_eq!(app.message.as_deref(), Some(CONTROLLERS_UNREADABLE));
        let logged = app.library.problems();
        assert!(logged[0].contains("controllers.json"), "{logged:?}");
        assert!(logged[1].contains("settings.json"), "{logged:?}");
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json")).unwrap(),
            "not json"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("controllers.json")).unwrap(),
            "{oops"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Escape backs out of the archive, which is a screen of its own and had
    /// no way out from the keyboard. Whatever is being said is read first: a
    /// sentence and a screen are two steps, not one.
    #[test]
    fn escape_leaves_the_archive_once_there_is_nothing_left_to_read() {
        let dir = temp_dir("shell-archive");
        let mut app = App::blank(&dir);
        app.show_archive = true;
        assert_eq!(escape(&app), Action::ShowArchive(false));
        app.message = Some("Game A is back on the shelf, exactly where you left it.".into());
        assert_eq!(escape(&app), Action::CloseMessage);
        app.message = None;
        // And in full screen the window comes back first, the way it does
        // from the shelf.
        app.fullscreen = true;
        assert_eq!(escape(&app), Action::ToggleFullscreen);
        app.fullscreen = false;
        // On the shelf itself there is nothing behind it to back out to.
        app.show_archive = false;
        assert_eq!(escape(&app), Action::CloseMessage);
        // The last game brought back takes the archive with it either way.
        app.show_archive = true;
        app.library.add("aaaa", "Game A", &test_rom()).unwrap();
        app.library.set_archived("aaaa", true).unwrap();
        leave_empty_archive(&mut app);
        assert!(app.show_archive, "there is still something in it");
        app.library.set_archived("aaaa", false).unwrap();
        leave_empty_archive(&mut app);
        assert!(!app.show_archive);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Without vsync the loop has to pace itself, and a millisecond is not
    /// the pace: it runs a thousand times a second to show sixty frames.
    #[test]
    fn an_unpaced_loop_sleeps_what_is_left_of_the_frame() {
        let sixtieth = Duration::from_micros(16_667);
        assert_eq!(
            pace(sixtieth, Duration::from_millis(4)),
            sixtieth - Duration::from_millis(4)
        );
        // A frame that overran still gives the rest of the machine a turn.
        assert_eq!(
            pace(sixtieth, Duration::from_millis(30)),
            Duration::from_millis(1)
        );
        assert_eq!(pace(sixtieth, sixtieth), Duration::from_millis(1));
        let dir = temp_dir("shell-pace");
        let mut app = App::blank(&dir);
        // With no game open there is no region to ask, so the display's own
        // sixtieth of a second is what it paces to.
        assert_eq!(frame_time(&app), sixtieth);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        let region = session.frame_time();
        app.session = Some(session);
        assert_eq!(frame_time(&app), region);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// The engine and the pill over the picture ask the same question of the
    /// same answer. Two copies of this drifted apart once already, and the
    /// symptom was a heads-up display counting down over a frozen game.
    #[test]
    fn the_time_speed_is_the_track_first_and_the_keys_after() {
        let dir = temp_dir("shell-speed");
        let mut app = App::blank(&dir);
        assert_eq!(app.time_speed(), 0);
        app.scrub_fraction = -1.0;
        assert_eq!(app.time_speed(), -crate::scrub::MAX);
        app.scrub_fraction = 1.0;
        assert_eq!(app.time_speed(), crate::scrub::MAX);
        // Back in the middle, the keys have it again.
        app.scrub_fraction = 0.0;
        assert_eq!(
            app.time_speed(),
            app.input
                .game_time_speed(app.session.as_ref().is_some_and(|s| s.engine.is_snes()))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// What decides whether a key is egui's. The gate used to ask whether
    /// anything at all was focused: egui moves the focus on Tab and on the
    /// arrows, and leaves it on a button that was clicked, so one Tab handed
    /// the keyboard to the title bar's Menu button and the game stopped
    /// answering until somebody clicked somewhere else. Nothing in this shell
    /// is typed into.
    #[test]
    fn a_focused_button_does_not_take_the_keyboard_from_the_game() {
        let ctx = egui::Context::default();
        let mut button = egui::Id::NULL;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.button("Menu");
            response.request_focus();
            button = response.id;
        })
        .textures_delta
        .clear();
        // Focused, and the old gate said the keyboard was egui's for it.
        assert_eq!(ctx.memory(|m| m.focused()), Some(button));
        assert!(ctx.egui_wants_keyboard_input());
        // Nobody is typing, so the key is still the game's.
        assert!(!typing(&ctx));
    }

    /// The keys the shell keeps for itself cannot be mapped to a button. The
    /// wizard runs before the hotkeys, so Space would be captured happily —
    /// and then swallowed by the pause hotkey every time it was pressed in a
    /// game, leaving somebody with a button that does nothing and no way to
    /// tell why. Escape is the way out of the wizard rather than a refusal.
    #[test]
    fn the_wizard_refuses_the_keys_the_shell_has_already_spoken_for() {
        for scancode in [
            Scancode::Escape,
            Scancode::Space,
            Scancode::Backspace,
            Scancode::F5,
            Scancode::F8,
            Scancode::F11,
        ] {
            assert!(reserved(scancode), "{scancode:?}");
        }
        // The two time keys are held rather than pressed and reach the game
        // through `Input` like any other key, so they are somebody's to map.
        for scancode in [
            Scancode::Comma,
            Scancode::Period,
            Scancode::X,
            Scancode::Z,
            Scancode::Return,
            Scancode::Tab,
            Scancode::W,
        ] {
            assert!(!reserved(scancode), "{scancode:?}");
        }

        let dir = temp_dir("shell-reserved");
        let mut app = App::blank(&dir);
        app.panel = Panel::Mapping;
        app.wizard = Some(Wizard::new());
        // A reserved key costs the wizard nothing: the step it is waiting for
        // is the step it was waiting for, and the sentence says why.
        for scancode in [Scancode::Space, Scancode::Backspace, Scancode::F5] {
            wizard_key(&mut app, scancode);
            assert_eq!(app.wizard.as_ref().unwrap().step(), 0, "{scancode:?}");
            assert!(app.wizard.as_ref().unwrap().device_name().is_none());
            assert_eq!(app.message.as_deref(), Some(RESERVED_KEY), "{scancode:?}");
            assert!(app.actions.is_empty(), "{:?}", app.actions);
        }
        // A key the shell has no use for is the button it was asked for, and
        // it answers the refusal: the sentence goes with the press it was
        // about, rather than hanging over the next button to be clicked away.
        wizard_key(&mut app, Scancode::X);
        assert_eq!(app.wizard.as_ref().unwrap().step(), 1);
        assert_eq!(app.message, None);
        // Only the refusal, though. A sentence somebody else put on the bar —
        // the controller that was unplugged halfway through this mapping —
        // is still there to be read and is somebody else's to put away.
        let unplugged = "Controller disconnected. Your game is paused.";
        app.message = Some(unplugged.to_string());
        wizard_key(&mut app, Scancode::Z);
        assert_eq!(app.wizard.as_ref().unwrap().step(), 2);
        assert_eq!(app.message.as_deref(), Some(unplugged));
        app.message = None;
        // And Escape is still the way out, rather than a key to be told off
        // for pressing.
        wizard_key(&mut app, Scancode::Escape);
        assert_eq!(app.message, None);
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::CancelWizard]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Walking away from a running game pauses it, which is what keeps the
    /// progress; somebody who wants to watch it run in a window behind
    /// something else turns that off. Either way the keys are let go of — a
    /// key released while another window has the keyboard is never reported,
    /// so a held direction would be held for ever — and the battery RAM is
    /// written, because with no pause coming nothing else is going to write
    /// it until the next flush comes round.
    #[test]
    fn losing_the_window_pauses_a_game_unless_that_was_turned_off() {
        let dir = temp_dir("shell-focus");
        let mut app = App::blank(&dir);
        // Nothing open is nothing to pause.
        assert_eq!(on_focus_lost(&app), None);
        let game = app.library.add("aaaa", "Test", &test_rom()).unwrap();
        let battery = app.library.battery_path(&game.id);
        let (session, _) = Session::open(&app.library, game, None).unwrap();
        app.session = Some(session);
        assert_eq!(on_focus_lost(&app), Some(Action::Pause));
        // A pause that is already up stays up, and the four presses the
        // wizard is waiting for are not interrupted by another window.
        app.panel = Panel::Pause;
        assert_eq!(on_focus_lost(&app), None);
        app.panel = Panel::None;
        app.wizard = Some(Wizard::new());
        assert_eq!(on_focus_lost(&app), None);
        app.wizard = None;

        // Pausing is on and there is nothing to pause: the panel in front of
        // the game stopped it when it opened, so nothing is asked for and
        // nothing is written — but a key held while somebody clicked away
        // from the panel is still let go of.
        app.panel = Panel::Pause;
        app.input.key(Scancode::X, true);
        let _ = std::fs::remove_file(&battery);
        focus_lost(&mut app);
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        assert_eq!(
            app.input
                .game_buttons(app.session.as_ref().is_some_and(|s| s.engine.is_snes()))
                .0
                 .0,
            0
        );
        assert!(!battery.exists());
        assert_eq!(app.message, None);
        app.panel = Panel::None;

        // Turned off, the game plays on — and the two halves of a pause that
        // are not about stopping still happen.
        app.settings.pause_on_focus_loss = false;
        assert_eq!(on_focus_lost(&app), None);
        app.input.key(Scancode::X, true);
        assert_ne!(
            app.input
                .game_buttons(app.session.as_ref().is_some_and(|s| s.engine.is_snes()))
                .0
                 .0,
            0
        );
        let _ = std::fs::remove_file(&battery);
        focus_lost(&mut app);
        assert!(app.actions.is_empty(), "{:?}", app.actions);
        assert_eq!(
            app.input
                .game_buttons(app.session.as_ref().is_some_and(|s| s.engine.is_snes()))
                .0
                 .0,
            0
        );
        assert!(battery.exists());
        assert_eq!(app.message, None);
        // With it on, the pause is what does both, so nothing happens here
        // but the asking.
        app.settings.pause_on_focus_loss = true;
        app.input.key(Scancode::X, true);
        focus_lost(&mut app);
        assert_eq!(std::mem::take(&mut app.actions), vec![Action::Pause]);
        assert_eq!(
            app.input
                .game_buttons(app.session.as_ref().is_some_and(|s| s.engine.is_snes()))
                .0
                 .0,
            0
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
