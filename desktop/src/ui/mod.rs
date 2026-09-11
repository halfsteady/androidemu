//! What the screens share: which panel is up, which question is being asked,
//! and what a drawn frame asked the shell to do.
//!
//! The panels never touch the engine, the library or the disk. They push an
//! `Action` and the shell applies it once the frame is over, so drawing stays
//! a pure function of `App` and every side effect happens in one place.

pub mod panels;
pub mod play;
pub mod settings;
pub mod shelf;
pub mod theme;
pub mod time;
pub mod widgets;

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

/// Every question the shell asks is a confirmation, so the shared prefix is
/// what the type means rather than noise on the variants.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
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
    /// Puts away the plain sentence at the bottom and nothing else. Separate
    /// from `CloseDialog` because both can be up at once: a message dismissed
    /// while a question is waiting must not answer the question.
    CloseMessage,
}
