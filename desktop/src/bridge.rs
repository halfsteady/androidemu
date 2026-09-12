//! SDL events in, egui paint out. Small on purpose: the parts of a windowing
//! backend that a game with a handful of panels needs, and nothing else.
//!
//! Building the frame and painting it are two calls rather than one. The
//! panels ask for things by pushing actions, and those actions have to be
//! applied once the pass is over — but the panels must land *over* the game
//! picture, which is drawn later still. So `frame` ends the pass and
//! tessellates, and `paint` waits until the picture is down.

use egui::{CursorIcon, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use sdl2::event::{Event as SdlEvent, WindowEvent};
use sdl2::keyboard::{Keycode, Mod, Scancode};
use sdl2::mouse::{Cursor, MouseButton, MouseWheelDirection, SystemCursor};
use sdl2::video::Window;
use std::collections::HashMap;
use std::{sync::Arc, time::Instant};

pub struct Bridge {
    pub ctx: egui::Context,
    painter: egui_glow::Painter,
    feed: Feed,
    /// What the last pass scaled by. The painter has to use the same number
    /// the shapes were tessellated with, not whatever the window says now: a
    /// display change between the two would smear the frame.
    scale: f32,
    start: Instant,
    /// The system cursors that have been asked for so far. Making one is a
    /// trip to the window server, and the pointer crosses a button sixty times
    /// a second; they are kept for the life of the window, which is also what
    /// keeps the one currently set from being freed under SDL.
    cursors: HashMap<SystemCursor, Cursor>,
    /// What the pointer is showing now, so it is only changed when it changes.
    cursor: CursorIcon,
}

/// What SDL says, in egui's words: the events waiting for the next pass, the
/// modifiers they carry, and whether the window has the keyboard.
///
/// Kept apart from the painter because the painter is the half that needs a
/// GL context, and this half is a table — the same reason `shell.rs` keeps
/// `escape` and `space` out of the loop, and what lets a test press a key.
struct Feed {
    events: Vec<Event>,
    modifiers: Modifiers,
    /// Whether the window has the keyboard. egui dims what it draws and drops
    /// held keys when it does not.
    focused: bool,
}

/// The SDL cursor for what egui asked for. Everything egui can ask for that
/// SDL has a system cursor for; anything else is the arrow, which is what a
/// pointer over something with nothing to say should be anyway.
fn system_cursor(icon: CursorIcon) -> SystemCursor {
    match icon {
        CursorIcon::PointingHand | CursorIcon::Grab | CursorIcon::Grabbing => SystemCursor::Hand,
        CursorIcon::Text | CursorIcon::VerticalText => SystemCursor::IBeam,
        CursorIcon::ResizeHorizontal
        | CursorIcon::ResizeColumn
        | CursorIcon::ResizeEast
        | CursorIcon::ResizeWest => SystemCursor::SizeWE,
        CursorIcon::ResizeVertical
        | CursorIcon::ResizeRow
        | CursorIcon::ResizeNorth
        | CursorIcon::ResizeSouth => SystemCursor::SizeNS,
        CursorIcon::ResizeNeSw | CursorIcon::ResizeNorthEast | CursorIcon::ResizeSouthWest => {
            SystemCursor::SizeNESW
        }
        CursorIcon::ResizeNwSe | CursorIcon::ResizeNorthWest | CursorIcon::ResizeSouthEast => {
            SystemCursor::SizeNWSE
        }
        CursorIcon::Move | CursorIcon::AllScroll => SystemCursor::SizeAll,
        CursorIcon::Crosshair | CursorIcon::Cell => SystemCursor::Crosshair,
        CursorIcon::NotAllowed | CursorIcon::NoDrop => SystemCursor::No,
        CursorIcon::Wait => SystemCursor::Wait,
        CursorIcon::Progress => SystemCursor::WaitArrow,
        _ => SystemCursor::Arrow,
    }
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

impl Feed {
    fn new() -> Feed {
        Feed {
            events: Vec::new(),
            modifiers: Modifiers::default(),
            focused: true,
        }
    }

    /// One SDL event, turned into however many egui ones it is worth.
    ///
    /// `typing` is whether a text field has the keyboard. Only then does egui
    /// see a key or a piece of text: its focus handler moves the focused
    /// widget on Tab, Shift+Tab and the arrows, and everything it draws
    /// swallows every key for as long as something is focused, so one Tab
    /// used to hand the keyboard to the title bar's Menu button and the game
    /// stopped answering until somebody clicked. The modifiers are told
    /// either way, because a click carries the ones that were held when it
    /// was made.
    fn handle(&mut self, event: &SdlEvent, typing: bool) {
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
                // Only a text field has any use for the key itself.
                if typing {
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
            }
            SdlEvent::TextInput { text, .. } if typing => {
                self.events.push(Event::Text(text.clone()))
            }
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
}

impl Bridge {
    pub fn new(gl: Arc<glow::Context>) -> Result<Bridge, String> {
        let painter = egui_glow::Painter::new(gl, "", None, false).map_err(|e| e.to_string())?;
        Ok(Bridge {
            ctx: egui::Context::default(),
            painter,
            feed: Feed::new(),
            scale: 1.0,
            start: Instant::now(),
            cursors: HashMap::new(),
            cursor: CursorIcon::Default,
        })
    }

    /// One SDL event on its way to egui.
    ///
    /// The keyboard belongs to the game and to the shell's hotkeys, so egui is
    /// only told about keys and typed text while a text field has the focus —
    /// of which this shell has none today. Everything else goes through
    /// whatever is on screen: see `Feed::handle`.
    pub fn handle(&mut self, event: &SdlEvent) {
        self.feed.handle(event, self.ctx.text_edit_focused());
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
    ) -> (Vec<egui::ClippedPrimitive>, egui::TexturesDelta) {
        let (w, h) = window.size();
        let mut input = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(w as f32, h as f32),
            )),
            max_texture_side: Some(self.painter.max_texture_side()),
            time: Some(self.start.elapsed().as_secs_f64()),
            events: std::mem::take(&mut self.feed.events),
            focused: self.feed.focused,
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
        // The pointer is the only part of the platform output this shell has
        // anything to say about. Copied text is the other half of it, and
        // nothing here puts anything on the clipboard.
        self.set_cursor(output.platform_output.cursor_icon);
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        (primitives, output.textures_delta)
    }

    /// Shows what the thing under the pointer is: a hand over a button, an
    /// I-beam over text. Only on a change — SDL sets the cursor on the window
    /// server every time it is asked, whether or not anything is different.
    ///
    /// A cursor that cannot be made is left as whatever is already showing:
    /// the wrong shape over a control is a small thing, and there is nothing
    /// useful to say about it to the person holding the mouse.
    fn set_cursor(&mut self, icon: CursorIcon) {
        if icon == self.cursor {
            return;
        }
        self.cursor = icon;
        let wanted = system_cursor(icon);
        let cursor = match self.cursors.entry(wanted) {
            std::collections::hash_map::Entry::Occupied(held) => held.into_mut(),
            std::collections::hash_map::Entry::Vacant(empty) => match Cursor::from_system(wanted) {
                Ok(cursor) => empty.insert(cursor),
                Err(_) => return,
            },
        };
        cursor.set();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// What the pointer says about the thing under it. The platform output
    /// used to be thrown away, which left every control on every screen under
    /// a plain arrow — the one piece of feedback a mouse gets before it
    /// clicks. Nothing here needs a window: it is a table.
    #[test]
    fn the_pointer_says_what_it_is_over() {
        assert_eq!(system_cursor(CursorIcon::Default), SystemCursor::Arrow);
        assert_eq!(system_cursor(CursorIcon::PointingHand), SystemCursor::Hand);
        assert_eq!(system_cursor(CursorIcon::Text), SystemCursor::IBeam);
        assert_eq!(
            system_cursor(CursorIcon::ResizeHorizontal),
            SystemCursor::SizeWE
        );
        assert_eq!(
            system_cursor(CursorIcon::ResizeVertical),
            SystemCursor::SizeNS
        );
        assert_eq!(
            system_cursor(CursorIcon::ResizeNeSw),
            SystemCursor::SizeNESW
        );
        assert_eq!(
            system_cursor(CursorIcon::ResizeNwSe),
            SystemCursor::SizeNWSE
        );
        assert_eq!(
            system_cursor(CursorIcon::Crosshair),
            SystemCursor::Crosshair
        );
        assert_eq!(system_cursor(CursorIcon::NotAllowed), SystemCursor::No);
        assert_eq!(system_cursor(CursorIcon::Wait), SystemCursor::Wait);
        // And anything with no system cursor behind it is the arrow rather
        // than nothing at all.
        assert_eq!(system_cursor(CursorIcon::ZoomIn), SystemCursor::Arrow);
        assert_eq!(system_cursor(CursorIcon::Help), SystemCursor::Arrow);
    }

    /// A key down, the way SDL reports one.
    fn key_down(keycode: Keycode, keymod: Mod) -> SdlEvent {
        SdlEvent::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(keycode),
            scancode: Scancode::from_keycode(keycode),
            keymod,
            repeat: false,
        }
    }

    /// The keyboard belongs to the game and to the shell's hotkeys. egui moves
    /// widget focus on Tab, Shift+Tab and the arrow keys, and then swallows
    /// every key that reaches it for as long as something is focused: one Tab
    /// handed the keyboard to the title bar's Menu button and the game stopped
    /// answering until somebody clicked or pressed Escape. Nothing in this
    /// shell is typed into, so nothing here has any use for a key.
    #[test]
    fn the_keyboard_is_the_games_until_something_is_being_typed_into() {
        let mut feed = Feed::new();
        for keycode in [
            Keycode::Tab,
            Keycode::Up,
            Keycode::Down,
            Keycode::Left,
            Keycode::Right,
            Keycode::X,
        ] {
            feed.handle(&key_down(keycode, Mod::NOMOD), false);
        }
        feed.handle(
            &SdlEvent::TextInput {
                timestamp: 0,
                window_id: 0,
                text: "x".to_string(),
            },
            false,
        );
        assert!(
            !feed
                .events
                .iter()
                .any(|e| matches!(e, Event::Key { .. } | Event::Text(_))),
            "{:?}",
            feed.events
        );
        // The modifiers go through either way: a click carries the ones that
        // were held when it was made, and that is what they are kept for.
        feed.handle(&key_down(Keycode::LShift, Mod::LSHIFTMOD), false);
        assert!(
            feed.events
                .iter()
                .any(|e| matches!(e, Event::ModifiersChanged(m) if m.shift)),
            "{:?}",
            feed.events
        );
        // And a text field, the day there is one, is typed into as usual.
        let mut feed = Feed::new();
        feed.handle(&key_down(Keycode::Tab, Mod::NOMOD), true);
        feed.handle(
            &SdlEvent::TextInput {
                timestamp: 0,
                window_id: 0,
                text: "x".to_string(),
            },
            true,
        );
        assert!(
            feed.events.iter().any(|e| matches!(
                e,
                Event::Key {
                    key: Key::Tab,
                    pressed: true,
                    ..
                }
            )),
            "{:?}",
            feed.events
        );
        assert!(
            feed.events
                .iter()
                .any(|e| matches!(e, Event::Text(t) if t == "x")),
            "{:?}",
            feed.events
        );
    }
}
