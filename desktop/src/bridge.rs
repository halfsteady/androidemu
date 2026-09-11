//! SDL events in, egui paint out. Small on purpose: the parts of a windowing
//! backend that a game with a handful of panels needs, and nothing else.
//!
//! Building the frame and painting it are two calls rather than one. The
//! panels ask for things by pushing actions, and those actions have to be
//! applied once the pass is over — but the panels must land *over* the game
//! picture, which is drawn later still. So `frame` ends the pass and
//! tessellates, and `paint` waits until the picture is down.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use sdl2::event::{Event as SdlEvent, WindowEvent};
use sdl2::keyboard::{Keycode, Mod, Scancode};
use sdl2::mouse::{MouseButton, MouseWheelDirection};
use sdl2::video::Window;
use std::{sync::Arc, time::Instant};

pub struct Bridge {
    pub ctx: egui::Context,
    painter: egui_glow::Painter,
    events: Vec<Event>,
    modifiers: Modifiers,
    /// Whether the window has the keyboard. egui dims what it draws and drops
    /// held keys when it does not.
    focused: bool,
    /// What the last pass scaled by. The painter has to use the same number
    /// the shapes were tessellated with, not whatever the window says now: a
    /// display change between the two would smear the frame.
    scale: f32,
    start: Instant,
}

/// The name a physical key is saved under in a controller profile. Physical,
/// not logical: a profile written on one keyboard layout has to keep meaning
/// the same place on the keyboard.
pub fn key_name(scancode: Scancode) -> String {
    scancode.name().to_string()
}

fn modifiers(keymod: Mod) -> Modifiers {
    let mac = cfg!(target_os = "macos");
    let cmd = keymod.intersects(Mod::LGUIMOD | Mod::RGUIMOD);
    let ctrl = keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
    Modifiers {
        alt: keymod.intersects(Mod::LALTMOD | Mod::RALTMOD),
        ctrl,
        shift: keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD),
        mac_cmd: mac && cmd,
        command: if mac { cmd } else { ctrl },
    }
}

fn key(keycode: Keycode) -> Option<Key> {
    Key::from_name(&keycode.name())
}

fn button(b: MouseButton) -> Option<PointerButton> {
    match b {
        MouseButton::Left => Some(PointerButton::Primary),
        MouseButton::Right => Some(PointerButton::Secondary),
        MouseButton::Middle => Some(PointerButton::Middle),
        _ => None,
    }
}

impl Bridge {
    pub fn new(gl: Arc<glow::Context>) -> Result<Bridge, String> {
        let painter = egui_glow::Painter::new(gl, "", None, false).map_err(|e| e.to_string())?;
        Ok(Bridge {
            ctx: egui::Context::default(),
            painter,
            events: Vec::new(),
            modifiers: Modifiers::default(),
            focused: true,
            scale: 1.0,
            start: Instant::now(),
        })
    }

    /// What is held down right now, for the panels that read a shift-click.
    // No panel reads a shift-click, and egui is handed the modifiers on every
    // event anyway, so this accessor is the bridge's surface rather than a use.
    #[allow(dead_code)]
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn handle(&mut self, event: &SdlEvent) {
        match event {
            SdlEvent::MouseMotion { x, y, .. } => self
                .events
                .push(Event::PointerMoved(Pos2::new(*x as f32, *y as f32))),
            SdlEvent::MouseButtonDown {
                mouse_btn, x, y, ..
            }
            | SdlEvent::MouseButtonUp {
                mouse_btn, x, y, ..
            } => {
                if let Some(button) = button(*mouse_btn) {
                    let pressed = matches!(event, SdlEvent::MouseButtonDown { .. });
                    self.events.push(Event::PointerButton {
                        pos: Pos2::new(*x as f32, *y as f32),
                        button,
                        pressed,
                        modifiers: self.modifiers,
                    });
                }
            }
            SdlEvent::MouseWheel {
                precise_x,
                precise_y,
                direction,
                ..
            } => {
                // Natural scrolling arrives as a flipped wheel rather than
                // negated numbers, so the sign has to be put back by hand or
                // every list scrolls the wrong way on a Mac trackpad.
                let sign = match direction {
                    MouseWheelDirection::Flipped => -1.0,
                    _ => 1.0,
                };
                self.events.push(Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(*precise_x * sign, *precise_y * sign),
                    phase: egui::TouchPhase::Move,
                    modifiers: self.modifiers,
                });
            }
            SdlEvent::KeyDown {
                keycode: Some(keycode),
                keymod,
                repeat,
                ..
            }
            | SdlEvent::KeyUp {
                keycode: Some(keycode),
                keymod,
                repeat,
                ..
            } => {
                let now = modifiers(*keymod);
                if now != self.modifiers {
                    self.modifiers = now;
                    self.events.push(Event::ModifiersChanged(now));
                }
                if let Some(key) = key(*keycode) {
                    let pressed = matches!(event, SdlEvent::KeyDown { .. });
                    self.events.push(Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: *repeat,
                        modifiers: self.modifiers,
                    });
                }
            }
            SdlEvent::TextInput { text, .. } => self.events.push(Event::Text(text.clone())),
            SdlEvent::Window {
                win_event: WindowEvent::FocusGained,
                ..
            } => {
                self.focused = true;
                self.events.push(Event::WindowFocused(true));
            }
            SdlEvent::Window {
                win_event: WindowEvent::FocusLost,
                ..
            } => {
                // Modifiers released while another window had the keyboard are
                // never reported, so a Shift held on the way out would stay
                // held for ever. Nothing is down once we cannot see it.
                self.focused = false;
                self.modifiers = Modifiers::default();
                self.events
                    .push(Event::ModifiersChanged(Modifiers::default()));
                self.events.push(Event::WindowFocused(false));
                self.events.push(Event::PointerGone);
            }
            SdlEvent::Window {
                win_event: WindowEvent::Leave,
                ..
            } => self.events.push(Event::PointerGone),
            _ => {}
        }
    }

    /// Device pixels to a point. SDL reports mouse positions and window size
    /// in points and the drawable in pixels, so their ratio is the scale the
    /// display is running at — including a retina screen a window is dragged
    /// onto halfway through a session.
    pub fn pixels_per_point(window: &Window) -> f32 {
        let (w, _) = window.size();
        let (dw, _) = window.drawable_size();
        if w == 0 {
            1.0
        } else {
            dw as f32 / w as f32
        }
    }

    /// Builds one frame of UI and turns it into triangles without drawing
    /// anything. The panels are handed a root `Ui` rather than the context:
    /// that is what egui 0.36 shows a panel inside, and `run_ui` is what puts
    /// the built-in plugins — text selection, drag and drop — around it.
    pub fn frame(
        &mut self,
        window: &Window,
        build: impl FnMut(&mut egui::Ui),
    ) -> (
        egui::PlatformOutput,
        Vec<egui::ClippedPrimitive>,
        egui::TexturesDelta,
    ) {
        let (w, h) = window.size();
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(w as f32, h as f32),
            )),
            max_texture_side: Some(self.painter.max_texture_side()),
            time: Some(self.start.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.events),
            focused: self.focused,
            ..Default::default()
        };
        // Told as the display's own scale rather than as a zoom, so that a
        // reader who wants bigger text later has a zoom factor left to turn.
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(Self::pixels_per_point(window));
        let output = self.ctx.run_ui(input, build);
        self.scale = output.pixels_per_point;
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        (output.platform_output, primitives, output.textures_delta)
    }

    pub fn paint(
        &mut self,
        window: &Window,
        primitives: &[egui::ClippedPrimitive],
        textures: &mut egui::TexturesDelta,
    ) {
        let (dw, dh) = window.drawable_size();
        self.painter
            .paint_and_update_textures([dw, dh], self.scale, primitives, textures);
    }

    /// Gives the painter's GL objects back while the context is still current.
    pub fn destroy(&mut self) {
        self.painter.destroy();
    }
}
