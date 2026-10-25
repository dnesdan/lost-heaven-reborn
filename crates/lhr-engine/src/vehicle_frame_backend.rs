//! Vehicle frame effects over the same live model/scene arena.
//!
//! This descriptive adapter projects inert PC addresses; it does not clone frame
//! storage or acquire references merely by binding an address. Native heap and
//! gameplay callback effects remain owned by the supplied backend. Frame reads,
//! writes, enumeration, updates and ownership changes never use that backend.
//! Car CRT helpers cover the original initial C locale; locale mutation and its
//! OS conversion paths are outside this adapter's supported domain.
use crate::{
    frame_transform::{FrameKey, FrameKind},
    model_loading::{ModelArena, ModelError, ModelKey},
    native_transform::SQuat,
    vehicle_bodycol::{
        VehicleBodycolBackend, VehicleBodycolEnumeration, VehicleBodycolUnavailable,
    },
    vehicle_init::*,
};

#[path = "vehicle_frame_game.rs"]
mod game;
#[path = "vehicle_frame_game_host.rs"]
mod game_host;
pub use game_host::{VehicleFrameDamageNotification, VehicleFrameGameHost};
#[path = "vehicle_frame_memory.rs"]
mod memory;
pub use memory::{VehicleFrameMemory, VehicleMemoryError};
#[path = "runtime_audio_frame.rs"]
mod audio;
#[path = "car_frame_backend.rs"]
mod car;
#[path = "car_frame_effects.rs"]
mod car_effects;
#[path = "vehicle_frame_crash.rs"]
mod crash;
#[path = "vehicle_frame_deform.rs"]
mod deform;
#[path = "vehicle_frame_glass.rs"]
mod glass;
#[path = "vehicle_frame_lights.rs"]
mod lights;
#[path = "vehicle_frame_vertex_access.rs"]
mod vertex_access;
#[path = "vehicle_frame_wheel_pose.rs"]
mod wheel_pose;
pub use audio::sync_runtime_audio_frames;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VehicleFrameError<E> {
    Host(E),
    Arena(ModelError),
    Address(u32),
    UnsupportedField { frame: FrameKey, offset: u32 },
    UnsupportedEnumeration(u32),
    UnsupportedName(FrameKey),
    NumericDomain(&'static str),
    BodycolAdapterRequired,
    UnavailableFrameMethod(&'static str),
    UnavailableGameDependency(u32),
    UnavailableGameEffect(String),
    CarTrack(crate::car_effects::CarTrackError),
    UnsupportedCarLocale(u32),
    Audio(String),
}
impl<E> From<ModelError> for VehicleFrameError<E> {
    fn from(error: ModelError) -> Self {
        Self::Arena(error)
    }
}

// Disjoint virtual regions. Each stable arena key receives one frame slot and
// live property slots. Names use arena-wide allocation generations, so old
// pointers cannot alias a renamed frame. These are host identities, not original heaps.
const FRAME_BASE: u32 = 0x2000_0000;
const FRAME_END: u32 = 0x3000_0000;
const NAME_BASE: u32 = 0x4000_0000;
const NAME_END: u32 = 0xc000_0000;
const SLOT: u32 = 0x1000;
const NAME_SLOT: u32 = 0x8000;
const PROPERTY_BASE: u32 = 0x3000_0000;
const PROPERTY_END: u32 = 0x4000_0000;
const PROPERTY_SLOT: u32 = 0x2000;
const RESOURCE_BASE: u32 = 0xc000_0000;
const RESOURCE_END: u32 = 0xf000_0000;
// Filename allocation slots are separate from frame/name/resource identities.
// The native reference word precedes the C string by four bytes. Arena slots
// are never reused, so a released filename pointer cannot alias a later Open.
const FILENAME_BASE: u32 = RESOURCE_END;
const FILENAME_END: u32 = 0xff00_0000;
const FILENAME_SLOT: u32 = 0x10000;
const NATIVE_LITERALS: &[(u32, &[u8])] = &[
    (0x6322f0, b"TANK\0"),
    (0x6322f8, b"FUEL\0"),
    (0x632300, b"POLIMTUD\0"),
    (0x63230c, b"POLIMFOR\0"),
    (0x632318, b"POLI\0"),
    (0x632320, b"POLCAD\0"),
    (0x632328, b"LG%d*\0"),
    (0x632330, b"SM%d*\0"),
    (0x632338, b"AI01\0"),
    (0x632340, b"AI00\0"),
    (0x634944, b"2ft19r.bmp\0"),
    (0x634950, b"2ft19r+.bmp\0"),
];
fn native_literal_byte(address: u32) -> Option<u8> {
    NATIVE_LITERALS.iter().find_map(|(base, bytes)| {
        address
            .checked_sub(*base)
            .and_then(|offset| bytes.get(offset as usize).copied())
    })
}

pub struct VehicleFrameBackend<'a, B: VehicleInitBackend> {
    arena: &'a mut ModelArena,
    host: &'a mut B,
    installation: Option<&'a lhr_assets::Installation>,
    car_tracks: Option<&'a mut crate::car_effects::CarTrackResources>,
    game_init_stack: Option<u32>,
    game_host: Option<&'a mut dyn VehicleFrameGameHost<B::Error>>,
}
impl<'a, B: VehicleInitBackend> VehicleFrameBackend<'a, B> {
    pub fn new(arena: &'a mut ModelArena, host: &'a mut B) -> Self {
        Self {
            arena,
            host,
            installation: None,
            car_tracks: None,
            game_init_stack: None,
            game_host: None,
        }
    }
    pub fn with_installation(
        arena: &'a mut ModelArena,
        host: &'a mut B,
        installation: &'a lhr_assets::Installation,
    ) -> Self {
        Self {
            arena,
            host,
            installation: Some(installation),
            car_tracks: None,
            game_init_stack: None,
            game_host: None,
        }
    }
    /// Borrows the Game-owned animation-track cache used by car components.
    /// The model arena and this temporary adapter acquire no ownership of it.
    pub fn with_car_track_resources(
        mut self,
        resources: &'a mut crate::car_effects::CarTrackResources,
    ) -> Self {
        self.car_tracks = Some(resources);
        self
    }
    /// Native ESP after the outer C_car::GameInit prologue. It identifies
    /// already supplied caller storage, never an adapter-created stack.
    pub fn with_game_init_stack(mut self, stack: u32) -> Self {
        self.game_init_stack = Some(stack);
        self
    }
    pub fn with_game_host(mut self, host: &'a mut dyn VehicleFrameGameHost<B::Error>) -> Self {
        self.game_host = Some(host);
        self
    }
    pub fn arena(&self) -> &ModelArena {
        self.arena
    }
    pub fn arena_mut(&mut self) -> &mut ModelArena {
        self.arena
    }
    pub fn host(&self) -> &B {
        self.host
    }
    pub fn host_mut(&mut self) -> &mut B {
        self.host
    }
    pub fn frame_address(&self, key: FrameKey) -> Result<u32, VehicleFrameError<B::Error>> {
        self.arena.frame(key)?;
        if key.0 == 0 || key.0 >= (FRAME_END - FRAME_BASE) / SLOT {
            return Err(VehicleFrameError::Address(key.0));
        }
        key.0
            .checked_mul(SLOT)
            .and_then(|offset| FRAME_BASE.checked_add(offset))
            .filter(|address| *address < FRAME_END)
            .ok_or(VehicleFrameError::Address(key.0))
    }
    pub fn frame_key(&self, address: u32) -> Result<FrameKey, VehicleFrameError<B::Error>> {
        if !(FRAME_BASE..FRAME_END).contains(&address)
            || !(address - FRAME_BASE).is_multiple_of(SLOT)
        {
            return Err(VehicleFrameError::Address(address));
        }
        let key = FrameKey((address - FRAME_BASE) / SLOT);
        self.frame_address(key)?;
        Ok(key)
    }
    fn pointer(&self, key: Option<FrameKey>) -> Result<u32, VehicleFrameError<B::Error>> {
        key.map(|key| self.frame_address(key))
            .transpose()
            .map(|value| value.unwrap_or(0))
    }
    fn field(address: u32) -> Option<(FrameKey, u32)> {
        (FRAME_BASE..FRAME_END).contains(&address).then(|| {
            (
                FrameKey((address - FRAME_BASE) / SLOT),
                (address - FRAME_BASE) % SLOT,
            )
        })
    }
    fn name_address(&self, key: FrameKey) -> Result<u32, VehicleFrameError<B::Error>> {
        let generation = self.arena.frame(key)?.name_generation;
        u32::try_from(generation)
            .ok()
            .and_then(|generation| generation.checked_mul(NAME_SLOT))
            .and_then(|offset| NAME_BASE.checked_add(offset))
            .filter(|address| *address < NAME_END)
            .ok_or(VehicleFrameError::Address(key.0))
    }
    fn field_word(&self, key: FrameKey, offset: u32) -> Result<u32, VehicleFrameError<B::Error>> {
        self.frame_address(key)?;
        let frame = self.arena.frame(key)?;
        let state = self.arena.state(key)?;
        Ok(match offset {
            0x210 if state.kind == FrameKind::Scene => self.frame_address(
                self.arena
                    .scene(crate::scene_ownership::SceneKey(key))?
                    .sectors[0]
                    .frame(),
            )?,
            0x140..=0x1e8 if state.kind == FrameKind::Sound && offset.is_multiple_of(4) => {
                return self.game_audio_read_word(key, offset);
            }
            4 => frame.references,
            8 => match frame.user_data {
                crate::actor_frames::FrameUserData::Null => 0,
                crate::actor_frames::FrameUserData::ExternalRecord(address) => address,
                _ => return Err(VehicleFrameError::UnsupportedField { frame: key, offset }),
            },
            0xc => {
                if frame.properties.is_empty() {
                    0
                } else {
                    key.0
                        .checked_mul(PROPERTY_SLOT)
                        .and_then(|offset| PROPERTY_BASE.checked_add(offset))
                        .filter(|address| *address < PROPERTY_END)
                        .ok_or(VehicleFrameError::Address(key.0))?
                }
            }
            0x10..=0x4c if offset.is_multiple_of(4) => {
                state.world.elements[((offset - 0x10) / 4) as usize].to_bits()
            }
            0x50..=0x8c if offset.is_multiple_of(4) => {
                state.local.elements[((offset - 0x50) / 4) as usize].to_bits()
            }
            0x90..=0x98 if offset.is_multiple_of(4) => {
                state.scale[((offset - 0x90) / 4) as usize].to_bits()
            }
            0x9c..=0xa8 if offset.is_multiple_of(4) => {
                state.rotation.wxyz[((offset - 0x9c) / 4) as usize].to_bits()
            }
            0xac => state.flags,
            0xb0..=0xd4 if offset.is_multiple_of(4) => bounds_word(
                self.arena.hierarchy_bounds_snapshot(key)?,
                (offset - 0xb0) / 4,
            ),
            0xd8..=0xfc if offset.is_multiple_of(4) => bounds_word(
                self.arena.hierarchy_world_bounds_snapshot(key)?,
                (offset - 0xd8) / 4,
            ),
            0x100 => {
                if frame.name_is_null {
                    0
                } else {
                    self.name_address(key)?
                }
            }
            0x104 => {
                if frame.original_name_bytes.is_some() {
                    let offset = match frame.name_pointer {
                        crate::frame_fast_lookup::OriginalFrameName::Offset(offset) => offset,
                        _ => {
                            return Err(VehicleFrameError::UnsupportedField { frame: key, offset });
                        }
                    };
                    self.name_address(key)?
                        .checked_add(
                            u32::try_from(offset).map_err(|_| VehicleFrameError::Address(key.0))?,
                        )
                        .ok_or(VehicleFrameError::Address(key.0))?
                } else {
                    0
                }
            }
            0x110 => match state.kind {
                FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector => 1,
                FrameKind::Light => 2,
                FrameKind::Camera => 3,
                FrameKind::Sound => 4,
                FrameKind::Sector => 5,
                FrameKind::Dummy => 6,
                FrameKind::Model => 9,
                FrameKind::Joint => 10,
                FrameKind::Target => 7,
                FrameKind::Occluder => 12,
                FrameKind::Scene => {
                    return Err(VehicleFrameError::UnsupportedField { frame: key, offset });
                }
            },
            // Immutable visual-family tags established by the original
            // object, singlemesh and projector constructors.
            0x1d4 if state.kind == FrameKind::Mesh => 0x5f4a424f,
            0x1d4 if state.kind == FrameKind::Singlemesh => 0x4d474e53,
            0x1d4 if state.kind == FrameKind::Projector => 0x4a4f5250,
            0x10c => self.pointer(frame.owner.map(|owner| owner.root()))?,
            0x114 => state.world_scale.to_bits(),
            0x118 => state.update_count,
            0x11c => self.arena.links(key)?.child_count,
            0x120 => self.pointer(self.arena.links(key)?.parent)?,
            0x124 => self.pointer(self.arena.links(key)?.child)?,
            0x128 => self.pointer(self.arena.links(key)?.previous)?,
            0x12c => self.pointer(self.arena.links(key)?.next)?,
            0x13c => state.render_number,
            0x154 if state.kind == FrameKind::Model => {
                match self.arena.model_filename(ModelKey(key))? {
                    Some(filename) => self.filename_address(filename)?,
                    None => 0,
                }
            }
            0x140..=0x154 if offset.is_multiple_of(4) => {
                let bounds = frame
                    .dummy_bounds
                    .ok_or(VehicleFrameError::UnsupportedField { frame: key, offset })?;
                let i = ((offset - 0x140) / 4) as usize;
                bounds[i / 3][i % 3].to_bits()
            }
            _ => return Err(VehicleFrameError::UnsupportedField { frame: key, offset }),
        })
    }
    fn store_field(
        &mut self,
        key: FrameKey,
        offset: u32,
        value: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        self.arena.frame(key)?;
        if self.arena.state(key)?.kind == FrameKind::Sound && (0x140..=0x1e8).contains(&offset) {
            return self.game_audio_write_word(key, offset, value);
        }
        if offset == 8 {
            self.arena.set_user_data(
                key,
                if value == 0 {
                    crate::actor_frames::FrameUserData::Null
                } else {
                    crate::actor_frames::FrameUserData::ExternalRecord(value)
                },
            )?;
            return Ok(());
        }
        if offset == 4 {
            let old = self.arena.frame(key)?.references;
            if value == old.wrapping_add(1) {
                self.arena.retain(key)?;
                return Ok(());
            }
            return Err(VehicleFrameError::UnsupportedField { frame: key, offset });
        }
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        match offset {
            0x10..=0x4c if offset.is_multiple_of(4) => {
                state.world.elements[((offset - 0x10) / 4) as usize] = f32::from_bits(value)
            }
            0x50..=0x8c if offset.is_multiple_of(4) => {
                state.local.elements[((offset - 0x50) / 4) as usize] = f32::from_bits(value)
            }
            0x90..=0x98 if offset.is_multiple_of(4) => {
                state.scale[((offset - 0x90) / 4) as usize] = f32::from_bits(value)
            }
            0x9c..=0xa8 if offset.is_multiple_of(4) => {
                state.rotation.wxyz[((offset - 0x9c) / 4) as usize] = f32::from_bits(value)
            }
            0xac => state.flags = value,
            _ => return Err(VehicleFrameError::UnsupportedField { frame: key, offset }),
        }
        Ok(())
    }
    fn live_name_byte(&self, address: u32) -> Result<u8, VehicleFrameError<B::Error>> {
        let relative = address - NAME_BASE;
        let generation = u64::from(relative / NAME_SLOT);
        let offset = (relative % NAME_SLOT) as usize;
        let key = self
            .arena
            .frames
            .iter()
            .find(|frame| frame.name_generation == generation)
            .map(|frame| frame.key)
            .ok_or(VehicleFrameError::Address(address))?;
        let frame = self.arena.frame(key)?;
        if frame.name_is_null {
            return Err(VehicleFrameError::Address(address));
        }
        let name = &frame.name_bytes;
        if name.len() >= NAME_SLOT as usize {
            return Err(VehicleFrameError::UnsupportedName(key));
        }
        if offset == name.len() {
            Ok(0)
        } else {
            name.get(offset)
                .copied()
                .ok_or(VehicleFrameError::Address(address))
        }
    }
    fn filename_address(
        &self,
        key: crate::model_loading::ModelFileNameKey,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let filename = self.arena.filename(key)?;
        if filename.bytes().len() >= (FILENAME_SLOT - 4) as usize {
            return Err(VehicleFrameError::Address(FILENAME_BASE));
        }
        u32::try_from(key.projection_slot())
            .ok()
            .and_then(|slot| slot.checked_mul(FILENAME_SLOT))
            .and_then(|offset| FILENAME_BASE.checked_add(offset))
            .filter(|base| *base < FILENAME_END)
            .and_then(|base| base.checked_add(4))
            .ok_or(VehicleFrameError::Address(FILENAME_BASE))
    }
    fn live_filename_byte(&self, address: u32) -> Result<u8, VehicleFrameError<B::Error>> {
        let relative = address - FILENAME_BASE;
        let key = self
            .arena
            .projected_filename_key((relative / FILENAME_SLOT) as usize)?;
        self.filename_address(key)?;
        let offset = relative % FILENAME_SLOT;
        let filename = self.arena.filename(key)?;
        if offset < 4 {
            return Ok(filename.references().to_le_bytes()[offset as usize]);
        }
        let index = (offset - 4) as usize;
        if index == filename.bytes().len() {
            Ok(0)
        } else {
            filename
                .bytes()
                .get(index)
                .copied()
                .ok_or(VehicleFrameError::Address(address))
        }
    }
    /// Base EnumFrames/EnumSubtree over live child links. The callback may
    /// mutate this arena; the next sibling is captured before calling it.
    /// Returns 2 on native callback cancellation, 0 after normal traversal.
    pub fn enumerate_frames<E>(
        &mut self,
        root: u32,
        pattern: &[u8],
        mask: u32,
        mut callback: impl FnMut(&mut Self, u32) -> Result<u32, E>,
    ) -> Result<u32, E>
    where
        E: From<VehicleFrameError<B::Error>>,
    {
        self.enumerate_frames_at_depth(root, pattern, mask, |backend, frame, _depth| {
            callback(backend, frame)
        })
    }
    fn enumerate_frames_at_depth<E>(
        &mut self,
        root: u32,
        pattern: &[u8],
        mask: u32,
        callback: impl FnMut(&mut Self, u32, u32) -> Result<u32, E>,
    ) -> Result<u32, E>
    where
        E: From<VehicleFrameError<B::Error>>,
    {
        self.enumerate_frames_with_optional_pattern(root, Some(pattern), mask, callback)
    }
    fn enumerate_frames_with_optional_pattern<E>(
        &mut self,
        root: u32,
        pattern: Option<&[u8]>,
        mask: u32,
        mut callback: impl FnMut(&mut Self, u32, u32) -> Result<u32, E>,
    ) -> Result<u32, E>
    where
        E: From<VehicleFrameError<B::Error>>,
    {
        let key = self.frame_key(root).map_err(E::from)?;
        if self
            .arena
            .state(key)
            .map_err(VehicleFrameError::from)
            .map_err(E::from)?
            .kind
            == FrameKind::Scene
            || mask & !0x3ffff != 0
        {
            return Err(E::from(VehicleFrameError::UnsupportedEnumeration(mask)));
        }
        if pattern.is_some_and(|pattern| !pattern.is_ascii() || pattern.contains(&0)) {
            return Err(E::from(VehicleFrameError::UnsupportedName(key)));
        }
        let mut next = self
            .arena
            .links(key)
            .map_err(VehicleFrameError::from)
            .map_err(E::from)?
            .child;
        if next.is_some() {
            self.arena.last_destroyed_frame = None;
            next = self
                .arena
                .links(key)
                .map_err(VehicleFrameError::from)
                .map_err(E::from)?
                .child;
        }
        let mut pending = Vec::new();
        let mut depth = 0u32;
        while let Some(current) = next {
            let links = self
                .arena
                .links(current)
                .map_err(VehicleFrameError::from)
                .map_err(E::from)?;
            let state = self
                .arena
                .state(current)
                .map_err(VehicleFrameError::from)
                .map_err(E::from)?;
            let kind_mask = match state.kind {
                FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector => 1,
                FrameKind::Light => 2,
                FrameKind::Camera => 4,
                FrameKind::Sound => 8,
                FrameKind::Sector => 0x10,
                FrameKind::Dummy => 0x20,
                FrameKind::Target => 0x40,
                FrameKind::Model => 0x80,
                FrameKind::Joint => 0x400,
                // Actual EnumSubtree RVA0x1c020 skips kinds12/13 for
                // every partial mask; the all-types sentinel below bypasses it.
                FrameKind::Occluder | FrameKind::Scene => 0,
            };
            let mut result = 0;
            if mask & 0xffff == 0xffff || mask & kind_mask != 0 {
                let matches = match pattern {
                    None => true,
                    Some(pattern) => {
                        match self
                            .arena
                            .frame(current)
                            .map_err(VehicleFrameError::from)
                            .map_err(E::from)?
                            .original_name_bytes
                            .as_deref()
                        {
                            Some(name) => {
                                if !name.is_ascii() || name.contains(&0) {
                                    return Err(E::from(VehicleFrameError::UnsupportedName(
                                        current,
                                    )));
                                }
                                name_matches(pattern, name, mask)
                            }
                            None => false,
                        }
                    }
                };
                if matches {
                    let address = self.frame_address(current).map_err(E::from)?;
                    result = callback(self, address, depth)?;
                    if result == 2 {
                        return Ok(2);
                    }
                }
            }
            if let Some(sibling) = links.next {
                pending.push((sibling, depth));
            }
            // The original shared lastDestroyedFrame sentinel suppresses the
            // destroyed callback frame. Reentrant enumeration can reset it;
            // a then-stale child access fails explicitly instead of succeeding.
            // A live callback-mutated child pointer is read after the callback.
            next = if result != 1 && self.arena.last_destroyed_frame != Some(current) {
                self.arena
                    .links(current)
                    .map_err(VehicleFrameError::from)
                    .map_err(E::from)?
                    .child
            } else {
                None
            };
            if next.is_some() {
                depth = depth.wrapping_add(1);
            } else if let Some((sibling, sibling_depth)) = pending.pop() {
                next = Some(sibling);
                depth = sibling_depth;
            }
        }
        Ok(0)
    }
}
fn bounds_word(bounds: crate::mesh_bounds::MeshBoundVolume, index: u32) -> u32 {
    match index {
        0..=5 => bounds.bbox[index as usize / 3][index as usize % 3].to_bits(),
        6..=8 => bounds.center[index as usize - 6].to_bits(),
        9 => bounds.radius.to_bits(),
        _ => unreachable!(),
    }
}
fn name_matches(pattern: &[u8], name: &[u8], mask: u32) -> bool {
    if mask & 0x10000 == 0 {
        return if mask & 0x20000 != 0 {
            pattern == name
        } else {
            pattern.eq_ignore_ascii_case(name)
        };
    }
    for index in 0..=pattern.len().min(name.len()) {
        let a = pattern.get(index).copied().unwrap_or(0);
        let b = name.get(index).copied().unwrap_or(0);
        if a == b'*' {
            return true;
        }
        if a != b'?'
            && if mask & 0x20000 != 0 {
                a != b
            } else {
                !a.eq_ignore_ascii_case(&b)
            }
        {
            return false;
        }
    }
    true
}
impl<E> From<VehicleFrameError<E>> for VehicleInitUnavailable<VehicleFrameError<E>> {
    fn from(error: VehicleFrameError<E>) -> Self {
        Self::Effect(error)
    }
}
impl<E> From<VehicleFrameError<E>> for VehicleBodycolUnavailable<VehicleFrameError<E>> {
    fn from(error: VehicleFrameError<E>) -> Self {
        Self::Effect(error)
    }
}
impl<B: VehicleInitBackend> VehicleInitBackend for VehicleFrameBackend<'_, B> {
    type Error = VehicleFrameError<B::Error>;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        if (FILENAME_BASE..FILENAME_END).contains(&address) {
            return self.live_filename_byte(address);
        }
        if (RESOURCE_BASE..RESOURCE_END).contains(&address) {
            if let Some(value) = self.game_resource_byte(address)? {
                return Ok(value);
            }
            return Err(VehicleFrameError::Address(address));
        }
        if let Some((key, offset)) = Self::field(address) {
            return Ok(self.field_word(key, offset & !3)?.to_le_bytes()[(offset & 3) as usize]);
        }
        if (NAME_BASE..NAME_END).contains(&address) {
            return self.live_name_byte(address);
        }
        if (PROPERTY_BASE..PROPERTY_END).contains(&address) {
            let relative = address - PROPERTY_BASE;
            let key = FrameKey(relative / PROPERTY_SLOT);
            let offset = (relative % PROPERTY_SLOT) as usize;
            let properties = &self.arena.frame(key)?.properties;
            if properties.len() >= PROPERTY_SLOT as usize {
                return Err(VehicleFrameError::UnsupportedField {
                    frame: key,
                    offset: 0xc,
                });
            }
            return if offset == properties.len() {
                Ok(0)
            } else {
                properties
                    .get(offset)
                    .copied()
                    .ok_or(VehicleFrameError::Address(address))
            };
        }
        if let Some(byte) = native_literal_byte(address) {
            return Ok(byte);
        }
        if address == 0x63d2a0 {
            return Ok(0);
        }
        self.host
            .read_byte(address)
            .map_err(VehicleFrameError::Host)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        if (RESOURCE_BASE..RESOURCE_END).contains(&address) {
            let mut bytes = [0; 4];
            for (i, byte) in bytes.iter_mut().enumerate() {
                *byte = self.read_byte(address.wrapping_add(i as u32))?;
            }
            return Ok(u32::from_le_bytes(bytes));
        }
        if let Some((key, offset)) = Self::field(address) {
            if offset.is_multiple_of(4) {
                return self.field_word(key, offset);
            }
        }
        if Self::field(address).is_some()
            || (PROPERTY_BASE..NAME_END).contains(&address)
            || (FILENAME_BASE..FILENAME_END).contains(&address)
            || (0..4).any(|offset| native_literal_byte(address.wrapping_add(offset)).is_some())
        {
            let mut bytes = [0; 4];
            for (i, v) in bytes.iter_mut().enumerate() {
                *v = self.read_byte(address.wrapping_add(i as u32))?;
            }
            return Ok(u32::from_le_bytes(bytes));
        }
        self.host
            .read_word(address)
            .map_err(VehicleFrameError::Host)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        if self.game_write_resource_byte(address, value)? {
            return Ok(());
        }
        if (RESOURCE_BASE..FILENAME_END).contains(&address)
            || native_literal_byte(address).is_some()
        {
            return Err(VehicleFrameError::Address(address));
        }
        if let Some((key, offset)) = Self::field(address) {
            let mut bytes = self.field_word(key, offset & !3)?.to_le_bytes();
            bytes[(offset & 3) as usize] = value;
            return self.store_field(key, offset & !3, u32::from_le_bytes(bytes));
        }
        if (PROPERTY_BASE..NAME_END).contains(&address) {
            return Err(VehicleFrameError::Address(address));
        }
        self.host
            .write_byte(address, value)
            .map_err(VehicleFrameError::Host)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        if (0xc500_0000..0xf000_0000).contains(&address) {
            for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.write_byte(address.wrapping_add(i as u32), byte)?;
            }
            return Ok(());
        }
        if (RESOURCE_BASE..FILENAME_END).contains(&address)
            || (0..4).any(|offset| native_literal_byte(address.wrapping_add(offset)).is_some())
        {
            return Err(VehicleFrameError::Address(address));
        }
        if let Some((key, offset)) = Self::field(address) {
            if offset.is_multiple_of(4) {
                return self.store_field(key, offset, value);
            }
        }
        if Self::field(address).is_some() || (PROPERTY_BASE..NAME_END).contains(&address) {
            for (i, v) in value.to_le_bytes().into_iter().enumerate() {
                self.write_byte(address.wrapping_add(i as u32), v)?;
            }
            return Ok(());
        }
        self.host
            .write_word(address, value)
            .map_err(VehicleFrameError::Host)
    }
    fn free(&mut self, address: u32) -> Result<(), Self::Error> {
        if (RESOURCE_BASE..FILENAME_END).contains(&address)
            || native_literal_byte(address).is_some()
        {
            return Err(VehicleFrameError::Address(address));
        }
        if Self::field(address).is_some() || (PROPERTY_BASE..NAME_END).contains(&address) {
            return Err(VehicleFrameError::Address(address));
        }
        self.host.free(address).map_err(VehicleFrameError::Host)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        let address = self.host.allocate(bytes).map_err(VehicleFrameError::Host)?;
        if address != 0 {
            let end = address
                .checked_add(bytes.max(1))
                .ok_or(VehicleFrameError::Address(address))?;
            if [
                (FRAME_BASE, FRAME_END),
                (PROPERTY_BASE, NAME_END),
                (RESOURCE_BASE, FILENAME_END),
            ]
            .into_iter()
            .any(|(a, b)| address < b && a < end)
            {
                return Err(VehicleFrameError::Address(address));
            }
        }
        Ok(address)
    }
    fn allocate_projected(
        &mut self,
        memory: &mut VehicleComponentTailMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error> {
        let address = self
            .host
            .allocate_projected(memory, bytes)
            .map_err(VehicleFrameError::Host)?;
        if address != 0 {
            let end = address
                .checked_add(bytes.max(1))
                .ok_or(VehicleFrameError::Address(address))?;
            if [
                (FRAME_BASE, FRAME_END),
                (PROPERTY_BASE, NAME_END),
                (RESOURCE_BASE, FILENAME_END),
            ]
            .into_iter()
            .any(|(a, b)| address < b && a < end)
            {
                return Err(VehicleFrameError::Address(address));
            }
        }
        Ok(address)
    }
    fn free_projected(
        &mut self,
        memory: &mut VehicleComponentTailMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error> {
        if Self::field(allocation).is_some() || (PROPERTY_BASE..FILENAME_END).contains(&allocation)
        {
            return Err(VehicleFrameError::Address(allocation));
        }
        self.host
            .free_projected(memory, allocation)
            .map_err(VehicleFrameError::Host)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.frame_key(frame)?;
        // Original collecting callback 0x4d0eb0 appends one raw pointer.
        // It owns no frame reference. The vector header is the caller's live
        // Rust projection, so completed capacity/begin/end stores survive an
        // unavailable later access just as in the raw container routines.
        let end = vector.end;
        let available = (vector.capacity.wrapping_sub(end) as i32 >> 2) as u32;
        if available < 1 {
            let size = if vector.begin == 0 {
                0
            } else {
                (end.wrapping_sub(vector.begin) as i32 >> 2) as u32
            };
            let capacity = size.wrapping_add(size.max(1));
            let bytes = if (capacity as i32) < 0 {
                0
            } else {
                capacity.wrapping_mul(4)
            };
            let allocation = self.allocate(bytes)?;
            let mut source = vector.begin;
            let mut destination = allocation;
            while source != end {
                if destination != 0 {
                    let word = self.read_word(source)?;
                    self.write_word(destination, word)?;
                }
                source = source.wrapping_add(4);
                destination = destination.wrapping_add(4);
            }
            if destination != 0 {
                self.write_word(destination, frame)?;
            }
            self.free(vector.begin)?;
            let previous_begin = vector.begin;
            vector.capacity = allocation.wrapping_add(capacity.wrapping_mul(4));
            let size = if previous_begin == 0 {
                0
            } else {
                (vector.end.wrapping_sub(previous_begin) as i32 >> 2) as u32
            };
            vector.begin = allocation;
            vector.end = allocation
                .wrapping_add(size.wrapping_mul(4))
                .wrapping_add(4);
        } else {
            // With insertion=end the native suffix copy/assignment is empty.
            // Placement construction skips a null destination only.
            if end != 0 {
                self.write_word(end, frame)?;
            }
            vector.end = vector.end.wrapping_add(4);
        }
        Ok(())
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, query.pattern(), query.mask(), |backend, frame| {
            target.visit(frame, backend)
        })
        .map(|_| ())
    }
}
impl<B: VehicleInitBackend> VehicleBodycolBackend for VehicleFrameBackend<'_, B> {
    fn bodycol_update_world(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena
            .graph
            .update_w_matrix_proc(key)
            .map_err(ModelError::from)?;
        Ok(())
    }
    fn bodycol_link_frame(
        &mut self,
        frame: u32,
        parent: u32,
        flags: u32,
    ) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let parent = if parent == 0 {
            None
        } else {
            Some(self.frame_key(parent)?)
        };
        self.arena.link_frame(key, parent, flags)?;
        Ok(())
    }
    fn bodycol_remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error> {
        let model = ModelKey(self.frame_key(model)?);
        let key = self.frame_key(frame)?;
        self.arena.delete_frame(model, key)?;
        Ok(())
    }
    fn bodycol_enumerate(
        &mut self,
        frame: u32,
        pattern: &[u8],
        mask: u32,
        target: &mut VehicleBodycolEnumeration<'_>,
    ) -> Result<(), VehicleBodycolUnavailable<Self::Error>> {
        self.enumerate_frames(frame, pattern, mask, |backend, frame| {
            target.visit(frame, backend)
        })
        .map(|_| ())
    }
}
impl<B: VehicleComponentsBackend> VehicleComponentsBackend for VehicleFrameBackend<'_, B> {
    fn prepare_frame_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena
            .graph
            .update_matrix(key)
            .map_err(ModelError::from)?;
        Ok(())
    }
    fn normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        if !state.rotation.is_verified_input() {
            return Err(VehicleFrameError::NumericDomain("Normalize"));
        }
        state.rotation.normalize();
        Ok(())
    }
    fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        state
            .local
            .try_set_rot3(state.rotation)
            .map_err(|_| VehicleFrameError::NumericDomain("SetRot3"))
    }
    fn insert_exhaust(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        value: [u32; 6],
    ) -> Result<(), Self::Error> {
        self.host
            .insert_exhaust(vehicle, insertion, value)
            .map_err(VehicleFrameError::Host)
    }
    fn insert_special_point(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        point: [u32; 3],
    ) -> Result<(), Self::Error> {
        self.host
            .insert_special_point(vehicle, insertion, point)
            .map_err(VehicleFrameError::Host)
    }
    fn insert_special_control(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        kind: u32,
        enabled: u8,
    ) -> Result<(), Self::Error> {
        self.host
            .insert_special_control(vehicle, insertion, kind, enabled)
            .map_err(VehicleFrameError::Host)
    }
    fn special_collision_callback(
        &mut self,
        function: u32,
        context: u32,
        frame: u32,
        control_pointer: u32,
        index: u32,
    ) -> Result<(), Self::Error> {
        self.frame_key(frame)?;
        self.host
            .special_collision_callback(function, context, frame, control_pointer, index)
            .map_err(VehicleFrameError::Host)
    }
    fn link_frame(&mut self, frame: u32, parent: u32, flags: u32) -> Result<(), Self::Error> {
        self.bodycol_link_frame(frame, parent, flags)
    }
    fn remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error> {
        self.bodycol_remove_model_child(model, frame)
    }
    fn enumerate_components(
        &mut self,
        model: u32,
        query: VehicleComponentsQuery,
        target: &mut VehicleComponentsEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, query.pattern(), query.mask(), |backend, frame| {
            target.visit(frame, backend)
        })
        .map(|_| ())
    }
    fn body_collision_callback(
        &mut self,
        _: &mut [u8],
        _: VehicleBodyCollisionContext,
        _: u32,
    ) -> Result<u32, Self::Error> {
        Err(VehicleFrameError::BodycolAdapterRequired)
    }
    fn finish_body_collision_callbacks(
        &mut self,
        _: &mut [u8],
        _: VehicleBodyCollisionContext,
    ) -> Result<(), Self::Error> {
        Err(VehicleFrameError::BodycolAdapterRequired)
    }
    fn update_model_world_matrix(&mut self, model: u32) -> Result<(), Self::Error> {
        self.bodycol_update_world(model)
    }
    fn initialize_body_collision(
        &mut self,
        _: &mut [u8],
        _: u32,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(VehicleFrameError::BodycolAdapterRequired)
    }
    fn parse_component_number(&mut self, address: u32) -> Result<i32, Self::Error> {
        let mut address = address;
        let mut byte = self.read_byte(address)?;
        while matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
            address = address.wrapping_add(1);
            byte = self.read_byte(address)?;
        }
        let negative = byte == b'-';
        if matches!(byte, b'-' | b'+') {
            address = address.wrapping_add(1);
            byte = self.read_byte(address)?;
        }
        let mut value = 0u32;
        while byte.is_ascii_digit() {
            value = value.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'));
            address = address.wrapping_add(1);
            byte = self.read_byte(address)?;
        }
        Ok(if negative {
            0u32.wrapping_sub(value)
        } else {
            value
        } as i32)
    }
}
impl<B: VehicleComponentTailBackend> VehicleComponentTailBackend for VehicleFrameBackend<'_, B> {
    fn materialize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        state.rotation = SQuat::try_make_from_matrix(&state.local)
            .map_err(|_| VehicleFrameError::NumericDomain("QuaternionFromMatrix"))?;
        Ok(())
    }
    fn find_window(&mut self, door: u32) -> Result<u32, Self::Error> {
        let mut found = 0;
        self.enumerate_frames(door, b"WINDOW*", 0x10001, |_, frame| {
            found = frame;
            Ok::<_, Self::Error>(2)
        })?;
        Ok(found)
    }
    fn enumerate_tail(
        &mut self,
        root: u32,
        query: VehicleComponentTailQuery,
        target: &mut VehicleComponentTailEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(root, query.pattern(), query.mask(), |backend, frame| {
            target.visit(frame, backend)
        })
        .map(|_| ())
    }
    fn seat_callback(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.frame_key(frame)?;
        self.host
            .seat_callback(memory, function, context, frame)
            .map_err(VehicleFrameError::Host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        frame_transform::I3dFrameState,
        native_transform::SMatrix,
        vehicle_bodycol::{
            bodycol_callback, bodycol_descendant_pass, bodycol_transform_detach,
            VehicleBodycolScratch,
        },
    };
    use std::collections::BTreeMap;
    struct Memory {
        bytes: BTreeMap<u32, u8>,
        next: u32,
        freed: Vec<u32>,
    }
    impl Memory {
        fn new() -> Self {
            Self {
                bytes: (0x1000..0x4000).map(|a| (a, 0)).collect(),
                next: 0x10000,
                freed: Vec::new(),
            }
        }
    }
    impl VehicleInitBackend for Memory {
        type Error = &'static str;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.bytes.get(&a).copied().ok_or("unmapped")
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            let mut out = [0; 4];
            for (i, v) in out.iter_mut().enumerate() {
                *v = self.read_byte(a + i as u32)?;
            }
            Ok(u32::from_le_bytes(out))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            *self.bytes.get_mut(&a).ok_or("unmapped")? = v;
            Ok(())
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            for (i, v) in v.to_le_bytes().into_iter().enumerate() {
                self.write_byte(a + i as u32, v)?;
            }
            Ok(())
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.freed.push(a);
            Ok(())
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            let a = self.next;
            self.next += n.max(1) + 16;
            for i in 0..n {
                self.bytes.insert(a + i, 0);
            }
            Ok(a)
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            panic!("graph-only BODYCOL test does not collect pointer vectors")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            panic!("frame enumeration must never delegate")
        }
    }
    fn dummy(
        arena: &mut ModelArena,
        model: ModelKey,
        parent: FrameKey,
        name: &[u8],
        position: [f32; 3],
    ) -> FrameKey {
        let mut state = I3dFrameState::dummy();
        state.local.elements[12..15].copy_from_slice(&position);
        state.flags &= !0x20;
        let mut full = b"car.".to_vec();
        full.extend_from_slice(name);
        let key = arena
            .insert(state, Some(0), full, Vec::new(), Some([[-1.; 3], [1.; 3]]))
            .unwrap();
        arena.frames[(key.0 - 1) as usize].original_name_bytes = Some(name.to_vec());
        arena.frames[(key.0 - 1) as usize].name_pointer =
            crate::frame_fast_lookup::OriginalFrameName::Offset(4);
        arena.add_frame(model, key).unwrap();
        arena.release(key).unwrap();
        arena.link_frame(key, Some(parent), 0).unwrap();
        key
    }
    fn scratch() -> VehicleBodycolScratch {
        VehicleBodycolScratch {
            record: 0x2000,
            leading_byte: 0xa5,
            nested_byte: 0x5a,
        }
    }
    fn identity(b: &mut VehicleFrameBackend<'_, Memory>) {
        for (i, value) in SMatrix::identity().elements.into_iter().enumerate() {
            b.write_word(0x3000 + i as u32 * 4, value.to_bits())
                .unwrap();
        }
    }
    #[test]
    fn bodycol_uses_live_arena_for_world_detach_removal_and_external_survivors() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        arena.set_frame_name(model.root(), Some(b"car")).unwrap();
        let top = dummy(&mut arena, model, model.root(), b"BODYCOL0", [1., 2., 3.]);
        let nested = dummy(&mut arena, model, top, b"BODYCOL1", [4., 5., 6.]);
        let child = dummy(&mut arena, model, nested, b"SOLID0", [7., 8., 9.]);
        arena.retain(child).unwrap();
        arena.register_frame_name(child).unwrap();
        let mut memory = Memory::new();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let model_address = backend.frame_address(model.root()).unwrap();
        let top_address = backend.frame_address(top).unwrap();
        let nested_address = backend.frame_address(nested).unwrap();
        let child_address = backend.frame_address(child).unwrap();
        let context = VehicleBodyCollisionContext {
            model: model_address,
            name_length_with_nul: 4,
        };
        identity(&mut backend);
        for frame in [top_address, nested_address] {
            bodycol_callback(&mut backend, 0x1000, context, frame, scratch()).unwrap();
        }
        bodycol_descendant_pass(&mut backend, 0x1000, 4, 0x2100).unwrap();
        let record = backend.read_word(0x1004).unwrap();
        let nested_record = backend.read_word(record + 0x14).unwrap();
        let descendant = backend.read_word(nested_record + 4).unwrap();
        assert_eq!(backend.read_word(descendant), Ok(child_address));
        assert!(
            !bodycol_transform_detach(&mut backend, 0x1000, 0x3000, model_address, 4, false)
                .unwrap()
        );
        assert!(backend.arena().frame(top).is_err());
        assert!(backend.arena().frame(nested).is_err());
        let survivor = backend.arena().frame(child).unwrap();
        assert_eq!(survivor.references, 1);
        assert_eq!(survivor.owner, None);
        assert_eq!(survivor.name_bytes, b"car.SOLID0");
        assert_eq!(backend.arena().links(child).unwrap().parent, None);
        assert_eq!(backend.arena().fast_tree_root(), Some(child));
        assert!(backend.arena().model(model).unwrap().frames.is_empty());
        assert_eq!(backend.read_word(record + 0x20), Ok(0));
        assert_eq!(backend.read_word(nested_record + 0x20), Ok(0));
        assert_eq!(backend.read_word(descendant), Ok(0));
        assert_eq!(
            backend.read_word(descendant + 4).map(f32::from_bits),
            Ok(12.)
        );
        assert!(backend.read_word(top_address + 4).is_err());
        backend.arena_mut().release(child).unwrap();
        assert_eq!(backend.arena().fast_tree_root(), None);
    }
    #[test]
    fn failed_owner_removal_preserves_completed_detach_and_record_identity() {
        let mut arena = ModelArena::default();
        let owner = arena.create_model().unwrap();
        let wrong = arena.create_model().unwrap();
        let key = dummy(&mut arena, owner, owner.root(), b"BODYCOL0", [2., 3., 4.]);
        let mut memory = Memory::new();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let original = backend.frame_address(owner.root()).unwrap();
        let wrong = backend.frame_address(wrong.root()).unwrap();
        let frame = backend.frame_address(key).unwrap();
        identity(&mut backend);
        bodycol_callback(
            &mut backend,
            0x1000,
            VehicleBodyCollisionContext {
                model: original,
                name_length_with_nul: 4,
            },
            frame,
            scratch(),
        )
        .unwrap();
        let record = backend.read_word(0x1004).unwrap();
        assert!(matches!(
            bodycol_transform_detach(&mut backend, 0x1000, 0x3000, wrong, 4, false),
            Err(VehicleBodycolUnavailable::Effect(VehicleFrameError::Arena(
                ModelError::FrameNotOwned { .. }
            )))
        ));
        assert_eq!(backend.arena().links(key).unwrap().parent, None);
        assert_eq!(
            backend.arena().frame(key).unwrap().owner,
            Some(owner.into())
        );
        assert_eq!(backend.arena().frame(key).unwrap().references, 1);
        assert_eq!(backend.read_word(record + 0x20), Ok(frame));
    }
    #[test]
    fn enumeration_observes_callback_link_mutation_saved_siblings_and_live_names() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let a = dummy(&mut arena, model, model.root(), b"A", [0.; 3]);
        let b = dummy(&mut arena, model, model.root(), b"B", [0.; 3]);
        let c = dummy(&mut arena, model, a, b"C", [0.; 3]);
        // LinkTo appends in the native child list order.
        let first = arena.links(model.root()).unwrap().child.unwrap();
        let next = arena.links(first).unwrap().next.unwrap();
        let mut memory = Memory::new();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let root = backend.frame_address(model.root()).unwrap();
        let mut depths = Vec::new();
        backend
            .enumerate_frames_at_depth(root, b"*", 0x10020, |backend, address, depth| {
                depths.push((backend.frame_key(address)?, depth));
                Ok::<_, VehicleFrameError<&str>>(0)
            })
            .unwrap();
        assert!(depths.contains(&(a, 0)));
        assert!(depths.contains(&(b, 0)));
        assert!(depths.contains(&(c, 1)));
        let mut visited = Vec::new();
        backend
            .enumerate_frames(root, b"*", 0x10020, |backend, address| {
                let key = backend.frame_key(address)?;
                visited.push(key);
                if key == first {
                    backend.bodycol_link_frame(address, 0, 0)?;
                }
                Ok::<_, VehicleFrameError<&str>>(0)
            })
            .unwrap();
        assert_eq!(visited.first(), Some(&first));
        assert!(visited.contains(&next));
        assert!(visited.contains(&c));
        backend
            .arena_mut()
            .set_frame_name(b, Some(b"renamed"))
            .unwrap();
        backend
            .bodycol_link_frame(backend.frame_address(b).unwrap(), root, 0)
            .unwrap();
        let mut found = Vec::new();
        backend
            .enumerate_frames(root, b"RENAMED", 0x20, |_, address| {
                found.push(address);
                Ok::<_, VehicleFrameError<&str>>(2)
            })
            .unwrap();
        assert_eq!(found, [backend.frame_address(b).unwrap()]);
        let name = backend.read_word(found[0] + 0x100).unwrap();
        assert_eq!(backend.read_byte(name), Ok(b'r'));
        assert_eq!(backend.read_word(found[0] + 0x120), Ok(root));
    }
    #[test]
    fn enum_skip_children_cancel_and_unknown_callback_returns_are_distinct() {
        for result in [0, 1, 2, 99] {
            let mut arena = ModelArena::default();
            let model = arena.create_model().unwrap();
            let a = dummy(&mut arena, model, model.root(), b"A", [0.; 3]);
            let child = dummy(&mut arena, model, a, b"B", [0.; 3]);
            let mut memory = Memory::new();
            let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
            let root = backend.frame_address(model.root()).unwrap();
            let mut visited = Vec::new();
            let status = backend
                .enumerate_frames(root, b"*", 0x10020, |backend, address| {
                    let key = backend.frame_key(address)?;
                    visited.push(key);
                    Ok::<_, VehicleFrameError<&str>>(if key == a { result } else { 0 })
                })
                .unwrap();
            assert_eq!(
                visited,
                if matches!(result, 1 | 2) {
                    vec![a]
                } else {
                    vec![a, child]
                }
            );
            assert_eq!(status, if result == 2 { 2 } else { 0 });
        }
    }
}

impl<B: VehicleComponentsBackend> VehicleWheelsBackend for VehicleFrameBackend<'_, B> {
    fn parse_wheel_number(&mut self, address: u32) -> Result<i32, Self::Error> {
        self.parse_component_number(address)
    }
    fn enumerate_wheels(
        &mut self,
        model: u32,
        target: &mut VehicleWheelEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, b"WH*", 0x10021, |backend, frame| {
            target.visit(frame, backend)
        })
        .map(|_| ())
    }
    fn wheel_name_letter(&mut self, character: i32) -> Result<bool, Self::Error> {
        if !(-1..=127).contains(&character) {
            return Err(VehicleFrameError::NumericDomain("isalpha C locale input"));
        }
        Ok(character >= 0 && (character as u8).is_ascii_alphabetic())
    }
    fn update_wheel_world_bounds(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena.update_hr_bound_vol_w(key)?;
        Ok(())
    }
}

#[cfg(test)]
mod owned_memory_tests {
    use super::*;
    use crate::{
        frame_transform::I3dFrameState,
        vehicle_bodycol::{VehicleBodycolScratch, VehicleBodycolStorage},
    };
    fn add(arena: &mut ModelArena, model: ModelKey, name: &[u8]) -> FrameKey {
        let key = arena
            .insert(
                I3dFrameState::dummy(),
                Some(0),
                name.to_vec(),
                Vec::new(),
                Some([[-1.; 3], [1.; 3]]),
            )
            .unwrap();
        arena.add_frame(model, key).unwrap();
        arena.release(key).unwrap();
        arena.link_frame(key, Some(model.root()), 0).unwrap();
        key
    }
    #[test]
    fn external_actor_records_share_the_live_heap_without_frame_ownership() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let first = add(&mut arena, model, b"first");
        let second = add(&mut arena, model, b"second");
        let mut memory = VehicleFrameMemory::default();
        let record;
        {
            let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
            record = backend.allocate(4).unwrap();
            for key in [first, second] {
                let address = backend.frame_address(key).unwrap();
                backend.write_word(address + 8, record).unwrap();
                assert_eq!(backend.read_word(address + 8), Ok(record));
            }
            assert_eq!(
                backend.read_word(record),
                Err(VehicleFrameError::Host(VehicleMemoryError::Uninitialized(
                    record
                )))
            );
            backend.write_word(record, 0x1234).unwrap();
        }
        let reference = arena.lookup_reference(second);
        assert_eq!(
            crate::actor_frames::ModelActorFrames::borrow(&mut arena, true).actor_data(reference),
            Err(crate::actor_frames::ActorFrameError::UnsupportedUserData(
                reference
            ))
        );
        {
            let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
            assert_eq!(backend.read_word(record), Ok(0x1234));
            let first_address = backend.frame_address(first).unwrap();
            backend.write_word(first_address + 8, 0).unwrap();
            assert_eq!(backend.read_word(record), Ok(0x1234));
            backend.free(record).unwrap();
            let second_address = backend.frame_address(second).unwrap();
            assert_eq!(backend.read_word(second_address + 8), Ok(record));
            assert!(backend.read_word(record).is_err());
            backend.write_word(second_address + 8, 0).unwrap();
        }
        assert_eq!(memory.allocations().count(), 0);
    }
    #[test]
    fn concrete_heap_composes_init_bodycol_and_seat_tail_without_host_frame_callbacks() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let body = add(&mut arena, model, b"BODY");
        let collision = add(&mut arena, model, b"BODYCOL0");
        let seat = add(&mut arena, model, b"SEAT0");
        arena.set_frame_name(model.root(), Some(b"car")).unwrap();
        arena.propagate_model_name(model).unwrap();
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x2000, &[0xa5; 116]).unwrap();
        memory.map_bytes(0x2100, &[0x5a; 60]).unwrap();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let root = backend.frame_address(model.root()).unwrap();
        let mut state = VehicleInitState::from_bytes(vec![0; 0x2008]);
        let storage = VehicleComponentTailStorage {
            bodycol: VehicleBodycolStorage {
                vehicle_address: 0x100000,
                callback_scratch: VehicleBodycolScratch {
                    record: 0x2000,
                    leading_byte: 0xa5,
                    nested_byte: 0x5a,
                },
                descendant_scratch: 0x2100,
            },
            scratch: VehicleComponentTailScratch {
                address: 0x4000,
                bytes: [0x33; VEHICLE_INIT_STACK_BYTES],
            },
        };
        assert_eq!(
            state.init_component_tail(&mut backend, root, storage),
            Ok(VehicleComponentTailOutcome::UnavailableFrontier(
                VehicleComponentTailFrontier::WheelEnumeration
            ))
        );
        assert!(backend.arena().frame(collision).is_err());
        assert!(backend.arena().frame(seat).is_err());
        assert!(backend.arena().frame(body).is_ok());
        assert_eq!(backend.arena().model(model).unwrap().frames, [body]);
        assert_eq!(backend.arena().frame(model.root()).unwrap().references, 2);
        assert!(backend.host().allocations().count() > 0);
    }
    #[test]
    fn intrinsic_collector_copies_pointer_order_without_acquiring_references() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let a = add(&mut arena, model, b"A");
        let b = add(&mut arena, model, b"B");
        let mut memory = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let a = backend.frame_address(a).unwrap();
        let b = backend.frame_address(b).unwrap();
        let mut vector = VehicleInitFrameVector {
            begin: 0,
            end: 0,
            capacity: 0,
        };
        let values = [a, b, a, b, a];
        for value in values {
            backend.collect_frame(&mut vector, value).unwrap();
        }
        for (i, value) in values.into_iter().enumerate() {
            assert_eq!(backend.read_word(vector.begin + i as u32 * 4), Ok(value));
        }
        assert_eq!(vector.end - vector.begin, 20);
        assert_eq!(vector.capacity - vector.begin, 32);
        assert_eq!(backend.read_word(a + 4), Ok(1));
        assert_eq!(backend.read_word(b + 4), Ok(1));
        assert_eq!(backend.host().allocations().count(), 1);
    }
}

#[cfg(test)]
mod name_identity_tests {
    use super::*;
    #[test]
    fn renamed_name_allocation_rejects_old_pointer_and_original_aliases_full_name() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let child = arena
            .insert(
                crate::frame_transform::I3dFrameState::dummy(),
                Some(0),
                b"BODY".to_vec(),
                Vec::new(),
                Some([[-1.; 3], [1.; 3]]),
            )
            .unwrap();
        arena.add_frame(model, child).unwrap();
        arena.set_frame_name(model.root(), Some(b"car")).unwrap();
        arena.propagate_model_name(model).unwrap();
        let mut memory = VehicleFrameMemory::default();
        let (old, current, address) = {
            let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
            let address = backend.frame_address(child).unwrap();
            let old = backend.read_word(address + 0x100).unwrap();
            assert_eq!(backend.read_word(address + 0x104), Ok(old + 4));
            assert_eq!(backend.read_byte(old + 4), Ok(b'B'));
            backend
                .arena_mut()
                .set_frame_name(child, Some(b"new"))
                .unwrap();
            let current = backend.read_word(address + 0x100).unwrap();
            assert_ne!(old, current);
            assert!(backend.read_byte(old).is_err());
            assert_eq!(backend.read_word(address + 0x104), Ok(current));
            (old, current, address)
        };
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        assert_eq!(backend.read_word(address + 0x100), Ok(current));
        assert!(backend.read_byte(old).is_err());
        assert_eq!(backend.read_byte(current), Ok(b'n'));
    }
}

impl<B: VehicleInitFinishBackend> VehicleInitFinishBackend for VehicleFrameBackend<'_, B> {
    fn enumerate_init_centre(
        &mut self,
        model: u32,
        target: &mut VehicleInitCentreEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, b"CENTRE", 0x20, |_, frame| {
            Ok::<_, VehicleInitUnavailable<Self::Error>>(target.visit(frame))
        })
        .map(|_| ())
    }
    fn init_centre_callback(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        frame: u32,
        control: u32,
    ) -> Result<(), Self::Error> {
        self.frame_key(frame)?;
        self.host
            .init_centre_callback(memory, function, context, frame, control)
            .map_err(VehicleFrameError::Host)
    }
    fn update_init_model(&mut self, model: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(model)?;
        self.arena.graph.update(key).map_err(ModelError::from)?;
        Ok(())
    }
    fn update_init_model_bounds(&mut self, model: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(model)?;
        self.arena.update_hr_bound_vol(key)?;
        Ok(())
    }
}

impl<B: VehicleInitBackend> VehicleCarCallerBackend for VehicleFrameBackend<'_, B> {
    fn find_actor_target(
        &mut self,
        model: u32,
        target: VehicleActorTarget,
    ) -> Result<u32, Self::Error> {
        let mut found = 0;
        self.enumerate_frames(model, target.pattern(), target.mask(), |_, frame| {
            found = frame;
            Ok::<_, Self::Error>(2)
        })?;
        Ok(found)
    }
    fn set_actor_target_on(&mut self, frame: u32, on: bool) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena.set_frame_on(key, u32::from(on))?;
        Ok(())
    }
}
impl<B: VehicleDestructorBackend> VehicleDestructorBackend for VehicleFrameBackend<'_, B> {
    fn detach_vehicle_frame(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.bodycol_link_frame(frame, 0, 0)
    }
    fn release_vehicle_object(&mut self, object: u32) -> Result<(), Self::Error> {
        if (RESOURCE_BASE..0xc100_0000).contains(&object) {
            let key = self.texture_key(object)?;
            self.arena.release_cpu_texture(key)?;
            return Ok(());
        }

        if self.release_deform_resource(object)? {
            return Ok(());
        }
        if Self::field(object).is_some() {
            let key = self.frame_key(object)?;
            self.arena.release(key)?;
            Ok(())
        } else {
            self.host
                .release_vehicle_object(object)
                .map_err(VehicleFrameError::Host)
        }
    }
}
impl<B: VehicleComponentsBackend> VehicleAxisBackend for VehicleFrameBackend<'_, B> {
    fn enumerate_axes(
        &mut self,
        model: u32,
        target: &mut VehicleAxisEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, b"AXIS*", 0x10001, |backend, frame| {
            target.visit(frame, backend)
        })
        .map(|_| ())
    }
    fn parse_axis_number(&mut self, address: u32) -> Result<i32, Self::Error> {
        self.parse_component_number(address)
    }
}
impl<B: VehicleComponentTailBackend> VehicleAxisUpdateBackend for VehicleFrameBackend<'_, B> {
    fn set_axis_frame_on(&mut self, frame: u32, on: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena.set_frame_on(key, on)?;
        Ok(())
    }
    fn set_axis_frame_direction(
        &mut self,
        frame: u32,
        direction: [u32; 3],
    ) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let rotation = SQuat::try_make_frame_direction(direction.map(f32::from_bits), 0.)
            .map_err(|_| VehicleFrameError::NumericDomain("SetDir"))?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        state.rotation = rotation;
        state.flags = (state.flags & 0xffff_fecb) | 0x4000_0008;
        Ok(())
    }
    fn set_axis_matrix_direction(
        &mut self,
        frame: u32,
        direction: [u32; 3],
        up: [u32; 3],
    ) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?
            .local
            .try_set_dir3(direction.map(f32::from_bits), up.map(f32::from_bits))
            .map_err(|_| VehicleFrameError::NumericDomain("SetDir3"))
    }
}

impl<B: VehicleSourceParametersBackend + VehicleComponentTailBackend> VehicleSourceParametersBackend
    for VehicleFrameBackend<'_, B>
{
    fn parameter_update_frame_world(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena
            .graph
            .update_w_matrix_proc(key)
            .map_err(ModelError::from)?;
        Ok(())
    }
    fn parameter_frame_world_scale(&mut self, frame: u32) -> Result<[u32; 3], Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena
            .graph
            .state(key)
            .map_err(ModelError::from)?
            .world
            .try_get_scale()
            .map(|v| v.map(f32::to_bits))
            .map_err(|_| VehicleFrameError::NumericDomain("GetScale"))
    }
    fn parameter_initialize_sounds(
        &mut self,
        memory: Option<VehicleComponentTailMemory<'_>>,
        vehicle: u32,
        source: u32,
    ) -> Result<(), Self::Error> {
        if let Some(memory) = memory {
            self.game_vehicle_dependency(memory, vehicle, 0x4eac70, [source, 0])?;
            Ok(())
        } else {
            self.host
                .parameter_initialize_sounds(None, vehicle, source)
                .map_err(VehicleFrameError::Host)
        }
    }
}
