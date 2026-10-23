//! Read-only binding of original KLZ geometry references.
//!
//! The reference rules are observed inside PC `g_collision::LoadCollision`
//! (RVA `0x1c2b70`). This is a descriptive host API, not the complete native
//! loader: no ownership transfer, degeneracy/property mutation, post-load
//! classification, cell selection, query or contact physics is performed.
//! Stored planes, bounds, primitive bytes and ordered cell references remain
//! available through [`BoundCollisionWorld::source`].

use crate::frame_transform::FrameKey;
use lhr_assets::klz::{
    Klz, PrimitiveData, PrimitiveId, PrimitiveKind, PrimitiveRecord, VertexReference,
};

const MAX_BINDING_BYTES: usize = 64 * 1024 * 1024;

/// Collection construction failed before binding any records. Per-record
/// reference diagnostics instead remain in the returned collection audit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionWorldBindingError {
    StorageQuotaExceeded,
    AllocationFailed,
}

/// One candidate for an original caller link slot. The caller establishes this
/// mapping; this module makes no assumptions about frame-name matching.
#[derive(Clone, Copy, Debug)]
pub struct CollisionFrameBinding<'v> {
    pub link_index: usize,
    pub frame: FrameKey,
    /// Complete, original-index LOD0 positions in original Y-up world space.
    /// Include unused vertices and preserve the instance's source indices.
    /// Hierarchy/placement transforms and instance resolution are caller work.
    /// Volumes only need the owner frame, so their slot may omit this storage.
    pub lod0_world_vertices: Option<&'v [[f32; 3]]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionVertexSource {
    Frame { link_index: usize, frame: FrameKey },
    WorldPool,
}

/// Host diagnostics, not original loader error results. Every original record
/// has an outcome; failure never silently removes it from the collection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionBindingError {
    InvalidLink {
        link_index: u32,
        link_count: usize,
    },
    MissingFrame {
        link_index: usize,
    },
    AmbiguousFrame {
        link_index: usize,
        matches: usize,
    },
    MissingLod0Vertices {
        link_index: usize,
        frame: FrameKey,
    },
    InvalidVertex {
        source: CollisionVertexSource,
        corner: usize,
        index: u16,
        vertex_count: usize,
    },
    NonFiniteVertex {
        source: CollisionVertexSource,
        corner: usize,
        index: u16,
    },
}

/// A typed reference to complete caller storage, or positions from the KLZ's
/// own world pool. Neither case creates geometry from rendered faces.
#[derive(Clone, Copy, Debug)]
pub struct BoundCollisionTriangleGeometry<'v>(TriangleStorage<'v>);

#[derive(Clone, Copy, Debug)]
enum TriangleStorage<'v> {
    Frame {
        link_index: usize,
        frame: FrameKey,
        vertices: &'v [[f32; 3]],
        indices: [u16; 3],
    },
    WorldPool {
        indices: [u16; 3],
        positions: [[f32; 3]; 3],
    },
}

impl BoundCollisionTriangleGeometry<'_> {
    pub fn source(&self) -> CollisionVertexSource {
        match self.0 {
            TriangleStorage::Frame {
                link_index, frame, ..
            } => CollisionVertexSource::Frame { link_index, frame },
            TriangleStorage::WorldPool { .. } => CollisionVertexSource::WorldPool,
        }
    }

    pub fn indices(&self) -> [u16; 3] {
        match self.0 {
            TriangleStorage::Frame { indices, .. } | TriangleStorage::WorldPool { indices, .. } => {
                indices
            }
        }
    }

    /// Complete borrowed storage; never a compacted per-triangle buffer.
    pub fn original_lod0_world_vertices(&self) -> Option<&[[f32; 3]]> {
        match self.0 {
            TriangleStorage::Frame { vertices, .. } => Some(vertices),
            TriangleStorage::WorldPool { .. } => None,
        }
    }

    pub fn positions(&self) -> [[f32; 3]; 3] {
        match self.0 {
            TriangleStorage::Frame {
                vertices, indices, ..
            } => indices.map(|index| vertices[index as usize]),
            TriangleStorage::WorldPool { positions, .. } => positions,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BoundCollisionTriangle<'v> {
    /// All original references, including the two non-authoritative link words.
    pub references: [VertexReference; 3],
    /// Copied bit-for-bit; deliberately neither normalized nor recomputed.
    pub stored_plane: [f32; 4],
    pub geometry: BoundCollisionTriangleGeometry<'v>,
}

/// Owner binding only. Bounds and remaining fields are retained in the source
/// record. Line/shape query dispatch for every volume kind is unsupported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundCollisionVolume {
    pub link_index: usize,
    pub frame: FrameKey,
}

#[derive(Clone, Copy, Debug)]
pub enum BoundCollisionPrimitive<'v> {
    Triangle(BoundCollisionTriangle<'v>),
    Volume(BoundCollisionVolume),
}

#[derive(Clone, Copy, Debug)]
pub struct CollisionBindingEntry<'data, 'v> {
    /// Immutable original record, including properties and stored bounds.
    pub source: PrimitiveRecord<'data>,
    pub outcome: Result<BoundCollisionPrimitive<'v>, CollisionBindingError>,
}

/// A collection audit, which may contain failed bindings. Even a completely
/// bound collection is not query-ready: native post-load processing, cell
/// traversal, eligible volumes and dynamic collision are separate dependencies.
#[derive(Debug)]
pub struct BoundCollisionWorld<'klz, 'data, 'v> {
    source: &'klz Klz<'data>,
    entries: Vec<CollisionBindingEntry<'data, 'v>>,
}

impl<'klz, 'data, 'v> BoundCollisionWorld<'klz, 'data, 'v> {
    pub fn source(&self) -> &'klz Klz<'data> {
        self.source
    }

    /// Original array order: triangles, AABBs, XTOBBs, cylinders, OBBs, spheres.
    /// This is file array order, not a candidate order for a collision query.
    pub fn entries(&self) -> &[CollisionBindingEntry<'data, 'v>] {
        &self.entries
    }

    pub fn get(&self, id: PrimitiveId) -> Option<&CollisionBindingEntry<'data, 'v>> {
        if id.index >= self.source.array(id.kind).len() {
            return None;
        }
        let start: usize = PrimitiveKind::ALL
            .into_iter()
            .take_while(|kind| *kind != id.kind)
            .map(|kind| self.source.array(kind).len())
            .sum();
        self.entries.get(start + id.index)
    }

    pub fn failures(&self) -> impl Iterator<Item = &CollisionBindingEntry<'data, 'v>> {
        self.entries.iter().filter(|entry| entry.outcome.is_err())
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameSlot<'v> {
    first: Option<CollisionFrameBinding<'v>>,
    matches: usize,
}

fn frame_slots<'v>(
    link_count: usize,
    bindings: &[CollisionFrameBinding<'v>],
) -> Result<Vec<FrameSlot<'v>>, CollisionWorldBindingError> {
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(link_count)
        .map_err(|_| CollisionWorldBindingError::AllocationFailed)?;
    slots.resize(link_count, FrameSlot::default());
    for binding in bindings {
        if let Some(slot) = slots.get_mut(binding.link_index) {
            slot.first.get_or_insert(*binding);
            slot.matches += 1;
        }
    }
    Ok(slots)
}

fn resolve_frame<'v>(
    link_index: u32,
    slots: &[FrameSlot<'v>],
) -> Result<CollisionFrameBinding<'v>, CollisionBindingError> {
    let slot = slots
        .get(link_index as usize)
        .ok_or(CollisionBindingError::InvalidLink {
            link_index,
            link_count: slots.len(),
        })?;
    let link_index = link_index as usize;
    match (slot.first, slot.matches) {
        (None, _) => Err(CollisionBindingError::MissingFrame { link_index }),
        (Some(binding), 1) => Ok(binding),
        (_, matches) => Err(CollisionBindingError::AmbiguousFrame {
            link_index,
            matches,
        }),
    }
}

fn check_position(
    position: [f32; 3],
    source: CollisionVertexSource,
    corner: usize,
    index: u16,
) -> Result<[f32; 3], CollisionBindingError> {
    if position.into_iter().all(f32::is_finite) {
        Ok(position)
    } else {
        Err(CollisionBindingError::NonFiniteVertex {
            source,
            corner,
            index,
        })
    }
}

fn bind_triangle<'v>(
    references: [VertexReference; 3],
    stored_plane: [f32; 4],
    slots: &[FrameSlot<'v>],
    world_vertex_count: usize,
    world_position: impl Fn(usize) -> Option<[f32; 3]>,
) -> Result<BoundCollisionTriangle<'v>, CollisionBindingError> {
    let indices = references.map(|reference| reference.index);
    let geometry = if references[0].link == 0x7fff {
        let mut positions = [[0.; 3]; 3];
        for (corner, index) in indices.into_iter().enumerate() {
            let source = CollisionVertexSource::WorldPool;
            let position =
                world_position(index as usize).ok_or(CollisionBindingError::InvalidVertex {
                    source,
                    corner,
                    index,
                    vertex_count: world_vertex_count,
                })?;
            positions[corner] = check_position(position, source, corner, index)?;
        }
        TriangleStorage::WorldPool { indices, positions }
    } else {
        let binding = resolve_frame(u32::from(references[0].link), slots)?;
        let link_index = binding.link_index;
        let frame = binding.frame;
        let vertices = binding
            .lod0_world_vertices
            .ok_or(CollisionBindingError::MissingLod0Vertices { link_index, frame })?;
        let source = CollisionVertexSource::Frame { link_index, frame };
        for (corner, index) in indices.into_iter().enumerate() {
            let position = vertices.get(index as usize).copied().ok_or(
                CollisionBindingError::InvalidVertex {
                    source,
                    corner,
                    index,
                    vertex_count: vertices.len(),
                },
            )?;
            check_position(position, source, corner, index)?;
        }
        TriangleStorage::Frame {
            link_index,
            frame,
            vertices,
            indices,
        }
    };
    Ok(BoundCollisionTriangle {
        references,
        stored_plane,
        geometry: BoundCollisionTriangleGeometry(geometry),
    })
}

/// Bind each primitive independently, preserving every success and diagnostic.
/// No query is run and neither the source KLZ nor caller storage is mutated.
/// Expanded entry and temporary slot metadata have a combined 64 MiB quota;
/// quota and allocation failures are host policy, not native loader behavior.
pub fn bind_collision_world<'klz, 'data, 'v>(
    source: &'klz Klz<'data>,
    bindings: &[CollisionFrameBinding<'v>],
) -> Result<BoundCollisionWorld<'klz, 'data, 'v>, CollisionWorldBindingError> {
    let count = PrimitiveKind::ALL
        .into_iter()
        .try_fold(0_usize, |count, kind| {
            count.checked_add(source.array(kind).len())
        })
        .ok_or(CollisionWorldBindingError::StorageQuotaExceeded)?;
    count
        .checked_mul(std::mem::size_of::<CollisionBindingEntry<'_, '_>>())
        .and_then(|entry_bytes| {
            source
                .links
                .len()
                .checked_mul(std::mem::size_of::<FrameSlot<'_>>())
                .and_then(|slot_bytes| entry_bytes.checked_add(slot_bytes))
        })
        .filter(|bytes| *bytes <= MAX_BINDING_BYTES)
        .ok_or(CollisionWorldBindingError::StorageQuotaExceeded)?;
    let slots = frame_slots(source.links.len(), bindings)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| CollisionWorldBindingError::AllocationFailed)?;
    for kind in PrimitiveKind::ALL {
        let array = source.array(kind);
        for index in 0..array.len() {
            let record = array.get(index).expect("validated KLZ primitive index");
            let outcome = match record.data() {
                PrimitiveData::Triangle {
                    vertices,
                    stored_plane,
                } => bind_triangle(
                    vertices,
                    stored_plane,
                    &slots,
                    source.world_vertices.len(),
                    |index| source.world_vertices.get(index),
                )
                .map(BoundCollisionPrimitive::Triangle),
                _ => {
                    // Every native volume array resolves its u32 record+4 word
                    // through caller slots; this is not a triangle's u16 link.
                    let link_index = u32::from_le_bytes(record.raw()[4..8].try_into().unwrap());
                    resolve_frame(link_index, &slots).map(|binding| {
                        BoundCollisionPrimitive::Volume(BoundCollisionVolume {
                            link_index: binding.link_index,
                            frame: binding.frame,
                        })
                    })
                }
            };
            entries.push(CollisionBindingEntry {
                source: record,
                outcome,
            });
        }
    }
    Ok(BoundCollisionWorld { source, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(bytes: &mut [u8], offset: usize, word: u32) {
        bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 40 + 104];
        bytes[..4].copy_from_slice(b"GifC");
        put(&mut bytes, 4, 5);
        put(&mut bytes, 8, 40);
        put(&mut bytes, 12, 2);
        // Identical names do not merge independently indexed caller slots.
        put(&mut bytes, 24, 32);
        put(&mut bytes, 28, 32);
        put(&mut bytes, 32, 1);
        bytes[36] = b'f';
        for (index, value) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            put(&mut bytes, 40 + index * 4, value.to_bits());
        }
        put(&mut bytes, 40 + 24, 1);
        put(&mut bytes, 40 + 28, 1);
        for (offset, count) in [
            (0x30, 3),
            (0x40, 1),
            (0x38, 1),
            (0x58, 1),
            (0x50, 1),
            (0x48, 1),
            (0x60, 3),
        ] {
            put(&mut bytes, 40 + offset, count);
        }
        for value in [0_f32, 10., 0., 10.] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        for kind in PrimitiveKind::ALL {
            let count = if kind == PrimitiveKind::Triangle {
                3
            } else {
                1
            };
            for index in 0..count {
                let start = bytes.len();
                bytes.resize(start + kind.stride(), 0);
                bytes[start] = if kind == PrimitiveKind::Triangle {
                    3
                } else {
                    kind.tag()
                };
                bytes[start + 1..start + 4].copy_from_slice(&[0x12, 0x34, 0x56]);
                if kind == PrimitiveKind::Triangle {
                    let first_link = [0, 1, 0x7fff][index];
                    let indices = if index == 0 { [3, 1, 2] } else { [0, 1, 2] };
                    for (corner, vertex_index) in indices.into_iter().enumerate() {
                        let link = if corner == 0 { first_link } else { 0xffff };
                        put(
                            &mut bytes,
                            start + 4 + corner * 4,
                            (link << 16) | vertex_index,
                        );
                    }
                    for (component, bits) in
                        [0x7fc1_2345, 0x8000_0000, 2_f32.to_bits(), 123_f32.to_bits()]
                            .into_iter()
                            .enumerate()
                    {
                        put(&mut bytes, start + 16 + component * 4, bits);
                    }
                } else {
                    put(&mut bytes, start + 4, 1);
                    for offset in (8..kind.stride()).step_by(4) {
                        put(&mut bytes, start + offset, (offset as f32).to_bits());
                    }
                }
            }
        }
        for value in 0..9 {
            bytes.extend_from_slice(&(value as f32).to_le_bytes());
        }
        bytes.extend_from_slice(&[0x87, 0x65, 0x43, 0x21]);
        let start = bytes.len();
        bytes.resize(start + 16, 0);
        put(&mut bytes, start, 8);
        put(&mut bytes, start + 4, 0xdead_beef);
        put(&mut bytes, start + 8, 0xabcd_1234);
        put(&mut bytes, start + 12, 75_f32.to_bits());
        for packed in [
            64_u32,
            0,
            0,
            0x8100_0000,
            0x8000_0000,
            0x8400_0000,
            0x8300_0000,
            0x8200_0000,
        ] {
            bytes.extend_from_slice(&packed.to_le_bytes());
        }
        bytes.extend_from_slice(&[7, 6, 5, 4, 3, 2, 1, 0]);
        bytes
    }

    #[test]
    fn preserves_partial_collection_all_volume_owners_planes_cells_and_original_storage() {
        let bytes = fixture();
        let original = bytes.clone();
        let klz = Klz::parse(&bytes).unwrap();
        let vertices = [[999.; 3], [11., 12., 13.], [21., 22., 23.], [31., 32., 33.]];
        let bindings = [
            CollisionFrameBinding {
                link_index: 0,
                frame: FrameKey(4),
                lod0_world_vertices: Some(&vertices),
            },
            CollisionFrameBinding {
                link_index: 1,
                frame: FrameKey(5),
                lod0_world_vertices: None,
            },
        ];
        let world = bind_collision_world(&klz, &bindings).unwrap();
        assert!(std::ptr::eq(world.source(), &klz));
        assert_eq!(world.entries().len(), 8);
        assert_eq!(world.failures().count(), 1);
        let frame = world
            .get(PrimitiveId {
                kind: PrimitiveKind::Triangle,
                index: 0,
            })
            .unwrap();
        let Ok(BoundCollisionPrimitive::Triangle(triangle)) = frame.outcome else {
            panic!()
        };
        assert_eq!(
            triangle.geometry.source(),
            CollisionVertexSource::Frame {
                link_index: 0,
                frame: FrameKey(4)
            }
        );
        assert_eq!(triangle.references[1].link, 0xffff);
        assert_eq!(triangle.geometry.indices(), [3, 1, 2]);
        assert_eq!(
            triangle.geometry.positions(),
            [vertices[3], vertices[1], vertices[2]]
        );
        assert!(std::ptr::eq(
            triangle.geometry.original_lod0_world_vertices().unwrap(),
            &vertices[..]
        ));
        assert_eq!(
            triangle.stored_plane.map(f32::to_bits),
            [0x7fc1_2345, 0x8000_0000, 2_f32.to_bits(), 123_f32.to_bits()]
        );
        assert_eq!(frame.source.properties().word(), 0x5634_1203);
        let failed = world
            .get(PrimitiveId {
                kind: PrimitiveKind::Triangle,
                index: 1,
            })
            .unwrap();
        assert!(matches!(
            failed.outcome,
            Err(CollisionBindingError::MissingLod0Vertices {
                link_index: 1,
                frame: FrameKey(5)
            })
        ));
        let pool = world
            .get(PrimitiveId {
                kind: PrimitiveKind::Triangle,
                index: 2,
            })
            .unwrap();
        let Ok(BoundCollisionPrimitive::Triangle(triangle)) = pool.outcome else {
            panic!()
        };
        assert_eq!(triangle.geometry.source(), CollisionVertexSource::WorldPool);
        assert_eq!(
            triangle.geometry.positions(),
            [[0., 1., 2.], [3., 4., 5.], [6., 7., 8.]]
        );
        for kind in PrimitiveKind::ALL.into_iter().skip(1) {
            let entry = world.get(PrimitiveId { kind, index: 0 }).unwrap();
            assert!(matches!(
                entry.outcome,
                Ok(BoundCollisionPrimitive::Volume(BoundCollisionVolume {
                    link_index: 1,
                    frame: FrameKey(5)
                }))
            ));
            assert_eq!(entry.source.kind(), kind);
            assert_eq!(entry.source.raw(), klz.array(kind).get(0).unwrap().raw());
        }
        let refs: Vec<_> = world.source().cells[0].references().collect();
        assert_eq!(refs[0].primitive.index, 2);
        assert_eq!(refs[1].primitive, refs[2].primitive);
        assert_eq!(
            refs.iter()
                .map(|reference| reference.flag)
                .collect::<Vec<_>>(),
            [7, 6, 5, 4, 3, 2, 1, 0]
        );
        assert_eq!(
            world.source().cells[0].pointer_words(),
            [0xdead_beef, 0xabcd_1234]
        );
        assert_eq!(world.source().cells[0].max_y(), 75.);
        assert_eq!(bytes, original);
        assert!(world
            .get(PrimitiveId {
                kind: PrimitiveKind::Aabb,
                index: 1
            })
            .is_none());
    }

    #[test]
    fn missing_ambiguous_and_invalid_slots_are_distinct_and_world_pool_ignores_slots() {
        let bytes = fixture();
        let klz = Klz::parse(&bytes).unwrap();
        let duplicate = CollisionFrameBinding {
            link_index: 0,
            frame: FrameKey(99),
            lod0_world_vertices: None,
        };
        // Even duplicate candidates naming the same frame are ambiguous.
        let slots = frame_slots(2, &[duplicate, duplicate]).unwrap();
        assert_eq!(
            resolve_frame(0, &slots).unwrap_err(),
            CollisionBindingError::AmbiguousFrame {
                link_index: 0,
                matches: 2
            }
        );
        assert_eq!(
            resolve_frame(1, &slots).unwrap_err(),
            CollisionBindingError::MissingFrame { link_index: 1 }
        );
        assert_eq!(
            resolve_frame(0x1_0000, &slots).unwrap_err(),
            CollisionBindingError::InvalidLink {
                link_index: 0x1_0000,
                link_count: 2
            }
        );
        let world = bind_collision_world(&klz, &[duplicate, duplicate]).unwrap();
        assert_eq!(world.entries().len(), 8);
        assert_eq!(world.failures().count(), 7);
        assert!(matches!(
            world.entries()[0].outcome,
            Err(CollisionBindingError::AmbiguousFrame { .. })
        ));
        assert!(matches!(
            world.entries()[1].outcome,
            Err(CollisionBindingError::MissingFrame { .. })
        ));
        assert!(matches!(
            world.entries()[2].outcome,
            Ok(BoundCollisionPrimitive::Triangle(_))
        ));
    }

    #[test]
    fn validates_authoritative_buffer_indices_and_only_referenced_positions() {
        let vertices = [[f32::NAN; 3], [1., 2., 3.], [4., 5., 6.]];
        let slots = frame_slots(
            1,
            &[CollisionFrameBinding {
                link_index: 0,
                frame: FrameKey(12),
                lod0_world_vertices: Some(&vertices),
            }],
        )
        .unwrap();
        let mut refs = [VertexReference { index: 1, link: 0 }; 3];
        refs[1].link = 0x7fff;
        refs[2].link = 0xffff;
        let bind = |refs| bind_triangle(refs, [f32::NAN; 4], &slots, 0, |_| None);
        assert!(bind(refs).is_ok());
        refs[1].index = 3;
        assert!(matches!(
            bind(refs),
            Err(CollisionBindingError::InvalidVertex {
                corner: 1,
                index: 3,
                vertex_count: 3,
                source: CollisionVertexSource::Frame { .. }
            })
        ));
        refs[1].index = 0;
        assert!(matches!(
            bind(refs),
            Err(CollisionBindingError::NonFiniteVertex {
                corner: 1,
                index: 0,
                ..
            })
        ));
        refs[0].link = 0x7fff;
        assert!(matches!(
            bind(refs),
            Err(CollisionBindingError::InvalidVertex {
                source: CollisionVertexSource::WorldPool,
                corner: 0,
                ..
            })
        ));
        refs[0].link = 3;
        assert!(matches!(
            bind(refs),
            Err(CollisionBindingError::InvalidLink {
                link_index: 3,
                link_count: 1
            })
        ));
    }
}
