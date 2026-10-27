//! Bounded unconstructed I3D_lit_object resource ownership. Source: pinned
//! mafia-re 29b65b0, I3d_lit_object.cpp constructor/destructor and litLod::Create.
//! Saved-lighting CPU state is separately owned from the shared mesh. Offline
//! Construct/Save and native GPU upload remain separate boundaries.
use super::*;
#[path = "lightmap_atlas.rs"]
pub mod lightmap_atlas;
#[path = "saved_lightmaps.rs"]
pub mod saved_lightmaps;
use saved_lightmaps::{SavedLightingLoad, SavedLitLod};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectVisualType {
    Plain,
    LitUnconstructed,
    Singlemesh,
    Singlemorph,
    Morph,
    Billboard,
}
impl ObjectVisualType {
    pub fn source_fourcc(self) -> u32 {
        match self {
            Self::Plain => 0x5f4a424f,            // OBJ_
            Self::LitUnconstructed => 0x50414d4c, // LMAP (not serialized ordinal 1)
            Self::Singlemesh => 0x4d474e53,       // SNGM
            Self::Morph => 0x4850_524d,           // MRPH
            Self::Singlemorph => 0x524d4d53,      // SMMR
            Self::Billboard => 0x4452_4242,       // BBRD
        }
    }
}
#[derive(Debug, PartialEq)]
pub struct LitLevelResources {
    pub constructed: bool,
    pub mode: u32,
    pub resolution: f32,
    pub angle: f32,
    pub lods: [Option<PlainLodKey>; 10],
    /// litLod's independently owned loaded arrays, atlas and index buffer.
    pub saved_lods: [Option<SavedLitLod>; 10],
}
#[derive(Debug, PartialEq)]
pub struct LitObjectResources {
    pub active_level: usize,
    pub levels: [LitLevelResources; 6],
}
impl LitObjectResources {
    fn new() -> Self {
        Self {
            active_level: 0,
            levels: std::array::from_fn(|_| LitLevelResources {
                constructed: false,
                mode: 2,
                resolution: 5.,
                angle: 45.,
                lods: [None; 10],
                saved_lods: std::array::from_fn(|_| None),
            }),
        }
    }
}
impl I3dObjectResources {
    pub fn lit_object(&self) -> Option<&LitObjectResources> {
        self.lit_object.as_deref()
    }
}
impl ObjectResources {
    /// SetActiveLevel frees only the old active lighting's render buffers;
    /// constructed lighting arrays and shared lightmap texture owners survive.
    /// This arena has no native m_PosVB allocation: GPU snapshots own theirs.
    pub fn set_active_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        level: usize,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        if level >= 6 {
            return Err(ObjectError::InvalidIndex(level as u32).into());
        }
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        let old = lit.active_level;
        let keys = lit.levels[old].lods;
        for key in keys.iter().flatten() {
            let lod = self.lod(*key)?;
            if lod.owner != Some(object) {
                return Err(ObjectError::LodOwned(*key).into());
            }
            if lod.state != LodState::Constructed {
                return Err(ObjectError::LodDestructed(*key).into());
            }
        }
        for key in keys.into_iter().flatten() {
            self.release_lit_render_vertices(key, backend)?;
        }
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .active_level = level;
        Ok(())
    }
    /// Lit SetMesh keeps its independent lightings when requested; unlike the
    /// plain class it does not run the plain LOD compatibility path. Visual
    /// bounds are projected by the frame adapter that owns the same object.
    pub(crate) fn lit_set_object_mesh<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        keep_lighting: bool,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let record = self.object(object)?;
        if record.lit_object.is_none() {
            return Err(ObjectError::UnsupportedVisualSubtype.into());
        }
        let old = record.mesh;
        if old == mesh {
            return Ok(());
        }
        if let Some(mesh) = mesh {
            self.managed
                .resources()
                .mesh_object(mesh)
                .map_err(ManagedError::from)?;
        }
        let result = (|| {
            if !keep_lighting {
                self.destruct_active_lighting(object, backend)?;
            }
            if let Some(old) = old {
                managed_result(self.managed.release_mesh_object(old, backend))?;
            }
            self.objects[(object.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .mesh = mesh;
            if let Some(mesh) = mesh {
                self.managed.retain_mesh_object(mesh)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// Native Destruct(0) clears only the active lighting. Completed inactive
    /// levels remain owned. Effects stop at the first backend failure.
    pub fn destruct_active_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        let active = lit.active_level;
        let keys = lit.levels[active].lods;
        for key in keys.iter().flatten() {
            let lod = self.lod(*key)?;
            if lod.owner != Some(object) {
                return Err(ObjectError::LodOwned(*key).into());
            }
            if lod.state != LodState::Constructed {
                return Err(ObjectError::LodDestructed(*key).into());
            }
        }
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[active]
            .constructed = false;
        for (i, key) in keys.into_iter().enumerate() {
            if let Some(key) = key {
                self.cleanup_saved_lit_lod(object, active, i, backend)?;
                self.lods[(key.slot - 1) as usize] = None;
            }
            let level = &mut self.objects[(object.slot - 1) as usize]
                .as_mut()
                .unwrap()
                .lit_object
                .as_mut()
                .unwrap()
                .levels[active];
            level.lods[i] = None;
            level.saved_lods[i] = None;
        }
        Ok(())
    }
    /// Duplicate copies construction settings, never the constructed arrays or
    /// active lighting. Plain objects additionally inherit source fade.
    pub(crate) fn copy_object_duplicate_state(
        &mut self,
        destination: ObjectResourceKey,
        source: ObjectResourceKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        let src = self.object(source)?;
        let fade = src.fade;
        let settings = src.lit_object.as_ref().map(|lit| {
            std::array::from_fn::<_, 6, _>(|i| {
                let l = &lit.levels[i];
                (l.mode, l.resolution, l.angle)
            })
        });
        self.object(destination)?;
        let dst = self.objects[(destination.slot - 1) as usize]
            .as_mut()
            .unwrap();
        if let Some(lit) = dst.lit_object.as_mut() {
            if let Some(settings) = settings {
                for (level, (mode, resolution, angle)) in lit.levels.iter_mut().zip(settings) {
                    level.mode = mode;
                    level.resolution = resolution;
                    level.angle = angle;
                }
            }
        } else {
            dst.fade = fade;
        }
        Ok(())
    }
    /// Transfers completed Load allocations into their actual object owner.
    /// The caller first performs native Destruct on the active lighting. A
    /// failing Load's partial result is installed through this same operation.
    pub fn install_saved_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        loaded: SavedLightingLoad,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        for (old, new) in lit.levels.iter().zip(&loaded.levels) {
            if let Some(new) = new {
                for (index, lod) in new.lods.iter().enumerate() {
                    if lod.is_some() {
                        if let Some(key) = old.lods[index] {
                            let common = self.lod(key)?;
                            if common.owner != Some(object) {
                                return Err(ObjectError::LodOwned(key).into());
                            }
                            if common.state != LodState::Constructed {
                                return Err(ObjectError::LodDestructed(key).into());
                            }
                        }
                    }
                }
            }
        }
        for (lighting, new) in loaded.levels.into_iter().enumerate() {
            if let Some(new) = new {
                let old = &mut self.objects[(object.slot - 1) as usize]
                    .as_mut()
                    .unwrap()
                    .lit_object
                    .as_mut()
                    .unwrap()
                    .levels[lighting];
                old.mode = new.mode;
                old.resolution = new.resolution;
                old.angle = new.angle;
                // On duplicate saved headers native completed LOD pointers are
                // overwritten. Rust drops replaced allocations exactly once.
                for (index, new) in new.lods.into_iter().enumerate() {
                    if let Some(new) = new {
                        if let Some(key) = self.object(object)?.lit_object.as_ref().unwrap().levels
                            [lighting]
                            .lods[index]
                        {
                            self.cleanup_saved_lit_lod(object, lighting, index, backend)?;
                            self.lods[(key.slot - 1) as usize] = None;
                            self.objects[(object.slot - 1) as usize]
                                .as_mut()
                                .unwrap()
                                .lit_object
                                .as_mut()
                                .unwrap()
                                .levels[lighting]
                                .lods[index] = None;
                        }
                        if self.object(object)?.lit_object.as_ref().unwrap().levels[lighting].lods
                            [index]
                            .is_none()
                        {
                            self.create_lit_lod_record(object, lighting, index)?;
                        }
                        self.objects[(object.slot - 1) as usize]
                            .as_mut()
                            .unwrap()
                            .lit_object
                            .as_mut()
                            .unwrap()
                            .levels[lighting]
                            .saved_lods[index] = Some(new);
                    }
                }
                if new.constructed {
                    self.objects[(object.slot - 1) as usize]
                        .as_mut()
                        .unwrap()
                        .lit_object
                        .as_mut()
                        .unwrap()
                        .levels[lighting]
                        .constructed = true;
                }
            }
        }
        Ok(())
    }
    pub fn saved_lit_lod(
        &self,
        object: ObjectResourceKey,
        lod: usize,
    ) -> Result<Option<&SavedLitLod>, ObjectError> {
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if lod >= 10 {
            return Err(ObjectError::InvalidIndex(lod as u32));
        }
        Ok(lit.levels[lit.active_level].saved_lods[lod].as_ref())
    }
    pub fn create_unconstructed_lit_object_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        let lit = Box::new(LitObjectResources::new());
        let key = self.create_object_resources(mesh)?;
        let record = self.objects[(key.slot - 1) as usize].as_mut().unwrap();
        record.visual_type = ObjectVisualType::LitUnconstructed;
        record.lit_object = Some(lit);
        Ok(key)
    }
    pub(crate) fn finish_unconstructed_lit_geometry(
        &mut self,
        lod: PlainLodKey,
    ) -> Result<(), ObjectError> {
        self.available()?;
        let index = self.index(lod.arena, lod.slot)?;
        self.lod(lod)?;
        self.lods[index].as_mut().unwrap().dirty = false;
        Ok(())
    }
    /// The host intake eagerly creates lighting-zero geometry. Native creation
    /// is lazy in m_AddFGroups. No constructed lit arrays/maps/index buffer are
    /// installed: all indices remain owned by the shared mesh face groups.
    pub(crate) fn create_unconstructed_lit_lod(
        &mut self,
        object: ObjectResourceKey,
        index: usize,
    ) -> Result<PlainLodKey, ObjectError> {
        self.create_lit_lod_record(object, 0, index)
    }
    fn create_lit_lod_record(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        index: usize,
    ) -> Result<PlainLodKey, ObjectError> {
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index as u32));
        }
        if lit.levels[lighting].lods[index].is_some() {
            return Err(ObjectError::SlotOccupied);
        }
        Self::reserve(&mut self.lit_storage)?;
        let key = self.create_plain_lod()?;
        self.lit_storage.push(LitLodStorage::new(key));
        let lod = self.lods[(key.slot - 1) as usize].as_mut().unwrap();
        // litLod's inline derived constructor overwrites these base words.
        lod.update_count = 0;
        lod.dirty = true;
        lod.owner = Some(object);
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .unwrap()
            .levels[lighting]
            .lods[index] = Some(key);
        Ok(key)
    }
}
