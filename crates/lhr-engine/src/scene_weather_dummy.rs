//! Weather dummy registry: m_PrepareDummyStruct + Add/Del/Count/Array, PC
//! RVAs 0x4b510/0x4bbc0/0x4bc30/0x4bcc0/0x4bcd0, LS3DF SHA256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! DUMMY_STRUCT packing order/widths from the 0x4b510 Ghidra body (475
//! insn/50 BB, differential); registry behavior from pinned mafia-re
//! (I3d_scene.cpp:1056-1095, contract: Ghidra formed no function at those
//! entries, entry-byte probes only). FRMFLAGS_WEATHER bit from I3D.h:288.
//! Per-scene storage lives on SceneFrameOwner (the native ws_dummies
//! member). Precision: PC24/RN f32 pipeline; the plane-normal 1e-8 policy
//! mirrors the evidenced normalize blocks literally (including the
//! degenerate axis fallback). Upstream working names (mafia-re), not
//! evidenced original symbols.
#![forbid(unsafe_code)]

use crate::frame_transform::FrameKind;
use crate::model_loading::{ModelArena, ModelError};
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::native_transform::SMatrix;
use crate::scene_ownership::SceneKey;

/// Native FRMFLAGS_WEATHER, I3D.h:288.
pub const WEATHER_FLAG: u32 = 0x8000_0000;

/// Typed DUMMY_STRUCT (I3D.h:1356-1363, 0x54 bytes). Planes are [nx,ny,nz,d]
/// word order; sizes are the three edge lengths in plane order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeatherDummyEntry {
    pub dummy: crate::frame_transform::FrameKey,
    pub update_count: u32,
    pub center: [f32; 3],
    pub radius_sq: f32,
    pub planes: [[f32; 4]; 3],
    pub sizes: [f32; 3],
}

fn sign_one(value: f32) -> f32 {
    if value >= 0.0 {
        1.0
    } else {
        -1.0
    }
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    add(add(mul(a[0], b[0]), mul(a[1], b[1])), mul(a[2], b[2]))
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        sub(mul(a[1], b[2]), mul(a[2], b[1])),
        sub(mul(a[2], b[0]), mul(a[0], b[2])),
        sub(mul(a[0], b[1]), mul(a[1], b[0])),
    ]
}
/// S_plane three-point constructor as written out in 0x4b510: GetNormal,
/// NormalizeNormal with the 1e-8 policy, d = -Dot(n, a).
fn compute_plane(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 4] {
    let u = [sub(b[0], a[0]), sub(b[1], a[1]), sub(b[2], a[2])];
    let v = [sub(c[0], a[0]), sub(c[1], a[1]), sub(c[2], a[2])];
    let mut n = cross(u, v);
    let sq = dot(n, n);
    if (sq - 1.0).abs() >= 1e-8 {
        if sq >= 1e-8 {
            let scale = div(1.0, sqrt(sq));
            n = [mul(n[0], scale), mul(n[1], scale), mul(n[2], scale)];
        } else if n[0] == 0.0 {
            if n[2] == 0.0 {
                n[1] = sign_one(n[1]);
            } else {
                n[2] = sign_one(n[2]);
            }
        } else {
            n[0] = sign_one(n[0]);
        }
    }
    [n[0], n[1], n[2], -dot(n, a)]
}
fn magnitude(d: [f32; 3]) -> f32 {
    sqrt(dot(d, d))
}

/// Original m_PrepareDummyStruct, PC RVA 0x4b510 (differential: Ghidra body
/// ev-cited 475 insn/50 BB + packing-order tests). `matrix` is the dummy's
/// world matrix (the GetMatrix/UpdateWMatrixProc effect is owned by the
/// caller); `bounds` its I3D_dummy bbox words. Packing order: dummy word,
/// update_count, three transformed corners, center/radius_sq words, three
/// planes with the evidenced vertex triples, three edge sizes.
pub fn prepare_dummy_entry(
    matrix: &SMatrix,
    bounds: [[f32; 3]; 2],
    dummy: crate::frame_transform::FrameKey,
    update_count: u32,
) -> WeatherDummyEntry {
    let (bb_min, bb_max) = (bounds[0], bounds[1]);
    let v_max = matrix.transform_point(bb_max);
    let v_min = matrix.transform_point(bb_min);
    let v1 = matrix.transform_point([bb_min[0], bb_max[1], bb_min[2]]);
    let v2 = matrix.transform_point([bb_max[0], bb_max[1], bb_min[2]]);
    let v3 = matrix.transform_point([bb_max[0], bb_min[1], bb_min[2]]);
    let half = [
        mul(sub(v_max[0], v_min[0]), 0.5),
        mul(sub(v_max[1], v_min[1]), 0.5),
        mul(sub(v_max[2], v_min[2]), 0.5),
    ];
    WeatherDummyEntry {
        dummy,
        update_count,
        center: [
            add(v_min[0], half[0]),
            add(v_min[1], half[1]),
            add(v_min[2], half[2]),
        ],
        radius_sq: dot(half, half),
        planes: [
            compute_plane(v3, v2, v1),
            compute_plane(v3, v_max, v2),
            compute_plane(v2, v_max, v1),
        ],
        sizes: [
            magnitude([
                sub(v2[0], v_max[0]),
                sub(v2[1], v_max[1]),
                sub(v2[2], v_max[2]),
            ]),
            magnitude([sub(v1[0], v2[0]), sub(v1[1], v2[1]), sub(v1[2], v2[2])]),
            magnitude([sub(v2[0], v3[0]), sub(v2[1], v3[1]), sub(v2[2], v3[2])]),
        ],
    }
}

/// Original box-hiding test from m_UpdateWeatherSystem, RVA 0x4c660: sphere
/// reject on both points, then each point inside all three slabs keeps it.
/// `tail_pos` = pos + tail (-ws_dir*ws_size).
pub fn weather_dummy_hides(entry: &WeatherDummyEntry, pos: [f32; 3], tail_pos: [f32; 3]) -> bool {
    for point in [pos, tail_pos] {
        let to_center = [
            sub(entry.center[0], point[0]),
            sub(entry.center[1], point[1]),
            sub(entry.center[2], point[2]),
        ];
        let to_tail = [
            sub(entry.center[0], tail_pos[0]),
            sub(entry.center[1], tail_pos[1]),
            sub(entry.center[2], tail_pos[2]),
        ];
        if dot(to_center, to_center) > entry.radius_sq && dot(to_tail, to_tail) > entry.radius_sq {
            continue;
        }
        let inside = entry.planes.iter().zip(entry.sizes).all(|(plane, size)| {
            let f = add(
                add(
                    add(mul(plane[0], point[0]), mul(plane[1], point[1])),
                    mul(plane[2], point[2]),
                ),
                plane[3],
            );
            !(f < 0.0 || f > size)
        });
        if inside {
            return true;
        }
    }
    false
}

impl ModelArena {
    /// Original WeatherSystemAddDummy, PC RVA 0x4bbc0 (contract: no Ghidra
    /// body; entry-byte probe). Returns false on the idempotent no-op (flag
    /// set or duplicate). Effect order: guards -> live matrix
    /// (UpdateWMatrixProc analog via world_matrix) + live bounds/update_count
    /// -> reserve -> push -> flag set. Reserve failure returns
    /// AllocationOverflow with the registry and flag untouched (no poisoned
    /// prefix: the push never partially ran). The Dummy-kind check is host
    /// admission; native scans raw pointers without one.
    pub fn weather_add_dummy(
        &mut self,
        scene: SceneKey,
        dummy: crate::frame_transform::FrameKey,
    ) -> Result<bool, ModelError> {
        let si = self.scene_index(scene)?;
        if self.state(dummy)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(dummy));
        }
        if self.state(dummy)?.flags & WEATHER_FLAG != 0 {
            return Ok(false);
        }
        if self.scenes[si]
            .weather_dummies
            .iter()
            .any(|entry| entry.dummy == dummy)
        {
            return Ok(false);
        }
        let matrix = self.world_matrix(dummy)?;
        let bounds = self.dummy_bound_box(dummy)?;
        let update_count = self.state(dummy)?.update_count;
        let entry = prepare_dummy_entry(&matrix, bounds, dummy, update_count);
        self.check_owner_available(scene.into())?;
        self.scenes[si]
            .weather_dummies
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        self.scenes[si].weather_dummies.push(entry);
        self.graph.change_flags(dummy, 0, WEATHER_FLAG)?;
        Ok(true)
    }

    /// Original WeatherSystemDelDummy, PC RVA 0x4bc30 (contract). Erases the
    /// topmost match (reverse scan, first hit wins), then clears the flag
    /// even on a miss. Host liveness validation precedes mutation (native
    /// has none).
    pub fn weather_del_dummy(
        &mut self,
        scene: SceneKey,
        dummy: crate::frame_transform::FrameKey,
    ) -> Result<bool, ModelError> {
        if self.state(dummy)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(dummy));
        }
        let si = self.scene_index(scene)?;
        let found = self.scenes[si]
            .weather_dummies
            .iter()
            .rposition(|entry| entry.dummy == dummy)
            .map(|index| {
                self.scenes[si].weather_dummies.remove(index);
            })
            .is_some();
        self.graph.change_flags(dummy, WEATHER_FLAG, 0)?;
        Ok(found)
    }

    /// Original WeatherSystemGetDummyCnt, PC RVA 0x4bcc0 (contract: entry
    /// bytes read [EAX+0x334] = used_size).
    pub fn weather_dummy_count(&self, scene: SceneKey) -> Result<u32, ModelError> {
        u32::try_from(self.scene(scene)?.weather_dummies.len())
            .map_err(|_| ModelError::AllocationOverflow)
    }

    /// Original WeatherSystemGetDumies, PC RVA 0x4bcd0 (contract: entry bytes
    /// read [EAX+0x32C] = array). An empty registry yields an empty slice;
    /// native hands a NULL array (disclosed difference).
    pub fn weather_dummies(&self, scene: SceneKey) -> Result<&[WeatherDummyEntry], ModelError> {
        Ok(&self.scene(scene)?.weather_dummies)
    }

    /// Stale-entry refresh from m_UpdateWeatherSystem, RVA 0x4c660:
    /// `if (update_count != dummy->m_UpdateCount) m_PrepareDummyStruct(...)`.
    /// Refreshes every stale entry in place (no allocation, no OOM path)
    /// and returns the refreshed count; live matrix/bounds reread per entry.
    pub fn refresh_weather_dummies(&mut self, scene: SceneKey) -> Result<u32, ModelError> {
        let si = self.scene_index(scene)?;
        let mut refreshed = 0u32;
        for index in 0..self.scenes[si].weather_dummies.len() {
            let dummy = self.scenes[si].weather_dummies[index].dummy;
            let live = self.state(dummy)?.update_count;
            if self.scenes[si].weather_dummies[index].update_count == live {
                continue;
            }
            let matrix = self.world_matrix(dummy)?;
            let bounds = self.dummy_bound_box(dummy)?;
            self.scenes[si].weather_dummies[index] =
                prepare_dummy_entry(&matrix, bounds, dummy, live);
            refreshed += 1;
        }
        Ok(refreshed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_precision::Pc24PrecisionState;

    #[test]
    fn prepare_packing_order_widths_and_sizes() {
        Pc24PrecisionState::default().run(|| {
            let matrix = SMatrix::identity();
            let entry = prepare_dummy_entry(
                &matrix,
                [[-1., -2., -3.], [4., 5., 6.]],
                crate::frame_transform::FrameKey(7),
                9,
            );
            assert_eq!(
                (entry.dummy, entry.update_count),
                (crate::frame_transform::FrameKey(7), 9)
            );
            assert_eq!(entry.center, [1.5, 1.5, 1.5]);
            assert_eq!(entry.radius_sq.to_bits(), 38.75f32.to_bits());
            // v_max=(4,5,6) v1=(-1,5,-3) v2=(4,5,-3) v3=(4,-2,-3):
            // |v2-v_max|=9, |v1-v2|=5, |v2-v3|=7.
            assert_eq!(
                entry.sizes.map(f32::to_bits),
                [9f32, 5., 7.].map(f32::to_bits)
            );
            // First plane normal is +z unit length with d = -dot(n, v3) = 3.
            // The 1/35 normalize scale rounds, so compare with tolerance.
            assert!((entry.planes[0][2] - 1.0).abs() < 1e-6);
            assert!((entry.planes[0][0]).abs() < 1e-6);
            assert!((entry.planes[0][1]).abs() < 1e-6);
            assert!((entry.planes[0][3] - 3.0).abs() < 1e-5);
        });
    }

    #[test]
    fn add_delete_roundtrip_idempotency_flag_and_kind_guard() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let dummy = arena.create_dummy_frame().unwrap();
        arena
            .set_dummy_bound_box(dummy, [[-1., -1., -1.], [1., 1., 1.]])
            .unwrap();
        Pc24PrecisionState::default().run(|| {
            assert!(arena.weather_add_dummy(scene, dummy).unwrap());
            assert_eq!(arena.weather_dummy_count(scene).unwrap(), 1);
            assert!(!arena.weather_add_dummy(scene, dummy).unwrap());
            assert_eq!(arena.weather_dummies(scene).unwrap().len(), 1);
            assert!(arena.state(dummy).unwrap().flags & WEATHER_FLAG != 0);
            assert!(arena.weather_del_dummy(scene, dummy).unwrap());
            assert_eq!(arena.weather_dummy_count(scene).unwrap(), 0);
            assert!(arena.state(dummy).unwrap().flags & WEATHER_FLAG == 0);
            // Miss still clears the flag and reports absence.
            assert!(!arena.weather_del_dummy(scene, dummy).unwrap());
            // Non-dummy frames are rejected with the registry untouched.
            let model = arena.create_model().unwrap();
            assert!(matches!(
                arena.weather_add_dummy(scene, model.root()),
                Err(ModelError::DummyFrameRequired(_))
            ));
            assert_eq!(arena.weather_dummy_count(scene).unwrap(), 0);
            arena.release(model.root()).unwrap();
        });
        arena.release(dummy).unwrap();
    }

    #[test]
    fn stale_update_count_refreshes_and_hides() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let dummy = arena.create_dummy_frame().unwrap();
        arena
            .set_dummy_bound_box(dummy, [[-1., -1., -1.], [1., 1., 1.]])
            .unwrap();
        Pc24PrecisionState::default().run(|| {
            arena.weather_add_dummy(scene, dummy).unwrap();
            assert_eq!(arena.refresh_weather_dummies(scene).unwrap(), 0);
            // Stale the stored count directly (same-module access): the next
            // refresh rebuilds the entry and reports one refresh.
            let si = arena.scene_index(scene).unwrap();
            arena.scenes[si].weather_dummies[0].update_count = arena.scenes[si].weather_dummies[0]
                .update_count
                .wrapping_add(1);
            assert_eq!(arena.refresh_weather_dummies(scene).unwrap(), 1);
            assert_eq!(arena.refresh_weather_dummies(scene).unwrap(), 0);
            let entry = arena.weather_dummies(scene).unwrap()[0];
            assert!(weather_dummy_hides(&entry, [0., 0., 0.], [0., 0., 0.]));
            assert!(!weather_dummy_hides(
                &entry,
                [50., 50., 50.],
                [51., 51., 51.]
            ));
            assert!(arena.weather_del_dummy(scene, dummy).unwrap());
        });
        arena.release(dummy).unwrap();
    }
}
