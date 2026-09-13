//! Explicit debug-build harness for local ROM qualification without OS input automation.
//! The command file is consumed once; results go to a sibling .status file.
use super::*;
#[derive(serde::Deserialize)]
struct Command {
    action: String,
    #[serde(default)]
    slot: u8,
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: i16,
    #[serde(default)]
    position: Option<[f64; 2]>,
}
impl Probe {
    pub(super) fn qualify(&mut self, app: &mut App) {
        let Some(path) = std::env::var_os("EMULIA_DOLPHIN_PROBE_COMMAND") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        if let Ok(bytes) = fs::read(&path) {
            let _ = fs::remove_file(&path);
            if let Ok(command) = serde_json::from_slice::<Command>(&bytes) {
                let action = match command.action.as_str() {
                    "pointer" => {
                        self.controls.test_pointer = command
                            .position
                            .filter(|xy| xy.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
                        None
                    }
                    "range" => {
                        if let Some(range) = command.position.filter(|xy| {
                            xy.iter().all(|v| v.is_finite() && (0.1..=2.0).contains(v))
                        }) {
                            self.controls.config.mouse_range = Some(range);
                            self.controls.dirty = true;
                        }
                        None
                    }
                    "style" => {
                        if command.slot < 4 {
                            self.controls.config.style = command.slot as usize;
                            self.controls.dirty = true;
                        }
                        None
                    }
                    "key" | "axis" => {
                        self.controls
                            .qualify_input(&command.action, &command.name, command.value);
                        None
                    }
                    "bind" => {
                        self.controls
                            .config
                            .maps
                            .entry(command.name)
                            .or_default()
                            .keyboard = Some(crate::dolphin_controls::Physical::Key("Q".into()));
                        self.controls.dirty = true;
                        None
                    }
                    "dismiss" => Some(Action::CloseMessage),
                    "pause" => Some(Action::Pause),
                    "resume" => Some(Action::Resume),
                    "slots" => Some(Action::OpenSlots),
                    "controls" => Some(Action::OpenSettings),
                    "save" => Some(Action::SaveConfirmed(command.slot)),
                    "load" => Some(Action::Load(command.slot)),
                    "back" => Some(Action::BackToShelf),
                    "reset" => Some(Action::ResetConfirmed),
                    "quit" => {
                        self.quit_after_leave = true;
                        Some(Action::BackToShelf)
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    self.action(&action, app);
                }
            }
        }
        let (pad, native_style) = unsafe {
            let pad: libloading::Symbol<unsafe extern "C" fn() -> u64> = self
                ._library
                .get(b"emulia_dolphin_pad_diagnostics\0")
                .unwrap();
            let style: libloading::Symbol<unsafe extern "C" fn() -> u32> = self
                ._library
                .get(b"emulia_dolphin_style_diagnostics\0")
                .unwrap();
            (pad(), style())
        };
        let mut evaluated = std::collections::BTreeMap::new();
        unsafe {
            let read: libloading::Symbol<unsafe extern "C" fn(u32) -> f64> = self
                ._library
                .get(b"emulia_dolphin_control_diagnostics\0")
                .unwrap();
            for (i, name) in self.controls.schema.iter().enumerate() {
                if name == "Wii/Buttons/A"
                    || name == "Classic/Buttons/A"
                    || name == "Nunchuk/Buttons/C"
                    || name == "Wii/IR/Right"
                {
                    evaluated.insert(name, read(i as u32));
                }
            }
        }
        let status = serde_json::json!({"evaluated":evaluated,"native_pad":pad,"native_style":native_style,"paused":self.paused,"busy":app.busy,"panel":format!("{:?}",app.panel),"message":app.message,"style":self.style,"inputs":self.controls.schema,"pid":std::process::id()});
        let _ = crate::files::write_atomic(
            &path.with_extension("status"),
            status.to_string().as_bytes(),
        );
    }
}
