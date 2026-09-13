//! Opt-in macOS experiment, excluded from default builds and packages.
use libloading::Library;
use std::{
    ffi::{c_void, CString},
    mem::ManuallyDrop,
    path::Path,
};

type Start = unsafe extern "C" fn(*mut c_void, *const i8, *const i8) -> i32;
type Pump = unsafe extern "C" fn() -> i32;
type Stop = unsafe extern "C" fn();

pub struct Probe {
    // Dolphin has process-lifetime globals. Never unload code while its callbacks
    // or static destructors can still be referenced; the OS reclaims it at exit.
    _library: ManuallyDrop<Library>,
    pump: Pump,
    stop: Stop,
}
impl Probe {
    pub fn requested() -> bool {
        std::env::var_os("EMULIA_DOLPHIN_PROBE_LIB").is_some()
    }
    pub fn start(window: &sdl2::video::Window, game: &Path) -> Result<Self, String> {
        let dylib = std::env::var_os("EMULIA_DOLPHIN_PROBE_LIB").ok_or("Missing probe library")?;
        let disc = crate::dolphin::Disc::inspect(game)?;
        let user =
            std::env::temp_dir().join(format!("emulia-dolphin-embedded-{}", std::process::id()));
        std::fs::create_dir(&user).map_err(|e| e.to_string())?;
        let config = user.join("Config");
        std::fs::create_dir(&config).map_err(|e| e.to_string())?;
        std::fs::write(config.join("Dolphin.ini"), "[Core]\nGFXBackend = Metal\n[Interface]\nSkipNKitWarning = True\n[Analytics]\nEnabled = False\nPermissionAsked = True\n[Movie]\nDumpFrames = False\n").map_err(|e| e.to_string())?;
        std::fs::write(
            config.join("GFX.ini"),
            "[Settings]\nShowFPS = True\nShowVPS = True\nShowSpeed = True\n",
        )
        .map_err(|e| e.to_string())?;
        eprintln!("Embedded Dolphin probe profile: {}", user.display());
        let cpath =
            |p: &Path| CString::new(p.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string());
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
            let library = ManuallyDrop::new(library);
            if start(native, game.as_ptr(), user.as_ptr()) == 0 {
                return Err("Embedded Dolphin failed to boot; see terminal diagnostics".into());
            }
            Ok(Self {
                _library: library,
                pump,
                stop,
            })
        }
    }
    pub fn pump(&self) -> bool {
        unsafe { (self.pump)() != 0 }
    }
    pub fn draw(&self, ui: &mut egui::Ui) -> bool {
        let mut stop = false;
        egui::Panel::top("dolphin-probe-title")
            .exact_size(crate::shell::TITLE_BAR)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label("Emulia · Dolphin");
                    stop = ui.button("Back to shelf").clicked();
                });
            });
        egui::Panel::bottom("dolphin-probe-footer")
            .exact_size(crate::shell::TIME_ROW)
            .show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label("Dolphin controls · Escape to return");
                });
            });
        stop
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        unsafe {
            (self.stop)();
        }
    }
}
