//! Owned static-world composition for the existing original collision workers.
//!
//! This is a host adapter, not an additional original function. It keeps KLZ
//! membership and order, prepared property words, stored planes and complete
//! source vertex indices. Scene construction, external model expansion and
//! dynamic object registration remain caller dependencies. Arithmetic and
//! unsupported-query limits are those of `collision_query` and its workers.

use crate::{
    collision_dispatch::{
        line_in_tree_level, primitive_from_prepared, static_volume_from_source, CollisionPrimitive,
    },
    collision_links::CollisionLinkSlot,
    collision_prepare::{
        prepare_bound_collision_world, CollisionPreparationError, CollisionWorldPreparationError,
        PreparedCollisionPrimitive, PreparedCollisionWorld,
    },
    collision_query::{
        test_line_v_grid, CollisionLineHit, CollisionLineOutputs, CollisionLineRequest,
        CollisionQueryError, CollisionTraversalState, StaticGridGeometry, StaticQueryGrid,
    },
    collision_volumes::VolumeError,
    collision_world::{
        bind_resolved_collision_world, BoundCollisionVolume, CollisionBindingError,
        CollisionFrameGeometry, ResolvedCollisionWorldError,
    },
    frame_transform::{FrameError, FrameKey, I3dFrameGraph},
};
use lhr_assets::klz::{Klz, PrimitiveId, PrimitiveKind};

const MAX_WORLD_BYTES: usize = 256 * 1024 * 1024;

/// Complete, original-index LOD0 positions of one already constructed instance.
/// A volume owner can omit positions. Names and rendered faces are not used to
/// infer collision membership or manufacture missing model children.
#[derive(Clone, Copy, Debug)]
pub struct SceneCollisionGeometry<'a> {
    pub frame: FrameKey,
    pub lod0_local_vertices: Option<&'a [[f32; 3]]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrivingCollisionWorldError {
    StorageQuotaExceeded,
    AllocationFailed,
    InvalidGrid,
    InvalidReference {
        cell: usize,
        primitive: PrimitiveId,
    },
    IdentityOverflow,
    Projection {
        primitive: PrimitiveId,
        error: VolumeError,
    },
    Binding(ResolvedCollisionWorldError),
    Preparation(CollisionWorldPreparationError),
    Frame {
        frame: FrameKey,
        error: FrameError,
    },
    VertexTransform {
        frame: FrameKey,
        vertex: usize,
    },
}

/// Stable host identity and source fields of the returned original primitive.
/// `properties` includes the admitted native preparation/classification changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrivingCollisionPrimitiveInfo {
    pub identity: u32,
    pub primitive: PrimitiveId,
    pub source_offset: usize,
    pub properties: u32,
    pub unavailable: Option<CollisionPreparationError>,
    /// Owner binding audit, independent of static nonface query availability.
    /// A null volume owner does not remove the stored original static shape.
    pub owner_unavailable: Option<CollisionPreparationError>,
    /// Successful original nonface record+4 frame binding. This is a key into
    /// the owning scene graph, never a canonical heap address or actor+0x44.
    /// Failed/null bindings remain distinguished by `owner_unavailable`.
    pub bound_volume: Option<BoundCollisionVolume>,
}

impl DrivingCollisionPrimitiveInfo {
    /// Original byte at primitive+3, used by the wheel surface lookup.
    pub fn material(self) -> u8 {
        (self.properties >> 24) as u8
    }
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    max_y: f32,
    begin: usize,
    end: usize,
}

/// Owns query data independently of the KLZ and temporary frame vertex storage.
/// Preparation failures stay in candidate order and fail only if reached after
/// the original mask/early-break tests. A query error leaves outputs unchanged.
pub struct DrivingCollisionWorld {
    geometry: StaticGridGeometry,
    cells: Vec<Cell>,
    candidates: Vec<CollisionPrimitive>,
    primitives: Vec<DrivingCollisionPrimitiveInfo>,
    descriptors: Vec<CollisionPrimitive>,
}

fn reserve<T>(values: &mut Vec<T>, count: usize) -> Result<(), DrivingCollisionWorldError> {
    values
        .try_reserve_exact(count)
        .map_err(|_| DrivingCollisionWorldError::AllocationFailed)
}

fn quota(parts: &[(usize, usize)]) -> Result<(), DrivingCollisionWorldError> {
    parts
        .iter()
        .try_fold(0_usize, |total, (count, stride)| {
            total.checked_add(count.checked_mul(*stride)?)
        })
        .filter(|total| *total <= MAX_WORLD_BYTES)
        .ok_or(DrivingCollisionWorldError::StorageQuotaExceeded)?;
    Ok(())
}

impl DrivingCollisionWorld {
    /// The boolean represents actual dynamic-grid presence. Passing true keeps
    /// non-`-1` selectors explicitly unavailable until that worker is supplied.
    pub fn from_prepared(
        prepared: &PreparedCollisionWorld<'_, '_>,
        dynamic_candidates_present: bool,
    ) -> Result<Self, DrivingCollisionWorldError> {
        let source = prepared.source();
        let width = u32::try_from(source.grid.width)
            .map_err(|_| DrivingCollisionWorldError::InvalidGrid)?;
        let height = u32::try_from(source.grid.length)
            .map_err(|_| DrivingCollisionWorldError::InvalidGrid)?;
        if source.grid.width.checked_mul(source.grid.length) != Some(source.cells.len()) {
            return Err(DrivingCollisionWorldError::InvalidGrid);
        }
        let references = source
            .cells
            .iter()
            .try_fold(0_usize, |total, cell| total.checked_add(cell.len()))
            .ok_or(DrivingCollisionWorldError::StorageQuotaExceeded)?;
        quota(&[
            (source.cells.len(), std::mem::size_of::<Cell>()),
            (references, std::mem::size_of::<CollisionPrimitive>()),
            (
                prepared.entries().len(),
                std::mem::size_of::<DrivingCollisionPrimitiveInfo>(),
            ),
            (
                prepared.entries().len(),
                std::mem::size_of::<CollisionPrimitive>(),
            ),
        ])?;
        if prepared.entries().len() > u32::MAX as usize || source.cells.len() > u32::MAX as usize {
            return Err(DrivingCollisionWorldError::IdentityOverflow);
        }
        let mut starts = [0_usize; 6];
        let mut start = 0;
        for (slot, kind) in PrimitiveKind::ALL.into_iter().enumerate() {
            starts[slot] = start;
            start += source.array(kind).len();
        }
        let mut projected = Vec::new();
        let mut primitives = Vec::new();
        reserve(&mut projected, prepared.entries().len())?;
        reserve(&mut primitives, prepared.entries().len())?;
        for (index, entry) in prepared.entries().iter().enumerate() {
            let identity = index as u32 + 1;
            let primitive = PrimitiveId {
                kind: entry.source.kind(),
                index: entry.source.index,
            };
            let owner_unavailable = entry.outcome.err().filter(|error| {
                primitive.kind != PrimitiveKind::Triangle
                    && matches!(error, CollisionPreparationError::Binding(_))
            });
            let candidate = if primitive.kind != PrimitiveKind::Triangle
                && matches!(
                    owner_unavailable,
                    Some(CollisionPreparationError::Binding(
                        CollisionBindingError::MissingFrame { .. }
                    ))
                ) {
                static_volume_from_source(entry.source, identity).map(CollisionPrimitive::Volume)
            } else {
                primitive_from_prepared(entry, identity)
            }
            .map_err(|error| DrivingCollisionWorldError::Projection { primitive, error })?;
            primitives.push(DrivingCollisionPrimitiveInfo {
                identity,
                primitive,
                source_offset: entry.source.offset,
                properties: candidate.properties(),
                unavailable: match candidate {
                    CollisionPrimitive::Unavailable { error, .. } => Some(error),
                    _ => None,
                },
                owner_unavailable,
                bound_volume: match entry.outcome {
                    Ok(PreparedCollisionPrimitive::Volume(volume)) => Some(volume),
                    _ => None,
                },
            });
            projected.push(candidate);
        }
        let mut cells = Vec::new();
        let mut candidates = Vec::new();
        reserve(&mut cells, source.cells.len())?;
        reserve(&mut candidates, references)?;
        for (index, cell) in source.cells.iter().enumerate() {
            let begin = candidates.len();
            for reference in cell.references() {
                let slot = PrimitiveKind::ALL
                    .iter()
                    .position(|kind| *kind == reference.primitive.kind)
                    .expect("validated KLZ kind");
                if reference.primitive.index >= source.array(reference.primitive.kind).len() {
                    return Err(DrivingCollisionWorldError::InvalidReference {
                        cell: index,
                        primitive: reference.primitive,
                    });
                }
                candidates.push(projected[starts[slot] + reference.primitive.index]);
            }
            cells.push(Cell {
                max_y: cell.max_y(),
                begin,
                end: candidates.len(),
            });
        }
        Ok(Self {
            geometry: StaticGridGeometry {
                origin_xz: [source.grid.min_x, source.grid.min_z],
                cell_size_xz: [source.grid.cell_width, source.grid.cell_length],
                width,
                height,
                dynamic_candidates_present,
            },
            cells,
            candidates,
            primitives,
            descriptors: projected,
        })
    }

    /// Connect an existing scene graph to the original bind/prepare/query path.
    /// Applies the already ported checked LS3DF point transform at every source
    /// index, including unused vertices. The frame graph owns lazy world-cache
    /// updates; completed updates remain visible if a later dependency fails.
    pub fn from_resolved_local_geometry(
        source: &Klz<'_>,
        links: &[CollisionLinkSlot<'_>],
        geometry: &[SceneCollisionGeometry<'_>],
        frames: &mut I3dFrameGraph,
        dynamic_candidates_present: bool,
    ) -> Result<Self, DrivingCollisionWorldError> {
        let vertices = geometry
            .iter()
            .try_fold(0_usize, |total, entry| {
                total.checked_add(entry.lod0_local_vertices.map_or(0, <[_]>::len))
            })
            .ok_or(DrivingCollisionWorldError::StorageQuotaExceeded)?;
        quota(&[
            (vertices, std::mem::size_of::<[f32; 3]>()),
            (geometry.len(), std::mem::size_of::<Option<Vec<[f32; 3]>>>()),
            (
                geometry.len(),
                std::mem::size_of::<CollisionFrameGeometry<'_>>(),
            ),
        ])?;
        let mut storage = Vec::new();
        reserve(&mut storage, geometry.len())?;
        for entry in geometry {
            let transformed = if let Some(local) = entry.lod0_local_vertices {
                let matrix = frames.world_matrix(entry.frame).map_err(|error| {
                    DrivingCollisionWorldError::Frame {
                        frame: entry.frame,
                        error,
                    }
                })?;
                let mut world = Vec::new();
                reserve(&mut world, local.len())?;
                for (vertex, position) in local.iter().enumerate() {
                    world.push(matrix.try_transform_point(*position).map_err(|_| {
                        DrivingCollisionWorldError::VertexTransform {
                            frame: entry.frame,
                            vertex,
                        }
                    })?);
                }
                Some(world)
            } else {
                None
            };
            storage.push(transformed);
        }
        let mut world_geometry = Vec::new();
        reserve(&mut world_geometry, geometry.len())?;
        for (entry, storage) in geometry.iter().zip(&storage) {
            world_geometry.push(CollisionFrameGeometry {
                frame: entry.frame,
                lod0_world_vertices: storage.as_deref(),
            });
        }
        let bound = bind_resolved_collision_world(source, links, &world_geometry)
            .map_err(DrivingCollisionWorldError::Binding)?;
        let prepared = prepare_bound_collision_world(&bound)
            .map_err(DrivingCollisionWorldError::Preparation)?;
        Self::from_prepared(&prepared, dynamic_candidates_present)
    }

    pub fn primitives(&self) -> &[DrivingCollisionPrimitiveInfo] {
        &self.primitives
    }

    pub(crate) fn query_geometry(&self) -> StaticGridGeometry {
        self.geometry
    }

    pub fn grid_geometry(&self) -> StaticGridGeometry {
        self.geometry
    }

    /// Read-only original cell membership and order for inspectable captures.
    /// Primitive descriptors contain owned values, never process pointers.
    pub fn cell_candidates(&self, index: usize) -> Option<(f32, u32, &[CollisionPrimitive])> {
        self.cells.get(index)?;
        Some(self.query_cell_candidates(index))
    }

    pub(crate) fn query_cell_candidates(&self, index: usize) -> (f32, u32, &[CollisionPrimitive]) {
        let cell = self.cells[index];
        (
            cell.max_y,
            index as u32 + 1,
            &self.candidates[cell.begin..cell.end],
        )
    }

    /// The same composition for frames retained by the shared model/scene
    /// arena. It uses that arena's live graph; a second transform hierarchy is
    /// never constructed from model names or rendered positions.
    pub fn from_resolved_arena_geometry(
        source: &Klz<'_>,
        links: &[CollisionLinkSlot<'_>],
        geometry: &[SceneCollisionGeometry<'_>],
        arena: &mut crate::model_loading::ModelArena,
        dynamic_candidates_present: bool,
    ) -> Result<Self, DrivingCollisionWorldError> {
        Self::from_resolved_local_geometry(
            source,
            links,
            geometry,
            &mut arena.graph,
            dynamic_candidates_present,
        )
    }

    pub fn primitive(&self, identity: u32) -> Option<&DrivingCollisionPrimitiveInfo> {
        self.primitives.get(identity.checked_sub(1)? as usize)
    }

    /// Owned query descriptor for an opaque primitive identity. Geometry here
    /// is independent of host vehicle memory and original process addresses.
    pub fn primitive_descriptor(&self, identity: u32) -> Option<CollisionPrimitive> {
        self.descriptors
            .get(identity.checked_sub(1)? as usize)
            .copied()
    }

    /// Original GetFrameCollisions5ca5c0 array scan order. Triangles have
    /// vertex pointers at +4 and do not participate in this frame-owner scan.
    pub fn frame_primitives(&self, frame: FrameKey) -> Vec<u32> {
        let mut result = Vec::new();
        for kind in [
            PrimitiveKind::XtObb,
            PrimitiveKind::Aabb,
            PrimitiveKind::Sphere,
            PrimitiveKind::Obb,
            PrimitiveKind::Cylinder,
        ] {
            result.extend(
                self.primitives
                    .iter()
                    .filter(|entry| {
                        entry.primitive.kind == kind
                            && entry.bound_volume.is_some_and(|owner| owner.frame == frame)
                    })
                    .map(|entry| entry.identity),
            );
        }
        result
    }

    /// The ModelCollisionsOFF430bb0 reached property store. Synchronize each
    /// borrowed-cell projection of the same original descriptor identity.
    pub fn disable_frame_collisions(&mut self, frame: FrameKey) -> Vec<u32> {
        let identities = self.frame_primitives(frame);
        for identity in &identities {
            self.primitives[*identity as usize - 1].properties |= 0x20;
            for descriptor in std::iter::once(&mut self.descriptors[*identity as usize - 1]).chain(
                self.candidates
                    .iter_mut()
                    .filter(|candidate| match candidate {
                        CollisionPrimitive::Triangle(value) => value.identity == *identity,
                        CollisionPrimitive::Volume(value) => value.identity == *identity,
                        CollisionPrimitive::Unavailable {
                            identity: value, ..
                        } => *value == *identity,
                    }),
            ) {
                match descriptor {
                    CollisionPrimitive::Triangle(value) => value.properties |= 0x20,
                    CollisionPrimitive::Volume(value) => value.properties |= 0x20,
                    CollisionPrimitive::Unavailable { properties, .. } => *properties |= 0x20,
                }
            }
        }
        identities
    }

    pub fn test_line_v(
        &self,
        request: &CollisionLineRequest,
        outputs: &mut CollisionLineOutputs,
        traversal: &mut CollisionTraversalState,
    ) -> Result<Option<CollisionLineHit>, CollisionQueryError> {
        test_line_v_grid(Some(self), request, outputs, traversal)
    }

    /// Original body sphere arguments retain the two native trailing words:
    /// the first selects dynamic registration, the second rejects properties.
    /// Callers retain the reached point/normal outputs and the original
    /// collision-manager traversal storage across coarse and fine passes.
    pub fn body_sphere(
        &self,
        request: &crate::vehicle_dynamics_body_collision::VehicleBodySphereSweep,
        outputs: &mut CollisionLineOutputs,
        traversal: &mut crate::collision_swept_world::SphereTraversalState,
    ) -> Result<
        Option<crate::collision_swept_world::SphereSweepHit>,
        crate::collision_swept::SphereSweepError,
    > {
        crate::collision_swept_world::test_sphere_b(
            Some(self),
            &crate::collision_swept::SphereSweepRequest {
                origin: request.start,
                delta: request.displacement,
                radius: request.radius,
                selector: request.mask as i32,
                reject_mask: request.flags,
            },
            outputs,
            traversal,
        )
    }

    /// Original body box predicate. Bounds are decoded from the caller-owned
    /// collision volume record, independently of the opaque KLZ descriptors.
    /// The original worker returns a predicate, without a contact descriptor.
    pub fn body_volume(
        &self,
        request: &crate::vehicle_dynamics_body_collision::VehicleBodyVolumeSweep,
        bounds: [[f32; 3]; 2],
    ) -> Result<bool, crate::collision_swept_box::BoxSweepError> {
        use crate::collision_swept_box::{test_d_box, BoxDynamicQuery, BoxSweepRequest};
        test_d_box(
            Some(self),
            if self.geometry.dynamic_candidates_present {
                BoxDynamicQuery::Unavailable
            } else {
                BoxDynamicQuery::Absent
            },
            &BoxSweepRequest {
                bounds,
                first_matrix: request.previous,
                second_matrix: request.current,
                vectors: [request.displacement, request.auxiliary, request.up],
                dynamic_mask: request.mask,
                static_mask: request.flags,
            },
        )
    }
}

impl StaticQueryGrid for DrivingCollisionWorld {
    fn geometry(&self) -> StaticGridGeometry {
        self.geometry
    }
    fn cell_count(&self) -> usize {
        self.cells.len()
    }
    fn cell(&self, index: usize) -> (f32, u32) {
        (self.cells[index].max_y, index as u32 + 1)
    }
    fn query_cell(
        &self,
        index: usize,
        query: &mut crate::collision_line::LineQuery,
    ) -> Result<(), CollisionQueryError> {
        let cell = self.cells[index];
        line_in_tree_level(&self.candidates[cell.begin..cell.end], query).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision_links::CollisionLinkWord,
        collision_world::{bind_collision_world, CollisionFrameBinding},
        frame_transform::I3dFrameState,
        native_transform::SQuat,
    };

    fn put(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture(pool: bool) -> Vec<u8> {
        let mut bytes = vec![0; 36 + 104];
        bytes[..4].copy_from_slice(b"GifC");
        for (offset, value) in [(4, 5), (8, 36), (12, 1), (24, 28), (28, 1)] {
            put(&mut bytes, offset, value);
        }
        bytes[32] = b'f';
        for (index, value) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            put(&mut bytes, 36 + index * 4, value.to_bits());
        }
        for (offset, count) in [
            (24, 1),
            (28, 1),
            (0x30, 2),
            (0x60, if pool { 3 } else { 0 }),
        ] {
            put(&mut bytes, 36 + offset, count);
        }
        for value in [0_f32, 10., 0., 10.] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 4]);
        for index in 0..2 {
            let start = bytes.len();
            bytes.resize(start + 32, 0);
            put(&mut bytes, start, ((index + 7) << 24) | 0x10000);
            for (corner, vertex) in [3, 1, 2].into_iter().enumerate() {
                let vertex = if pool && vertex == 3 { 0 } else { vertex };
                put(
                    &mut bytes,
                    start + 4 + corner * 4,
                    (if pool { 0x7fff0000 } else { 0 }) | vertex,
                );
            }
            for (component, value) in [0_f32, -1., 0., 0.].into_iter().enumerate() {
                put(&mut bytes, start + 16 + component * 4, value.to_bits());
            }
        }
        if pool {
            for position in [[0_f32, 0., 0.], [0., 0., 2.], [2., 0., 0.]] {
                for value in position {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        bytes.extend_from_slice(&[0; 4]);
        let cell = bytes.len();
        bytes.resize(cell + 16, 0);
        put(&mut bytes, cell, 3);
        put(&mut bytes, cell + 12, 10_f32.to_bits());
        for reference in [32_u32, 0, 32] {
            bytes.extend_from_slice(&reference.to_le_bytes());
        }
        bytes.extend_from_slice(&[11, 22, 33, 0]);
        bytes
    }

    fn request() -> CollisionLineRequest {
        CollisionLineRequest {
            origin: [0.5, 1., 0.5],
            delta: [0., -2., 0.],
            selector: -1,
            reject_mask: 0,
        }
    }

    #[test]
    fn null_volume_owner_retains_real_static_shape_for_line_and_sweep() {
        let mut bytes = vec![0; 36 + 104];
        bytes[..4].copy_from_slice(b"GifC");
        for (offset, value) in [(4, 5), (8, 36), (12, 1), (24, 28), (28, 1)] {
            put(&mut bytes, offset, value);
        }
        bytes[32] = b'f';
        for (index, value) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            put(&mut bytes, 36 + index * 4, value.to_bits());
        }
        for offset in [24, 28, 0x40] {
            put(&mut bytes, 36 + offset, 1);
        }
        for value in [0_f32, 10., 0., 10.] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 4]);
        let shape = bytes.len();
        bytes.resize(shape + 32, 0);
        put(&mut bytes, shape, 0x0900_0081);
        for (axis, value) in [0_f32, 0., 0., 2., 2., 2.].into_iter().enumerate() {
            put(&mut bytes, shape + 8 + axis * 4, value.to_bits());
        }
        bytes.extend_from_slice(&[0; 4]);
        let cell = bytes.len();
        bytes.resize(cell + 28, 0);
        put(&mut bytes, cell, 2);
        put(&mut bytes, cell + 12, 10_f32.to_bits());
        put(&mut bytes, cell + 16, 0x8100_0000);
        put(&mut bytes, cell + 20, 0x8100_0000);
        let source = Klz::parse(&bytes).unwrap();
        let bound = bind_collision_world(&source, &[]).unwrap();
        assert_eq!(bound.failures().count(), 1);
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        let world = DrivingCollisionWorld::from_prepared(&prepared, false).unwrap();
        let info = world.primitive(1).unwrap();
        assert!(info.owner_unavailable.is_some());
        assert_eq!(info.unavailable, None);
        assert_eq!(info.material(), 9);
        let request = CollisionLineRequest {
            origin: [1., 3., 1.],
            delta: [0., -4., 0.],
            selector: -1,
            reject_mask: 0,
        };
        let mut output = outputs();
        let hit = world
            .test_line_v(
                &request,
                &mut output,
                &mut CollisionTraversalState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            hit,
            CollisionLineHit {
                primitive: 1,
                fraction: 0.25
            }
        );
        assert_eq!(output.point, Some([1., 2., 1.]));
        let request = crate::collision_swept::SphereSweepRequest {
            origin: request.origin,
            delta: request.delta,
            radius: 0.25,
            selector: -1,
            reject_mask: 0,
        };
        assert!(crate::collision_swept_world::test_sphere_b(
            Some(&world),
            &request,
            &mut output,
            &mut Default::default()
        )
        .unwrap()
        .is_some());
        let body_sphere = crate::vehicle_dynamics_body_collision::VehicleBodySphereSweep {
            record: 123,
            start: request.origin,
            displacement: request.delta,
            radius: request.radius,
            mask: u32::MAX,
            flags: 0,
        };
        assert!(world
            .body_sphere(&body_sphere, &mut output, &mut Default::default())
            .unwrap()
            .is_some());
        let mut body_box = crate::vehicle_dynamics_body_collision::VehicleBodyVolumeSweep {
            record: 123,
            volume: 456,
            previous: crate::native_transform::SMatrix::identity(),
            current: crate::native_transform::SMatrix::identity(),
            displacement: [0.; 3],
            auxiliary: [0.; 3],
            up: [0., 1., 0.],
            mask: u32::MAX,
            flags: 0,
        };
        let bounds = [[0.5; 3], [1.5; 3]];
        assert_eq!(world.body_volume(&body_box, bounds), Ok(true));
        body_box.flags = u32::MAX;
        assert_eq!(world.body_volume(&body_box, bounds), Ok(false));

        let owner = FrameKey(456);
        let bound = bind_collision_world(
            &source,
            &[CollisionFrameBinding {
                link_index: 0,
                frame: owner,
                lod0_world_vertices: None,
            }],
        )
        .unwrap();
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        let mut owned = DrivingCollisionWorld::from_prepared(&prepared, false).unwrap();
        assert_eq!(
            owned.primitive(1).unwrap().bound_volume,
            Some(crate::collision_world::BoundCollisionVolume {
                link_index: 0,
                frame: owner,
            })
        );
        assert_eq!(owned.frame_primitives(FrameKey(999)), Vec::<u32>::new());
        assert_eq!(owned.disable_frame_collisions(owner), vec![1]);
        assert_eq!(owned.primitive(1).unwrap().properties, 0x0900_00a1);
        assert_eq!(
            owned.primitive_descriptor(1).unwrap().properties(),
            0x0900_00a1
        );
        assert_eq!(owned.cell_candidates(0).unwrap().2.len(), 2);
        assert!(owned
            .cell_candidates(0)
            .unwrap()
            .2
            .iter()
            .all(|entry| entry.properties() == 0x0900_00a1));
        assert_eq!(
            owned.test_line_v(
                &CollisionLineRequest {
                    origin: [1., 3., 1.],
                    delta: [0., -4., 0.],
                    selector: -1,
                    reject_mask: 0,
                },
                &mut outputs(),
                &mut Default::default()
            ),
            Ok(None)
        );
    }
    fn outputs() -> CollisionLineOutputs {
        CollisionLineOutputs {
            point: Some([99.; 3]),
            normal: Some([88.; 3]),
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    enum BackendError {
        Missing,
        Query(CollisionQueryError),
    }
    struct Backend;
    impl crate::vehicle_init::VehicleInitBackend for Backend {
        type Error = BackendError;
        fn read_byte(&mut self, _: u32) -> Result<u8, Self::Error> {
            Ok(0x55)
        }
        fn read_word(&mut self, _: u32) -> Result<u32, Self::Error> {
            Ok(0xabcdef01)
        }
        fn write_byte(&mut self, _: u32, _: u8) -> Result<(), Self::Error> {
            Err(BackendError::Missing)
        }
        fn write_word(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
            Err(BackendError::Missing)
        }
        fn free(&mut self, _: u32) -> Result<(), Self::Error> {
            Err(BackendError::Missing)
        }
        fn allocate(&mut self, _: u32) -> Result<u32, Self::Error> {
            Err(BackendError::Missing)
        }
        fn collect_frame(
            &mut self,
            _: &mut crate::vehicle_init::VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err(BackendError::Missing)
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: crate::vehicle_init::VehicleInitQuery,
            _: &mut crate::vehicle_init::VehicleInitEnumeration<'_>,
        ) -> Result<(), crate::vehicle_init::VehicleInitUnavailable<Self::Error>> {
            Err(crate::vehicle_init::VehicleInitUnavailable::Effect(
                BackendError::Missing,
            ))
        }
    }
    impl crate::vehicle_contacts::VehicleContactsBackend for Backend {
        fn contact_test_line_v(
            &mut self,
            _: CollisionLineRequest,
            _: &mut CollisionLineOutputs,
        ) -> Result<Option<crate::vehicle_contacts::VehicleContactHit>, Self::Error> {
            Err(BackendError::Missing)
        }
        fn contact_filter(
            &mut self,
            _: u32,
            _: u32,
            primitive: u32,
            _: u32,
        ) -> Result<u32, Self::Error> {
            Ok(primitive)
        }
        fn contact_update_frame_world(&mut self, _: u32) -> Result<(), Self::Error> {
            Err(BackendError::Missing)
        }
        fn contact_frame_direction(&mut self, _: u32) -> Result<[f32; 3], Self::Error> {
            Err(BackendError::Missing)
        }
        fn contact_effect(
            &mut self,
            _: u32,
            _: crate::vehicle_contacts::VehicleContactEffect<'_>,
        ) -> Result<(), Self::Error> {
            Err(BackendError::Missing)
        }
    }

    #[test]
    fn contact_bridge_keeps_property_identity_distinct_from_live_memory() {
        use crate::{
            collision_world_driving_contacts::DrivingWorldContacts,
            vehicle_contacts::VehicleContactsBackend, vehicle_init::VehicleInitBackend,
        };
        let bytes = fixture(true);
        let source = Klz::parse(&bytes).unwrap();
        let bound = bind_collision_world(&source, &[]).unwrap();
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        let world = DrivingCollisionWorld::from_prepared(&prepared, true).unwrap();
        let mut backend = Backend;
        let mut contacts = DrivingWorldContacts::new(&world, &mut backend, BackendError::Query);
        let mut outputs = outputs();
        let hit = contacts
            .contact_test_line_v(request(), &mut outputs)
            .unwrap()
            .unwrap();
        assert_eq!(hit.primitive, 2);
        assert_eq!(contacts.contact_primitive_material(2).unwrap(), 8);
        assert_eq!(
            contacts.contact_primitive_properties(2).unwrap(),
            0x08010003
        );
        assert_eq!(contacts.read_word(2).unwrap(), 0xabcdef01);
        assert_eq!(contacts.read_byte(5).unwrap(), 0x55);
        assert_eq!(
            contacts.contact_primitive_properties(100).unwrap(),
            0xabcdef01
        );
        assert_eq!(contacts.contact_filter(7, 8, 2, 0).unwrap(), 2);
        assert_eq!(
            contacts.contact_update_frame_world(7),
            Err(BackendError::Missing)
        );
        let saved = outputs;
        let traversal = contacts.traversal;
        let mut dynamic = request();
        dynamic.selector = 0;
        assert_eq!(
            contacts.contact_test_line_v(dynamic, &mut outputs),
            Err(BackendError::Query(
                CollisionQueryError::UnsupportedDynamicQuery
            ))
        );
        assert_eq!(outputs, saved);
        assert_eq!(contacts.traversal, traversal);
    }

    #[test]
    fn owned_world_preserves_duplicate_membership_first_hit_material_and_input() {
        let world = {
            let bytes = fixture(true);
            let original = bytes.clone();
            let klz = Klz::parse(&bytes).unwrap();
            let bound = bind_collision_world(&klz, &[]).unwrap();
            let prepared = prepare_bound_collision_world(&bound).unwrap();
            let world = DrivingCollisionWorld::from_prepared(&prepared, false).unwrap();
            assert_eq!(world.cells[0].end, 3);
            assert_eq!(world.candidates.len(), 3);
            assert_eq!(bytes, original);
            world
        };
        let mut outputs = outputs();
        let hit = world
            .test_line_v(
                &request(),
                &mut outputs,
                &mut CollisionTraversalState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            hit,
            CollisionLineHit {
                primitive: 2,
                fraction: 0.5
            }
        );
        assert_eq!(outputs.point, Some([0.5, 0., 0.5]));
        assert_eq!(outputs.normal.unwrap(), [-0., 1., -0.]);
        let primitive = world.primitive(hit.primitive).unwrap();
        assert_eq!(
            primitive.primitive,
            PrimitiveId {
                kind: PrimitiveKind::Triangle,
                index: 1
            }
        );
        assert_eq!(primitive.material(), 8);
        assert_eq!(primitive.properties & 0xff, 3);
        assert!(world.primitive(0).is_none());
        assert!(world.primitive(3).is_none());
    }

    #[test]
    fn unavailable_geometry_stays_reachable_and_masked_geometry_can_be_skipped() {
        let bytes = fixture(false);
        let klz = Klz::parse(&bytes).unwrap();
        let bound = bind_collision_world(&klz, &[]).unwrap();
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        let world = DrivingCollisionWorld::from_prepared(&prepared, false).unwrap();
        assert_eq!(
            world
                .primitives()
                .iter()
                .filter(|entry| entry.unavailable.is_some())
                .count(),
            2
        );
        let mut outputs = outputs();
        let original = outputs;
        let mut traversal = CollisionTraversalState::default();
        assert!(matches!(
            world.test_line_v(&request(), &mut outputs, &mut traversal),
            Err(CollisionQueryError::Preparation(_))
        ));
        assert_eq!(outputs, original);
        assert_eq!(traversal, CollisionTraversalState::default());
        let mut request = request();
        request.reject_mask = 0x10000;
        assert_eq!(
            world
                .test_line_v(&request, &mut outputs, &mut traversal)
                .unwrap(),
            None
        );
        assert_eq!(outputs, original);
    }

    #[test]
    fn dynamic_presence_requires_the_real_worker_for_non_bypass_selector() {
        let bytes = fixture(true);
        let klz = Klz::parse(&bytes).unwrap();
        let bound = bind_collision_world(&klz, &[]).unwrap();
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        let world = DrivingCollisionWorld::from_prepared(&prepared, true).unwrap();
        let mut request = request();
        request.selector = 3;
        let mut outputs = outputs();
        let original = outputs;
        assert_eq!(
            world.test_line_v(
                &request,
                &mut outputs,
                &mut CollisionTraversalState::default()
            ),
            Err(CollisionQueryError::UnsupportedDynamicQuery)
        );
        assert_eq!(outputs, original);
        request.selector = -1;
        assert!(world
            .test_line_v(
                &request,
                &mut outputs,
                &mut CollisionTraversalState::default()
            )
            .unwrap()
            .is_some());
    }

    #[test]
    fn scene_transform_keeps_original_unused_index_and_instance_world_hierarchy() {
        let bytes = fixture(false);
        let klz = Klz::parse(&bytes).unwrap();
        let mut frames = I3dFrameGraph::default();
        let quat = SQuat {
            wxyz: [1., 0., 0., 0.],
        };
        let parent = frames
            .insert(I3dFrameState::source_mesh([1., 0., 0.], [1.; 3], quat))
            .unwrap();
        let frame = frames
            .insert(I3dFrameState::source_mesh([-1., 0., 0.], [2.; 3], quat))
            .unwrap();
        frames.link_to(frame, Some(parent)).unwrap();
        let vertices = [[77., 77., 77.], [0., 0., 1.], [1., 0., 0.], [0., 0., 0.]];
        let links = [CollisionLinkSlot {
            name: b"f",
            word: CollisionLinkWord::Frame(Some(frame)),
        }];
        let geometry = [SceneCollisionGeometry {
            frame,
            lod0_local_vertices: Some(&vertices),
        }];
        let world = DrivingCollisionWorld::from_resolved_local_geometry(
            &klz,
            &links,
            &geometry,
            &mut frames,
            false,
        )
        .unwrap();
        assert!(world
            .primitives()
            .iter()
            .all(|entry| entry.unavailable.is_none()));
        let mut output = outputs();
        let hit = world
            .test_line_v(
                &request(),
                &mut output,
                &mut CollisionTraversalState::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(hit.primitive, 2);
        assert_eq!(output.point, Some([0.5, 0., 0.5]));
        let mut invalid_vertices = vertices;
        invalid_vertices[0][0] = f32::NAN;
        let geometry = [SceneCollisionGeometry {
            frame,
            lod0_local_vertices: Some(&invalid_vertices),
        }];
        assert!(matches!(
            DrivingCollisionWorld::from_resolved_local_geometry(
                &klz,
                &links,
                &geometry,
                &mut frames,
                false
            ),
            Err(DrivingCollisionWorldError::VertexTransform { vertex: 0, .. })
        ));
        // A stopped link-resolution pass must not be converted to a native null.
        let links = [CollisionLinkSlot::unresolved(1, b"f")];
        assert!(matches!(
            DrivingCollisionWorld::from_resolved_local_geometry(
                &klz,
                &links,
                &[],
                &mut frames,
                false
            ),
            Err(DrivingCollisionWorldError::Binding(
                ResolvedCollisionWorldError::UnresolvedLink { link_index: 0 }
            ))
        ));
    }
}
