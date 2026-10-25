//! Shared-arena UpdateHRBoundVol/UpdateHRBoundVolW, PC RVAs 0x1cc50/0x1cf90.
//! Original LS3DF e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Hierarchy traversal and stored caches use live frames. Visual boxes must come
//! from admitted loaded resources or the existing singlemesh definition path.
//! Finite normal-or-zero PC24/RN arithmetic follows native_matrix. Host errors
//! retain preceding stores, with separate error markers preventing stale use.
use crate::frame_transform::{FrameError, FrameKey, FrameKind};
use crate::mesh_bounds::MeshBoundVolume;
use crate::model_loading::{ModelArena, ModelError};
use crate::native_matrix::{checked_f32_add, checked_f32_mul};

fn valid(bbox: [[f32; 3]; 2]) -> bool {
    bbox[0][0] <= bbox[1][0]
}
fn corners(bbox: [[f32; 3]; 2]) -> [[f32; 3]; 8] {
    std::array::from_fn(|i| {
        [
            bbox[usize::from(i < 4)][0],
            bbox[usize::from(i % 4 < 2)][1],
            bbox[i % 2][2],
        ]
    })
}
fn absorb(bbox: &mut [[f32; 3]; 2], point: [f32; 3]) {
    for axis in 0..3 {
        if bbox[0][axis] > point[axis] {
            bbox[0][axis] = point[axis];
        }
        if bbox[1][axis] < point[axis] {
            bbox[1][axis] = point[axis];
        }
    }
}
fn sphere(volume: &mut MeshBoundVolume) -> Result<(), ModelError> {
    let mut half = [0.; 3];
    for (axis, value) in half.iter_mut().enumerate() {
        *value = checked_f32_add(volume.bbox[1][axis], -volume.bbox[0][axis])
            .and_then(|n| checked_f32_mul(n, 0.5))
            .map_err(|_| FrameError::NumericDomain("hierarchy half extent"))?;
    }
    for (axis, value) in half.iter().enumerate() {
        volume.center[axis] = checked_f32_add(volume.bbox[0][axis], *value)
            .map_err(|_| FrameError::NumericDomain("hierarchy center"))?;
    }
    let squared = (|| {
        checked_f32_add(
            checked_f32_add(
                checked_f32_mul(half[2], half[2])?,
                checked_f32_mul(half[1], half[1])?,
            )?,
            checked_f32_mul(half[0], half[0])?,
        )
    })()
    .map_err(|_| FrameError::NumericDomain("hierarchy radius"))?;
    volume.radius = squared.sqrt();
    Ok(())
}
impl ModelArena {
    /// Raw stored projection, including an interrupted prefix. This is suitable
    /// for native memory diagnostics; call the getter below for usable bounds.
    pub fn hierarchy_bounds_snapshot(&self, key: FrameKey) -> Result<MeshBoundVolume, ModelError> {
        Ok(self.frames[self.frame_index(key)?].hr_bounds)
    }
    pub fn hierarchy_world_bounds_snapshot(
        &self,
        key: FrameKey,
    ) -> Result<MeshBoundVolume, ModelError> {
        Ok(self.frames[self.frame_index(key)?].hr_world_bounds)
    }
    /// Own-visual GetWBoundVolume producer used by LOD distance. Unlike HR
    /// bounds this excludes descendants. Computes the source eight-corner world
    /// AABB and its sphere without fabricating a native cache-valid flag.
    pub fn visual_world_bounds(&mut self, key: FrameKey) -> Result<MeshBoundVolume, ModelError> {
        let fi = self.frame_index(key)?;
        if self.state(key)?.visual_flags & 0x40 != 0 {
            return Err(ModelError::VisualBoundsUnavailable(key));
        }
        let local = self.frames[fi]
            .visual_bounds
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let mut output = MeshBoundVolume::CONSTRUCTOR;
        if valid(local.bbox) {
            let matrix = self.world_matrix(key)?;
            for (i, point) in corners(local.bbox).into_iter().enumerate() {
                let point = matrix
                    .try_transform_point(point)
                    .map_err(|_| FrameError::NumericDomain("visual world bound transform"))?;
                if i == 0 {
                    output.bbox = [point, point];
                } else {
                    absorb(&mut output.bbox, point);
                }
            }
            sphere(&mut output)?;
        } else {
            output.center = [0.; 3];
            output.radius = 0.;
        }
        Ok(output)
    }
    pub fn hr_bound_volume(&mut self, key: FrameKey) -> Result<MeshBoundVolume, ModelError> {
        if self.state(key)?.flags & 0x40 == 0 {
            self.update_hr_bound_vol(key)?;
        }
        let frame = self.frame(key)?;
        if let Some(error) = &frame.hr_bounds_error {
            return Err(error.clone());
        }
        Ok(frame.hr_bounds)
    }
    pub fn hr_bound_volume_w(&mut self, key: FrameKey) -> Result<MeshBoundVolume, ModelError> {
        if self.state(key)?.flags & 0x100 == 0 {
            self.update_hr_bound_vol_w(key)?;
        }
        let frame = self.frame(key)?;
        if let Some(error) = &frame.hr_world_bounds_error {
            return Err(error.clone());
        }
        Ok(frame.hr_world_bounds)
    }
    pub fn update_hr_bound_vol(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.update_hr_bound_vol_at(key, 0)
    }
    fn update_hr_bound_vol_at(&mut self, key: FrameKey, depth: usize) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        if depth >= 256 {
            return Err(ModelError::BoundsDepthUnavailable(key));
        }
        let result = self.update_hr_bound_vol_inner(key, depth);
        self.frames[fi].hr_bounds_error = result.as_ref().err().cloned();
        result
    }
    fn update_hr_bound_vol_inner(&mut self, key: FrameKey, depth: usize) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        let kind = self.state(key)?.kind;
        if kind == FrameKind::Sector {
            // Original type5 immediately marks the stored box valid; no box,
            // sphere, visual computation or descendant traversal occurs.
            self.graph.projected_state_mut(key)?.flags |= 0x40;
            return Ok(());
        }
        if matches!(
            kind,
            FrameKind::Mesh | FrameKind::Projector | FrameKind::Singlemesh
        ) {
            if kind == FrameKind::Singlemesh {
                // The admitted definition/binding owner must already have
                // completed its genuine derived ComputeBounds path.
                let bounds = self.graph.singlemesh_bound_volume(key)?;
                self.frames[fi].visual_bounds = Some(MeshBoundVolume {
                    bbox: [bounds.bbox.min, bounds.bbox.max],
                    center: bounds.center,
                    radius: bounds.radius,
                });
            } else if self.state(key)?.visual_flags & 0x20000 == 0 {
                // Plain ComputeBounds5a4f0 only sets the valid bit. SetMesh must
                // already have supplied the actual object's visual box.
                if self.frames[fi].visual_bounds.is_none() {
                    return Err(ModelError::VisualBoundsUnavailable(key));
                }
                self.graph.projected_state_mut(key)?.visual_flags |= 0x20000;
            }
            self.frames[fi].hr_bounds.bbox = self.frames[fi]
                .visual_bounds
                .ok_or(ModelError::VisualBoundsUnavailable(key))?
                .bbox;
        } else {
            self.frames[fi].hr_bounds.bbox = MeshBoundVolume::CONSTRUCTOR.bbox;
        }
        let mut child = self.links(key)?.child;
        while let Some(at) = child {
            let state = *self.state(at)?;
            if state.flags & 1 != 0 {
                if state.flags & 0x40 == 0 {
                    self.update_hr_bound_vol_at(at, depth + 1)?;
                }
                let child_frame = self.frame(at)?;
                if let Some(error) = &child_frame.hr_bounds_error {
                    return Err(error.clone());
                }
                let bbox = child_frame.hr_bounds.bbox;
                if valid(bbox) {
                    self.graph.update_matrix(at)?;
                    let matrix = self.state(at)?.local;
                    for point in corners(bbox) {
                        let point = matrix
                            .try_transform_point(point)
                            .map_err(|_| FrameError::NumericDomain("hierarchy transform"))?;
                        absorb(&mut self.frames[fi].hr_bounds.bbox, point);
                    }
                }
            }
            child = self.links(at)?.next;
        }
        if valid(self.frames[fi].hr_bounds.bbox) {
            sphere(&mut self.frames[fi].hr_bounds)?;
        } else {
            self.frames[fi].hr_bounds.radius = 0.;
        }
        self.graph.projected_state_mut(key)?.flags |= 0x40;
        Ok(())
    }
    pub fn update_hr_bound_vol_w(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        let result = self.update_hr_bound_vol_w_inner(key);
        self.frames[fi].hr_world_bounds_error = result.as_ref().err().cloned();
        result
    }
    fn update_hr_bound_vol_w_inner(&mut self, key: FrameKey) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        let local = self.hr_bound_volume(key)?;
        self.frames[fi].hr_world_bounds.bbox = MeshBoundVolume::CONSTRUCTOR.bbox;
        if valid(local.bbox) {
            for (index, point) in corners(local.bbox).into_iter().enumerate() {
                let matrix = self.world_matrix(key)?;
                let point = matrix
                    .try_transform_point(point)
                    .map_err(|_| FrameError::NumericDomain("world hierarchy transform"))?;
                if index == 0 {
                    self.frames[fi].hr_world_bounds.bbox = [point, point];
                } else {
                    absorb(&mut self.frames[fi].hr_world_bounds.bbox, point);
                }
            }
            sphere(&mut self.frames[fi].hr_world_bounds)?;
        } else {
            self.frames[fi].hr_world_bounds.center = [0.; 3];
            self.frames[fi].hr_world_bounds.radius = 0.;
        }
        self.graph.projected_state_mut(key)?.flags |= 0x100;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_loading::tests::frame;
    use std::sync::Arc;

    #[test]
    fn real_loaded_mesh_bounds_hierarchy_world_and_last_release_compose() {
        let dir = std::env::temp_dir().join(format!(
            "lhr-bound-resources-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        let mut mesh = frame(1, None, [3., 4., 5.]);
        mesh.render_flags = 0;
        mesh.logic_flags = 0;
        mesh.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-1., -2., -3.], [1., 2., 3.], [0., 0., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0., 0.],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: lhr_assets::UNASSIGNED_MATERIAL,
                indices: vec![0, 1, 2],
            }],
        });
        let source = Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![mesh],
        });
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let model = arena.create_model().unwrap();
        arena.scene_add_frame(scene, model.root()).unwrap();
        arena
            .link_frame(
                model.root(),
                Some(arena.scene(scene).unwrap().sectors[0].frame()),
                0,
            )
            .unwrap();
        let loaded = arena.load_parsed_into_model(model, source).unwrap();
        let visual = loaded.frames()[0];
        assert_eq!(
            arena.update_hr_bound_vol(visual),
            Err(ModelError::VisualBoundsUnavailable(visual))
        );
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let local = arena.hr_bound_volume(visual).unwrap();
        assert_eq!(local.bbox, [[-1., -2., -3.], [1., 2., 3.]]);
        let root = arena.hr_bound_volume(model.root()).unwrap();
        assert_eq!(root.bbox, [[2., 2., 2.], [4., 6., 8.]]);
        let world = arena.hr_bound_volume_w(visual).unwrap();
        assert_eq!(world.bbox, root.bbox);
        assert_eq!(world.center, [3., 4., 5.]);
        assert_eq!(world.radius.to_bits(), 14_f32.sqrt().to_bits());
        arena.retain(visual).unwrap();
        arena.close(model).unwrap();
        assert_eq!(arena.frame(visual).unwrap().references, 1);
        assert!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend()
                .live_graphics()
                > 0
        );
        arena.release(visual).unwrap();
        assert!(arena.frame(visual).is_err());
        // Native per-object teardown frees blocks; manager arrays await reclaim.
        assert!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend()
                .live_graphics()
                > 0
        );
        arena.reclaim_unused_visual_buffers().unwrap();
        let backend = arena.loaded_visual_resources().unwrap().backend();
        assert_eq!(backend.live_graphics(), 0);
        assert_eq!(backend.live_allocations(), 0);
        assert_eq!(backend.live_textures(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn set_mesh_recopy_lock_null_and_last_release_preserve_native_cache_order() {
        let dir = std::env::temp_dir().join(format!(
            "lhr-set-mesh-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        let mut mesh = frame(1, None, [0.; 3]);
        mesh.render_flags = 0;
        mesh.logic_flags = 0;
        mesh.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-2., -3., -4.], [2., 3., 4.], [0.; 3]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: lhr_assets::UNASSIGNED_MATERIAL,
                indices: vec![0, 1, 2],
            }],
        });
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let loaded = arena
            .load_parsed_into_model(
                model,
                Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames: vec![mesh],
                }),
            )
            .unwrap();
        let visual = loaded.frames()[0];
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let object = arena.frame_object_resources(visual).unwrap().unwrap();
        let mesh = arena
            .loaded_visual_resources()
            .unwrap()
            .object_mesh(object)
            .unwrap();
        let bounds = arena.hr_bound_volume(visual).unwrap();
        let flags = arena.state(visual).unwrap().flags;
        let index = arena.frame_index(visual).unwrap();
        arena.frames[index].visual_bounds = Some(MeshBoundVolume::CONSTRUCTOR);
        arena
            .graph
            .projected_state_mut(visual)
            .unwrap()
            .visual_flags |= 0x40;
        arena.set_frame_mesh(visual, mesh, false).unwrap();
        assert_eq!(
            arena.frames[index].visual_bounds,
            Some(MeshBoundVolume::CONSTRUCTOR)
        );
        arena
            .graph
            .projected_state_mut(visual)
            .unwrap()
            .visual_flags &= !0x40;
        arena.set_frame_mesh(visual, mesh, false).unwrap();
        assert_eq!(arena.frames[index].visual_bounds, Some(bounds));
        arena.set_frame_mesh(visual, None, false).unwrap();
        assert_eq!(
            arena.frames[index].visual_bounds,
            Some(MeshBoundVolume::CONSTRUCTOR)
        );
        assert_eq!(arena.state(visual).unwrap().flags, flags);
        assert_eq!(arena.hr_bound_volume(visual).unwrap(), bounds);
        arena.set_frame_mesh(visual, None, true).unwrap();
        arena.update_hr_bound_vol(visual).unwrap();
        assert_eq!(arena.hr_bound_volume(visual).unwrap().radius, 0.);
        arena.close(model).unwrap();
        assert!(arena.frame(visual).is_err());
        arena.reclaim_unused_visual_buffers().unwrap();
        let backend = arena.loaded_visual_resources().unwrap().backend();
        assert_eq!(backend.live_graphics(), 0);
        assert_eq!(backend.live_allocations(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_local_preserves_center_world_clears_center_and_sector_skips_children() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let fi = arena.frame_index(model.root()).unwrap();
        arena.frames[fi].hr_bounds.center = [7., 8., 9.];
        arena.update_hr_bound_vol(model.root()).unwrap();
        assert_eq!(
            arena.hr_bound_volume(model.root()).unwrap().center,
            [7., 8., 9.]
        );
        arena.update_hr_bound_vol_w(model.root()).unwrap();
        let world = arena.hr_bound_volume_w(model.root()).unwrap();
        assert_eq!(world, MeshBoundVolume::CONSTRUCTOR);
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0].frame();
        let before = arena.hierarchy_bounds_snapshot(sector).unwrap();
        arena.link_frame(model.root(), Some(sector), 0).unwrap();
        arena.update_hr_bound_vol(sector).unwrap();
        arena.graph.update(model.root()).unwrap();
        assert_ne!(arena.state(sector).unwrap().flags & 0x40, 0);
        assert_eq!(arena.hierarchy_bounds_snapshot(sector).unwrap(), before);
        assert_eq!(before.bbox, [[-10000.; 3], [10000.; 3]]);
    }
}
