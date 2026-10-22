//! Lazy LS3DF frame caches and the closed model/mesh/dummy/joint hierarchy subset.
//! UpdateWMatrixProc is an original DLL export, RVA0x1c7d0. UpdateMatrix,
//! LinkTo, RemoveChild and Update have retained PS2 symbols; PC entries are
//! 0x1a6e0,0x1b9d0,0x1bf20,0x1ca60. Setter names are descriptive views of
//! observed inline operations, not additional original symbols. Host storage
//! does not implement native reference counting/destructors. Only LinkTo flags0/1,
//! model/mesh/dummy/joint virtual updates, and an empty deferred queue are represented.
use crate::{
    joint_state::{I3dJointState, SinglemeshInvalidationState},
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::{SMatrix, SQuat},
};
const DIRTY: u32 = 0x40002000;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameKey(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    Model,
    Mesh,
    Dummy,
    Joint,
    /// Only an explicit Joint::UpdateV invalidation target. Its own virtual
    /// update requires the unavailable singlemesh backend.
    Singlemesh,
}
/// Fields observed by these methods, not a complete native object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct I3dFrameState {
    pub kind: FrameKind,
    pub local: SMatrix,
    pub world: SMatrix,
    pub scale: [f32; 3],
    pub rotation: SQuat,
    pub flags: u32,
    pub world_scale: f32,
    pub update_count: u32,
    pub visual_flags: u32,
    /// Native dummy-only field at +0x198; independent of mesh visual flags.
    pub dummy_flags: u32,
}
impl I3dFrameState {
    /// Actual model/base-frame constructor field projection.
    pub fn model() -> Self {
        Self {
            kind: FrameKind::Model,
            local: SMatrix::identity(),
            world: SMatrix::identity(),
            scale: [1.; 3],
            rotation: SQuat {
                wxyz: [1., 0., 0., 0.],
            },
            flags: 0x4000003d,
            world_scale: 1.,
            update_count: 0,
            visual_flags: 0,
            dummy_flags: 0,
        }
    }
    /// Original standard Object/LitObject common constructor projection,
    /// RVAs0x367b0/0x21570. Mesh payload and visual subtype remain loader-owned.
    pub fn mesh() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Mesh;
        state.visual_flags = 0x2a00;
        state
    }
    /// Original dummy constructor projection, RVA0x1a1e0. Bounds and other
    /// derived payload fields are owned by the loader arena, outside this graph.
    pub fn dummy() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Dummy;
        state
    }
    /// Common base-frame projection of the PC Joint constructor, RVA0x1ed30.
    /// The graph separately owns the derived joint fields.
    pub fn joint() -> Self {
        let mut state = Self::model();
        state.kind = FrameKind::Joint;
        state
    }
    /// Supported FrameLoadImage result. Caller separately proves mesh type/flags.
    pub fn source_mesh(position: [f32; 3], scale: [f32; 3], rotation: SQuat) -> Self {
        let mut s = Self::model();
        s.kind = FrameKind::Mesh;
        s.local.elements[12..15].copy_from_slice(&position);
        s.scale = scale;
        s.rotation = rotation;
        s.flags = 0x40000009;
        s.visual_flags = 0x2a00;
        s
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameLinks {
    pub parent: Option<FrameKey>,
    pub child: Option<FrameKey>,
    pub previous: Option<FrameKey>,
    pub next: Option<FrameKey>,
    pub child_count: u32,
}
#[derive(Clone, Copy, Debug)]
struct Node {
    state: I3dFrameState,
    links: FrameLinks,
    joint: Option<I3dJointState>,
    singlemesh: Option<SinglemeshInvalidationState>,
    // Host availability markers, not original frame fields. An interrupted
    // guarded computation must not later masquerade as a valid native cache.
    local_error: Option<FrameError>,
    world_error: Option<FrameError>,
}
#[derive(Clone, Copy, Debug)]
struct Visit {
    key: FrameKey,
    child: Option<FrameKey>,
    dirty: bool,
}
#[derive(Debug, Default)]
pub struct I3dFrameGraph {
    nodes: Vec<Node>,
    world_path: Vec<FrameKey>,
    visits: Vec<Visit>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    UnknownFrame(FrameKey),
    IdentityOverflow,
    Cycle,
    NumericDomain(&'static str),
    QuaternionFromMatrixUnavailable,
    AllocationOverflow,
    UnsupportedLinkFlags(u32),
    UnsupportedLinkState,
    DirectionFromVectorsUnavailable,
    NotJoint(FrameKey),
    InvalidSinglemeshTarget(FrameKey),
    UnsupportedSinglemeshUpdate(FrameKey),
}
impl I3dFrameGraph {
    pub fn insert(&mut self, state: I3dFrameState) -> Result<FrameKey, FrameError> {
        let id = u32::try_from(self.nodes.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(FrameError::IdentityOverflow)?;
        self.nodes.push(Node {
            state,
            links: FrameLinks::default(),
            joint: (state.kind == FrameKind::Joint).then(I3dJointState::default),
            singlemesh: None,
            local_error: None,
            world_error: None,
        });
        // Retain bounded traversal storage so the scene charges all allocations
        // before ticking or lazily rendering; traversal cannot grow past nodes.
        self.world_path
            .reserve(self.nodes.len().saturating_sub(self.world_path.len()));
        self.visits
            .reserve(self.nodes.len().saturating_sub(self.visits.len()));
        Ok(FrameKey(id))
    }
    fn index(&self, key: FrameKey) -> Result<usize, FrameError> {
        let i = key.0.checked_sub(1).ok_or(FrameError::UnknownFrame(key))? as usize;
        if i >= self.nodes.len() {
            return Err(FrameError::UnknownFrame(key));
        }
        Ok(i)
    }
    fn node(&self, key: FrameKey) -> Result<&Node, FrameError> {
        Ok(&self.nodes[self.index(key)?])
    }
    fn node_mut(&mut self, key: FrameKey) -> Result<&mut Node, FrameError> {
        let i = self.index(key)?;
        Ok(&mut self.nodes[i])
    }
    pub fn state(&self, key: FrameKey) -> Result<&I3dFrameState, FrameError> {
        Ok(&self.node(key)?.state)
    }
    pub fn joint_state(&self, key: FrameKey) -> Result<&I3dJointState, FrameError> {
        self.node(key)?
            .joint
            .as_ref()
            .ok_or(FrameError::NotJoint(key))
    }
    /// Complete derived LoadImage payload; the caller applies common fields
    /// first. Matrix/index bits are retained even outside numeric getter domains.
    pub fn apply_joint_load_image_fields(
        &mut self,
        key: FrameKey,
        bone_matrix: SMatrix,
        index: u32,
    ) -> Result<(), FrameError> {
        self.node_mut(key)?
            .joint
            .as_mut()
            .ok_or(FrameError::NotJoint(key))?
            .apply_load_image_fields(bone_matrix, index);
        Ok(())
    }
    /// Import only a known singlemesh frame/counter projection for linked
    /// Joint::UpdateV. This does not construct or load a native singlemesh.
    pub fn insert_singlemesh_invalidation_target(
        &mut self,
        mut state: I3dFrameState,
        invalidation: SinglemeshInvalidationState,
    ) -> Result<FrameKey, FrameError> {
        state.kind = FrameKind::Singlemesh;
        let key = self.insert(state)?;
        self.node_mut(key)?.singlemesh = Some(invalidation);
        Ok(key)
    }
    pub fn singlemesh_invalidation_state(
        &self,
        key: FrameKey,
    ) -> Result<&SinglemeshInvalidationState, FrameError> {
        self.node(key)?
            .singlemesh
            .as_ref()
            .ok_or(FrameError::InvalidSinglemeshTarget(key))
    }
    /// Assign the observed pointer field using an owned target. Joint discovery,
    /// index arrays, reference ownership and vertex binding are separate work.
    pub fn assign_joint_singlemesh_target(
        &mut self,
        key: FrameKey,
        target: Option<FrameKey>,
    ) -> Result<(), FrameError> {
        self.joint_state(key)?;
        if let Some(target) = target {
            self.singlemesh_invalidation_state(target)?;
        }
        self.node_mut(key)?.joint.as_mut().unwrap().singlemesh = target;
        Ok(())
    }
    pub fn links(&self, key: FrameKey) -> Result<FrameLinks, FrameError> {
        Ok(self.node(key)?.links)
    }
    pub fn storage_bytes(&self) -> Result<usize, FrameError> {
        self.nodes
            .capacity()
            .checked_mul(std::mem::size_of::<Node>())
            .and_then(|n| {
                n.checked_add(
                    self.world_path
                        .capacity()
                        .checked_mul(std::mem::size_of::<FrameKey>())?,
                )
            })
            .and_then(|n| {
                n.checked_add(
                    self.visits
                        .capacity()
                        .checked_mul(std::mem::size_of::<Visit>())?,
                )
            })
            .ok_or(FrameError::AllocationOverflow)
    }
    /// Common field projection of FrameLoadImage, RVA0x1d1c0, after a
    /// complete record has been validated. The loader first constructs and
    /// links the frame, and separately owns raw names/properties. These stores
    /// ignore position lock and preserve quaternion bits without normalization.
    /// Short reads, name-cache effects and original allocation are outside scope.
    pub fn apply_common_load_image_fields(
        &mut self,
        key: FrameKey,
        position: [f32; 3],
        scale: [f32; 3],
        rotation: SQuat,
        raw_logic_flags: u8,
    ) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.local.elements[12..15].copy_from_slice(&position);
        node.state.scale = scale;
        node.state.rotation = rotation;
        node.state.flags = (node.state.flags & 0xffffff00) | u32::from(raw_logic_flags);
        node.state.flags = (node.state.flags & 0xfffffe8b) | 0x40000008;
        // The native mask invalidates both caches. Subsequent getters recheck
        // the complete new SRT rather than retaining a previous host interruption.
        node.local_error = None;
        node.world_error = None;
        Ok(())
    }
    /// Observed LoadHierarchy EndLoad root store, followed by original Update.
    /// This does not implement resource loading or a populated deferred queue.
    pub fn mark_loaded_root(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let state = &mut self.node_mut(key)?.state;
        state.flags = (state.flags & 0xfffffeff) | 0x40000000;
        self.update(key)
    }
    /// Original bound-cache invalidation at RVA0x1cc00, used by model Close.
    /// All represented kinds differ from native sector type5, which stops the
    /// original ancestor walk and is deliberately absent from this graph.
    pub fn discard_hr_bound_vol(&mut self, key: FrameKey) -> Result<(), FrameError> {
        self.node_mut(key)?.state.flags &= 0xfffffebf;
        let mut parent = self.links(key)?.parent;
        while let Some(key) = parent {
            let node = self.node_mut(key)?;
            if node.state.flags & 0x40 == 0 {
                break;
            }
            node.state.flags &= 0xfffffebf;
            parent = node.links.parent;
        }
        Ok(())
    }
    /// Copies even equal values, preserving signed zero. Flag0x80 locks position.
    pub fn assign_position(&mut self, key: FrameKey, p: [f32; 3]) -> Result<(), FrameError> {
        let s = &mut self.node_mut(key)?.state;
        if s.flags & 0x80 == 0 {
            s.local.elements[12..15].copy_from_slice(&p);
            s.flags = (s.flags & 0xfffffedf) | 0x40000000;
        }
        Ok(())
    }
    /// Scale stores precede the guarded matrix-to-quaternion dependency.
    pub fn assign_scale(&mut self, key: FrameKey, scale: [f32; 3]) -> Result<(), FrameError> {
        let node = self.node_mut(key)?;
        node.state.scale = scale;
        if node.state.flags & 8 == 0 {
            match SQuat::try_make_from_matrix(&node.state.local) {
                Ok(rotation) => node.state.rotation = rotation,
                Err(_) => {
                    // Preserve native stores/flags preceding the unsupported
                    // dependency, while keeping interrupted caches unavailable.
                    let error = FrameError::QuaternionFromMatrixUnavailable;
                    node.local_error = Some(error);
                    node.world_error = Some(error);
                    return Err(error);
                }
            }
        }
        node.state.flags = (node.state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// Observed animation assignment: copy, original Normalize, then invalidate.
    /// Availability is checked before Normalize; a later matrix operation must
    /// recheck its resulting quaternion, including tiny normalized components.
    pub fn assign_rotation(
        &mut self,
        key: FrameKey,
        mut rotation: SQuat,
    ) -> Result<(), FrameError> {
        self.index(key)?;
        if !rotation.is_verified_input() {
            return Err(FrameError::NumericDomain("Normalize"));
        }
        rotation.normalize();
        let state = &mut self.node_mut(key)?.state;
        state.rotation = rotation;
        state.flags = (state.flags & 0xfffffecb) | 0x40000008;
        Ok(())
    }
    /// RemoveChild preserves the detached child's world cache, even if now stale.
    pub fn remove_child(&mut self, parent: FrameKey, child: FrameKey) -> Result<(), FrameError> {
        self.index(parent)?;
        let l = self.links(child)?;
        if l.parent != Some(parent) {
            return Ok(());
        }
        let p = self.node_mut(parent)?;
        p.links.child_count = p.links.child_count.wrapping_sub(1);
        if let Some(prev) = l.previous {
            self.node_mut(prev)?.links.next = l.next;
            if let Some(next) = l.next {
                self.node_mut(next)?.links.previous = Some(prev);
            }
        } else if let Some(next) = l.next {
            self.node_mut(parent)?.links.child = Some(next);
            self.node_mut(next)?.links.previous = None;
        } else {
            let p = self.node_mut(parent)?;
            p.links.child = None;
            p.links.child_count = 0;
        }
        let c = self.node_mut(child)?;
        c.links.parent = None;
        c.links.previous = None;
        c.links.next = None;
        self.node_mut(parent)?.state.flags &= 0xfffffebf;
        Ok(())
    }
    /// Native LinkTo flags0: prepend, preserve local pose, invalidate this world
    /// cache. Descendant caches stay unchanged until Update. Cycles are rejected.
    pub fn link_to(&mut self, child: FrameKey, parent: Option<FrameKey>) -> Result<(), FrameError> {
        self.link_to_with_flags(child, parent, 0).map(|_| ())
    }
    /// Original LinkTo, RVA0x1b9d0, for flags0 and world-preserving flags1.
    /// The native return value is zero, including the unchanged-parent path.
    /// Flags1 computes the old world cache before unlinking, then derives the
    /// new local position, scale and direction in native write order. Availability
    /// failures retain preceding link/cache/pose writes and poison interrupted
    /// caches. Unsupported flag2/4 ownership/callback paths are explicit errors.
    pub fn link_to_with_flags(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, FrameError> {
        if flags > 1 {
            return Err(FrameError::UnsupportedLinkFlags(flags));
        }
        let result = self.link_to_inner(child, parent, flags);
        if let Err(
            error @ (FrameError::NumericDomain(_)
            | FrameError::QuaternionFromMatrixUnavailable
            | FrameError::DirectionFromVectorsUnavailable),
        ) = result
        {
            let node = self.node_mut(child)?;
            node.local_error = Some(error);
            node.world_error = Some(error);
        }
        result
    }
    fn link_to_inner(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, FrameError> {
        let old = self.links(child)?.parent;
        if old == parent {
            return Ok(0);
        }
        if flags == 1 && self.state(child)?.flags & 0x1c0000 != 0 {
            return Err(FrameError::UnsupportedLinkState);
        }
        let mut at = parent;
        let mut count = 0;
        while let Some(k) = at {
            if k == child {
                return Err(FrameError::Cycle);
            }
            at = self.links(k)?.parent;
            count += 1;
            if count > self.nodes.len() {
                return Err(FrameError::Cycle);
            }
        }
        if flags & 1 != 0 {
            self.world_matrix(child)?;
        }
        if let Some(old) = old {
            self.remove_child(old, child)?;
        }
        if let Some(p) = parent {
            let first = self.links(p)?.child;
            if let Some(first) = first {
                self.node_mut(first)?.links.previous = Some(child);
            }
            let c = self.node_mut(child)?;
            c.links.parent = Some(p);
            c.links.previous = None;
            c.links.next = first;
            let p = self.node_mut(p)?;
            p.links.child = Some(child);
            p.links.child_count = p.links.child_count.wrapping_add(1);
            p.state.flags &= 0xfffffebf;
        }
        if flags & 1 == 0 {
            let s = &mut self.node_mut(child)?.state;
            s.flags = (s.flags & !0x20) | 0x40000000;
        } else {
            let relative = if let Some(p) = parent {
                let parent_world = self.world_matrix(p)?;
                // Native discards Inverse's false flag and uses its identity
                // fallback for a singular parent.
                let (inverse, _) = parent_world
                    .inverse()
                    .map_err(|_| FrameError::NumericDomain("LinkTo inverse"))?;
                let child_world = self.world_matrix(child)?;
                if !child_world.is_verified_input() || !inverse.is_verified_input() {
                    return Err(FrameError::NumericDomain("LinkTo multiply"));
                }
                let relative = child_world.mul_affine(&inverse);
                if !relative.is_verified_input() {
                    return Err(FrameError::NumericDomain("LinkTo multiply"));
                }
                relative
            } else {
                self.world_matrix(child)?
            };
            self.assign_position(child, relative.elements[12..15].try_into().unwrap())?;
            let scale = relative
                .try_get_scale()
                .map_err(|_| FrameError::NumericDomain("LinkTo GetScale"))?;
            // This setter intentionally derives any missing quaternion from the
            // previous local matrix, before SetDir3 replaces the basis.
            self.assign_scale(child, scale)?;
            self.node_mut(child)?
                .state
                .local
                .try_set_dir3(
                    relative.elements[8..11].try_into().unwrap(),
                    relative.elements[4..7].try_into().unwrap(),
                )
                .map_err(|_| FrameError::DirectionFromVectorsUnavailable)?;
            let node = self.node_mut(child)?;
            node.state.flags = (node.state.flags & 0xfffffed3) | 0x40000010;
            node.local_error = None;
            node.world_error = None;
        }
        Ok(0)
    }
    /// Original PC0x1a6e0. Preserve flag writes before an unavailable dependency.
    pub fn update_matrix(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let result = self.update_matrix_inner(key);
        if let Ok(node) = self.node_mut(key) {
            node.local_error = result.as_ref().err().copied();
        }
        result
    }
    fn update_matrix_inner(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let flags = self.state(key)?.flags;
        if flags & 0x10 == 0 {
            self.node_mut(key)?.state.flags |= 0x14;
            let s = *self.state(key)?;
            self.node_mut(key)?
                .state
                .local
                .try_set_rot3(s.rotation)
                .map_err(|_| FrameError::NumericDomain("SetRot3"))?;
        } else if flags & 4 == 0 {
            if flags & 8 == 0 {
                let rotation = SQuat::try_make_from_matrix(&self.state(key)?.local)
                    .map_err(|_| FrameError::QuaternionFromMatrixUnavailable)?;
                self.node_mut(key)?.state.rotation = rotation;
            }
            self.node_mut(key)?.state.flags |= 0xc;
        } else {
            if let Some(error) = self.node(key)?.local_error {
                return Err(error);
            }
            return Ok(());
        }
        for row in 0..3 {
            for col in 0..3 {
                let i = row * 4 + col;
                let s = self.state(key)?;
                self.node_mut(key)?.state.local.elements[i] =
                    checked_f32_mul(s.local.elements[i], s.scale[row])
                        .map_err(|_| FrameError::NumericDomain("local row scaling"))?;
            }
        }
        Ok(())
    }
    pub fn local_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        self.update_matrix(key)?;
        Ok(self.state(key)?.local)
    }
    /// Observed inline guard. A valid cache skips the exported procedure entirely.
    pub fn world_matrix(&mut self, key: FrameKey) -> Result<SMatrix, FrameError> {
        if self.state(key)?.flags & 0x20 == 0 {
            self.update_w_matrix_proc(key)?;
        } else if let Some(error) = self.node(key)?.world_error {
            return Err(error);
        }
        Ok(self.state(key)?.world)
    }
    /// Original export0x1c7d0. An explicit stack preserves child-local-before-parent
    /// ordering without a host recursion limit or an invented native update budget.
    pub fn update_w_matrix_proc(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let mut path = std::mem::take(&mut self.world_path);
        path.clear();
        let result = (|| {
            let mut at = key;
            loop {
                self.update_matrix(at)?;
                path.push(at);
                match self.links(at)?.parent {
                    Some(p) if self.state(p)?.flags & 0x20 == 0 => at = p,
                    _ => break,
                }
            }
            for &k in path.iter().rev() {
                let result = self.assemble_world(k);
                self.node_mut(k)?.world_error = result.as_ref().err().copied();
                result?;
            }
            Ok(())
        })();
        self.world_path = path;
        if let Ok(node) = self.node_mut(key) {
            node.world_error = result.as_ref().err().copied();
        }
        result
    }
    fn assemble_world(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let s = *self.state(key)?;
        let parent = self.links(key)?.parent;
        let world = if let Some(p) = parent {
            if let Some(error) = self.node(p)?.world_error {
                return Err(error);
            }
            let right = self.state(p)?.world;
            s.local
                .try_mul_affine(&right)
                .map_err(|_| FrameError::NumericDomain("matrix multiply"))?
        } else {
            s.local
        };
        let unavailable = |_| FrameError::NumericDomain("world scale");
        let zy = checked_f32_add(s.scale[2], s.scale[1]).map_err(unavailable)?;
        // The parent branch adds X before the matrix copy; the root copies first.
        let total = if parent.is_some() {
            Some(checked_f32_add(zy, s.scale[0]).map_err(unavailable)?)
        } else {
            None
        };
        self.node_mut(key)?.state.world = world;
        let mut scale = match total {
            Some(v) => v,
            None => checked_f32_add(zy, s.scale[0]).map_err(unavailable)?,
        };
        if let Some(p) = parent {
            scale = checked_f32_mul(scale, self.state(p)?.world_scale).map_err(unavailable)?;
        }
        scale = checked_f32_mul(scale, f32::from_bits(0x3eaaaaab)).map_err(unavailable)?;
        let s = &mut self.node_mut(key)?.state;
        s.world_scale = scale;
        s.flags |= 0x20;
        Ok(())
    }
    fn enter_update(&mut self, key: FrameKey) -> Result<Visit, FrameError> {
        let n = self.node_mut(key)?;
        // A target-only singlemesh projection cannot enter its native virtual
        // update traversal. Reject before claiming any own Update side effects.
        if n.state.kind == FrameKind::Singlemesh {
            return Err(FrameError::UnsupportedSinglemeshUpdate(key));
        }
        let dirty = n.state.flags & DIRTY != 0;
        if dirty {
            n.state.update_count = n.state.update_count.wrapping_add(1);
            if n.state.kind == FrameKind::Mesh && n.state.visual_flags & 0x40 == 0 {
                n.state.visual_flags &= !0x40000;
            }
        }
        Ok(Visit {
            key,
            child: n.links.child,
            dirty,
        })
    }
    /// Observed UpdateV effects for the represented frame kinds. Dummy's
    /// original entry is RVA0x1a330; it clears its own bound flag only if dirty.
    pub fn update_v(&mut self, key: FrameKey) -> Result<(), FrameError> {
        if self.state(key)?.kind == FrameKind::Singlemesh {
            return Err(FrameError::UnsupportedSinglemeshUpdate(key));
        }
        if self.state(key)?.kind == FrameKind::Joint {
            let node = self.node_mut(key)?;
            let target = node
                .joint
                .as_mut()
                .unwrap()
                .invalidate_for_update(node.state.flags);
            if let Some(target) = target {
                let target_node = self.node_mut(target)?;
                let singlemesh = target_node
                    .singlemesh
                    .as_mut()
                    .ok_or(FrameError::InvalidSinglemeshTarget(target))?;
                singlemesh.joints_changed = singlemesh.joints_changed.wrapping_add(1);
                target_node.state.visual_flags &= !0x60000;
                self.discard_hr_bound_vol(target)?;
                let flags = &mut self.node_mut(target)?.state.flags;
                *flags = (*flags & 0xfffffebf) | 0x40000000;
            }
            return Ok(());
        }
        let s = &mut self.node_mut(key)?.state;
        if s.flags & DIRTY != 0 {
            if s.kind == FrameKind::Mesh && s.visual_flags & 0x40 == 0 {
                s.visual_flags &= !0x1c0000;
            } else if s.kind == FrameKind::Dummy {
                s.dummy_flags &= !0x10;
            }
        }
        Ok(())
    }
    /// Original Update/recursive-child effects for model/standard-mesh/dummy/joint
    /// objects. The deferred list is empty; singlemesh virtual updates fail.
    pub fn update(&mut self, key: FrameKey) -> Result<(), FrameError> {
        let mut visits = std::mem::take(&mut self.visits);
        visits.clear();
        let result = (|| {
            visits.push(self.enter_update(key)?);
            while let Some(v) = visits.last().copied() {
                if let Some(child) = v.child {
                    if v.dirty {
                        let s = &mut self.node_mut(child)?.state;
                        s.flags = (s.flags & 0xfffffedf) | 0x2000;
                    }
                    visits.push(self.enter_update(child)?);
                } else {
                    let done = visits.pop().unwrap();
                    if let Some(p) = visits.last_mut() {
                        self.update_v(done.key)?;
                        if self.state(done.key)?.flags & DIRTY != 0 {
                            self.node_mut(done.key)?.state.flags &= !DIRTY;
                            let s = &mut self.node_mut(p.key)?.state;
                            s.flags = (s.flags & 0xfffffebf) | 0x2000;
                        }
                        p.child = self.links(done.key)?.next;
                    }
                }
            }
            self.update_v(key)?;
            self.node_mut(key)?.state.flags &= 0xfffffebf;
            let mut parent = self.links(key)?.parent;
            while let Some(p) = parent {
                if self.state(p)?.flags & 0x40 == 0 {
                    break;
                }
                self.node_mut(p)?.state.flags &= 0xfffffebf;
                parent = self.links(p)?.parent;
            }
            let s = &mut self.node_mut(key)?.state;
            if s.kind == FrameKind::Mesh && s.flags & DIRTY != 0 && s.visual_flags & 0x40 == 0 {
                s.visual_flags &= !0x40000;
            }
            s.flags &= !DIRTY;
            Ok(())
        })();
        self.visits = visits;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joint_update_invalidates_linked_singlemesh_and_only_cached_ancestors() {
        let mut graph = I3dFrameGraph::default();
        let far = graph.insert(I3dFrameState::model()).unwrap();
        let stop = graph.insert(I3dFrameState::dummy()).unwrap();
        let near = graph.insert(I3dFrameState::dummy()).unwrap();
        let target = graph
            .insert_singlemesh_invalidation_target(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState {
                    joints_changed: u32::MAX,
                },
            )
            .unwrap();
        graph.link_to(stop, Some(far)).unwrap();
        graph.link_to(near, Some(stop)).unwrap();
        graph.link_to(target, Some(near)).unwrap();
        for key in [far, near, target] {
            graph.node_mut(key).unwrap().state.flags = 0x175;
        }
        graph.node_mut(stop).unwrap().state.flags = 0x135;
        graph.node_mut(target).unwrap().state.visual_flags = 0xffffffff;
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(target))
            .unwrap();
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().flags = u32::MAX;
        let own_frame_flags = graph.state(joint).unwrap().flags;
        graph.update_v(joint).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().flags, 0xffffffe3);
        assert_eq!(graph.state(joint).unwrap().flags, own_frame_flags);
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            0
        );
        assert_eq!(graph.state(target).unwrap().visual_flags, 0xfff9ffff);
        assert_eq!(graph.state(target).unwrap().flags, 0x40000035);
        assert_eq!(graph.state(near).unwrap().flags, 0x35);
        assert_eq!(graph.state(stop).unwrap().flags, 0x135);
        assert_eq!(graph.state(far).unwrap().flags, 0x175);
        assert_eq!(
            graph.update_v(target),
            Err(FrameError::UnsupportedSinglemeshUpdate(target))
        );
        assert_eq!(
            graph.update(target),
            Err(FrameError::UnsupportedSinglemeshUpdate(target))
        );
    }

    #[test]
    fn inherited_joint_change_does_not_invalidate_singlemesh_vertices() {
        let mut graph = I3dFrameGraph::default();
        let target = graph
            .insert_singlemesh_invalidation_target(
                I3dFrameState::mesh(),
                SinglemeshInvalidationState { joints_changed: 7 },
            )
            .unwrap();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let joint = graph.insert(I3dFrameState::joint()).unwrap();
        graph.link_to(joint, Some(parent)).unwrap();
        graph
            .assign_joint_singlemesh_target(joint, Some(target))
            .unwrap();
        graph.node_mut(joint).unwrap().state.flags = 0x3d;
        graph.node_mut(joint).unwrap().joint.as_mut().unwrap().flags = 0x1f;
        let target_before = *graph.state(target).unwrap();
        graph.update(parent).unwrap();
        assert_eq!(graph.joint_state(joint).unwrap().flags, 3);
        assert_eq!(graph.state(joint).unwrap().flags & DIRTY, 0);
        assert_eq!(
            graph
                .singlemesh_invalidation_state(target)
                .unwrap()
                .joints_changed,
            7
        );
        assert_eq!(*graph.state(target).unwrap(), target_before);
        assert_eq!(
            graph.assign_joint_singlemesh_target(joint, Some(parent)),
            Err(FrameError::InvalidSinglemeshTarget(parent))
        );
        assert_eq!(graph.joint_state(joint).unwrap().singlemesh, Some(target));
    }

    fn mesh(position: [f32; 3]) -> I3dFrameState {
        I3dFrameState::source_mesh(
            position,
            [1.; 3],
            SQuat {
                wxyz: [1., 0., 0., 0.],
            },
        )
    }
    #[test]
    fn row_scaling_retains_earlier_stores_before_an_unavailable_product() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([0.; 3]);
        state.flags = 0x18;
        state.local.elements[0] = 2.;
        state.local.elements[1] = f32::MIN_POSITIVE;
        state.scale = [0.5, 1., 1.];
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("local row scaling"))
        );
        assert_eq!(graph.state(key).unwrap().local.elements[0], 1.);
        assert_eq!(
            graph.state(key).unwrap().local.elements[1],
            f32::MIN_POSITIVE
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x1c);
        assert_eq!(
            graph.world_matrix(key),
            Err(FrameError::NumericDomain("local row scaling"))
        );
        assert_eq!(graph.state(key).unwrap().local.elements[0], 1.);
    }
    #[test]
    fn small_normal_srt_is_available_but_extended_exponent_products_remain_errors() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut state = mesh([1e-20, 0., 0.]);
        state.scale = [1e-20; 3];
        let child = graph.insert(state).unwrap();
        graph.link_to(child, Some(root)).unwrap();
        let world = graph.world_matrix(child).unwrap();
        assert_eq!(world.elements[0], 1e-20);
        assert_eq!(world.elements[12], 1e-20);
        let mut state = mesh([0.; 3]);
        state.rotation.wxyz = [0.5, 1e-23, 0., 0.];
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        assert_eq!(graph.state(key).unwrap().local, state.local);
        assert_eq!(graph.state(key).unwrap().flags, state.flags | 0x14);
        assert_eq!(
            graph.world_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
    }
    #[test]
    fn common_image_fields_are_raw_stores_after_linking() {
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.link_to(child, Some(parent)).unwrap();
        graph.node_mut(child).unwrap().state.flags |= 0x20000080;
        let position = [f32::from_bits(0x80000000), f32::from_bits(0x7fc01234), 7.];
        let rotation = SQuat {
            wxyz: [2., -3., 4., -0.],
        };
        graph
            .apply_common_load_image_fields(child, position, [-2., 0., 3.], rotation, 0x80)
            .unwrap();
        let state = graph.state(child).unwrap();
        assert_eq!(
            state.local.elements[12..15]
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            position.map(f32::to_bits)
        );
        assert_eq!(state.scale, [-2., 0., 3.]);
        assert_eq!(
            state.rotation.wxyz.map(f32::to_bits),
            rotation.wxyz.map(f32::to_bits)
        );
        assert_eq!(state.flags, 0x60000088);
        assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        assert_eq!(state.world, SMatrix::identity());
    }
    #[test]
    fn dummy_virtual_update_has_its_own_flags_and_clean_update_discards_bounds() {
        let mut graph = I3dFrameGraph::default();
        let mut state = I3dFrameState::dummy();
        state.flags = 0x140;
        state.dummy_flags = 0x31;
        state.visual_flags = u32::MAX;
        let key = graph.insert(state).unwrap();
        graph.update_v(key).unwrap();
        assert_eq!(graph.state(key).unwrap().dummy_flags, 0x31);
        graph.update(key).unwrap();
        assert_eq!(graph.state(key).unwrap().flags, 0);
        assert_eq!(graph.state(key).unwrap().update_count, 0);
        graph.mark_loaded_root(key).unwrap();
        assert_eq!(graph.state(key).unwrap().dummy_flags, 0x21);
        assert_eq!(graph.state(key).unwrap().visual_flags, u32::MAX);
        assert_eq!(graph.state(key).unwrap().update_count, 1);
        assert_eq!(graph.state(key).unwrap().flags, 0);
    }
    #[test]
    fn bound_invalidation_stops_at_first_clean_ancestor() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let middle = graph.insert(I3dFrameState::dummy()).unwrap();
        let leaf = graph.insert(I3dFrameState::mesh()).unwrap();
        graph.link_to(middle, Some(root)).unwrap();
        graph.link_to(leaf, Some(middle)).unwrap();
        graph.node_mut(root).unwrap().state.flags = 0x140;
        graph.node_mut(middle).unwrap().state.flags = 0x100;
        graph.node_mut(leaf).unwrap().state.flags = 0x140;
        graph.discard_hr_bound_vol(leaf).unwrap();
        assert_eq!(graph.state(leaf).unwrap().flags, 0);
        assert_eq!(graph.state(middle).unwrap().flags, 0x100);
        assert_eq!(graph.state(root).unwrap().flags, 0x140);
        graph.node_mut(middle).unwrap().state.flags |= 0x40;
        graph.discard_hr_bound_vol(leaf).unwrap();
        assert_eq!(graph.state(middle).unwrap().flags, 0);
        assert_eq!(graph.state(root).unwrap().flags, 0);
    }
    #[test]
    fn world_preserving_reparent_detach_and_locked_position_follow_native_rules() {
        for locked in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let a = graph.insert(mesh([10., 0., 0.])).unwrap();
            let b = graph.insert(mesh([20., 0., 0.])).unwrap();
            let mut state = mesh([3., 0., 0.]);
            if locked {
                state.flags |= 0x80;
            }
            let child = graph.insert(state).unwrap();
            graph.link_to(child, Some(a)).unwrap();
            assert_eq!(graph.world_matrix(child).unwrap().elements[12], 13.);
            assert_eq!(graph.link_to_with_flags(child, Some(b), 1), Ok(0));
            assert_eq!(graph.links(child).unwrap().parent, Some(b));
            assert_eq!(
                graph.state(child).unwrap().local.elements[12],
                if locked { 3. } else { -7. }
            );
            assert_eq!(
                graph.world_matrix(child).unwrap().elements[12],
                if locked { 23. } else { 13. }
            );
            graph.link_to_with_flags(child, None, 1).unwrap();
            assert_eq!(
                graph.world_matrix(child).unwrap().elements[12],
                if locked { 3. } else { 13. }
            );
        }
    }
    #[test]
    fn relink_errors_preserve_native_prefix_writes_and_mark_caches_unavailable() {
        let mut graph = I3dFrameGraph::default();
        let parent = graph.insert(mesh([10., 0., 0.])).unwrap();
        let mut state = mesh([3., 0., 0.]);
        state.flags = 0x30;
        state.world.elements[12] = 7.;
        state.local.elements[1] = f32::NAN;
        let child = graph.insert(state).unwrap();
        assert_eq!(
            graph.link_to_with_flags(child, Some(parent), 2),
            Err(FrameError::UnsupportedLinkFlags(2))
        );
        assert_eq!(graph.links(child).unwrap().parent, None);
        assert_eq!(
            graph.link_to_with_flags(child, Some(parent), 1),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.links(child).unwrap().parent, Some(parent));
        assert_eq!(graph.state(child).unwrap().local.elements[12], -3.);
        assert_eq!(graph.state(child).unwrap().scale, [1.; 3]);
        assert_eq!(graph.state(child).unwrap().flags, 0x40000010);
        assert_eq!(
            graph.world_matrix(child),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
    }
    #[test]
    fn singular_parent_uses_native_inverse_identity_fallback() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([10., 0., 0.]);
        state.flags = 0x1c;
        for i in [0, 5, 10] {
            state.local.elements[i] = 0.;
        }
        let parent = graph.insert(state).unwrap();
        let child = graph.insert(mesh([3., 0., 0.])).unwrap();
        assert_eq!(graph.link_to_with_flags(child, Some(parent), 1), Ok(0));
        assert_eq!(graph.state(child).unwrap().local.elements[12], 3.);
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 10.);
    }
    #[test]
    fn parent_changes_stay_cached_until_update_then_repeated_reads_are_clean() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let child = graph.insert(mesh([1., 0., 0.])).unwrap();
        graph.link_to(child, Some(root)).unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 1.);
        graph.assign_position(root, [10., 0., 0.]).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 1.);
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(child).unwrap().elements[12], 11.);
        assert_eq!(
            (
                graph.state(root).unwrap().update_count,
                graph.state(child).unwrap().update_count
            ),
            (2, 2)
        );
        let state = *graph.state(child).unwrap();
        graph.world_matrix(child).unwrap();
        assert_eq!(*graph.state(child).unwrap(), state);
    }
    #[test]
    fn native_head_order_and_remove_child_do_not_invent_matrix_invalidation() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let a = graph.insert(mesh([1., 0., 0.])).unwrap();
        let b = graph.insert(mesh([2., 0., 0.])).unwrap();
        graph.link_to(a, Some(root)).unwrap();
        graph.link_to(b, Some(root)).unwrap();
        assert_eq!(graph.links(root).unwrap().child, Some(b));
        assert_eq!(graph.links(b).unwrap().next, Some(a));
        assert_eq!(graph.links(a).unwrap().previous, Some(b));
        graph.assign_position(root, [10., 0., 0.]).unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        graph.remove_child(root, a).unwrap();
        assert_eq!(graph.links(a).unwrap().parent, None);
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        graph.update_w_matrix_proc(a).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 1.);
        graph.link_to(a, Some(root)).unwrap();
        assert_eq!(graph.world_matrix(a).unwrap().elements[12], 11.);
        let root_links = graph.links(root).unwrap();
        assert_eq!(graph.link_to(root, Some(a)), Err(FrameError::Cycle));
        assert_eq!(graph.links(root).unwrap(), root_links);
    }
    #[test]
    fn equal_and_signed_zero_position_writes_invalidate_but_lock_suppresses() {
        let mut graph = I3dFrameGraph::default();
        let key = graph.insert(mesh([0.; 3])).unwrap();
        graph.world_matrix(key).unwrap();
        graph.assign_position(key, [-0., 0., -0.]).unwrap();
        assert_eq!(
            graph.state(key).unwrap().local.elements[12].to_bits(),
            0x80000000
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x20, 0);
        assert_ne!(graph.state(key).unwrap().flags & 0x40000000, 0);
        let mut state = mesh([1., 2., 3.]);
        state.flags |= 0x80;
        let locked = graph.insert(state).unwrap();
        graph.assign_position(locked, [9.; 3]).unwrap();
        assert_eq!(*graph.state(locked).unwrap(), state);
    }
    #[test]
    fn admitted_matrix_rotation_is_stored_before_cache_flags_and_scaling() {
        for setter in [false, true] {
            let mut graph = I3dFrameGraph::default();
            let mut state = mesh([10., 20., 30.]);
            state.flags = 0x10;
            state.local.elements[0] = 2.;
            state.local.elements[5] = 0.5;
            state.local.elements[10] = -4.;
            state.rotation.wxyz = [0., 1., 0., 0.];
            state.scale = [3., 4., 5.];
            let key = graph.insert(state).unwrap();
            if setter {
                graph.assign_scale(key, [3., 4., 5.]).unwrap();
                assert_eq!(graph.state(key).unwrap().flags, 0x40000008);
                assert_eq!(graph.state(key).unwrap().local, state.local);
            }
            graph.update_matrix(key).unwrap();
            let got = *graph.state(key).unwrap();
            assert_eq!(
                got.rotation.wxyz.map(f32::to_bits),
                [1.0f32.to_bits(), 0, 0, 0]
            );
            let expected_diagonal = if setter { [3., 4., 5.] } else { [6., 2., -20.] };
            assert_eq!(
                [
                    got.local.elements[0],
                    got.local.elements[5],
                    got.local.elements[10]
                ],
                expected_diagonal
            );
            assert_eq!(&got.local.elements[12..15], &[10., 20., 30.]);
            assert_eq!(got.flags, if setter { 0x4000001c } else { 0x1c });
            graph.update_matrix(key).unwrap();
            assert_eq!(*graph.state(key).unwrap(), got);
        }
    }

    #[test]
    fn missing_matrix_quaternion_keeps_prior_scale_store_and_flag_order() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([0.; 3]);
        state.flags = 0x10;
        state.local.elements[0] = 0.0;
        let key = graph.insert(state).unwrap();
        assert_eq!(
            graph.assign_scale(key, [2., 3., 4.]),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
        assert_eq!(graph.state(key).unwrap().flags, 0x10);
        assert_eq!(
            graph.update_matrix(key),
            Err(FrameError::QuaternionFromMatrixUnavailable)
        );
        assert_eq!(graph.state(key).unwrap().flags, 0x10);
        for flags in [0x30, 0x34] {
            let mut cached = mesh([0.; 3]);
            cached.flags = flags;
            cached.local.elements[0] = 0.0;
            let key = graph.insert(cached).unwrap();
            assert!(graph.world_matrix(key).is_ok());
            assert_eq!(
                graph.assign_scale(key, [2., 3., 4.]),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            assert_eq!(graph.state(key).unwrap().flags, flags);
            assert_eq!(graph.state(key).unwrap().scale, [2., 3., 4.]);
            assert_eq!(
                graph.world_matrix(key),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            assert_eq!(
                graph.local_matrix(key),
                Err(FrameError::QuaternionFromMatrixUnavailable)
            );
            graph
                .assign_rotation(
                    key,
                    SQuat {
                        wxyz: [1., 0., 0., 0.],
                    },
                )
                .unwrap();
            assert!(graph.world_matrix(key).is_ok());
        }
        let mut bad = mesh([0.; 3]);
        bad.rotation.wxyz[0] = f32::NAN;
        let key = graph.insert(bad).unwrap();
        assert_eq!(
            graph.update_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        assert_eq!(graph.state(key).unwrap().flags & 0x14, 0x14);
        assert_eq!(
            graph.local_matrix(key),
            Err(FrameError::NumericDomain("SetRot3"))
        );
        graph
            .assign_rotation(
                key,
                SQuat {
                    wxyz: [1., 0., 0., 0.],
                },
            )
            .unwrap();
        assert!(graph.local_matrix(key).is_ok());
        let mut state = I3dFrameState::model();
        state.scale = [f32::MAX; 3];
        let huge = graph.insert(state).unwrap();
        assert!(graph.update_w_matrix_proc(huge).is_err());
        assert!(graph.world_matrix(huge).is_err());
        graph.assign_scale(huge, [1.; 3]).unwrap();
        assert!(graph.world_matrix(huge).is_ok());
    }
    #[test]
    fn nonunit_source_quaternion_and_world_scale_follow_native_order() {
        let mut graph = I3dFrameGraph::default();
        let mut state = mesh([1., 2., 3.]);
        state.rotation.wxyz = [1.25, 2., 3., 4.];
        state.scale = [100_000_000., -100_000_000., 1.];
        state.local.elements[3] = 7.;
        let key = graph.insert(state).unwrap();
        graph.update_w_matrix_proc(key).unwrap();
        let got = graph.state(key).unwrap();
        assert_eq!(got.local.elements[0], 100_000_000.);
        assert_eq!(got.local.elements[5], -100_000_000.);
        assert_eq!(got.local.elements[10], 1.);
        assert_eq!(got.local.elements[3], 7.);
        assert_eq!(got.world.elements[12..15], [1., 2., 3.]);
        assert_eq!(got.world_scale.to_bits(), 0);
    }
    #[test]
    fn deep_valid_tree_uses_precharged_traversal_storage_without_recursion_budget() {
        let mut graph = I3dFrameGraph::default();
        let root = graph.insert(I3dFrameState::model()).unwrap();
        let mut last = root;
        for _ in 0..512 {
            let key = graph.insert(mesh([1., 0., 0.])).unwrap();
            graph.link_to(key, Some(last)).unwrap();
            last = key;
        }
        let bytes = graph.storage_bytes().unwrap();
        graph.update(root).unwrap();
        assert_eq!(graph.world_matrix(last).unwrap().elements[12], 512.);
        assert_eq!(graph.storage_bytes().unwrap(), bytes);
    }
}
