//! Bounded pristine source-model cache copy; not general DuplicateHierarchy.
//! PC Game wrapper 0x5c23a0 invokes LS3DF DuplicateHierarchy 0x33e10 then
//! cbNullData enumeration. Base Duplicate 0x1c5b0 supplies copied common state.
//! Success is admitted CPU ownership only; unsupported/native failure domains
//! remain host errors, and the returned zero is not a recovered copy HRESULT.
use super::*;
impl ModelArena {
    /// Preserve the destination root while copying each supported source visual
    /// through its own derived owner. Runtime-mutated frames remain unavailable.
    pub(crate) fn duplicate_pristine_cache_model(
        &mut self,
        destination: ModelKey,
        source: ModelKey,
    ) -> Result<u32, ModelError> {
        let unavailable =
            || ModelError::OriginalModelOpenUnavailable("non-pristine cache hierarchy");
        if destination == source {
            return Err(unavailable());
        }
        let source_model = self.model(source)?;
        if source_model.closing || source_model.original_ipol_set.is_some() {
            return Err(unavailable());
        }
        self.model(destination)?;
        let mut source_frames = source_model.frames.clone();
        source_frames.sort_by_key(|&key| self.frame(key).ok().and_then(|f| f.source_index));
        let first = *source_frames.first().ok_or_else(unavailable)?;
        let backing = self
            .frame(first)?
            .source
            .as_ref()
            .cloned()
            .ok_or_else(unavailable)?;
        if source_frames.len() != backing.frames.len() {
            return Err(unavailable());
        }
        for (i, &key) in source_frames.iter().enumerate() {
            let frame = self.frame(key)?;
            let state = self.state(key)?;
            let original = &backing.frames[i];
            if frame.source_index != Some(i)
                || !frame
                    .source
                    .as_ref()
                    .is_some_and(|s| Arc::ptr_eq(s, &backing))
                || !matches!(
                    (original.kind, original.visual),
                    (1, Some(0 | 2 | 3 | 4 | 5 | 6)) | (6, None) | (10, None) | (7, None)
                )
                || (original.visual == Some(4)) != frame.billboard.is_some()
                || frame.original_name_bytes.as_deref() != Some(original.name_bytes.as_slice())
                || frame.properties != original.properties
                || frame.dummy_bounds != original.dummy_bounds
                || !matches!(
                    (original.kind, state.kind),
                    (1, FrameKind::Mesh)
                        | (1, FrameKind::Singlemesh)
                        | (1, FrameKind::Lensflare)
                        | (6, FrameKind::Dummy)
                        | (10, FrameKind::Joint)
                        | (7, FrameKind::Target)
                )
                || (original.kind == 1 && original.visual != Some(6)
                    && (frame.object_resources.is_none()
                        || (state.visual_flags
                            & if matches!(original.visual, Some(2 | 3)) {
                                !0x20000 // checked initial singlemesh bound cache
                            } else {
                                u32::MAX
                            })
                            != u32::from(u16::from_le_bytes([
                                original.render_flags,
                                original.logic_flags,
                            ]))))
                || (original.visual == Some(6)
                    && (frame.lensflare_release_cursor != 0
                        || frame.lensflare_materials.as_ref().map(Vec::len)
                            != original.lensflare.as_ref().map(|record| record.glows.len())))
                || state.local.elements[12..15]
                    .iter()
                    .zip(original.position)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
                || state
                    .scale
                    .iter()
                    .zip(original.scale)
                    .any(|(a, b)| a.to_bits() != b.to_bits())
                // Initial target evaluation can rotate joints while the
                // singlemesh pose is prepared. DuplicateHierarchy copies the
                // live joint rotation, not the raw LoadImage quaternion.
                || (state.kind != FrameKind::Joint
                    && state
                        .rotation
                        .wxyz
                        .iter()
                        .zip(original.rotation_wxyz)
                        .any(|(a, b)| a.to_bits() != b.to_bits()))
                || self.links(key)?.parent
                    != original
                        .parent
                        .map(|p| source_frames[p])
                        .or(Some(source.root()))
            {
                return Err(unavailable());
            }
        }
        // Native duplicateChildren visits live child chains depth first.
        let mut traversal = Vec::new();
        let mut pending = Vec::new();
        if let Some(child) = self.links(source.root())?.child {
            pending.push(child);
        }
        while let Some(key) = pending.pop() {
            if traversal.len() >= source_frames.len() {
                return Err(unavailable());
            }
            traversal.push(key);
            let links = self.links(key)?;
            if let Some(next) = links.next {
                pending.push(next);
            }
            if let Some(child) = links.child {
                pending.push(child);
            }
        }
        if traversal.len() != source_frames.len()
            || traversal.iter().any(|key| !source_frames.contains(key))
        {
            return Err(unavailable());
        }
        self.close(destination)?;
        self.copy_model_filename_reference(destination, source)?;
        let fast_find = self.fast_tree.enabled;
        self.fast_tree.enabled = false;
        // Native DuplTargetList stores each copied member and its live first
        // Target owner, in source DFS order, for post-SetName relinking.
        let mut deferred_target_pairs = Vec::new();
        let result: Result<(), ModelError> = (|| {
            // Reuse frame intake without LoadHierarchy's EndLoad root flag store:
            // DuplicateHierarchy preserves the destination root and calls Update.
            let mut content = SharedModelContent {
                model: destination,
                frames: Vec::new(),
                transferred_frames: 0,
                arena: self.lookup_owner.reference(0),
                epoch: self.lifecycle_epoch(destination)?,
            };
            let mut poses = Vec::new();
            for (index, &source_frame) in source_frames.iter().enumerate() {
                let billboard = self.frame(source_frame)?.billboard.and_then(|b| b.source);
                if let Err(error) = self.load_parsed_frame(
                    &backing,
                    &mut content.frames,
                    &mut poses,
                    index,
                    billboard,
                ) {
                    self.transfer_shared_model_content(&mut content)?;
                    return Err(error);
                }
            }
            self.transfer_shared_model_content(&mut content)?;
            for (&from, &to) in source_frames.iter().zip(content.frames()) {
                let src = self.frame_index(from)?;
                let dst = self.frame_index(to)?;
                let state = *self.state(from)?;
                let copied = self.graph.projected_state_mut(to)?;
                copied.local = state.local;
                copied.scale = state.scale;
                copied.rotation = state.rotation;
                copied.flags = (state.flags & 0x0fa7_3e9f) | 0x4000_0000;
                copied.update_count = 1;
                // Billboard::Duplicate copies the live derived orientation,
                // after the common Frame::Duplicate state projection.
                self.frames[dst].billboard = self.frames[src].billboard;
                self.frames[dst].morph = self.frames[src]
                    .morph
                    .as_ref()
                    .map(|m| m.duplicate())
                    .transpose()?;
                if state.kind == FrameKind::Target {
                    let live_flags = self.graph.target_state(from)?.flags();
                    self.graph.copy_target_duplicate_flags(to, live_flags)?;
                }

                if let Some(original) = self.frames[src].object_resources {
                    let subtype = backing.frames[self.frames[src].source_index.unwrap()].visual;
                    let object = if matches!(subtype, Some(2 | 3)) {
                        self.visual_resource_owner_mut()?
                            .load_singlemesh_instance(original)
                    } else if subtype == Some(5) {
                        self.visual_resource_owner_mut()?
                            .load_morph_instance(original)
                    } else if subtype == Some(4) {
                        self.visual_resource_owner_mut()?
                            .load_billboard_instance(original)
                    } else {
                        self.visual_resource_owner_mut()?
                            .load_plain_instance(original)
                    }
                    .map_err(ModelError::LoadedVisual)?;
                    self.frames[dst].object_resources = Some(object);
                    self.frames[dst].visual_bounds = self.frames[src].visual_bounds;
                    self.graph.discard_hr_bound_vol(to)?;
                }
                if let Some(materials) = self.frames[src].lensflare_materials.as_ref().cloned() {
                    let mut copied = Vec::new();
                    if copied.try_reserve(materials.len()).is_err() {
                        self.visual_resource_owner_mut()?
                            .mark_incomplete_admission();
                        return Err(ModelError::AllocationOverflow);
                    }
                    self.frames[dst].lensflare_materials = Some(copied);
                    let source_index = self.frames[src].source_index.unwrap();
                    let glows = &backing.frames[source_index]
                        .lensflare
                        .as_ref()
                        .ok_or(ModelError::VisualBoundsUnavailable(from))?
                        .glows;
                    for (glow, material) in glows.iter().zip(materials) {
                        let key = self
                            .visual_resource_owner_mut()?
                            .duplicate_lensflare_glow_material(&backing, glow.material, material)
                            .map_err(ModelError::LoadedVisual)?;
                        self.frames[dst]
                            .lensflare_materials
                            .as_mut()
                            .unwrap()
                            .push(key);
                    }
                }
            }
            // DuplicateHierarchy queues source frames with TARGET once each,
            // using GetTarget's first live owner rather than parsed reference
            // words. This also preserves any live AddToTarget reassignment.
            for &from in &traversal {
                if self.state(from)?.flags & 0x4000 != 0 {
                    let source_owner = self
                        .graph
                        .first_target_owner(from)?
                        .ok_or_else(unavailable)?;
                    let index = source_frames
                        .iter()
                        .position(|&key| key == from)
                        .ok_or_else(unavailable)?;
                    deferred_target_pairs
                        .try_reserve(1)
                        .map_err(|_| ModelError::AllocationOverflow)?;
                    deferred_target_pairs.push((source_owner, content.frames()[index]));
                }
            }
            // Reproduce native DFS AddFrame and prepend AddChild order, while
            // retaining original backing indices for resource/name provenance.
            for &key in content.frames() {
                self.link_frame(key, None, 0)?;
            }
            let mut ordered = Vec::new();
            for &from in &traversal {
                let index = source_frames.iter().position(|&key| key == from).unwrap();
                let to = content.frames()[index];
                let parent = self.links(from)?.parent.unwrap();
                let parent = if parent == source.root() {
                    destination.root()
                } else {
                    let index = source_frames.iter().position(|&key| key == parent).unwrap();
                    content.frames()[index]
                };
                self.link_frame(to, Some(parent), 0)?;
                ordered.push(to);
            }
            let mi = self.model_index(destination)?;
            self.models[mi].frames = ordered;
            if backing
                .frames
                .iter()
                .any(|frame| matches!(frame.visual, Some(2 | 3)))
            {
                let pose =
                    SinglemeshPose::build(&backing, content.frames(), &mut self.graph, &mut poses)
                        .map_err(ModelError::SkinPose)?;
                self.models[mi].skin_pose = Some(pose);
                self.models[mi].morph_poses = poses;
            }
            Ok(())
        })();
        self.fast_tree.enabled = fast_find;
        result?;
        self.propagate_model_name(destination)?;
        // Native DuplicateHierarchy calls SetName before resolving the queued
        // source Target by original name in the destination's live frame list.
        // AddToTarget then removes any earlier owner and appends just once per
        // copied member, including when LoadImage held duplicate raw links.
        for (source_owner, member) in deferred_target_pairs {
            let original = self
                .frame(source_owner)?
                .original_name_bytes
                .as_ref()
                .cloned()
                .ok_or_else(unavailable)?;
            let mut target = None;
            for &candidate in &self.model(destination)?.frames {
                if self.frame(candidate)?.original_name_bytes.as_deref()
                    == Some(original.as_slice())
                {
                    target = Some(candidate);
                    break;
                }
            }
            if let Some(target) = target {
                self.graph.add_duplicate_target_member(target, member)?;
            }
        }
        self.update_frame(destination.root())?;
        // Cache wrapper cbNullData visits descendants, not the destination root.
        for key in self.model(destination)?.frames.clone() {
            self.set_user_data(key, crate::actor_frames::FrameUserData::Null)?;
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_source_frame_is_copied_with_live_native_flag_mask() {
        let mut arena = ModelArena::default();
        let src = arena.create_model().unwrap();
        let destination = arena.create_model().unwrap();
        let child = arena
            .load_parsed_into_model(src, source())
            .unwrap()
            .frames()[0];
        arena.graph.projected_state_mut(child).unwrap().flags = 0x8;
        arena
            .duplicate_pristine_cache_model(destination, src)
            .unwrap();
        let copied = arena.model(destination).unwrap().frames[0];
        // Duplicate writes the update bit, then its final Update consumes it.
        assert_eq!(arena.state(copied).unwrap().flags, 0x8);
        arena.close(destination).unwrap();
        arena.close(src).unwrap();
    }
    fn source() -> Arc<lhr_assets::Model> {
        Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![super::super::tests::frame(6, None, [0.; 3])],
        })
    }
    #[test]
    fn duplicated_target_remap_restores_linked_joint_flag_before_close() {
        let mut joint = super::super::tests::frame(10, None, [0.; 3]);
        joint.joint = Some(lhr_assets::JointRecord {
            bone_matrix: SMatrix::identity().elements,
            index: 0,
        });
        joint.name = "joint".into();
        joint.name_bytes = b"joint".to_vec();
        let mut target = super::super::tests::frame(7, None, [1., 0., 0.]);
        target.name = "targetA".into();
        target.name_bytes = b"targetA".to_vec();
        target.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![1],
        });
        let source = Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![joint, target],
        });
        let mut arena = ModelArena::default();
        let src = arena.create_model().unwrap();
        let dst = arena.create_model().unwrap();
        arena.load_parsed_into_model(src, source).unwrap();
        arena.duplicate_pristine_cache_model(dst, src).unwrap();
        let joint = arena
            .model(dst)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(0))
            .unwrap();
        let target = arena
            .model(dst)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(1))
            .unwrap();
        assert_eq!(arena.graph.target_state(target).unwrap().frames(), &[joint]);
        assert_ne!(arena.state(joint).unwrap().flags & 0x4000, 0);
        arena.close(src).unwrap();
        arena.close(dst).unwrap();
    }
    #[test]
    fn duplicate_raw_target_links_collapse_to_one_live_member() {
        let mut joint = super::super::tests::frame(10, None, [0.; 3]);
        joint.name = "joint".into();
        joint.name_bytes = b"joint".to_vec();
        joint.joint = Some(lhr_assets::JointRecord {
            bone_matrix: SMatrix::identity().elements,
            index: 0,
        });
        let mut target = super::super::tests::frame(7, None, [1., 0., 0.]);
        target.name = "targetA".into();
        target.name_bytes = b"targetA".to_vec();
        target.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![1, 1],
        });
        let mut arena = ModelArena::default();
        let src = arena.create_model().unwrap();
        let dst = arena.create_model().unwrap();
        arena
            .load_parsed_into_model(
                src,
                Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames: vec![joint, target],
                }),
            )
            .unwrap();
        let src_target = arena
            .model(src)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.state(key).unwrap().kind == FrameKind::Target)
            .unwrap();
        assert_eq!(
            arena.graph.target_state(src_target).unwrap().frames().len(),
            2
        );
        arena.duplicate_pristine_cache_model(dst, src).unwrap();
        let target = arena
            .model(dst)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.state(key).unwrap().kind == FrameKind::Target)
            .unwrap();
        assert_eq!(arena.graph.target_state(target).unwrap().frames().len(), 1);
        arena.close(dst).unwrap();
        // The source retains two raw LoadImage links; its guarded teardown is
        // outside this duplicate test's successful destination domain.
    }
    #[test]
    fn duplicate_uses_live_target_reassignment_instead_of_parsed_links() {
        let mut joint = super::super::tests::frame(10, None, [0.; 3]);
        joint.name = "joint".into();
        joint.name_bytes = b"joint".to_vec();
        joint.joint = Some(lhr_assets::JointRecord {
            bone_matrix: SMatrix::identity().elements,
            index: 0,
        });
        let mut a = super::super::tests::frame(7, None, [1., 0., 0.]);
        a.name = "targetA".into();
        a.name_bytes = b"targetA".to_vec();
        a.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![1],
        });
        let mut b = super::super::tests::frame(7, None, [2., 0., 0.]);
        b.name = "targetB".into();
        b.name_bytes = b"targetB".to_vec();
        b.target = Some(lhr_assets::TargetRecord {
            flags: 0,
            frame_indices: vec![],
        });
        let mut arena = ModelArena::default();
        let src = arena.create_model().unwrap();
        let dst = arena.create_model().unwrap();
        arena
            .load_parsed_into_model(
                src,
                Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames: vec![joint, a, b],
                }),
            )
            .unwrap();
        let source_frames = arena.model(src).unwrap().frames.clone();
        let member = source_frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(0))
            .unwrap();
        let new_owner = source_frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(2))
            .unwrap();
        arena
            .graph
            .add_duplicate_target_member(new_owner, member)
            .unwrap();
        assert_eq!(
            arena.graph.first_target_owner(member).unwrap(),
            Some(new_owner)
        );
        arena.duplicate_pristine_cache_model(dst, src).unwrap();
        let copied = arena.model(dst).unwrap().frames.clone();
        let copied_member = copied
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(0))
            .unwrap();
        let copied_a = copied
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(1))
            .unwrap();
        let copied_b = copied
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(2))
            .unwrap();
        assert!(arena
            .graph
            .target_state(copied_a)
            .unwrap()
            .frames()
            .is_empty());
        assert_eq!(
            arena.graph.target_state(copied_b).unwrap().frames(),
            &[copied_member]
        );
        arena.close(src).unwrap();
        arena.close(dst).unwrap();
    }
    #[test]
    fn duplicated_skin_joint_invalidation_recomputes_owned_bounds_lazily() {
        let directory =
            std::env::temp_dir().join(format!("lhr-cache-skin-bounds-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut arena = ModelArena::default();
        let src = arena.create_model().unwrap();
        let dst = arena.create_model().unwrap();
        arena
            .load_parsed_into_model(src, super::super::tests::skinned_source())
            .unwrap();
        arena
            .load_plain_visual_resources(src, &installation)
            .unwrap();
        arena.duplicate_pristine_cache_model(dst, src).unwrap();
        let visual = arena
            .model(dst)
            .unwrap()
            .frames
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(0))
            .unwrap();
        assert_eq!(arena.state(visual).unwrap().visual_flags & 0x20000, 0);
        assert!(arena
            .graph
            .singlemesh_binding(visual)
            .unwrap()
            .bound_volume
            .is_some());
        let volume = arena.hr_bound_volume(visual).unwrap();
        let actual = arena.singlemesh_bound_volume(visual).unwrap();
        assert_eq!(volume.bbox, [actual.bbox.min, actual.bbox.max]);
        assert_ne!(arena.state(visual).unwrap().visual_flags & 0x20000, 0);
        arena.close(src).unwrap();
        arena.close(dst).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn failed_lazy_pose_keeps_terminal_error_after_bound_bit_is_set() {
        let directory =
            std::env::temp_dir().join(format!("lhr-cache-skin-error-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut arena = ModelArena::default();
        let src = arena.create_model().unwrap();
        let dst = arena.create_model().unwrap();
        arena
            .load_parsed_into_model(src, super::super::tests::skinned_source())
            .unwrap();
        arena
            .load_plain_visual_resources(src, &installation)
            .unwrap();
        arena.duplicate_pristine_cache_model(dst, src).unwrap();
        let keys = arena.model(dst).unwrap().frames.clone();
        let visual = keys
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(0))
            .unwrap();
        let joint = keys
            .iter()
            .copied()
            .find(|&key| arena.frame(key).unwrap().source_index == Some(1))
            .unwrap();
        arena.hr_bound_volume(visual).unwrap();
        let mut changed = arena
            .frame(visual)
            .unwrap()
            .source
            .as_ref()
            .unwrap()
            .clone();
        Arc::make_mut(&mut changed).frames[0]
            .skin
            .as_mut()
            .unwrap()
            .lods[0]
            .definition
            .as_mut()
            .unwrap()
            .root_vertex_count = 2;
        for &key in &keys {
            let fi = arena.frame_index(key).unwrap();
            arena.frames[fi].source = Some(changed.clone());
        }
        arena.graph.assign_position(joint, [5., 0., 0.]).unwrap();
        arena.update_frame(dst.root()).unwrap();
        let first = arena.update_hr_bound_vol(visual).unwrap_err();
        assert_eq!(
            first,
            ModelError::SkinPose(SkinPoseError::Deformation(
                crate::singlemesh_deformation::DeformationUnavailable::VertexCount,
            ))
        );
        // The bounds calculation ran before deformation rejected the changed
        // partition. A valid graph bit cannot replay this interrupted pose.
        assert_ne!(arena.state(visual).unwrap().visual_flags & 0x20000, 0);
        assert_eq!(arena.update_hr_bound_vol(visual), Err(first));
        arena.close(src).unwrap();
        arena.close(dst).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn prefixed_source_keeps_destination_name_filename_and_external_survivor() {
        let mut a = ModelArena::default();
        let src = a.create_model().unwrap();
        let dst = a.create_model().unwrap();
        a.load_parsed_into_model(src, source()).unwrap();
        a.allocate_model_filename(src, b"Models/test.i3d").unwrap();
        a.set_frame_name(src.root(), Some(b"source")).unwrap();
        a.propagate_model_name(src).unwrap();
        a.set_frame_name(dst.root(), Some(b"destination")).unwrap();
        let root_data = crate::actor_frames::FrameUserData::ExternalRecord(123);
        a.set_user_data(dst.root(), root_data).unwrap();
        assert_eq!(a.duplicate_pristine_cache_model(dst, src).unwrap(), 0);
        let child = a.model(dst).unwrap().frames[0];
        assert_eq!(a.frame(child).unwrap().name_bytes, b"destination.fixture");
        assert_eq!(a.user_data(dst.root()).unwrap(), root_data);
        assert_eq!(
            a.user_data(child).unwrap(),
            crate::actor_frames::FrameUserData::Null
        );
        assert_eq!(
            a.model_filename(src).unwrap(),
            a.model_filename(dst).unwrap()
        );
        a.retain(child).unwrap();
        a.close(src).unwrap();
        a.close(dst).unwrap();
        assert!(a.frame(child).is_ok());
        a.release(child).unwrap();
        a.release(src.root()).unwrap();
        a.release(dst.root()).unwrap();
        assert_eq!(a.live_frame_count(), 0);
    }
    #[test]
    fn changed_source_is_rejected_before_destination_close() {
        let mut a = ModelArena::default();
        let src = a.create_model().unwrap();
        let dst = a.create_model().unwrap();
        a.load_parsed_into_model(src, source()).unwrap();
        a.load_parsed_into_model(dst, source()).unwrap();
        let old = a.model(dst).unwrap().frames.clone();
        let key = a.model(src).unwrap().frames[0];
        a.set_frame_name(key, Some(b"changed")).unwrap();
        assert!(a.duplicate_pristine_cache_model(dst, src).is_err());
        assert_eq!(a.model(dst).unwrap().frames, old);
    }
    #[test]
    fn copied_plain_mesh_shares_storage_until_last_owner() {
        let directory = std::env::temp_dir().join(format!("lhr-cache-copy-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::create_dir_all(directory.join("maps")).unwrap();
        let mut bmp = vec![0u8; 58];
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp[54..57].copy_from_slice(&[10, 20, 30]);
        std::fs::write(directory.join("maps/fixture.bmp"), bmp).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut frame = super::super::tests::frame(1, None, [0.; 3]);
        frame.render_flags = 0;
        frame.logic_flags = 0;
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
        let backing = Arc::new(lhr_assets::Model {
            frames: vec![frame],
            materials: vec![lhr_assets::Material {
                flags: 0,
                ambient: [1.; 3],
                diffuse: [1.; 3],
                texture: "fixture.bmp".into(),
                emission: [0.; 3],
                opacity: 1.,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        });
        let mut a = ModelArena::default();
        let src = a.create_model().unwrap();
        let dst = a.create_model().unwrap();
        a.load_parsed_into_model(src, backing).unwrap();
        a.load_plain_visual_resources(src, &installation).unwrap();
        let src_object = a
            .frame_object_resources(a.model(src).unwrap().frames[0])
            .unwrap()
            .unwrap();
        assert_eq!(
            a.loaded_visual_resources()
                .unwrap()
                .backend()
                .live_textures(),
            1
        );
        let before = a
            .loaded_visual_resources()
            .unwrap()
            .backend()
            .graphics_bytes();
        a.duplicate_pristine_cache_model(dst, src).unwrap();
        let dst_object = a
            .frame_object_resources(a.model(dst).unwrap().frames[0])
            .unwrap()
            .unwrap();
        assert_ne!(src_object, dst_object);
        let owner = a.loaded_visual_resources().unwrap();
        assert_eq!(
            owner.object_mesh(src_object).unwrap(),
            owner.object_mesh(dst_object).unwrap()
        );
        assert_eq!(owner.backend().graphics_bytes(), before);
        assert_eq!(owner.backend().live_textures(), 1);
        a.close(src).unwrap();
        assert!(a
            .loaded_visual_resources()
            .unwrap()
            .object_mesh(dst_object)
            .is_ok());
        a.close(dst).unwrap();
        a.reclaim_unused_visual_buffers().unwrap();
        let b = a.loaded_visual_resources().unwrap().backend();
        assert_eq!(
            (b.live_graphics(), b.live_allocations(), b.live_textures()),
            (0, 0, 0)
        );
        a.release(src.root()).unwrap();
        a.release(dst.root()).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn live_depth_first_membership_and_prepend_child_order_are_copied() {
        let mut frames = Vec::new();
        for (name, parent) in [
            (b"one".as_slice(), None),
            (b"two".as_slice(), None),
            (b"nested".as_slice(), Some(0)),
        ] {
            let mut f = super::super::tests::frame(6, parent, [0.; 3]);
            f.name_bytes = name.to_vec();
            f.name = String::from_utf8(name.to_vec()).unwrap();
            frames.push(f);
        }
        let mut a = ModelArena::default();
        let src = a.create_model().unwrap();
        let dst = a.create_model().unwrap();
        a.load_parsed_into_model(
            src,
            Arc::new(lhr_assets::Model {
                frames,
                materials: vec![],
            }),
        )
        .unwrap();
        a.duplicate_pristine_cache_model(dst, src).unwrap();
        let names = a
            .model(dst)
            .unwrap()
            .frames
            .iter()
            .map(|&key| a.frame(key).unwrap().original_name_bytes.clone().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![b"two".to_vec(), b"one".to_vec(), b"nested".to_vec()]
        );
        let first = a.links(dst.root()).unwrap().child.unwrap();
        assert_eq!(a.frame(first).unwrap().name_bytes, b"one");
        let next = a.links(first).unwrap().next.unwrap();
        assert_eq!(a.frame(next).unwrap().name_bytes, b"two");
    }
    #[test]
    fn cache_hit_populates_destination_without_any_asset_file() {
        use crate::mission_resources::{CpuMissionCacheMemory, MissionCacheKind, MissionCaches};
        let directory = std::env::temp_dir().join(format!("lhr-cache-hit-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut a = ModelArena::default();
        let src = a.create_model().unwrap();
        let dst = a.create_model().unwrap();
        let backing = source();
        a.load_parsed_into_model(src, backing.clone()).unwrap();
        a.allocate_model_filename(src, b"Models/nonexistent.i3d")
            .unwrap();
        a.set_frame_name(src.root(), Some(b"cached")).unwrap();
        a.propagate_model_name(src).unwrap();
        a.set_frame_name(dst.root(), Some(b"placed")).unwrap();
        let mut cache = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        cache
            .retain_model(src, b"nonexistent.i3d", &mut a, &mut memory)
            .unwrap();
        assert_eq!(
            cache
                .open_static_model(&mut a, &installation, dst, b"NONEXISTENT.i3d", &mut memory)
                .unwrap(),
            0
        );
        let child = a.model(dst).unwrap().frames[0];
        assert_eq!(a.frame(child).unwrap().name_bytes, b"placed.fixture");
        assert!(Arc::ptr_eq(
            a.frame(child).unwrap().source.as_ref().unwrap(),
            &backing
        ));
        let filename = a.model_filename(src).unwrap().unwrap();
        assert_eq!(a.model_filename(dst).unwrap(), Some(filename));
        assert_eq!(a.filename(filename).unwrap().references(), 2);
        a.release(src.root()).unwrap();
        cache
            .clear(MissionCacheKind::Model, &mut a, &mut memory)
            .unwrap();
        assert!(a.frame(child).is_ok());
        assert_eq!(a.filename(filename).unwrap().references(), 1);
        a.close(dst).unwrap();
        a.release(dst.root()).unwrap();
        // retain_model is an admitted cache-item intake: it owns no native
        // array header. Clear releases every admitted model/string owner.
        assert_eq!(cache.len(MissionCacheKind::Model), 0);
        assert_eq!(a.live_frame_count(), 0);
        assert_eq!(memory.live_allocations(), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
