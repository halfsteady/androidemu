#[cfg(debug_assertions)]
mod qualification;
// Opt-in macOS Dolphin host in the existing SDL/egui desktop shell.
use crate::{
    dolphin_controls::Controls,
    library::{Game, Slot},
    shell::App,
    ui::{Action, Dialog, Panel},
};
use libloading::Library;
use std::{
    ffi::{c_void, CStr, CString},
    fs::{self, File},
    mem::ManuallyDrop,
    path::Path,
    time::{Duration, Instant},
};
type Start = unsafe extern "C" fn(*mut c_void, *const i8, *const i8, i32) -> i32;
type Pump = unsafe extern "C" fn() -> i32;
type Stop = unsafe extern "C" fn();
type Pause = unsafe extern "C" fn(i32) -> i32;
type PathCall = unsafe extern "C" fn(*const i8) -> i32;
type StateCall = unsafe extern "C" fn(*const i8, i32) -> i32;
type Pointer = unsafe extern "C" fn(*mut f64) -> u32;
type Input = unsafe extern "C" fn(u32, f64);
#[derive(Clone, Copy)]
enum Job {
    Save(Slot),
    Load(Slot),
    Leave,
    Restart,
}
pub struct Probe {
    _library: ManuallyDrop<Library>,
    _profile_lock: File,
    pump: Pump,
    stop: Stop,
    pause: Pause,
    screenshot: PathCall,
    state: StateCall,
    input: Input,
    pointer: Pointer,
    pub game: Game,
    controls: Controls,
    style: usize,
    wii: bool,
    snapshot: Option<egui::TextureHandle>,
    snapshot_path: std::path::PathBuf,
    capturing: Option<Instant>,
    job: Option<Job>,
    paused: bool,
    pub leaving: bool,
    pub restarting: bool,
    started: Instant,
    boot_restore: bool,
    quit_after_leave: bool,
}
fn cpath(path: &Path) -> Result<CString, String> {
    CString::new(path.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())
}
impl Probe {
    pub fn requested() -> bool {
        std::env::var_os("EMULIA_DOLPHIN_PROBE_LIB").is_some()
    }
    pub fn start(
        window: &sdl2::video::Window,
        entry: Game,
        app: &mut App,
        controllers: &sdl2::GameControllerSubsystem,
        restore: bool,
    ) -> Result<Self, String> {
        let dylib = std::env::var_os("EMULIA_DOLPHIN_PROBE_LIB").ok_or("Missing probe library")?;
        let disc = app.library.disc(&entry.id)?;
        // Verify the file still belongs to this shelf entry before booting or restoring.
        if crate::dolphin::Disc::inspect(&disc.path)?.id() != entry.id {
            return Err("The disc image has changed; relink it from the shelf".into());
        }
        let user = app.data_dir.join("dolphin/embedded-2606a");
        let config = user.join("Config");
        fs::create_dir_all(&config).map_err(|e| e.to_string())?;
        let lock = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(user.join("emulia.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "This Dolphin profile is already in use".to_string())?;
        for (name, text) in [
            ("Dolphin.ini", "[Core]\nGFXBackend = Metal\n[Interface]\nSkipNKitWarning = True\n[Analytics]\nEnabled = False\nPermissionAsked = True\n[Movie]\nDumpFrames = False\n"),
            ("GFX.ini", "[Settings]\nShowFPS = False\nShowVPS = False\nShowSpeed = False\n"),
            ("WiimoteNew.ini", "[Wiimote1]\nSource = 1\n[Wiimote2]\nSource = 0\n[Wiimote3]\nSource = 0\n[Wiimote4]\nSource = 0\n")
        ] { if !config.join(name).exists() { crate::files::write_atomic(&config.join(name), text.as_bytes())?; } }
        let controls_path = app
            .library
            .directory(&entry.id)
            .join("dolphin-controls.json");
        let new_controls = !controls_path.exists();
        let mut controls = Controls::open(
            &app.library
                .directory(&entry.id)
                .join("dolphin-controls.json"),
        )?;
        if new_controls && disc.system == "wii" {
            controls.config.style = 2;
        }
        let style = controls.config.style;
        let snapshot_path = user.join("pause.png");
        eprintln!("Embedded Dolphin profile: {}", user.display());
        let game = cpath(&disc.path)?;
        let user = cpath(&user)?;
        // SDL_SysWMinfo reserves a 64-byte union on all platforms. Its Cocoa
        // member is a single NSWindow pointer; generated SDL bindings omit that
        // named member, so read its documented first pointer from the union.
        unsafe {
            let mut info: sdl2::sys::SDL_SysWMinfo = std::mem::zeroed();
            sdl2::sys::SDL_GetVersion(&mut info.version);
            if sdl2::sys::SDL_GetWindowWMInfo(window.raw(), &mut info)
                != sdl2::sys::SDL_bool::SDL_TRUE
                || info.subsystem != sdl2::sys::SDL_SYSWM_TYPE::SDL_SYSWM_COCOA
            {
                return Err("Probe needs an SDL Cocoa window".into());
            }
            let native = info
                .info
                .dummy
                .as_ptr()
                .cast::<*mut c_void>()
                .read_unaligned();
            let library = Library::new(dylib).map_err(|e| e.to_string())?;
            let start: Start = *library
                .get(b"emulia_dolphin_start\0")
                .map_err(|e| e.to_string())?;
            let pump: Pump = *library
                .get(b"emulia_dolphin_pump\0")
                .map_err(|e| e.to_string())?;
            let stop: Stop = *library
                .get(b"emulia_dolphin_stop\0")
                .map_err(|e| e.to_string())?;
            let abi: unsafe extern "C" fn() -> i32 = *library
                .get(b"emulia_dolphin_abi\0")
                .map_err(|e| e.to_string())?;
            if abi() != 3 {
                return Err("Rebuild the Dolphin probe: host ABI mismatch".into());
            }
            let pause = *library
                .get(b"emulia_dolphin_pause\0")
                .map_err(|e| e.to_string())?;
            let screenshot = *library
                .get(b"emulia_dolphin_screenshot\0")
                .map_err(|e| e.to_string())?;
            let state = *library
                .get(b"emulia_dolphin_state\0")
                .map_err(|e| e.to_string())?;
            let pointer = *library
                .get(b"emulia_dolphin_pointer\0")
                .map_err(|e| e.to_string())?;
            let input = *library
                .get(b"emulia_dolphin_input\0")
                .map_err(|e| e.to_string())?;
            let schema: unsafe extern "C" fn() -> *const i8 = *library
                .get(b"emulia_dolphin_controls\0")
                .map_err(|e| e.to_string())?;
            let library = ManuallyDrop::new(library);
            if start(native, game.as_ptr(), user.as_ptr(), style as i32) == 0 {
                return Err("Embedded Dolphin failed to boot; see terminal diagnostics".into());
            }
            controls.schema = CStr::from_ptr(schema())
                .to_string_lossy()
                .lines()
                .map(str::to_owned)
                .collect();
            controls.connect(controllers);
            Ok(Self {
                _library: library,
                pump,
                stop,
                pause,
                screenshot,
                state,
                input,
                pointer,
                _profile_lock: lock,
                game: entry,
                controls,
                style,
                wii: disc.system == "wii",
                snapshot: None,
                snapshot_path,
                capturing: None,
                job: None,
                paused: false,
                leaving: false,
                restarting: false,
                started: Instant::now(),
                boot_restore: restore,
                quit_after_leave: false,
            })
        }
    }
    fn begin_pause(&mut self, app: &mut App) {
        if self.paused || self.capturing.is_some() {
            return;
        }
        self.controls.clear();
        self.snapshot = None;
        let _ = fs::remove_file(&self.snapshot_path);
        match cpath(&self.snapshot_path) {
            Ok(path) if unsafe { (self.screenshot)(path.as_ptr()) } != 0 => {
                self.capturing = Some(Instant::now());
                app.busy = true;
            }
            _ => app.report(
                "Couldn't pause Dolphin.",
                "The renderer is not ready yet; try again after boot.",
            ),
        }
    }
    fn resume(&mut self, app: &mut App) {
        if self.capturing.is_some() || self.job.is_some() {
            return;
        }
        self.controls.clear();
        self.controls.capture = None;
        if unsafe { (self.pause)(0) } != 0 {
            self.paused = false;
            app.panel = Panel::None;
            app.dialog = None;
        }
    }
    pub fn event(
        &mut self,
        event: &sdl2::event::Event,
        app: &mut App,
        controllers: &sdl2::GameControllerSubsystem,
    ) {
        use sdl2::{
            event::{Event, WindowEvent},
            keyboard::Scancode,
        };
        self.controls
            .event(event, controllers, !self.paused && self.capturing.is_none());
        match event {
            Event::Quit { .. } => {
                self.quit_after_leave = true;
                self.job = Some(Job::Leave);
                self.begin_pause(app);
                app.busy = true;
            }
            Event::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                self.controls.clear();
                if app.settings.pause_on_focus_loss {
                    self.begin_pause(app);
                }
            }
            Event::KeyDown {
                scancode: Some(Scancode::Escape),
                repeat: false,
                ..
            } => {
                if self.controls.capture.take().is_some() {
                    return;
                }
                if self.capturing.is_some() || self.job.is_some() {
                    return;
                }
                if app.dialog.take().is_some() {
                    return;
                }
                if app.panel == Panel::Settings || app.panel == Panel::Slots {
                    app.panel = Panel::Pause;
                } else if self.paused {
                    self.resume(app);
                } else {
                    self.begin_pause(app);
                }
            }
            Event::KeyDown {
                scancode: Some(Scancode::F11),
                repeat: false,
                ..
            } => app.actions.push(Action::ToggleFullscreen),
            _ => {}
        }
    }
    pub fn pump(&mut self, app: &mut App) -> bool {
        #[cfg(debug_assertions)]
        self.qualify(app);
        if unsafe { (self.pump)() } == 0 {
            return false;
        }
        if self.controls.dirty {
            if let Err(e) = self.controls.save() {
                self.controls.dirty = false;
                app.report("Couldn't save the controls.", &e);
            }
        }
        if self.wii && !self.paused && self.capturing.is_none() {
            let mut position = [0.0; 2];
            let flags = unsafe { (self.pointer)(position.as_mut_ptr()) };
            self.controls
                .pointer(position, flags & 3, flags & 0x80000000 != 0);
        }
        for (index, name) in self.controls.schema.iter().enumerate() {
            let value = if self.paused || self.capturing.is_some() {
                0.0
            } else {
                self.controls.value(name)
            };
            unsafe {
                (self.input)(index as u32, value);
            }
        }
        if self.boot_restore
            && self.job.is_none()
            && self.capturing.is_none()
            && self.started.elapsed() > Duration::from_secs(2)
        {
            self.boot_restore = false;
            if app.library.has_autosave(&self.game.id) {
                self.job = Some(Job::Load(Slot::Auto));
                self.begin_pause(app);
            }
        }
        if let Some(started) = self.capturing {
            let captured = crate::files::read_png(&self.snapshot_path).is_ok();
            if captured || started.elapsed() > Duration::from_secs(5) {
                if unsafe { (self.pause)(1) } == 0 {
                    self.capturing = None;
                    self.job = None;
                    app.busy = false;
                    app.report(
                        "Couldn't pause Dolphin.",
                        "The game stopped before pause completed.",
                    );
                    return true;
                }
                self.paused = true;
                self.capturing = None;
                self.controls.clear();
                app.panel = Panel::Pause;
                if !captured {
                    app.report(
                        "The pause picture wasn't available.",
                        "Dolphin did not produce a screenshot within five seconds.",
                    );
                }
                if self.job.is_none() {
                    self.job = Some(Job::Save(Slot::Auto));
                }
                // Paint the shared busy panel before the synchronous guarded state operation.
                return true;
            }
        } else if let Some(job) = self.job.take() {
            let result = match job {
                Job::Save(slot) => self.save(app, slot),
                Job::Load(slot) => self.load(app, slot),
                Job::Leave => self.save(app, Slot::Auto),
                Job::Restart => Ok(()),
            };
            if let Err(e) = result {
                app.report(&e, &e);
            } else if matches!(job, Job::Save(Slot::Number(_))) {
                app.notice("Moment saved.");
            }
            app.busy = false;
            if matches!(job, Job::Leave | Job::Restart) {
                self.leaving = true;
                self.restarting = matches!(job, Job::Restart);
            }
        }
        true
    }
    fn save(&self, app: &mut App, slot: Slot) -> Result<(), String> {
        let target = app.library.state_path(&self.game.id, slot);
        let staging = target.with_extension("dolphin-staging");
        // Never let Dolphin move the previous good slot into its undo folder.
        if staging.exists() {
            fs::remove_file(&staging).map_err(|e| e.to_string())?;
        }
        let path = cpath(&staging)?;
        if unsafe { (self.state)(path.as_ptr(), 0) } == 0 {
            return Err("State save failed; the previous slot was kept.".into());
        }
        File::open(&staging)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&staging, &target).map_err(|e| e.to_string())?;
        let thumb = app.library.thumbnail_path(&self.game.id, slot);
        // A missing picture must not leave the old slot's picture attached to a new state.
        let _ = fs::remove_file(&thumb);
        if let Ok(bytes) = fs::read(&self.snapshot_path) {
            crate::files::write_atomic(&thumb, &bytes)?;
        }
        app.thumbs.clear();
        app.covers.clear();
        Ok(())
    }
    fn load(&mut self, app: &mut App, slot: Slot) -> Result<(), String> {
        let path = cpath(&app.library.state_path(&self.game.id, slot))?;
        if unsafe { (self.state)(path.as_ptr(), 1) } == 0 {
            return Err("The state could not be loaded. It may be damaged or incompatible with this Dolphin version.".into());
        }
        self.snapshot = None;
        let _ = fs::remove_file(&self.snapshot_path);
        let thumb = app.library.thumbnail_path(&self.game.id, slot);
        if thumb.exists() {
            fs::copy(thumb, &self.snapshot_path).map_err(|e| e.to_string())?;
        }
        self.controls.clear();
        app.notice("Moment loaded. Resume when you're ready.");
        Ok(())
    }
    /// Returns false only for window actions the ordinary shell should handle.
    pub fn action(&mut self, action: &Action, app: &mut App) -> bool {
        if matches!(action, Action::ToggleFullscreen) {
            return false;
        }
        if self.capturing.is_some() || self.job.is_some() {
            return true;
        }
        match *action {
            Action::Pause => self.begin_pause(app),
            Action::Resume => self.resume(app),
            Action::OpenSlots => app.panel = Panel::Slots,
            Action::OpenSettings => app.panel = Panel::Settings,
            Action::OpenProblems => app.panel = Panel::Problems,
            Action::CloseSettings | Action::ClosePanel => {
                self.controls.capture = None;
                app.panel = Panel::Pause;
            }
            Action::CloseDialog => app.dialog = None,
            Action::CloseMessage => app.message = None,
            Action::SaveRequested(n) => app.dialog = Some(Dialog::ConfirmReplace(n)),
            Action::SaveConfirmed(n) if n < 10 => {
                app.dialog = None;
                self.job = Some(Job::Save(Slot::Number(n)));
                app.busy = true;
            }
            Action::Load(n) if n < 10 => {
                self.job = Some(Job::Load(Slot::Number(n)));
                app.busy = true;
            }
            Action::ResetRequested => app.dialog = Some(Dialog::ConfirmReset),
            Action::ResetConfirmed => {
                app.dialog = None;
                self.job = Some(Job::Restart);
                app.busy = true;
            }
            Action::BackToShelf => {
                self.job = Some(Job::Leave);
                if !self.paused {
                    self.begin_pause(app);
                }
                app.busy = true;
            }
            Action::Screenshot => {
                let result = (|| {
                    let dir = crate::files::pictures_dir()?;
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let path = dir.join(format!("Emulia-{}.png", crate::files::now_millis()));
                    let bytes = fs::read(&self.snapshot_path).map_err(|e| e.to_string())?;
                    crate::files::write_atomic(&path, &bytes)?;
                    Ok::<_, String>(path)
                })();
                match result {
                    Ok(path) => app.notice(&format!("Screenshot saved to {}", path.display())),
                    Err(e) => app.report("Couldn't save the screenshot.", &e),
                }
            }
            _ => {}
        }
        true
    }
    pub fn draw(&mut self, ui: &mut egui::Ui, app: &mut App) {
        egui::Panel::top("dolphin-title")
            .exact_size(crate::shell::TITLE_BAR)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(&self.game.title);
                    if ui.button("Menu").clicked() {
                        app.actions.push(Action::Pause);
                    }
                });
            });
        egui::Panel::bottom("dolphin-footer")
            .exact_size(crate::shell::TIME_ROW)
            .show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(if self.capturing.is_some() {
                        "Pausing…"
                    } else {
                        app.notice
                            .as_ref()
                            .map(|(text, _)| text.as_str())
                            .unwrap_or("Dolphin · Escape opens the menu")
                    });
                });
            });
        if self.paused {
            if self.snapshot.is_none() {
                if let Ok((w, h, rgba)) = crate::files::read_png(&self.snapshot_path) {
                    self.snapshot = Some(ui.ctx().load_texture(
                        "dolphin-pause",
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            if let Some(texture) = &self.snapshot {
                let rect = ui.available_rect_before_wrap();
                let size = texture.size_vec2();
                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                ui.painter().image(
                    texture.id(),
                    egui::Rect::from_center_size(rect.center(), size * scale),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            crate::ui::panels::show(ui, app);
            if app.panel == Panel::Settings && self.controls.show(ui.ctx(), self.style, self.wii) {
                app.actions.push(Action::CloseSettings);
            }
            crate::shell::over_everything(ui.ctx(), app);
        }
    }
    pub fn finish(&mut self, app: &mut App) {
        app.quit |= self.quit_after_leave;
        let _ = app
            .library
            .record(&self.game.id, self.started.elapsed().as_secs() as i64);
        app.embedded_game = None;
        app.panel = Panel::None;
        app.dialog = None;
        app.busy = false;
        app.thumbs.clear();
        app.covers.clear();
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        unsafe {
            (self.stop)();
        }
    }
}
