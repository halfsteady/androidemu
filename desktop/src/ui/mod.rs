//! What the screens share: which panel is up, which question is being asked,
//! and what a drawn frame asked the shell to do.
//!
//! The panels never touch the engine, the library or the disk. They push an
//! `Action` and the shell applies it once the frame is over, so drawing stays
//! a pure function of `App` and every side effect happens in one place.

// Each submodule arrives with its task.
pub mod theme;
// The widget set is written whole; the screens that call every part of it
// arrive in Tasks 11 to 14, which is what the module's `allow(dead_code)` in
// `main.rs` is covering until then.
pub mod widgets;
// pub mod shelf;      // Task 11
// pub mod time;       // Task 12
// pub mod play;       // Task 12
// pub mod panels;     // Task 13
// pub mod settings;   // Task 14

use crate::library::Game;
use crate::picture::{Aspect, Look, PaletteChoice};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    None,
    Pause,
    Slots,
    Settings,
    Mapping,
    Problems,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    Message(String),
    ConfirmReset,
    ConfirmReplace(u8),
    ConfirmDelete(Game),
}

/// What a frame of UI asked for. Drawn code pushes these; the shell applies
/// them after the frame, so the panels never touch the engine or the disk.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Import,
    ImportFrom(PathBuf),
    OpenGame(Game),
    ChooseArt(Game),
    ChooseArtFrom(Game, PathBuf),
    ClearArt(Game),
    SetArchived(Game, bool),
    DeleteRequested(Game),
    DeleteConfirmed(Game),
    ShowArchive(bool),
    ToggleShelfList,
    OpenSettings,
    CloseSettings,
    OpenProblems,
    ClosePanel,
    Pause,
    Resume,
    OpenSlots,
    SaveRequested(u8),
    SaveConfirmed(u8),
    Load(u8),
    Screenshot,
    ToggleFullscreen,
    ResetRequested,
    ResetConfirmed,
    BackToShelf,
    SetLook(Look),
    SetAspect(Aspect),
    SetPalette(PaletteChoice),
    SetTrim(bool),
    ImportPalette,
    StartWizard,
    CancelWizard,
    Scrub(f32),
    ScrubReleased,
    JumpBack(u32),
    CloseDialog,
}
