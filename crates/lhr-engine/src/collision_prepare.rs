//! Descriptive projection of PC collision triangle post-load processing.
//!
//! The frame degeneracy checks are inside `g_collision::LoadCollision` (RVA
//! `0x1c2b70`); the cell-driven classifier at RVA `0x1c2980` has no evidenced
//! original name. Neither body sorts vertices or cell references. Stored planes
//! remain bit-exact, even when inconsistent with the supplied world vertices.
//!
//! Arithmetic models x87 precision 24, round-to-nearest (CW `0x007f`) on a host
//! with ordinary IEEE round-to-nearest arithmetic. Used operands/intermediates
//! must be normal or zero and have magnitude at most `2^40`. Floating-point status/traps,
//! other control words and the extended x87 exponent range are outside scope.
//! Errors and allocation quotas are host policy, not original loader results.
//! This module does not establish scene ownership, candidate order, volume
//! queries or a complete collision world.

use crate::collision_world::{
    BoundCollisionPrimitive, BoundCollisionVolume, BoundCollisionWorld, CollisionBindingError,
    CollisionVertexSource,
};
use lhr_assets::klz::{Klz, PrimitiveId, PrimitiveKind, PrimitiveRecord};

#[cfg(test)]
const MAX_MAGNITUDE: f32 = (1_u64 << 40) as f32;
const MAX_PREPARATION_BYTES: usize = 64 * 1024 * 1024;

/// Distinguishes the two native load-time paths. World-pool triangles bypass
/// the frame degeneracy checks, including repeated or collinear vertices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrianglePreparationSource {
    Frame,
    WorldPool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionPreparationError {
    Binding(CollisionBindingError),
    NumericDomain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionWorldPreparationError {
    StorageQuotaExceeded,
    AllocationFailed,
}

/// Post-load state, retaining the original corner order. A record may be
/// disabled (`properties == 0x20`) or unreferenced and therefore unclassified.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedCollisionTriangle {
    pub properties: u32,
    pub vertices: [[f32; 3]; 3],
    pub stored_plane: [f32; 4],
}

impl PreparedCollisionTriangle {
    /// An exact field projection. The line worker still validates its numeric,
    /// classification and ascending-X vertex/candidate prerequisites.
    pub fn line_triangle(self, identity: u32) -> crate::collision_line::Triangle {
        crate::collision_line::Triangle {
            properties: self.properties,
            vertices: self.vertices,
            plane: self.stored_plane,
            identity,
        }
    }
}

fn number(value: f32) -> Result<f32, CollisionPreparationError> {
    if (value == 0.0 || value.is_normal()) && value.abs() <= (1_u64 << 40) as f32 {
        Ok(value)
    } else {
        Err(CollisionPreparationError::NumericDomain)
    }
}

fn sub(a: f32, b: f32) -> Result<f32, CollisionPreparationError> {
    let value = crate::collision_swept::sub(number(a)?, number(b)?)
        .map_err(|_| CollisionPreparationError::NumericDomain)?;
    number(value)
}

fn squared_distance(a: [f32; 3], b: [f32; 3]) -> Result<f32, CollisionPreparationError> {
    let [dx, dy, dz] = [sub(a[0], b[0])?, sub(a[1], b[1])?, sub(a[2], b[2])?];
    // Original instruction order: z*z + y*y, then + x*x. Do not fuse.
    let zz = number(
        crate::collision_swept::mul(dz, dz)
            .map_err(|_| CollisionPreparationError::NumericDomain)?,
    )?;
    let yy = number(
        crate::collision_swept::mul(dy, dy)
            .map_err(|_| CollisionPreparationError::NumericDomain)?,
    )?;
    let xx = number(
        crate::collision_swept::mul(dx, dx)
            .map_err(|_| CollisionPreparationError::NumericDomain)?,
    )?;
    let zy = number(
        crate::collision_swept::add(zz, yy)
            .map_err(|_| CollisionPreparationError::NumericDomain)?,
    )?;
    number(
        crate::collision_swept::add(zy, xx)
            .map_err(|_| CollisionPreparationError::NumericDomain)?,
    )
}

fn load_triangle(
    properties: u32,
    vertices: [[f32; 3]; 3],
    stored_plane: [f32; 4],
    source: TrianglePreparationSource,
) -> Result<PreparedCollisionTriangle, CollisionPreparationError> {
    let mut triangle = PreparedCollisionTriangle {
        properties,
        vertices,
        stored_plane,
    };
    if source == TrianglePreparationSource::Frame {
        for value in vertices.into_iter().flatten() {
            number(value)?;
        }
        let [a, b, c] = vertices;
        let mut score = 0;
        for (start, end) in [(a, b), (b, c), (c, a)] {
            if squared_distance(start, end)? < 1.0e-8_f32 {
                score += 2;
            }
        }
        for axis in 0..3 {
            let mut min = a[axis];
            let mut max = a[axis];
            for value in [b[axis], c[axis]] {
                if value < min {
                    min = value;
                }
                if value > max {
                    max = value;
                }
            }
            // The native f64 constant is exactly the widened f32 1e-8.
            if f64::from(sub(max, min)?.abs()) < f64::from(1.0e-8_f32) {
                score += 1;
            }
        }
        if score > 1 {
            // Whole word replacement, including flags/material/sort bytes.
            triangle.properties = 0x20;
        }
    }
    Ok(triangle)
}

/// Classify one visited cell record. The native `properties & 0xa0` skip occurs
/// before reading geometry. Only byte zero changes; plane and corner order do
/// not. On a host numeric-domain error, the triangle remains unchanged.
pub fn classify_triangle(
    triangle: &mut PreparedCollisionTriangle,
) -> Result<(), CollisionPreparationError> {
    if triangle.properties & 0xa0 != 0 {
        return Ok(());
    }
    let [nx, ny, nz] = [
        number(triangle.stored_plane[0])?.abs(),
        number(triangle.stored_plane[1])?.abs(),
        number(triangle.stored_plane[2])?.abs(),
    ];
    // Ties select Y over X; Z must be strictly greater than either winner.
    let axis = if nx > ny {
        if nz > nx {
            2
        } else {
            0
        }
    } else if nz > ny {
        2
    } else {
        1
    };
    let [a, b, _] = triangle.vertices;
    let difference = |component: usize| -> Result<f64, CollisionPreparationError> {
        Ok(f64::from(
            sub(number(b[component])?, number(a[component])?)?.abs(),
        ))
    };
    let classification = match axis {
        0 => {
            if difference(1)? < 0.0001_f64 {
                1
            } else {
                2
            }
        }
        1 => {
            if difference(0)? < 0.0001_f64 {
                3
            } else if difference(2)? < 0.0001_f64 {
                4
            } else {
                5
            }
        }
        _ => {
            if difference(0)? < 0.0001_f64 {
                6
            } else {
                7
            }
        }
    };
    triangle.properties = (triangle.properties & 0xffff_ff00) | classification;
    Ok(())
}

/// Load-time checks followed by classification for one referenced triangle.
/// Use the collection API when source cell membership matters: the original
/// classifier never visits unreferenced triangle-array records.
pub fn prepare_triangle(
    properties: u32,
    vertices: [[f32; 3]; 3],
    stored_plane: [f32; 4],
    source: TrianglePreparationSource,
) -> Result<PreparedCollisionTriangle, CollisionPreparationError> {
    let mut triangle = load_triangle(properties, vertices, stored_plane, source)?;
    classify_triangle(&mut triangle)?;
    Ok(triangle)
}

#[derive(Clone, Copy, Debug)]
pub enum PreparedCollisionPrimitive {
    Triangle(PreparedCollisionTriangle),
    /// Retained for explicit query dispatch; preparation does not implement
    /// volume workers or grant a triangle-only query permission to skip one.
    Volume(BoundCollisionVolume),
}

#[derive(Clone, Copy, Debug)]
pub struct CollisionPreparationEntry<'data> {
    pub source: PrimitiveRecord<'data>,
    pub outcome: Result<PreparedCollisionPrimitive, CollisionPreparationError>,
}

/// Every source record has an outcome, including unresolved bindings and all
/// volume kinds. Original cell references/flags remain in [`Self::source`].
#[derive(Debug)]
pub struct PreparedCollisionWorld<'klz, 'data> {
    source: &'klz Klz<'data>,
    entries: Vec<CollisionPreparationEntry<'data>>,
}

impl<'klz, 'data> PreparedCollisionWorld<'klz, 'data> {
    pub fn source(&self) -> &'klz Klz<'data> {
        self.source
    }

    pub fn entries(&self) -> &[CollisionPreparationEntry<'data>] {
        &self.entries
    }

    pub fn get(&self, id: PrimitiveId) -> Option<&CollisionPreparationEntry<'data>> {
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

    pub fn failures(&self) -> impl Iterator<Item = &CollisionPreparationEntry<'data>> {
        self.entries.iter().filter(|entry| entry.outcome.is_err())
    }
}

/// Project the evidenced load-time triangle changes and cell classifier onto
/// an existing binding audit. No caller/source storage is mutated. Binding
/// failures remain explicit instead of treating missing geometry as no hit.
pub fn prepare_bound_collision_world<'klz, 'data>(
    bound: &BoundCollisionWorld<'klz, 'data, '_>,
) -> Result<PreparedCollisionWorld<'klz, 'data>, CollisionWorldPreparationError> {
    bound
        .entries()
        .len()
        .checked_mul(std::mem::size_of::<CollisionPreparationEntry<'data>>())
        .filter(|bytes| *bytes <= MAX_PREPARATION_BYTES)
        .ok_or(CollisionWorldPreparationError::StorageQuotaExceeded)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(bound.entries().len())
        .map_err(|_| CollisionWorldPreparationError::AllocationFailed)?;
    for entry in bound.entries() {
        let outcome = match entry.outcome {
            Err(error) => Err(CollisionPreparationError::Binding(error)),
            Ok(BoundCollisionPrimitive::Volume(volume)) => {
                Ok(PreparedCollisionPrimitive::Volume(volume))
            }
            Ok(BoundCollisionPrimitive::Triangle(triangle)) => load_triangle(
                entry.source.properties().word(),
                triangle.geometry.positions(),
                triangle.stored_plane,
                match triangle.geometry.source() {
                    CollisionVertexSource::Frame { .. } => TrianglePreparationSource::Frame,
                    CollisionVertexSource::WorldPool => TrianglePreparationSource::WorldPool,
                },
            )
            .map(PreparedCollisionPrimitive::Triangle),
        };
        entries.push(CollisionPreparationEntry {
            source: entry.source,
            outcome,
        });
    }
    // Cells are row-major and references retain their exact source order.
    // Triangles lead the array, so their array index is also their entry index.
    for cell in &bound.source().cells {
        for reference in cell.references() {
            if reference.primitive.kind != PrimitiveKind::Triangle {
                continue;
            }
            let outcome = &mut entries[reference.primitive.index].outcome;
            if let Ok(PreparedCollisionPrimitive::Triangle(triangle)) = outcome {
                if let Err(error) = classify_triangle(triangle) {
                    *outcome = Err(error);
                }
            }
        }
    }
    Ok(PreparedCollisionWorld {
        source: bound.source(),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision_world::{bind_collision_world, CollisionFrameBinding};
    use crate::frame_transform::FrameKey;

    #[test]
    fn seven_classes_use_stored_plane_and_retain_every_other_bit() {
        let cases = [
            ([3., 1., 2., -77.], [1., 0., 1.], 1),
            ([3., 1., 2., -77.], [1., 1., 1.], 2),
            ([1., 3., 2., -77.], [0., 1., 1.], 3),
            ([1., 3., 2., -77.], [1., 1., 0.], 4),
            ([1., 3., 2., -77.], [1., 1., 1.], 5),
            ([1., 2., 3., -77.], [0., 1., 1.], 6),
            ([1., 2., 3., -77.], [1., 1., 1.], 7),
        ];
        for (plane, second, class) in cases {
            // Unsorted X and unrelated third vertex/plane are intentional.
            let vertices = [[0.; 3], second, [-9., 6., 2.]];
            let prepared = prepare_triangle(
                0x5a34_1200,
                vertices,
                plane,
                TrianglePreparationSource::WorldPool,
            )
            .unwrap();
            assert_eq!(prepared.properties, 0x5a34_1200 | class);
            assert_eq!(prepared.vertices, vertices);
            assert_eq!(
                prepared.stored_plane.map(f32::to_bits),
                plane.map(f32::to_bits)
            );
            let line = prepared.line_triangle(93);
            assert_eq!(line.vertices, vertices);
            assert_eq!(line.identity, 93);
        }
    }

    #[test]
    fn axis_ties_choose_y_then_x_before_z() {
        let vertices = [[0.; 3], [1.; 3], [2.; 3]];
        for (normal, expected) in [
            ([0., 0., 0.], 5),
            ([1., 1., 0.], 5),
            ([1., 1., 1.], 5),
            ([2., 1., 2.], 2),
            ([1., 2., 2.], 5),
            ([-1., -1., -2.], 7),
        ] {
            let prepared = prepare_triangle(
                0,
                vertices,
                [normal[0], normal[1], normal[2], 0.],
                TrianglePreparationSource::WorldPool,
            )
            .unwrap();
            assert_eq!(prepared.properties, expected);
        }
    }

    #[test]
    fn classifier_compares_rounded_delta_against_double_threshold() {
        let below = 0.0001_f32;
        let above = f32::from_bits(below.to_bits() + 1);
        assert!(f64::from(below) < 0.0001_f64);
        assert!(f64::from(above) > 0.0001_f64);
        for (dx, expected) in [(below, 6), (-below, 6), (above, 7), (-above, 7)] {
            let result = prepare_triangle(
                0,
                [[-0., 0., 0.], [dx, 0., 1.], [0.; 3]],
                [0., 0., 1., f32::from_bits(0x7fc1_2345)],
                TrianglePreparationSource::WorldPool,
            )
            .unwrap();
            assert_eq!(result.properties, expected);
            assert_eq!(result.vertices[0][0].to_bits(), (-0.0_f32).to_bits());
            assert_eq!(result.stored_plane[3].to_bits(), 0x7fc1_2345);
        }
    }

    #[test]
    fn frame_disable_replaces_whole_word_and_world_pool_bypasses_degeneracy() {
        let vertices = [[1., 2., 3.]; 3];
        let plane = [f32::NAN; 4];
        let disabled = prepare_triangle(
            0x1234_5600,
            vertices,
            plane,
            TrianglePreparationSource::Frame,
        )
        .unwrap();
        assert_eq!(disabled.properties, 0x20);
        assert_eq!(
            disabled.stored_plane.map(f32::to_bits),
            plane.map(f32::to_bits)
        );
        let pool = prepare_triangle(
            0x1234_5600,
            vertices,
            [0., 1., 0., 100.],
            TrianglePreparationSource::WorldPool,
        )
        .unwrap();
        assert_eq!(pool.properties, 0x1234_5603);
        // A collinear diagonal has no coincident edge or two flat axes.
        let diagonal = prepare_triangle(
            0x1234_5600,
            [[0.; 3], [1.; 3], [2.; 3]],
            [0., 1., 0., 100.],
            TrianglePreparationSource::Frame,
        )
        .unwrap();
        assert_eq!(diagonal.properties, 0x1234_5605);
    }

    #[test]
    fn skipped_classifier_never_reads_geometry_and_numeric_failure_is_transactional() {
        for properties in [0x20, 0x80, 0xa0, 0xffff_ffff] {
            let mut triangle = PreparedCollisionTriangle {
                properties,
                vertices: [[f32::NAN; 3]; 3],
                stored_plane: [f32::NAN; 4],
            };
            classify_triangle(&mut triangle).unwrap();
            assert_eq!(triangle.properties, properties);
        }
        let mut triangle = PreparedCollisionTriangle {
            properties: 0x1234_5600,
            vertices: [[0.; 3], [MAX_MAGNITUDE, 1., 0.], [0.; 3]],
            stored_plane: [0., 0., 1., 0.],
        };
        triangle.vertices[0][0] = -MAX_MAGNITUDE;
        let before = triangle;
        assert_eq!(
            classify_triangle(&mut triangle),
            Err(CollisionPreparationError::NumericDomain)
        );
        assert_eq!(triangle, before);
    }

    fn put(bytes: &mut [u8], offset: usize, word: u32) {
        bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    fn world_fixture() -> Vec<u8> {
        let mut bytes = vec![0; 36 + 104];
        bytes[..4].copy_from_slice(b"GifC");
        for (offset, value) in [(4, 5), (8, 36), (12, 1), (24, 28), (28, 1)] {
            put(&mut bytes, offset, value);
        }
        bytes[32] = b'f';
        for (index, value) in [0_f32, 0., 10., 10., 10., 10.].into_iter().enumerate() {
            put(&mut bytes, 36 + index * 4, value.to_bits());
        }
        for (offset, count) in [(24, 1), (28, 1), (0x30, 3), (0x40, 1), (0x60, 3)] {
            put(&mut bytes, 36 + offset, count);
        }
        for value in [0_f32, 10., 0., 10.] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 4]);
        for index in 0..3 {
            let start = bytes.len();
            bytes.resize(start + 32, 0);
            put(&mut bytes, start, 0x1234_5600);
            for corner in 0..3 {
                let link = if index == 2 { 0 } else { 0x7fff };
                put(
                    &mut bytes,
                    start + 4 + corner * 4,
                    (link << 16) | corner as u32,
                );
            }
            let plane = if index == 1 {
                [f32::NAN; 4]
            } else {
                [0., 1., 0., -44.]
            };
            for (component, value) in plane.into_iter().enumerate() {
                put(&mut bytes, start + 16 + component * 4, value.to_bits());
            }
        }
        let volume = bytes.len();
        bytes.resize(volume + 32, 0);
        put(&mut bytes, volume, 0x1234_5681);
        for position in [[2_f32, 0., 0.], [0., 0., 1.], [1., 0., 0.]] {
            for value in position {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&[0; 4]);
        let cell = bytes.len();
        bytes.resize(cell + 16, 0);
        put(&mut bytes, cell, 4);
        for packed in [0_u32, 0x8100_0000, 0, 64] {
            bytes.extend_from_slice(&packed.to_le_bytes());
        }
        bytes.extend_from_slice(&[11, 22, 33, 44]);
        bytes
    }

    #[test]
    fn collection_retains_unreferenced_records_binding_errors_volumes_and_source_order() {
        let bytes = world_fixture();
        let original = bytes.clone();
        let klz = Klz::parse(&bytes).unwrap();
        let bound = bind_collision_world(&klz, &[]).unwrap();
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        assert_eq!(prepared.entries().len(), 4);
        assert_eq!(prepared.failures().count(), 2);
        let Ok(PreparedCollisionPrimitive::Triangle(triangle)) = prepared.entries()[0].outcome
        else {
            panic!()
        };
        assert_eq!(triangle.properties, 0x1234_5605);
        assert_eq!(
            triangle.vertices,
            [[2., 0., 0.], [0., 0., 1.], [1., 0., 0.]]
        );
        let Ok(PreparedCollisionPrimitive::Triangle(unreferenced)) = prepared.entries()[1].outcome
        else {
            panic!()
        };
        assert_eq!(unreferenced.properties, 0x1234_5600);
        assert!(unreferenced.stored_plane[0].is_nan());
        assert!(matches!(
            prepared.entries()[2].outcome,
            Err(CollisionPreparationError::Binding(
                CollisionBindingError::MissingFrame { link_index: 0 }
            ))
        ));
        let binding = CollisionFrameBinding {
            link_index: 0,
            frame: FrameKey(8),
            lod0_world_vertices: None,
        };
        let bound = bind_collision_world(&klz, &[binding]).unwrap();
        let prepared = prepare_bound_collision_world(&bound).unwrap();
        assert_eq!(prepared.failures().count(), 1);
        assert!(matches!(
            prepared
                .get(PrimitiveId {
                    kind: PrimitiveKind::Aabb,
                    index: 0
                })
                .unwrap()
                .outcome,
            Ok(PreparedCollisionPrimitive::Volume(BoundCollisionVolume {
                frame: FrameKey(8),
                ..
            }))
        ));
        assert!(std::ptr::eq(prepared.source(), &klz));
        assert_eq!(
            prepared.source().cells[0]
                .references()
                .map(|r| r.flag)
                .collect::<Vec<_>>(),
            [11, 22, 33, 44]
        );
        assert_eq!(bytes, original);
    }
}
