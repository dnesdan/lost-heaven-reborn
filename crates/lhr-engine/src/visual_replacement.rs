//! Owned object factories and common Duplicate state used by scene2 replacement.
//! Game 543a10..543b50 is an arm of Open541030, not a standalone function.
//! The admitted OBJ_/unconstructed LMAP owners share real mesh references;
//! other implemented-native factory types remain explicit availability bounds.
use crate::frame_fast_lookup::OriginalFrameName;
use crate::frame_transform::{FrameKey, FrameKind, I3dFrameState};
use crate::i3d_object::ObjectVisualType;
use crate::model_loading::{ModelArena, ModelError};

pub const OBJECT_VISUAL: u32 = 0x5f4a424f;
pub const LIT_OBJECT_VISUAL: u32 = 0x50414d4c;

/// Inspectable host continuation at an unavailable replacement effect. Earlier
/// native effects and the creator reference on `new` remain owned by the arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingVisualReplacement {
    pub old: FrameKey,
    pub new: FrameKey,
    pub entry: u32,
}

pub(crate) struct VisualFactoryError {
    pub created: Option<FrameKey>,
    pub error: ModelError,
}
impl From<ModelError> for VisualFactoryError {
    fn from(error: ModelError) -> Self {
        Self {
            created: None,
            error,
        }
    }
}

impl ModelArena {
    pub(crate) fn owned_object_visual_type(&self, key: FrameKey) -> Result<u32, ModelError> {
        if self.state(key)?.kind != FrameKind::Mesh {
            return Err(ModelError::OriginalModelOpenUnavailable(
                "derived visual class witness unavailable",
            ));
        }
        let object = self
            .frame(key)?
            .object_resources
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let class = self
            .loaded_visual_resources()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?
            .objects()
            .object(object)
            .map_err(|_| ModelError::VisualBoundsUnavailable(key))?
            .visual_type();
        match class {
            ObjectVisualType::Plain => self.plain_object_visual_type(key),
            ObjectVisualType::LitUnconstructed => Ok(class.source_fourcc()),
        }
    }

    /// Driver CreateVisual18d80's admitted object constructors. A native-invalid
    /// FourCC really returns null; native-valid unsupported classes cannot be
    /// represented by a null success or a changed class tag.
    pub(crate) fn create_owned_object_visual(
        &mut self,
        fourcc: u32,
    ) -> Result<Option<FrameKey>, VisualFactoryError> {
        let class = match fourcc {
            OBJECT_VISUAL => ObjectVisualType::Plain,
            LIT_OBJECT_VISUAL => ObjectVisualType::LitUnconstructed,
            0x44524242 | 0x4d474e53 | 0x534e454c | 0x4850524d | 0x524d4d53 | 0x4a4f5250
            | 0x42545250 | 0x45545250 | 0x50444e4c | 0x5252494d => {
                return Err(ModelError::OriginalModelOpenUnavailable(
                    "native visual factory class unavailable",
                )
                .into());
            }
            _ => return Ok(None),
        };
        let key = self.insert(I3dFrameState::mesh(), None, Vec::new(), Vec::new(), None)?;
        let object = self
            .visual_resource_owner_mut()
            .and_then(|owner| {
                owner
                    .create_empty_object_visual(class)
                    .map_err(ModelError::LoadedVisual)
            })
            .map_err(|error| VisualFactoryError {
                created: Some(key),
                error,
            })?;
        let fi = self.frame_index(key)?;
        self.frames[fi].object_resources = Some(object);
        self.frames[fi].visual_bounds = Some(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR);
        Ok(Some(key))
    }

    /// Base Duplicate1c5b0, followed by the admitted object Duplicate36b10 or
    /// lit Duplicate275d0 state. Hierarchy, membership and userdata are excluded
    /// by the original Duplicate contract and are handled by its caller.
    pub(crate) fn duplicate_owned_object_visual(
        &mut self,
        destination: FrameKey,
        source: FrameKey,
    ) -> Result<(), ModelError> {
        if destination == source {
            return Err(ModelError::OriginalModelOpenUnavailable(
                "self visual duplicate",
            ));
        }
        let destination_type = self.owned_object_visual_type(destination)?;
        self.owned_object_visual_type(source)?;
        let from = self.frame_index(source)?;
        let to = self.frame_index(destination)?;
        let state = *self.state(source)?;
        if state.flags & 0x1e00 != 0 {
            return Err(ModelError::OriginalModelOpenUnavailable(
                "frame callback copy unavailable",
            ));
        }
        if matches!(
            self.frames[from].name_pointer,
            OriginalFrameName::Released { .. }
        ) {
            return Err(ModelError::UnavailableOriginalName(source));
        }
        let original = self.frames[from].original_name_bytes.clone();
        let pointer = self.frames[from].name_pointer;
        let name = (!self.frames[from].name_is_null).then(|| self.frames[from].name_bytes.clone());
        let property = &self.frames[from].properties;
        let properties = property[..property
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(property.len())]
            .to_vec();
        let retained_flags = self.state(destination)?.flags & (0x400000 | 0x8000 | 0x4000);
        self.remove_frame_name_registration(destination)?;
        {
            let target = self.graph.projected_state_mut(destination)?;
            target.local = state.local;
            target.scale = state.scale;
            target.rotation = state.rotation;
            target.flags = (state.flags & 0x0fa73e9f) | retained_flags | 0x40000000;
            target.update_count = target.update_count.wrapping_add(1);
        }
        self.frames[to].properties = properties;
        self.set_frame_name(destination, name.as_deref())?;
        self.frames[to].original_name_bytes = original;
        self.frames[to].name_pointer = pointer;
        let source_object = self.frames[from].object_resources.unwrap();
        let target_object = self.frames[to].object_resources.unwrap();
        self.visual_resource_owner_mut()?
            .duplicate_object_visual(target_object, source_object)
            .map_err(ModelError::LoadedVisual)?;
        self.graph.projected_state_mut(destination)?.visual_flags = (state.visual_flags & 0x2ffd0)
            | if destination_type == OBJECT_VISUAL {
                0x800
            } else {
                0
            };
        self.frames[to].visual_bounds = self.frames[from].visual_bounds;
        // These immutable host fields retain geometry provenance for actual CPU
        // consumers; they introduce no native allocation or ownership transfer.
        self.frames[to].source = self.frames[from].source.clone();
        self.frames[to].source_index = self.frames[from].source_index;
        if state.visual_flags & 0x40 != 0 {
            return Err(ModelError::OriginalModelOpenUnavailable(
                "world-space visual duplicate bounds unavailable",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::scene_ownership::SceneKey;
    use std::sync::Arc;
    static FIXTURE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

    /// Public synthetic triangle, admitted through the same CPU material/mesh
    /// owners as installation data. No original game assets enter this fixture.
    pub(crate) fn triangle(arena: &mut ModelArena, scene: SceneKey, model_owned: bool) -> FrameKey {
        let model = arena.create_model().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0].frame();
        arena.link_frame(model.root(), Some(primary), 0).unwrap();
        let mut frame = crate::model_loading::tests::frame(1, None, [3., 4., 5.]);
        frame.render_flags = 0;
        frame.logic_flags = 0x2a;
        frame.lods = vec![lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        }];
        let source = Arc::new(lhr_assets::Model {
            frames: vec![frame],
            materials: vec![lhr_assets::Material {
                flags: 0,
                ambient: [1.; 3],
                diffuse: [1.; 3],
                emission: [0.; 3],
                opacity: 1.,
                texture: String::new(),
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        });
        let content = arena.load_parsed_into_model(model, source).unwrap();
        let key = content.frames()[0];
        let dir = std::env::temp_dir().join(format!(
            "lhr-visual-fixture-{}-{}-{}",
            std::process::id(),
            key.0,
            FIXTURE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        std::fs::remove_dir(&dir).unwrap();
        if !model_owned {
            arena.scene_add_frame(scene, key).unwrap();
            arena.link_frame(key, Some(primary), 0).unwrap();
            arena.release(model.root()).unwrap();
        }
        key
    }

    #[test]
    fn duplicate_shares_mesh_but_preserves_distinct_frame_resource_and_userdata_owners() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let old = triangle(&mut arena, scene, false);
            arena.set_frame_model_name(old, b"prefix").unwrap();
            arena
                .set_user_data(old, crate::actor_frames::FrameUserData::ExternalRecord(123))
                .unwrap();
            let new = arena
                .create_owned_object_visual(LIT_OBJECT_VISUAL)
                .unwrap_or_else(|e| panic!("{:?}", e.error))
                .unwrap();
            arena.duplicate_owned_object_visual(new, old).unwrap();
            assert_eq!(arena.frame(new).unwrap().name_bytes, b"prefix.fixture");
            assert_eq!(
                arena.frame(new).unwrap().original_name_bytes.as_deref(),
                Some(b"fixture".as_slice())
            );
            assert_eq!(
                arena.user_data(new).unwrap(),
                crate::actor_frames::FrameUserData::Null
            );
            assert_eq!(arena.frame(new).unwrap().references, 1);
            assert_eq!(arena.frame(new).unwrap().owner, None);
            assert_eq!(arena.links(new).unwrap().parent, None);
            let old_object = arena.frame_object_resources(old).unwrap().unwrap();
            let new_object = arena.frame_object_resources(new).unwrap().unwrap();
            assert_ne!(old_object, new_object);
            let mesh = arena
                .loaded_visual_resources()
                .unwrap()
                .object_mesh(new_object)
                .unwrap()
                .unwrap();
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .object_mesh(old_object)
                    .unwrap(),
                Some(mesh)
            );
            arena.scene_delete_frame(scene, old).unwrap();
            assert!(arena
                .loaded_visual_resources()
                .unwrap()
                .objects()
                .object(old_object)
                .is_err());
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .object_mesh(new_object)
                    .unwrap(),
                Some(mesh)
            );
            arena.release(new).unwrap();
            assert!(arena
                .loaded_visual_resources()
                .unwrap()
                .objects()
                .managed()
                .resources()
                .mesh_object(mesh)
                .is_err());
        });
    }
}
