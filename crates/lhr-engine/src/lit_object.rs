//! Bounded unconstructed I3D_lit_object resource ownership. Source: pinned
//! mafia-re 29b65b0, I3d_lit_object.cpp constructor/destructor and litLod::Create.
//! Construct/Load, changing the active lighting, lightmaps and vertex lighting
//! have no admitted producer here. The six constructor records remain real
//! owned state; only lighting zero can acquire unconstructed base geometry.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectVisualType {
    Plain,
    LitUnconstructed,
}
impl ObjectVisualType {
    pub fn source_fourcc(self) -> u32 {
        match self {
            Self::Plain => 0x5f4a424f,            // OBJ_
            Self::LitUnconstructed => 0x50414d4c, // LMAP (not serialized ordinal 1)
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
}
