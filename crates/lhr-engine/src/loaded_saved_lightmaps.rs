//! Actual object-associated saved-lighting intake. The object owns constructed
//! arrays independently of its shared mesh; all objects in this resource owner
//! share the same atlas remaining-room list.
use super::*;
use crate::i3d_object::saved_lightmaps::SavedLightingError;
use crate::i3d_object::saved_lightmaps::{read_saved_lighting, SavedLightMeshLod, SavedLitLod};

/// Explicit CPU/GPU destination profile. This selects true-color conversion;
/// native 16-bit/dither and compressed texture branches are not implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavedLightmapLoadContext {
    TrueColorRgba8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedLightmapLoadOutcome {
    pub consumed: usize,
    pub native_result: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedLightmapLoadFailure {
    pub consumed: usize,
    pub error: LoadedVisualError,
}
fn native_result(error: SavedLightingError) -> Option<i32> {
    Some(match error {
        SavedLightingError::MissingMesh
        | SavedLightingError::Version(_)
        | SavedLightingError::LodCount
        | SavedLightingError::FaceGroupCount => 0x80000007u32 as i32,
        SavedLightingError::Truncated
        | SavedLightingError::FaceCount
        | SavedLightingError::VertexCount
        | SavedLightingError::RgbSingleSize => 0x8000000au32 as i32,
        // A bad rectangle is explicitly checked by postProcess. Vertex and
        // index checks are host safety bounds around unchecked native accesses.
        SavedLightingError::VertexRectangleIndex => 0x80000005u32 as i32,
        _ => return None,
    })
}

impl LoadedVisualResources {
    pub fn set_active_lighting(&mut self, object: ObjectResourceKey, level: usize) -> Result<()> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        self.objects
            .set_active_lighting(object, level, &mut self.backend)
            .map_err(Into::into)
    }
    /// I3D_lit_object::Load's source-derived resource boundary. The scene loader
    /// supplies the current LMAP frame's payload, never a parallel parser owner.
    /// Completed allocations are retained on a later malformed input error.
    pub fn load_saved_lightmaps(
        &mut self,
        object: ObjectResourceKey,
        bytes: &[u8],
        _context: SavedLightmapLoadContext,
    ) -> Result<usize> {
        let outcome = self
            .load_saved_lightmaps_attempt(object, bytes, _context)
            .map_err(|f| f.error)?;
        match outcome.1 {
            None => Ok(outcome.0),
            Some(e) => Err(LoadedVisualError::SavedLighting(e)),
        }
    }
    /// Exact native negative outcomes remain normal returned statuses; host
    /// safety/provider interruptions are errors with their consumed prefix.
    pub fn load_saved_lightmaps_native(
        &mut self,
        object: ObjectResourceKey,
        bytes: &[u8],
        context: SavedLightmapLoadContext,
    ) -> std::result::Result<SavedLightmapLoadOutcome, SavedLightmapLoadFailure> {
        let (consumed, error) = self.load_saved_lightmaps_attempt(object, bytes, context)?;
        match error {
            None => Ok(SavedLightmapLoadOutcome {
                consumed,
                native_result: 0,
            }),
            Some(error) => match native_result(error) {
                Some(native_result) => Ok(SavedLightmapLoadOutcome {
                    consumed,
                    native_result,
                }),
                None => Err(SavedLightmapLoadFailure {
                    consumed,
                    error: LoadedVisualError::SavedLighting(error),
                }),
            },
        }
    }
    fn load_saved_lightmaps_attempt(
        &mut self,
        object: ObjectResourceKey,
        bytes: &[u8],
        _context: SavedLightmapLoadContext,
    ) -> std::result::Result<(usize, Option<SavedLightingError>), SavedLightmapLoadFailure> {
        let mut consumed = 0;
        let result = (|| -> Result<(usize, Option<SavedLightingError>)> {
            if self.incomplete {
                return Err(LoadedVisualError::IncompleteAdmission);
            }
            // Native Load invokes Destruct before even checking its mesh pointer.
            self.objects
                .destruct_active_lighting(object, &mut self.backend)?;
            let Some(mesh) = self.objects.object(object)?.mesh else {
                return Ok((0, Some(SavedLightingError::MissingMesh)));
            };
            let resources = self.objects.managed().resources();
            let mesh = resources
                .mesh_object(mesh)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            if mesh.num_lods > 10 {
                return Err(LoadedVisualError::InvalidGeometry);
            }
            let mut shape = Vec::new();
            shape
                .try_reserve_exact(mesh.num_lods as usize)
                .map_err(|_| LoadedVisualError::Backend("saved lighting shape allocation"))?;
            for level in &mesh.levels[..mesh.num_lods as usize] {
                let key = level.ok_or(LoadedVisualError::InvalidGeometry)?;
                let level = resources
                    .mesh_level(key)
                    .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
                let mut groups = Vec::new();
                groups
                    .try_reserve_exact(level.num_fgroups as usize)
                    .map_err(|_| LoadedVisualError::Backend("saved lighting group allocation"))?;
                let mut next = level.fgroups;
                while let Some(group) = next {
                    if groups.len() >= level.num_fgroups as usize {
                        return Err(LoadedVisualError::InvalidGeometry);
                    }
                    let group = resources
                        .face_group(group)
                        .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
                    groups.push(group.num_faces);
                    next = group.next;
                }
                if groups.len() != level.num_fgroups as usize {
                    return Err(LoadedVisualError::InvalidGeometry);
                }
                shape.push(SavedLightMeshLod {
                    vertex_count: level.num_vertices,
                    face_group_faces: groups,
                });
            }
            match read_saved_lighting(bytes, &shape, &mut self.lightmap_atlases) {
                Ok(loaded) => {
                    consumed = loaded.bytes_read;
                    self.objects.install_saved_lighting(object, loaded)?;
                    Ok((consumed, None))
                }
                Err(failure) => {
                    consumed = failure.partial.bytes_read;
                    self.objects
                        .install_saved_lighting(object, failure.partial)?;
                    Ok((consumed, Some(failure.error)))
                }
            }
        })();
        result.map_err(|error| SavedLightmapLoadFailure { consumed, error })
    }
    pub fn saved_lit_lod(
        &self,
        object: ObjectResourceKey,
        lod: u32,
    ) -> Result<Option<&SavedLitLod>> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        if self.objects.object(object)?.lit_object().is_none() {
            return Ok(None);
        }
        self.objects
            .saved_lit_lod(object, lod as usize)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload() -> Vec<u8> {
        let mut b = vec![1, 0x21, 2];
        b.extend(1u32.to_le_bytes());
        b.extend(5f32.to_le_bytes());
        b.extend(45f32.to_le_bytes());
        b.push(1);
        b.extend(0u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.push(1);
        b.extend(0xff123456u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        for f in [0f32, 0., 1., 0., 0., 1.] {
            b.extend(f.to_le_bytes());
        }
        for v in 0u16..3 {
            b.extend(v.to_le_bytes());
            b.extend(0u16.to_le_bytes());
        }
        b.extend(3u32.to_le_bytes());
        for i in 0u16..3 {
            b.extend(i.to_le_bytes());
        }
        b.extend(1u32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b
    }
    fn fixture() -> (LoadedVisualResources, ObjectResourceKey, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "lhr-saved-lm-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.visual = Some(1);
        frame.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: vec![
                lhr_assets::Vertex {
                    position: [0.; 3],
                    normal: [0.; 3],
                    uv: [0.; 2]
                };
                3
            ],
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        });
        let source = lhr_assets::Model {
            frames: vec![frame],
            materials: vec![lhr_assets::Material {
                flags: 0,
                ambient: [0.; 3],
                diffuse: [1.; 3],
                texture: String::new(),
                emission: [0.; 3],
                opacity: 1.,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        };
        let mut owner = LoadedVisualResources::new().unwrap();
        let object = owner
            .load_unconstructed_lit_object(&source, 0, &installation)
            .unwrap();
        (owner, object, path)
    }
    #[test]
    fn actual_resource_load_retires_old_lod_and_releases_constructed_arrays_before_mesh() {
        let (mut owner, object, path) = fixture();
        let old = owner
            .objects
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0]
            .lods[0]
            .unwrap();
        let b = payload();
        assert_eq!(
            owner
                .load_saved_lightmaps(object, &b, SavedLightmapLoadContext::TrueColorRgba8)
                .unwrap(),
            b.len()
        );
        assert!(owner.objects.lod(old).is_err());
        let SavedLitLod::Lightmap(lod) = owner.saved_lit_lod(object, 0).unwrap().unwrap() else {
            panic!()
        };
        let atlas = std::sync::Arc::downgrade(&lod.atlas);
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        // Native remaining-room pointer is weak; last litLod releases the map.
        assert_eq!(atlas.strong_count(), 0);
        drop(owner);
        assert_eq!(atlas.strong_count(), 0);
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn actual_resource_failed_load_keeps_completed_prefix_and_empty_active_destruct_clears_it() {
        let (mut owner, object, path) = fixture();
        let mut b = payload();
        b[0] = 3;
        b.extend([0x21, 2]);
        b.extend(1u32.to_le_bytes());
        b.extend(5f32.to_le_bytes());
        b.extend(45f32.to_le_bytes());
        b.push(2);
        assert!(matches!(
            owner.load_saved_lightmaps(object, &b, SavedLightmapLoadContext::TrueColorRgba8),
            Err(LoadedVisualError::SavedLighting(
                crate::i3d_object::saved_lightmaps::SavedLightingError::Truncated
            ))
        ));
        assert!(
            owner
                .objects
                .object(object)
                .unwrap()
                .lit_object()
                .unwrap()
                .levels[0]
                .constructed
        );
        assert!(owner.saved_lit_lod(object, 0).unwrap().is_some());
        owner
            .load_saved_lightmaps(object, &[0], SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap();
        assert!(owner.saved_lit_lod(object, 0).unwrap().is_none());
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn native_failures_and_host_interruptions_keep_distinct_status_and_stream_prefix() {
        let (mut owner, object, path) = fixture();
        let mut b = payload();
        b[1] = 0x20;
        let outcome = owner
            .load_saved_lightmaps_native(object, &b, SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap();
        assert_eq!(
            (outcome.consumed, outcome.native_result),
            (16, 0x80000007u32 as i32)
        );
        let mut b = payload();
        b[3..7].copy_from_slice(&u32::MAX.to_le_bytes());
        let outcome = owner
            .load_saved_lightmaps_native(object, &b, SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap();
        assert_eq!(
            (outcome.consumed, outcome.native_result),
            (16, 0x80000007u32 as i32)
        );
        let mut b = payload();
        b[2] = 5;
        b[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        let outcome = owner
            .load_saved_lightmaps_native(object, &b, SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap();
        assert_eq!(
            (outcome.consumed, outcome.native_result),
            (22, 0x8000000au32 as i32)
        );
        let mut b = payload();
        b[15] = 0;
        let failure = owner
            .load_saved_lightmaps_native(object, &b, SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap_err();
        assert_eq!(failure.consumed, 16);
        assert_eq!(
            failure.error,
            LoadedVisualError::SavedLighting(SavedLightingError::InvalidLevel(0))
        );
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn active_lighting_frees_real_old_buffers_and_reallocation_uses_fresh_binding() {
        let (mut owner, object, path) = fixture();
        let key = owner
            .objects
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0]
            .lods[0]
            .unwrap();
        owner.set_active_lighting(object, 0).unwrap();
        assert_eq!(
            owner.objects.lod(key).unwrap().vertex_buffer,
            crate::visual_resources::CompleteVertexBuffer::default()
        );
        assert!(owner.objects.lod(key).unwrap().dirty);
        assert!(owner
            .objects
            .allocate_plain_lod_vertices(
                key,
                VertexRequest {
                    fvf: 0x242,
                    usage: 0x10,
                    stride: 32,
                    count: 3,
                    format: 1,
                    exclusive: false
                },
                &mut owner.backend
            )
            .unwrap());
        assert_eq!(
            owner
                .objects
                .lod_bindings()
                .iter()
                .filter(|b| b.lod == key)
                .count(),
            2
        );
        owner.set_active_lighting(object, 1).unwrap();
        assert!(owner
            .objects
            .lod_bindings()
            .iter()
            .filter(|b| b.lod == key)
            .all(|b| b.state == crate::managed_visual_resources::BindingState::Consumed));
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        std::fs::remove_dir(path).unwrap();
    }
    #[test]
    fn constructed_lod_common_state_and_maps_survive_active_level_changes() {
        let (mut owner, object, path) = fixture();
        let mut b = payload();
        b[0] = 3;
        let mut second = b[1..].to_vec();
        second[14] = 2;
        b.extend(second);
        owner
            .load_saved_lightmaps(object, &b, SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap();
        let common = owner
            .objects
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0]
            .lods[0]
            .unwrap();
        let SavedLitLod::Lightmap(lod) = owner.saved_lit_lod(object, 0).unwrap().unwrap() else {
            panic!()
        };
        let atlas = std::sync::Arc::downgrade(&lod.atlas);
        assert_eq!(owner.objects.lod(common).unwrap().update_count, 0);
        owner.set_active_lighting(object, 1).unwrap();
        assert!(owner.objects.lod(common).unwrap().dirty);
        assert_eq!(owner.objects.lod(common).unwrap().lights_changed, u32::MAX);
        assert!(owner.saved_lit_lod(object, 0).unwrap().is_some());
        assert_eq!(atlas.strong_count(), 2);
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        assert_eq!(atlas.strong_count(), 0);
        std::fs::remove_dir(path).unwrap();
    }
}
