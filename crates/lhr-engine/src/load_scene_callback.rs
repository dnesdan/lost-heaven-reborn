//! The mission loading screen's load callback (`cbLoadScene`, PS2 name).
//!
//! FUNCTION: GAME RVA 0x1bcc10, VA 0x5bcc10 (mafia-re `fe7e746`,
//! `game/ui/Load_progress.cpp:17-70`). Game.exe SHA-256 08398317…16ab27,
//! Ghidra 12.1.4 body 0x5bcc10..0x5bcdca (REA ev_aff337665f). Its two
//! globals, `menuProgress` (GAME 0x647b20) and the function static
//! `last_bit` (GAME 0x647b1c), are owned by [`LoadSceneCallback`]; the
//! `mafiaLoadSave.progress` screen it prefers stays with
//! [`crate::game_loadsave_lifecycle::LoadSaveProgress`].
//!
//! Its device effects, the STEP's `igraph->SetState(DXS_ALPHA,
//! ALPHASTATE_BLEND)` and `GM_LoadProgress::Present` (GAME 0x5e3d40), go to
//! the caller's [`LoadingScreenDevice`]: during `C_mission::Open` that is the
//! Open's own device backend (the one its scene `Render` uses). The menu
//! drawing inside `Present` (`GM_Menu::Draw`, GAME 0x5ea7e0) needs the
//! component-class draw owners, so the device receives the live screen as a
//! read-only [`LoadingScreenFrame`] and nothing here draws it.
//!
//! Like the loading screen, the two globals are session-scoped here: a
//! process-wide `menuProgress`/`last_bit` would outlive the session that
//! owns the scene load reporting to them.
#![forbid(unsafe_code)]

use crate::game_loadsave_lifecycle::{LoadProgressCreateInputs, LoadProgressScreen};

/// `CBM_PROGRESS` (I3D.h:296): a loader's progress report.
pub const CBM_PROGRESS: u32 = 8;
/// The game's own mission-progress message (`cmp eax, 0x2710` at 0x5bcc2e;
/// mafia-re's `CBM_MISSION_PROGRESS`).
pub const CBM_MISSION_PROGRESS: u32 = 10_000;
/// `CBM_PROGRESS`' second parameter (I3D.h:324-326).
pub const LOAD_PROGRESS_BEGIN: u32 = 0;
pub const LOAD_PROGRESS_STEP: u32 = 1;
pub const LOAD_PROGRESS_END: u32 = 2;
/// `fcomp [0x6233b4]` (0x3ca3d70a): the least change a STEP shows.
const STEP_THRESHOLD: f32 = 0.02;

/// Which `GM_LoadProgress` a [`LoadingScreenFrame`] presents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadingScreenSource {
    /// `mafiaLoadSave.progress` (GAME 0x6d4bfc).
    MafiaLoadSave,
    /// `cbLoadScene`'s own `menuProgress` (GAME 0x647b20).
    LoadCallback,
}

/// The live screen at a `GM_LoadProgress::Present` (GAME 0x5e3d40), as the
/// device's `Draw(MENUDRAW_ALL)` would read it. Read-only presentation
/// data, never a second owner of the screen.
#[derive(Clone, Copy, Debug)]
pub struct LoadingScreenFrame<'a> {
    pub source: LoadingScreenSource,
    pub screen: &'a LoadProgressScreen,
}

/// The device effects the loading screen reaches.
pub trait LoadingScreenDevice {
    /// `igraph->SetState(DXS_ALPHA, ALPHASTATE_BLEND)` (vtable +0x44 with
    /// `push 1; push 0` at 0x5bccc6-0x5bcccd), before a STEP's bar update.
    fn set_alpha_blend_state(&mut self) -> Result<(), String>;
    /// `GM_LoadProgress::Present` (GAME 0x5e3d40, REA ev_47717f3abd):
    /// `ResetRenderProps`, `Clear(0, 1.0f, 3)`, `BeginScene`,
    /// `SetState(DXS_ALPHA, ALPHASTATE_BLEND)`, `Draw(MENUDRAW_ALL)`
    /// (0x5ea7e0), `EndScene`, `Present`. The original ignores every
    /// result; an `Err` here is a host stop, never a skipped frame.
    fn present_loading_screen(&mut self, frame: LoadingScreenFrame<'_>) -> Result<(), String>;
}

/// `cbLoadScene`'s globals: `menuProgress` (GAME 0x647b20, NULL at process
/// start) and `last_bit` (GAME 0x647b1c, zero-initialized static).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoadSceneCallback {
    menu_progress: Option<LoadProgressScreen>,
    last_bit: f32,
}

impl LoadSceneCallback {
    /// The callback's own screen, while one exists.
    pub fn menu_progress(&self) -> Option<&LoadProgressScreen> {
        self.menu_progress.as_ref()
    }

    /// The last bit a STEP showed (or BEGIN's 0.0).
    pub fn last_bit(&self) -> f32 {
        self.last_bit
    }

    /// `cbLoadScene(msg, prm1, prm2, context)` over `progress`
    /// (`mafiaLoadSave.progress`, reread at every test as the original
    /// does). Messages other than `CBM_PROGRESS` and `CBM_MISSION_PROGRESS`
    /// return at once. `CBM_PROGRESS` turns `LOAD_PROGRESS_END` into a STEP
    /// and halves `prm1`'s float; the mission's message maps `prm1` to
    /// `(p + 1.0f) * 0.5f` (0x5bcc39-0x5bcc50, PC24: every operation rounds
    /// to binary32). Then:
    ///
    /// - BEGIN (0x5bcd44): without `mafiaLoadSave.progress`, `menuProgress =
    ///   new GM_LoadProgress(0, 1.0f, NULL)` (stored before `Create`),
    ///   `Create`, `Present`; with it, `menuProgress = NULL`, its `Create`
    ///   and `Present`. Then `last_bit = 0`.
    /// - STEP (0x5bcca6): a change below 0.02f (`fsub; fcomp; test ah, 5;
    ///   jnp`, so NaN continues) returns; else `SetState`, then
    ///   `SetProgressMissionOpen(bit)` + `Present` on `mafiaLoadSave.progress`,
    ///   else on `menuProgress`, and `last_bit = bit` (stored even without a
    ///   screen).
    /// - END (0x5bcc64): `menuProgress->Destroy()` (GAME 0x5ea690) and NULL.
    ///
    /// Any other `prm2` does nothing. A host `Err` from `Create` or the
    /// device stops with the reached stores kept; the later stores of that
    /// branch do not run.
    #[allow(clippy::too_many_arguments)]
    pub fn cb_load_scene(
        &mut self,
        progress: &mut Option<LoadProgressScreen>,
        msg: u32,
        prm1: u32,
        prm2: u32,
        inputs: &mut dyn LoadProgressCreateInputs,
        device: &mut dyn LoadingScreenDevice,
    ) -> Result<(), String> {
        let (bit, phase) = match msg {
            CBM_PROGRESS => {
                let phase = if prm2 == LOAD_PROGRESS_END {
                    LOAD_PROGRESS_STEP
                } else {
                    prm2
                };
                (f32::from_bits(prm1) * 0.5, phase)
            }
            CBM_MISSION_PROGRESS => ((f32::from_bits(prm1) + 1.0) * 0.5, prm2),
            _ => return Ok(()),
        };
        match phase {
            LOAD_PROGRESS_BEGIN => {
                match progress.as_mut() {
                    None => {
                        let screen = self
                            .menu_progress
                            .insert(LoadProgressScreen::for_load_callback());
                        screen.create(inputs)?;
                        device.present_loading_screen(LoadingScreenFrame {
                            source: LoadingScreenSource::LoadCallback,
                            screen,
                        })?;
                    }
                    Some(screen) => {
                        self.menu_progress = None;
                        screen.create(inputs)?;
                        device.present_loading_screen(LoadingScreenFrame {
                            source: LoadingScreenSource::MafiaLoadSave,
                            screen,
                        })?;
                    }
                }
                self.last_bit = 0.0;
            }
            LOAD_PROGRESS_STEP => {
                if bit - self.last_bit < STEP_THRESHOLD {
                    return Ok(());
                }
                device.set_alpha_blend_state()?;
                if let Some(screen) = progress.as_mut() {
                    screen.set_progress_mission_open(bit);
                    device.present_loading_screen(LoadingScreenFrame {
                        source: LoadingScreenSource::MafiaLoadSave,
                        screen,
                    })?;
                } else if let Some(screen) = self.menu_progress.as_mut() {
                    screen.set_progress_mission_open(bit);
                    device.present_loading_screen(LoadingScreenFrame {
                        source: LoadingScreenSource::LoadCallback,
                        screen,
                    })?;
                }
                self.last_bit = bit;
            }
            LOAD_PROGRESS_END => {
                if let Some(mut screen) = self.menu_progress.take() {
                    // GM_Menu::Destroy: the components, then `delete this`
                    // (the drop). The destructor's menuCount half stays with
                    // the menu-resource owner.
                    screen.menu.destroy();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "load_scene_callback_tests.rs"]
mod tests;
