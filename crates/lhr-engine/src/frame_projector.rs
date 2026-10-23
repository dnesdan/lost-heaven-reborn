//! Actual PS2 I3D_projector symbols corroborate SetTexture, SetOrthogonal,
//! SetColor, SetMode and SetProjectorLock.
//! Original projector constructor/property/lifetime projection (PC LS3DF
//! 45e30,46000,46240..46480). Render-time clipping/material/IB/VB allocation is
//! separate: these objects start with the actual null render allocations and
//! only admitted operations may mutate their owned state.
use crate::{
    frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameState},
    mesh_bounds::MeshBoundVolume,
    model_loading::{ModelArena, ModelError},
    visual_resources::TextureKey,
};

#[derive(Clone, Debug)]
pub struct I3dProjectorState {
    pub(crate) texture: Option<TextureKey>,
    pub orthogonal: u8,
    pub range: f32,
    pub alpha: f32,
    pub color: [f32; 3],
    pub lock: u32,
    pub mode: u32,
    pub center_t: [f32; 3],
    pub reverse: f32,
    pub(crate) cache_254: u32,
    pub(crate) cache_25c: u32,
}
impl I3dProjectorState {
    pub(crate) fn constructor() -> Self {
        Self {
            texture: None,
            orthogonal: 1,
            range: 100.,
            alpha: 1.,
            color: [0.; 3],
            lock: 0,
            mode: 0,
            center_t: [0.; 3],
            reverse: 0.5,
            cache_254: 0,
            cache_25c: 0,
        }
    }
    pub fn texture(&self) -> Option<TextureKey> {
        self.texture
    }
    fn invalidate(&mut self) {
        self.cache_25c = 0;
        self.cache_254 = 0;
    }
}
impl ModelArena {
    /// Construct an actual CPU-owned projector frame; no graphics allocations
    /// are created by the original constructor either.
    pub fn create_projector(&mut self) -> Result<FrameKey, ModelError> {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Projector;
        state.visual_flags = 0x2200;
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        let index = self.frame_index(key)?;
        self.frames[index].projector = Some(I3dProjectorState::constructor());
        self.frames[index].visual_bounds = Some(MeshBoundVolume {
            bbox: [[-1., -1., 0.], [1., 1., 1.]],
            center: [0., 0., 0.5],
            radius: 1.5,
        });
        Ok(key)
    }
    pub fn projector(&self, key: FrameKey) -> Result<&I3dProjectorState, ModelError> {
        self.frame(key)?
            .projector
            .as_ref()
            .ok_or(ModelError::Frame(FrameError::UnsupportedFrameUpdate(key)))
    }
    fn projector_mut(&mut self, key: FrameKey) -> Result<&mut I3dProjectorState, ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index]
            .projector
            .as_mut()
            .ok_or(ModelError::Frame(FrameError::UnsupportedFrameUpdate(key)))
    }
    /// SetTexture releases the previous reference before storing and retaining
    /// the new one. Identical identity is a true no-op, including null.
    pub fn set_projector_texture(
        &mut self,
        key: FrameKey,
        texture: Option<TextureKey>,
    ) -> Result<(), ModelError> {
        let previous = self.projector(key)?.texture;
        if previous == texture {
            return Ok(());
        }
        if let Some(texture) = texture {
            let references = self
                .visual_resource_owner_mut()?
                .backend()
                .texture_references(texture)
                .ok_or(ModelError::LoadedVisual(
                    crate::loaded_visual_resources::LoadedVisualError::Backend(
                        "unknown CPU texture",
                    ),
                ))?;
            if references == u32::MAX {
                return Err(ModelError::AllocationOverflow);
            }
        }
        if let Some(previous) = previous {
            self.release_cpu_texture(previous)?;
        }
        self.projector_mut(key)?.texture = texture;
        if let Some(texture) = texture {
            self.retain_cpu_texture(texture)?;
        }
        Ok(())
    }
    pub fn set_projector_orthogonal(
        &mut self,
        key: FrameKey,
        orthogonal: u8,
    ) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.orthogonal = orthogonal;
        projector.invalidate();
        Ok(())
    }
    // Keep the original ordered comparisons and unmodified NaN/zero bits.
    #[allow(clippy::manual_clamp)]
    pub fn set_projector_color(
        &mut self,
        key: FrameKey,
        color: [f32; 3],
    ) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.color = color;
        for component in &mut projector.color {
            if *component < 0. {
                *component = 0.;
            } else if *component > 1. {
                *component = 1.;
            }
        }
        projector.invalidate();
        Ok(())
    }
    pub fn set_projector_mode(&mut self, key: FrameKey, mode: u32) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.mode = mode;
        projector.invalidate();
        Ok(())
    }
    pub fn set_projector_lock(&mut self, key: FrameKey, lock: u32) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.lock = lock;
        projector.invalidate();
        Ok(())
    }
    pub(crate) fn release_projector_resources(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if let Some(texture) = self.projector(key)?.texture {
            self.release_cpu_texture(texture)?;
            self.projector_mut(key)?.texture = None;
        }
        // Material, vertex/face/group allocations and manager block handles
        // are constructor-null throughout this admitted pre-render lifetime.
        let index = self.frame_index(key)?;
        self.frames[index].projector = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn properties_update_caches_and_scene_close_preserves_external_projector() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let projector = arena.create_projector().unwrap();
        assert_eq!(arena.projector(projector).unwrap().color, [0.; 3]);
        arena.add_frame(model, projector).unwrap();
        arena.release(projector).unwrap();
        arena.set_projector_orthogonal(projector, 0).unwrap();
        arena
            .set_projector_color(projector, [-1., 0.5, 2.])
            .unwrap();
        arena.set_projector_mode(projector, 3).unwrap();
        arena.set_projector_lock(projector, 1).unwrap();
        assert_eq!(arena.projector(projector).unwrap().color, [0., 0.5, 1.]);
        arena
            .graph
            .projected_state_mut(projector)
            .unwrap()
            .projector_cache_258 = 17;
        arena.graph.update(projector).unwrap();
        assert_eq!(arena.state(projector).unwrap().projector_cache_258, 0);
        assert_eq!(
            arena.hr_bound_volume(projector).unwrap().bbox,
            [[-1., -1., 0.], [1., 1., 1.]]
        );
        arena.retain(projector).unwrap();
        arena.close(model).unwrap();
        assert_eq!(arena.frame(projector).unwrap().references, 1);
        arena.release(projector).unwrap();
        assert!(arena.projector(projector).is_err());
    }
}
