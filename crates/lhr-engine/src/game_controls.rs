//! PC Game.exe control lifetime over one owned input object.
//!
//! Source: mafia-re f42a25d, game/input/controls.cpp, Game.exe entries
//! 0x4efb40 (constructor), 0x4efbd0 (destructor), 0x4efc80 (Close),
//! 0x4efde0 (Init), 0x4efdb0 (SetMouseScale), 0x4f00f0
//! (ClearControls), 0x4f02b0 (SetControl). Game.exe SHA-256 in the Rust
//! research ledger:
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! `C_controls` is a PC researcher-assigned name, not a recovered PC symbol.
//! This module does not implement IGraph, IJoy, polling, bindings or menus.

/// Identity from the live graph owner. Never a native pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlGraph(pub u64);

/// Identity from a live IJoy owner. `add_ref` and `release` below must act on it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlJoystick(pub u64);

/// S_joy_state is 68 bytes in the PC source: eight axes, four POV floats,
/// four centered bytes and four button words. These bytes are only storage for
/// the Init zero-fill; no native pointer or joystick behavior is inferred.
pub const JOYSTICK_STATE_BYTES: usize = 68;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnedJoystick {
    pub identity: ControlJoystick,
    pub state: [u8; JOYSTICK_STATE_BYTES],
    pub changes: [u8; JOYSTICK_STATE_BYTES],
}

/// Constructor-unwritten inline fields arrive from the caller's allocation.
/// Native constructor does not zero them; Init writes only the three arrays
/// and key_mask, and Close leaves them as they were.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlAllocationSeed {
    pub last_input: [u8; 12],
    pub keys: [u8; 64],
    pub key_refs: [u8; 64],
    pub mouse_refs: [u8; 4],
    pub key_mask: u64,
    pub mouse_mask: u32,
    pub name: [u8; 32],
}

/// Backend effects are ordered native dependency boundaries. A backend error
/// stops the caller at its reached prefix; it is never converted into success.
pub trait ControlRefBackend {
    type Error;
    fn add_ref(&mut self, joystick: ControlJoystick) -> Result<(), Self::Error>;
    fn release(&mut self, joystick: ControlJoystick) -> Result<(), Self::Error>;
}

/// The two external effects reached by SetControl. Their callbacks must act
/// on the actual graph and joystick owners; no event-only default is provided.
pub trait ControlBindingBackend: ControlRefBackend {
    /// The graph receives the live key array. The callback can also change a
    /// binding that SetControl will read again after this effect.
    fn define_keys(
        &mut self,
        graph: ControlGraph,
        owner: &mut GameControls,
    ) -> Result<(), Self::Error>;
    fn set_axis_mode(
        &mut self,
        joystick: ControlJoystick,
        axis: u16,
        mode: u32,
        owner: &mut GameControls,
    ) -> Result<(), Self::Error>;
}

/// PC S_control, preserving its 12-byte field widths and values. The device
/// and axis-mode labels are researcher-assigned; the numeric cases are source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlBinding {
    pub half: i32,
    pub device: u16,
    pub code: u16,
    pub joystick_index: i32,
}
impl ControlBinding {
    fn from_bytes(bytes: [u8; 12]) -> Self {
        Self {
            half: i32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            device: u16::from_le_bytes(bytes[4..6].try_into().unwrap()),
            code: u16::from_le_bytes(bytes[6..8].try_into().unwrap()),
            joystick_index: i32::from_le_bytes(bytes[8..12].try_into().unwrap()),
        }
    }
    fn into_bytes(self) -> [u8; 12] {
        let mut bytes = [0; 12];
        bytes[0..4].copy_from_slice(&self.half.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.device.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.code.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.joystick_index.to_le_bytes());
        bytes
    }
}

/// Source of the original `S_control *` argument. A slot names storage in
/// this same control owner; an external reader borrows a caller-owned live
/// binding and may observe changes made by graph/joystick callbacks. None of
/// these variants fabricates a native pointer or caches its contents.
pub enum LiveControlBinding<'a> {
    Null,
    Slot(usize),
    Borrowed(&'a ControlBinding),
    External(&'a dyn Fn() -> ControlBinding),
}
impl LiveControlBinding<'_> {
    fn read<E>(&self, owner: &GameControls) -> Result<ControlBinding, ControlFailure<E>> {
        match self {
            Self::Null => Err(ControlFailure::SourceUnavailable),
            Self::Slot(index) => owner
                .binding(*index)
                .ok_or(ControlFailure::SourceUnavailable),
            Self::Borrowed(binding) => Ok(**binding),
            Self::External(read) => Ok(read()),
        }
    }
}

/// Rust host admission failure, distinct from a native false return.
#[derive(Debug, PartialEq, Eq)]
pub enum ControlFailure<E> {
    Backend(E),
    Interrupted,
    GraphUnavailable,
    SourceUnavailable,
    DestinationUnavailable,
}

/// Live owner for the PC controls' constructor, Init and Close prefix.
/// The binding table's records stay opaque until SetControl is reconstructed.
pub struct GameControls {
    graph: Option<ControlGraph>,
    joys: Vec<OwnedJoystick>,
    controls: Vec<[u8; 12]>,
    values: Vec<f32>,
    pressed: Vec<f32>,
    mouse_scale: [f32; 3],
    seed: ControlAllocationSeed,
    interrupted: bool,
}

impl GameControls {
    /// PC constructor 0x4efb40. The three scale stores precede graph=NULL.
    pub fn construct(seed: ControlAllocationSeed) -> Self {
        Self {
            graph: None,
            joys: Vec::new(),
            controls: Vec::new(),
            values: Vec::new(),
            pressed: Vec::new(),
            mouse_scale: [20.0, 20.0, 20.0],
            seed,
            interrupted: false,
        }
    }

    pub fn graph(&self) -> Option<ControlGraph> {
        self.graph
    }
    pub fn joysticks(&self) -> &[OwnedJoystick] {
        &self.joys
    }
    pub fn mouse_scale(&self) -> [f32; 3] {
        self.mouse_scale
    }
    pub fn allocation_seed(&self) -> &ControlAllocationSeed {
        &self.seed
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    pub fn binding(&self, index: usize) -> Option<ControlBinding> {
        self.controls
            .get(index)
            .copied()
            .map(ControlBinding::from_bytes)
    }
    /// Typed mutation surface for a graph/joystick callback that changes a
    /// slot before the caller's final live binding read.
    pub fn replace_binding_slot(&mut self, index: usize, binding: ControlBinding) -> bool {
        let Some(slot) = self.controls.get_mut(index) else {
            return false;
        };
        *slot = binding.into_bytes();
        true
    }
    pub fn keys(&self) -> &[u8; 64] {
        &self.seed.keys
    }

    /// PC SetMouseScale 0x4efdb0 writes x, y, z in that order. Conversion is
    /// ordinary signed integer to binary32; no clamping or graph call occurs.
    pub fn set_mouse_scale(&mut self, x: i32, y: i32, z: i32) -> bool {
        self.mouse_scale[0] = x as f32;
        self.mouse_scale[1] = y as f32;
        self.mouse_scale[2] = z as f32;
        true
    }

    /// PC ClearControls 0x4f00f0 keeps graph and joystick references. It
    /// retains the key mask, mouse mask and mouse scales too.
    pub fn clear_controls(&mut self) {
        self.controls.clear();
        self.values.clear();
        self.pressed.clear();
        self.seed.keys.fill(0);
        self.seed.key_refs.fill(0);
        self.seed.mouse_refs.fill(0);
    }

    fn define_keys<B: ControlBindingBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ControlFailure<B::Error>> {
        let graph = self.graph.ok_or(ControlFailure::GraphUnavailable)?;
        if let Err(error) = backend.define_keys(graph, self) {
            self.interrupted = true;
            return Err(ControlFailure::Backend(error));
        }
        Ok(())
    }

    /// PC SetControl 0x4f02b0. The source is dereferenced after old-reference
    /// effects and again for the final 12-byte copy after new-binding effects.
    /// A native false result can leave the previous binding in place after
    /// removing its key/mouse reference; no rollback occurs.
    pub fn set_control<B: ControlBindingBackend>(
        &mut self,
        binding: LiveControlBinding<'_>,
        index: i32,
        backend: &mut B,
    ) -> Result<bool, ControlFailure<B::Error>> {
        if self.interrupted {
            return Err(ControlFailure::Interrupted);
        }
        if index < 0 {
            return Ok(false);
        }
        let index = index as usize;
        while self.controls.len() <= index {
            self.controls.push([0; 12]);
        }
        while self.values.len() <= index {
            self.values.push(0.0);
        }
        while self.pressed.len() <= index {
            self.pressed.push(0.0);
        }
        let old = ControlBinding::from_bytes(self.controls[index]);
        if old.device == 1 {
            if let Some(slot) = self.seed.keys.iter().position(|&key| key == old.code as u8) {
                self.seed.key_refs[slot] = self.seed.key_refs[slot].wrapping_sub(1);
                if self.seed.key_refs[slot] == 0 {
                    self.seed.keys[slot] = 0;
                    self.define_keys(backend)?;
                }
            }
        }
        // PC 0x4f03f4/0x4f03fa reloads the destination after the old-key
        // DefineKeys callback, then 0x4f0407 reads its current mouse code.
        let old_mouse = self
            .binding(index)
            .ok_or(ControlFailure::DestinationUnavailable)?;
        if old_mouse.device == 2 {
            if let Some(slot) = mouse_button_slot(old_mouse.code) {
                self.seed.mouse_refs[slot] = self.seed.mouse_refs[slot].wrapping_sub(1);
            }
        }
        if !matches!(binding, LiveControlBinding::Null) {
            // The PC reads the device word here, after old-reference removal.
            let device = binding.read::<B::Error>(self)?.device;
            if device == 1 {
                // The key byte is loaded once before the search, and retained
                // across DefineKeys. The full record is read again afterward.
                let code = binding.read::<B::Error>(self)?.code as u8;
                if let Some(slot) = self.seed.keys.iter().position(|&key| key == code) {
                    self.seed.key_refs[slot] = self.seed.key_refs[slot].wrapping_add(1);
                } else if let Some(slot) = self.seed.keys.iter().position(|&key| key == 0) {
                    self.seed.keys[slot] = code;
                    self.define_keys(backend)?;
                    self.seed.key_refs[slot] = self.seed.key_refs[slot].wrapping_add(1);
                } else {
                    return Ok(false);
                }
            } else if device == 2 {
                let code = binding.read::<B::Error>(self)?.code;
                if let Some(slot) = mouse_button_slot(code) {
                    self.seed.mouse_refs[slot] = self.seed.mouse_refs[slot].wrapping_add(1);
                }
            } else if device == 3 {
                let joystick_index = binding.read::<B::Error>(self)?.joystick_index;
                let Some(joystick) = self.joys.get(joystick_index as u32 as usize) else {
                    return Ok(false);
                };
                let joystick = joystick.identity;
                let axis = binding.read::<B::Error>(self)?;
                let mode = if axis.half == 3 && axis.code < 0xff {
                    4
                } else {
                    0
                };
                if let Err(error) = backend.set_axis_mode(joystick, axis.code, mode, self) {
                    self.interrupted = true;
                    return Err(ControlFailure::Backend(error));
                }
            }
            let final_binding = binding.read::<B::Error>(self)?;
            let Some(destination) = self.controls.get_mut(index) else {
                return Err(ControlFailure::DestinationUnavailable);
            };
            *destination = final_binding.into_bytes();
        } else {
            let Some(destination) = self.controls.get_mut(index) else {
                return Err(ControlFailure::DestinationUnavailable);
            };
            *destination = [0; 12];
        }
        Ok(true)
    }

    /// PC Init 0x4efde0. A null graph exits before reading joystick entries.
    /// Null joysticks are tested from last to first, before any member store.
    /// Reentry appends joystick references; the original does not call Close.
    /// `joysticks` represents the caller's valid list extent; a bad native
    /// pointer/count pair is outside this Rust API's admitted domain.
    pub fn init<B: ControlRefBackend>(
        &mut self,
        graph: Option<ControlGraph>,
        joysticks: &[Option<ControlJoystick>],
        backend: &mut B,
    ) -> Result<bool, ControlFailure<B::Error>> {
        if self.interrupted {
            return Err(ControlFailure::Interrupted);
        }
        let Some(graph) = graph else {
            return Ok(false);
        };
        for joystick in joysticks.iter().rev() {
            if joystick.is_none() {
                return Ok(false);
            }
        }
        self.seed.keys.fill(0);
        self.seed.key_refs.fill(0);
        self.seed.mouse_refs.fill(0);
        self.graph = Some(graph);
        for &joystick in joysticks {
            let joystick = joystick.expect("reverse preflight proved every entry present");
            if let Err(error) = backend.add_ref(joystick) {
                self.interrupted = true;
                return Err(ControlFailure::Backend(error));
            }
            self.joys.push(OwnedJoystick {
                identity: joystick,
                state: [0; JOYSTICK_STATE_BYTES],
                changes: [0; JOYSTICK_STATE_BYTES],
            });
        }
        self.seed.key_mask = u64::MAX;
        Ok(true)
    }

    /// PC Close 0x4efc80. It clears binding/value/pressed vectors, releases
    /// joysticks in descending order, then clears the joystick vector. It does
    /// not clear graph, key arrays, masks, last input or mouse scales.
    pub fn close<B: ControlRefBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ControlFailure<B::Error>> {
        if self.interrupted {
            return Err(ControlFailure::Interrupted);
        }
        self.controls.clear();
        self.values.clear();
        self.pressed.clear();
        for index in (0..self.joys.len()).rev() {
            let joystick = self.joys[index].identity;
            if let Err(error) = backend.release(joystick) {
                self.interrupted = true;
                return Err(ControlFailure::Backend(error));
            }
        }
        self.joys.clear();
        Ok(())
    }

    /// PC destructor 0x4efbd0 calls Close. Host ownership deliberately does
    /// not call a backend from Drop, where ordered errors cannot be reported.
    pub fn destruct<B: ControlRefBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ControlFailure<B::Error>> {
        self.close(backend)
    }
}

fn mouse_button_slot(code: u16) -> Option<usize> {
    match code {
        1 => Some(0),
        2 => Some(1),
        4 => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Refs {
        events: Vec<(bool, ControlJoystick)>,
        fail_at: Option<usize>,
        binding_events: Vec<BindingEvent>,
        fail_binding_at: Option<usize>,
    }
    #[derive(Clone, Debug, Eq, PartialEq)]
    enum BindingEvent {
        Define(ControlGraph, [u8; 64]),
        Axis(ControlJoystick, u16, u32),
    }
    impl ControlRefBackend for Refs {
        type Error = usize;
        fn add_ref(&mut self, joystick: ControlJoystick) -> Result<(), Self::Error> {
            self.events.push((true, joystick));
            if self.fail_at == Some(self.events.len()) {
                Err(self.events.len())
            } else {
                Ok(())
            }
        }
        fn release(&mut self, joystick: ControlJoystick) -> Result<(), Self::Error> {
            self.events.push((false, joystick));
            if self.fail_at == Some(self.events.len()) {
                Err(self.events.len())
            } else {
                Ok(())
            }
        }
    }
    impl ControlBindingBackend for Refs {
        fn define_keys(
            &mut self,
            graph: ControlGraph,
            owner: &mut GameControls,
        ) -> Result<(), Self::Error> {
            self.binding_events
                .push(BindingEvent::Define(graph, *owner.keys()));
            if self.fail_binding_at == Some(self.binding_events.len()) {
                Err(self.binding_events.len())
            } else {
                Ok(())
            }
        }
        fn set_axis_mode(
            &mut self,
            joystick: ControlJoystick,
            axis: u16,
            mode: u32,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            self.binding_events
                .push(BindingEvent::Axis(joystick, axis, mode));
            if self.fail_binding_at == Some(self.binding_events.len()) {
                Err(self.binding_events.len())
            } else {
                Ok(())
            }
        }
    }
    fn seed() -> ControlAllocationSeed {
        ControlAllocationSeed {
            last_input: [0x81; 12],
            keys: [0x82; 64],
            key_refs: [0x83; 64],
            mouse_refs: [0x84; 4],
            key_mask: 0x9753_1bdf_8585_8585,
            mouse_mask: 0x8686_8686,
            name: [0x87; 32],
        }
    }
    #[test]
    fn constructor_and_false_init_keep_unwritten_state_and_emit_no_effect() {
        let original = seed();
        let mut controls = GameControls::construct(original.clone());
        let mut refs = Refs::default();
        assert_eq!(controls.mouse_scale(), [20.0; 3]);
        assert_eq!(controls.graph(), None);
        assert_eq!(controls.allocation_seed(), &original);
        assert_eq!(
            controls.init(None, &[Some(ControlJoystick(1))], &mut refs),
            Ok(false)
        );
        assert_eq!(
            controls.init(
                Some(ControlGraph(4)),
                &[Some(ControlJoystick(1)), None],
                &mut refs
            ),
            Ok(false)
        );
        assert_eq!(controls.graph(), None);
        assert_eq!(controls.allocation_seed(), &original);
        assert!(refs.events.is_empty());
    }
    #[test]
    fn init_failure_preserves_both_seed_mask_halves_and_mouse_mask() {
        let original = seed();
        assert_ne!(original.key_mask >> 32, original.key_mask & 0xffff_ffff);
        let mut controls = GameControls::construct(original.clone());
        let mut refs = Refs {
            fail_at: Some(1),
            ..Default::default()
        };
        assert_eq!(
            controls.init(
                Some(ControlGraph(3)),
                &[Some(ControlJoystick(9))],
                &mut refs
            ),
            Err(ControlFailure::Backend(1))
        );
        assert_eq!(controls.allocation_seed().key_mask, original.key_mask);
        assert_eq!(controls.allocation_seed().mouse_mask, original.mouse_mask);
        assert_eq!(controls.close(&mut refs), Err(ControlFailure::Interrupted));
        assert_eq!(controls.allocation_seed().key_mask, original.key_mask);
        assert_eq!(controls.allocation_seed().mouse_mask, original.mouse_mask);
    }
    #[test]
    fn init_adds_references_forward_and_close_releases_reverse() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        let first = ControlJoystick(11);
        let second = ControlJoystick(22);
        assert_eq!(
            controls.init(
                Some(ControlGraph(7)),
                &[Some(first), Some(second)],
                &mut refs
            ),
            Ok(true)
        );
        assert_eq!(controls.graph(), Some(ControlGraph(7)));
        assert_eq!(
            controls
                .joysticks()
                .iter()
                .map(|j| j.identity)
                .collect::<Vec<_>>(),
            [first, second]
        );
        assert!(controls.joysticks().iter().all(
            |j| j.state == [0; JOYSTICK_STATE_BYTES] && j.changes == [0; JOYSTICK_STATE_BYTES]
        ));
        assert_eq!(controls.allocation_seed().keys, [0; 64]);
        assert_eq!(controls.allocation_seed().key_refs, [0; 64]);
        assert_eq!(controls.allocation_seed().mouse_refs, [0; 4]);
        assert_eq!(controls.allocation_seed().key_mask, u64::MAX);
        assert_eq!(controls.allocation_seed().mouse_mask, 0x8686_8686);
        controls.destruct(&mut refs).unwrap();
        assert_eq!(
            refs.events,
            [
                (true, first),
                (true, second),
                (false, second),
                (false, first)
            ]
        );
        assert!(controls.joysticks().is_empty());
        assert_eq!(controls.graph(), Some(ControlGraph(7)));
    }
    #[test]
    fn init_reentry_appends_and_failed_callback_keeps_completed_prefix() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        assert_eq!(
            controls.init(
                Some(ControlGraph(1)),
                &[Some(ControlJoystick(10))],
                &mut refs
            ),
            Ok(true)
        );
        refs.fail_at = Some(3);
        assert_eq!(
            controls.init(
                Some(ControlGraph(2)),
                &[Some(ControlJoystick(20)), Some(ControlJoystick(30))],
                &mut refs
            ),
            Err(ControlFailure::Backend(3))
        );
        assert_eq!(
            refs.events,
            [
                (true, ControlJoystick(10)),
                (true, ControlJoystick(20)),
                (true, ControlJoystick(30))
            ]
        );
        assert_eq!(controls.graph(), Some(ControlGraph(2)));
        assert_eq!(
            controls
                .joysticks()
                .iter()
                .map(|j| j.identity)
                .collect::<Vec<_>>(),
            [ControlJoystick(10), ControlJoystick(20)]
        );
        assert_eq!(controls.allocation_seed().key_mask, u64::MAX);
        assert_eq!(controls.allocation_seed().mouse_mask, 0x8686_8686);
        assert_eq!(controls.close(&mut refs), Err(ControlFailure::Interrupted));
        assert_eq!(refs.events.len(), 3);
    }
    #[test]
    fn failed_release_keeps_owner_for_diagnostics_and_stops_later_effects() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        assert_eq!(
            controls.init(
                Some(ControlGraph(1)),
                &[Some(ControlJoystick(10)), Some(ControlJoystick(20))],
                &mut refs
            ),
            Ok(true)
        );
        refs.fail_at = Some(3);
        assert_eq!(controls.close(&mut refs), Err(ControlFailure::Backend(3)));
        assert_eq!(
            refs.events,
            [
                (true, ControlJoystick(10)),
                (true, ControlJoystick(20)),
                (false, ControlJoystick(20))
            ]
        );
        assert_eq!(controls.joysticks().len(), 2);
        assert_eq!(controls.close(&mut refs), Err(ControlFailure::Interrupted));
    }
    #[test]
    fn clear_controls_and_mouse_scale_keep_joystick_refs_and_unwritten_masks() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        assert!(controls.set_mouse_scale(400, -400, 16_777_217));
        assert_eq!(controls.mouse_scale(), [400.0, -400.0, 16_777_216.0]);
        assert_eq!(
            controls.init(
                Some(ControlGraph(8)),
                &[Some(ControlJoystick(9))],
                &mut refs
            ),
            Ok(true)
        );
        controls.seed.keys[0] = 42;
        controls.seed.key_refs[0] = 3;
        controls.seed.mouse_refs[2] = 2;
        controls.controls.push([0x11; 12]);
        controls.values.push(0.25);
        controls.pressed.push(0.5);
        controls.clear_controls();
        assert!(controls.controls.is_empty());
        assert!(controls.values.is_empty());
        assert!(controls.pressed.is_empty());
        assert_eq!(controls.allocation_seed().keys, [0; 64]);
        assert_eq!(controls.allocation_seed().key_refs, [0; 64]);
        assert_eq!(controls.allocation_seed().mouse_refs, [0; 4]);
        assert_eq!(controls.allocation_seed().key_mask, u64::MAX);
        assert_eq!(controls.allocation_seed().mouse_mask, 0x8686_8686);
        assert_eq!(controls.graph(), Some(ControlGraph(8)));
        assert_eq!(controls.joysticks().len(), 1);
        assert_eq!(refs.events, [(true, ControlJoystick(9))]);
    }
    #[test]
    fn shared_key_refs_define_only_on_first_and_last_binding() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        controls
            .init(Some(ControlGraph(3)), &[], &mut refs)
            .unwrap();
        let key = ControlBinding {
            half: 0,
            device: 1,
            code: 42,
            joystick_index: 0,
        };
        assert_eq!(
            controls.set_control(LiveControlBinding::Borrowed(&key), 0, &mut refs),
            Ok(true)
        );
        assert_eq!(
            controls.set_control(LiveControlBinding::Borrowed(&key), 1, &mut refs),
            Ok(true)
        );
        assert_eq!(controls.allocation_seed().key_refs[0], 2);
        assert_eq!(refs.binding_events.len(), 1);
        assert_eq!(
            controls.set_control(LiveControlBinding::Null, 0, &mut refs),
            Ok(true)
        );
        assert_eq!(controls.allocation_seed().key_refs[0], 1);
        assert_eq!(refs.binding_events.len(), 1);
        assert_eq!(
            controls.set_control(LiveControlBinding::Null, 1, &mut refs),
            Ok(true)
        );
        assert_eq!(controls.allocation_seed().keys[0], 0);
        assert_eq!(controls.allocation_seed().key_refs[0], 0);
        assert_eq!(refs.binding_events.len(), 2);
        assert!(
            matches!(refs.binding_events[0], BindingEvent::Define(ControlGraph(3), keys) if keys[0] == 42)
        );
        assert!(
            matches!(refs.binding_events[1], BindingEvent::Define(ControlGraph(3), keys) if keys[0] == 0)
        );
    }
    #[test]
    fn failed_replacement_preserves_old_binding_after_releasing_mouse_ref() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        controls
            .init(Some(ControlGraph(3)), &[], &mut refs)
            .unwrap();
        let mouse = ControlBinding {
            half: 0,
            device: 2,
            code: 1,
            joystick_index: 0,
        };
        assert_eq!(
            controls.set_control(LiveControlBinding::Borrowed(&mouse), 4, &mut refs),
            Ok(true)
        );
        assert_eq!(controls.allocation_seed().mouse_refs[0], 1);
        let missing_joystick = ControlBinding {
            half: 3,
            device: 3,
            code: 2,
            joystick_index: -1,
        };
        assert_eq!(
            controls.set_control(
                LiveControlBinding::Borrowed(&missing_joystick),
                4,
                &mut refs
            ),
            Ok(false)
        );
        assert_eq!(controls.binding(4), Some(mouse));
        assert_eq!(controls.allocation_seed().mouse_refs[0], 0);
        assert!(refs.binding_events.is_empty());
    }
    #[test]
    fn joystick_axis_callback_and_failed_key_define_keep_exact_publication_prefix() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        controls
            .init(
                Some(ControlGraph(3)),
                &[Some(ControlJoystick(7))],
                &mut refs,
            )
            .unwrap();
        let pedal = ControlBinding {
            half: 3,
            device: 3,
            code: 8,
            joystick_index: 0,
        };
        assert_eq!(
            controls.set_control(LiveControlBinding::Borrowed(&pedal), 0, &mut refs),
            Ok(true)
        );
        assert_eq!(
            refs.binding_events,
            [BindingEvent::Axis(ControlJoystick(7), 8, 4)]
        );
        let key = ControlBinding {
            half: 0,
            device: 1,
            code: 37,
            joystick_index: 0,
        };
        refs.fail_binding_at = Some(2);
        assert_eq!(
            controls.set_control(LiveControlBinding::Borrowed(&key), 1, &mut refs),
            Err(ControlFailure::Backend(2))
        );
        assert_eq!(
            controls.binding(1),
            Some(ControlBinding::from_bytes([0; 12]))
        );
        assert_eq!(controls.allocation_seed().keys[0], 37);
        assert_eq!(controls.allocation_seed().key_refs[0], 0);
        assert_eq!(
            controls.set_control(LiveControlBinding::Null, 1, &mut refs),
            Err(ControlFailure::Interrupted)
        );
        assert_eq!(refs.binding_events.len(), 2);
    }

    #[test]
    fn live_self_and_cross_slot_sources_follow_old_reference_effects() {
        let mut controls = GameControls::construct(seed());
        let mut refs = Refs::default();
        controls
            .init(Some(ControlGraph(3)), &[], &mut refs)
            .unwrap();
        let a = ControlBinding {
            half: 0,
            device: 1,
            code: 41,
            joystick_index: 0,
        };
        let b = ControlBinding { code: 42, ..a };
        controls
            .set_control(LiveControlBinding::Borrowed(&a), 0, &mut refs)
            .unwrap();
        controls
            .set_control(LiveControlBinding::Borrowed(&b), 1, &mut refs)
            .unwrap();
        refs.binding_events.clear();

        // The source points at the very destination whose old key loses a
        // reference. Native leaves that record intact until the final copy.
        assert_eq!(
            controls.set_control(LiveControlBinding::Slot(0), 0, &mut refs),
            Ok(true)
        );
        assert_eq!(controls.binding(0), Some(a));
        assert_eq!(controls.allocation_seed().key_refs[0], 1);
        assert_eq!(refs.binding_events.len(), 2);
        assert!(matches!(refs.binding_events[0], BindingEvent::Define(_, keys) if keys[0] == 0));
        assert!(matches!(refs.binding_events[1], BindingEvent::Define(_, keys) if keys[0] == 41));

        // Replacing slot 1 from slot 0 first removes key 42; the later live
        // source read finds key 41 and increments its shared reference.
        refs.binding_events.clear();
        assert_eq!(
            controls.set_control(LiveControlBinding::Slot(0), 1, &mut refs),
            Ok(true)
        );
        assert_eq!(controls.binding(1), Some(a));
        assert_eq!(controls.allocation_seed().key_refs[0], 2);
        assert_eq!(controls.allocation_seed().keys[1], 0);
        assert_eq!(refs.binding_events.len(), 1);
    }

    struct MutatingCallback<'a> {
        external: Option<&'a std::cell::Cell<ControlBinding>>,
        replacement: ControlBinding,
        slot: Option<usize>,
        fail: bool,
        calls: usize,
    }
    impl ControlRefBackend for MutatingCallback<'_> {
        type Error = &'static str;
        fn add_ref(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
        fn release(&mut self, _: ControlJoystick) -> Result<(), Self::Error> {
            Ok(())
        }
    }
    impl ControlBindingBackend for MutatingCallback<'_> {
        fn define_keys(
            &mut self,
            _: ControlGraph,
            owner: &mut GameControls,
        ) -> Result<(), Self::Error> {
            self.calls += 1;
            if let Some(external) = self.external {
                external.set(self.replacement);
            }
            if let Some(slot) = self.slot {
                assert!(owner.replace_binding_slot(slot, self.replacement));
            }
            if self.fail {
                Err("DefineKeys")
            } else {
                Ok(())
            }
        }
        fn set_axis_mode(
            &mut self,
            _: ControlJoystick,
            _: u16,
            _: u32,
            _: &mut GameControls,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn external_binding_is_read_again_after_define_keys_callback() {
        let mut controls = GameControls::construct(seed());
        let original = ControlBinding {
            half: 0,
            device: 1,
            code: 71,
            joystick_index: 0,
        };
        let changed = ControlBinding {
            code: 72,
            ..original
        };
        let external = std::cell::Cell::new(original);
        let mut backend = MutatingCallback {
            external: Some(&external),
            replacement: changed,
            slot: None,
            fail: false,
            calls: 0,
        };
        controls
            .init(Some(ControlGraph(3)), &[], &mut backend)
            .unwrap();
        assert_eq!(
            controls.set_control(
                LiveControlBinding::External(&|| external.get()),
                0,
                &mut backend
            ),
            Ok(true)
        );
        assert_eq!(backend.calls, 1);
        assert_eq!(controls.binding(0), Some(changed));
        // The key byte loaded before the callback remains 71; PC copies the
        // callback-mutated full record later without reconciling the table.
        assert_eq!(controls.keys()[0], 71);
        assert_eq!(controls.allocation_seed().key_refs[0], 1);
    }

    #[test]
    fn old_key_callback_redirects_cross_slot_source_before_its_first_read() {
        let mut controls = GameControls::construct(seed());
        let mut setup = Refs::default();
        controls
            .init(Some(ControlGraph(3)), &[], &mut setup)
            .unwrap();
        let old = ControlBinding {
            half: 0,
            device: 1,
            code: 91,
            joystick_index: 0,
        };
        controls
            .set_control(LiveControlBinding::Borrowed(&old), 0, &mut setup)
            .unwrap();
        controls
            .set_control(LiveControlBinding::Null, 1, &mut setup)
            .unwrap();
        let before_callback = ControlBinding { code: 92, ..old };
        let after_callback = ControlBinding {
            device: 2,
            code: 1,
            ..old
        };
        assert!(controls.replace_binding_slot(1, before_callback));
        let mut backend = MutatingCallback {
            external: None,
            replacement: after_callback,
            slot: Some(1),
            fail: false,
            calls: 0,
        };

        // PC 0x4f03ee calls DefineKeys while releasing the old key. It first
        // reads the new pointer's device at 0x4f0439, after that callback.
        assert_eq!(
            controls.set_control(LiveControlBinding::Slot(1), 0, &mut backend),
            Ok(true)
        );
        assert_eq!(backend.calls, 1);
        assert_eq!(controls.binding(0), Some(after_callback));
        assert_eq!(controls.allocation_seed().mouse_refs[0], 1);
        assert_eq!(controls.keys()[0], 0);
    }

    #[test]
    fn old_key_callback_changes_destination_to_mouse_before_old_mouse_read() {
        let mut controls = GameControls::construct(seed());
        let mut setup = Refs::default();
        controls
            .init(Some(ControlGraph(3)), &[], &mut setup)
            .unwrap();
        let key = ControlBinding {
            half: 0,
            device: 1,
            code: 93,
            joystick_index: 0,
        };
        let mouse = ControlBinding {
            device: 2,
            code: 1,
            ..key
        };
        controls
            .set_control(LiveControlBinding::Borrowed(&key), 0, &mut setup)
            .unwrap();
        controls
            .set_control(LiveControlBinding::Borrowed(&mouse), 1, &mut setup)
            .unwrap();
        assert_eq!(controls.allocation_seed().mouse_refs[0], 1);
        let mut backend = MutatingCallback {
            external: None,
            replacement: mouse,
            slot: Some(0),
            fail: false,
            calls: 0,
        };
        assert_eq!(
            controls.set_control(LiveControlBinding::Null, 0, &mut backend),
            Ok(true)
        );
        assert_eq!(backend.calls, 1);
        assert_eq!(controls.keys()[0], 0);
        assert_eq!(controls.allocation_seed().key_refs[0], 0);
        assert_eq!(controls.allocation_seed().mouse_refs[0], 0);
        assert_eq!(controls.binding(0).unwrap().device, 0);

        let mut failed = GameControls::construct(seed());
        failed.init(Some(ControlGraph(4)), &[], &mut setup).unwrap();
        failed
            .set_control(LiveControlBinding::Borrowed(&key), 0, &mut setup)
            .unwrap();
        failed
            .set_control(LiveControlBinding::Borrowed(&mouse), 1, &mut setup)
            .unwrap();
        let mut failing_backend = MutatingCallback {
            external: None,
            replacement: mouse,
            slot: Some(0),
            fail: true,
            calls: 0,
        };
        assert_eq!(
            failed.set_control(LiveControlBinding::Null, 0, &mut failing_backend),
            Err(ControlFailure::Backend("DefineKeys"))
        );
        // Failure at PC 0x4f03ee stops before the destination reload at
        // 0x4f03f4; callback mutation remains, but mouse_refs is untouched.
        assert_eq!(failed.binding(0), Some(mouse));
        assert_eq!(failed.allocation_seed().mouse_refs[0], 1);
        assert_eq!(failed.keys()[0], 0);
        assert!(failed.interrupted());
    }

    #[test]
    fn cross_slot_callback_mutation_and_failure_keep_reached_prefix() {
        let mut controls = GameControls::construct(seed());
        let original = ControlBinding {
            half: 0,
            device: 1,
            code: 81,
            joystick_index: 0,
        };
        let changed = ControlBinding {
            device: 2,
            code: 1,
            ..original
        };
        let mut backend = MutatingCallback {
            external: None,
            replacement: changed,
            slot: Some(0),
            fail: false,
            calls: 0,
        };
        controls
            .init(Some(ControlGraph(3)), &[], &mut backend)
            .unwrap();
        controls
            .set_control(LiveControlBinding::Null, 0, &mut backend)
            .unwrap();
        assert!(controls.replace_binding_slot(0, original));
        assert_eq!(
            controls.set_control(LiveControlBinding::Slot(0), 1, &mut backend),
            Ok(true)
        );
        assert_eq!(controls.binding(0), Some(changed));
        assert_eq!(controls.binding(1), Some(changed));
        assert_eq!(controls.keys()[0], 81);
        assert_eq!(controls.allocation_seed().mouse_refs[0], 0);

        let mut failed = GameControls::construct(seed());
        let external = std::cell::Cell::new(original);
        let mut failing_backend = MutatingCallback {
            external: Some(&external),
            replacement: changed,
            slot: None,
            fail: true,
            calls: 0,
        };
        failed
            .init(Some(ControlGraph(4)), &[], &mut failing_backend)
            .unwrap();
        assert_eq!(
            failed.set_control(
                LiveControlBinding::External(&|| external.get()),
                0,
                &mut failing_backend
            ),
            Err(ControlFailure::Backend("DefineKeys"))
        );
        assert_eq!(external.get(), changed);
        assert_eq!(failed.keys()[0], 81);
        assert_eq!(failed.allocation_seed().key_refs[0], 0);
        assert_eq!(failed.binding(0).unwrap().device, 0);
        assert!(failed.interrupted());
    }
}
