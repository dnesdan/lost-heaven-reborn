//! Bounded I3D_lit_object resource ownership. Source: pinned
//! mafia-re 29b65b0, I3d_lit_object.cpp constructor/destructor and litLod::Create.
//! Saved vertex-lighting loads are implemented by saved_lighting.rs. Lightmap
//! atlases, Construct and active-lighting changes remain separate boundaries.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectVisualType {
    Plain,
    Lit,
}
impl ObjectVisualType {
    pub fn source_fourcc(self) -> u32 {
        match self {
            Self::Plain => 0x5f4a424f, // OBJ_
            Self::Lit => 0x50414d4c,   // LMAP (not serialized ordinal 1)
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
    /// The saved ARGB words owned by each litLod's m_pColors. These host
    /// owners are not a claim about the native level's in-memory layout.
    pub saved_vertex_colors: [Option<Box<[u32]>>; 10],
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
                saved_vertex_colors: std::array::from_fn(|_| None),
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
    pub fn create_unconstructed_lit_object_resources(
        &mut self,
        mesh: Option<MeshObjectKey>,
    ) -> Result<ObjectResourceKey, ObjectError> {
        let lit = Box::new(LitObjectResources::new());
        let key = self.create_object_resources(mesh)?;
        let record = self.objects[(key.slot - 1) as usize].as_mut().unwrap();
        record.visual_type = ObjectVisualType::Lit;
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
        self.available()?;
        let lit = self
            .object(object)?
            .lit_object
            .as_ref()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?;
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index as u32));
        }
        if lit.levels[0].lods[index].is_some() {
            return Err(ObjectError::SlotOccupied);
        }
        let key = self.create_plain_lod()?;
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
            .levels[0]
            .lods[index] = Some(key);
        Ok(key)
    }

    pub(crate) fn lit_level_mut(
        &mut self,
        object: ObjectResourceKey,
        level: usize,
    ) -> Result<&mut LitLevelResources, ObjectError> {
        self.available()?;
        self.object(object)?;
        self.objects[(object.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .lit_object
            .as_mut()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .levels
            .get_mut(level)
            .ok_or(ObjectError::InvalidIndex(level as u32))
    }

    /// I3D_lit_object::Destruct(0), source 0x10026930: only the active
    /// lighting is reset. Completed nonactive lightings remain owned.
    pub(crate) fn destruct_active_lighting<B: ManagedVisualBackend + ?Sized>(
        &mut self,
        object: ObjectResourceKey,
        backend: &mut B,
    ) -> Result<(), ObjectOperationError<B::Error>> {
        self.available()?;
        let record = self.object(object)?;
        if record.phase != ObjectCleanupPhase::Alive {
            return Err(ObjectError::UnsupportedVisualSubtype.into());
        }
        let active = record
            .lit_object()
            .ok_or(ObjectError::UnsupportedVisualSubtype)?
            .active_level;
        self.lit_level_mut(object, active)?.constructed = false;
        for index in 0..10 {
            let lod = self.lit_level_mut(object, active)?.lods[index];
            if let Some(lod) = lod {
                if self.lod(lod)?.owner != Some(object) {
                    return Err(ObjectError::LodOwned(lod).into());
                }
                self.lit_level_mut(object, active)?.saved_vertex_colors[index] = None;
                self.cleanup_lod(lod, backend)?;
                self.lods[(lod.slot - 1) as usize] = None;
                let level = self.lit_level_mut(object, active)?;
                level.lods[index] = None;
            }
        }
        Ok(())
    }

    /// A pending new litLod is not published into its lighting until its saved
    /// colors and host diagnostic buffer are complete. Failed pending owners
    /// can therefore be destroyed without retiring earlier completed LODs.
    pub(crate) fn create_pending_saved_lit_lod(&mut self) -> Result<PlainLodKey, ObjectError> {
        let key = self.create_plain_lod()?;
        let lod = self.lods[(key.slot - 1) as usize].as_mut().unwrap();
        lod.update_count = 0;
        lod.dirty = true;
        Ok(key)
    }

    pub(crate) fn publish_loaded_lit_lod(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        index: usize,
        key: PlainLodKey,
        colors: Option<Box<[u32]>>,
    ) -> Result<(), ObjectError> {
        if index >= 10 {
            return Err(ObjectError::InvalidIndex(index as u32));
        }
        let level = self.lit_level_mut(object, lighting)?;
        if level.lods[index].is_some() || level.saved_vertex_colors[index].is_some() {
            return Err(ObjectError::SlotOccupied);
        }
        let lod = self.lod(key)?;
        if lod.owner.is_some() {
            return Err(ObjectError::LodOwned(key));
        }
        if lod.state != LodState::Constructed {
            return Err(ObjectError::LodDestructed(key));
        }
        let lod = self.lods[(key.slot - 1) as usize].as_mut().unwrap();
        lod.owner = Some(object);
        lod.dirty = false;
        let level = self.lit_level_mut(object, lighting)?;
        level.lods[index] = Some(key);
        level.saved_vertex_colors[index] = colors;
        Ok(())
    }
}
