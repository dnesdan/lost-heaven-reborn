use super::*;
use crate::break_plain_glass::GlassPendingResource;
use crate::glass_subdivision::{GlassGeometry, GlassVertex};
use crate::native_transform::{SMatrix, SQuat};
use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
use crate::vehicle_init::{
    VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct Owner {
    memory: VehicleFrameMemory,
    calls: Vec<(u32, u32, Vec<u32>)>,
    created: Vec<u32>,
    rotations: BTreeMap<u32, SMatrix>,
    triangles: BTreeMap<u32, [GlassVertex; 3]>,
    groups: BTreeMap<u32, Vec<[u16; 3]>>,
    installed: Vec<(u32, Vec<GlassVertex>, Vec<[u16; 3]>)>,
    pending: Vec<GlassPendingResource>,
}
impl VehicleInitBackend for Owner {
    type Error = VehicleMemoryError;
    fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
        self.memory.read_byte(a)
    }
    fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
        self.memory.read_word(a)
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
        self.memory.write_byte(a, v)
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
        self.memory.write_word(a, v)
    }
    fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
        self.memory.allocate(n)
    }
    fn free(&mut self, a: u32) -> Result<(), Self::Error> {
        self.memory.free(a)
    }
    fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, f: u32) -> Result<(), Self::Error> {
        self.memory.collect_frame(v, f)
    }
    fn enumerate(
        &mut self,
        m: u32,
        q: VehicleInitQuery,
        t: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.memory.enumerate(m, q, t)
    }
}
impl BreakPlainGlassBackend for Owner {
    fn glass_frame_virtual(&mut self, f: u32, s: u32, a: &[u32]) -> Result<u32, Self::Error> {
        self.calls.push((f, s, a.to_vec()));
        if let (0x2c, [parent, 0]) = (s, a) {
            if self.read_word(f + 0x120).is_ok() {
                self.write_word(f + 0x120, *parent)?;
            }
        }
        Ok(0)
    }
    fn glass_update_world(&mut self, f: u32) -> Result<(), Self::Error> {
        let mut world = self
            .rotations
            .get(&f)
            .copied()
            .unwrap_or_else(SMatrix::identity);
        for i in 0..3 {
            world.elements[12 + i] = f32::from_bits(self.read_word(f + 0x80 + i as u32 * 4)?);
        }
        for (i, value) in world.elements.into_iter().enumerate() {
            self.write_word(f + 0x10 + i as u32 * 4, value.to_bits())?;
        }
        let flags = self.read_word(f + 0xac)?;
        self.write_word(f + 0xac, flags | 0x20)
    }
    fn glass_frame_direction(
        &mut self,
        f: u32,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), Self::Error> {
        let mut rotation = SMatrix::identity();
        rotation.set_rot3(SQuat::try_make_frame_direction(direction, roll).unwrap());
        self.rotations.insert(f, rotation);
        let flags = self.read_word(f + 0xac)?;
        self.write_word(f + 0xac, flags & !0x20)
    }
    fn glass_new_frame(&mut self) -> Result<u32, Self::Error> {
        let f = 0x9000 + self.created.len() as u32 * 0x400;
        self.memory.map_bytes(f, &[0; 0x300])?;
        self.write_word(f + 0xac, 1)?;
        self.created.push(f);
        Ok(f)
    }
    fn glass_source_geometry(&mut self, _: u32) -> Result<GlassGeometry, Self::Error> {
        panic!("unexpected geometry")
    }
    fn glass_new_material(&mut self) -> Result<u32, Self::Error> {
        panic!("unexpected material")
    }
    fn glass_copy_source_material(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
        panic!("unexpected copy")
    }
    fn glass_release_material(&mut self, _: u32) -> Result<(), Self::Error> {
        panic!("unexpected material release")
    }
    fn glass_begin_fragment(&mut self, _: u32) -> Result<u32, Self::Error> {
        panic!("unexpected group")
    }
    fn glass_cleanup_resources(&mut self) -> Result<(), Self::Error> {
        panic!("unexpected resource cleanup")
    }
    fn glass_install_fragment(
        &mut self,
        _: u32,
        _: u32,
        _: &[GlassVertex; 6],
    ) -> Result<bool, Self::Error> {
        panic!("unexpected install")
    }
    fn glass_place_sector(&mut self, f: u32) -> Result<(), Self::Error> {
        self.glass_update_world(f)
    }
    fn glass_debustize(&mut self, _: u32) -> Result<(), Self::Error> {
        panic!("unexpected visual")
    }
    fn glass_ground_plane(&mut self, _: [f32; 3]) -> Result<([f32; 3], [f32; 3]), Self::Error> {
        panic!("unexpected ground")
    }
    fn glass_play_sound(&mut self, _: u32, _: [f32; 3]) -> Result<(), Self::Error> {
        panic!("unexpected sound")
    }
    fn glass_test_line(
        &mut self,
        _: [f32; 3],
        _: [f32; 3],
        _: u32,
        _: u32,
        _: &mut [f32; 3],
        _: &mut [f32; 3],
    ) -> Result<u32, Self::Error> {
        panic!("unexpected line")
    }
    fn glass_merge_fragments(&mut self, _: u32) -> Result<bool, Self::Error> {
        panic!("unexpected merge")
    }
    fn glass_allocator_marker(&self) -> u8 {
        0xa5
    }
    fn glass_track(&mut self, resource: GlassPendingResource) {
        self.pending.push(resource);
    }
    fn glass_untrack(&mut self, resource: GlassPendingResource) -> Result<(), Self::Error> {
        let index = self
            .pending
            .iter()
            .rposition(|r| *r == resource)
            .expect("pending resource missing");
        self.pending.remove(index);
        Ok(())
    }
    fn glass_pending_last(&self) -> Option<GlassPendingResource> {
        self.pending.last().copied()
    }
}
impl GlassMergeBackend for Owner {
    fn glass_merge_material(&mut self, _: u32) -> Result<u32, Self::Error> {
        Ok(0xdead)
    }
    fn glass_merge_read_triangle(&mut self, frame: u32) -> Result<[GlassVertex; 3], Self::Error> {
        Ok(self.triangles[&frame])
    }
    fn glass_merge_begin_group(
        &mut self,
        material: u32,
        faces: &[[u16; 3]],
    ) -> Result<u32, Self::Error> {
        assert_eq!(material, 0xdead);
        let group = 0x10001 + self.groups.len() as u32;
        self.groups.insert(group, faces.to_vec());
        Ok(group)
    }
    fn glass_merge_install(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex],
    ) -> Result<bool, Self::Error> {
        let faces = self.groups.remove(&group).expect("missing staged group");
        self.installed.push((frame, vertices.to_vec(), faces));
        Ok(true)
    }
}
fn setup() -> Owner {
    let mut b = Owner::default();
    b.memory.map_bytes(0x1000, &[0xa5; 0x38]).unwrap();
    glass_construct(&mut b, 0x1000, 0x7b).unwrap();
    b
}
fn filled(b: &mut Owner, n: u32) -> u32 {
    let p = b.allocate(n).unwrap();
    for i in 0..n {
        b.write_byte(p + i, 0).unwrap();
    }
    p
}
fn header(b: &mut Owner, h: u32, p: u32, n: u32, capacity: u32) {
    for (offset, v) in [(4, p), (8, p + n), (12, p + capacity)] {
        b.write_word(h + offset, v).unwrap();
    }
}
#[test]
fn constructor_padding_and_five_slot_array_follow_original_lifetime() {
    let mut b = setup();
    assert_eq!(b.read_byte(0x1000).unwrap(), 0x7b);
    assert_eq!(b.read_byte(0x1001).unwrap(), 0xa5);
    assert_eq!(b.read_byte(0x1010).unwrap(), 0x7b);
    assert_eq!(b.read_word(0x1020).unwrap(), 0xa5a5a5a5);
    assert!(glass_init(&mut b, 0x1000, 5).unwrap());
    let cache = b.read_word(0x1020).unwrap();
    assert_eq!(b.read_word(cache - 4).unwrap(), 5);
    assert_eq!(b.read_word(0x1024).unwrap(), 5);
    for i in 0..120 {
        assert_eq!(b.read_byte(cache + i).unwrap(), 0);
    }
    assert_eq!(b.memory.allocations().count(), 1);
    assert!(glass_done(&mut b, 0x1000).unwrap());
    assert_eq!(b.read_word(0x1020).unwrap(), cache);
    assert_eq!(b.memory.allocations().count(), 0);
    glass_free_caller_storage(&mut b, 0x1000).unwrap();
    assert!(b.calls.is_empty());
}
#[test]
fn done_releases_cached_meshes_live_fragments_and_restores_originals_without_double_release() {
    let mut b = setup();
    glass_init(&mut b, 0x1000, 1).unwrap();
    let cache = b.read_word(0x1020).unwrap();
    let merged = filled(&mut b, 4);
    b.write_word(merged, 300).unwrap();
    header(&mut b, cache, merged, 4, 4);
    let old_rows = filled(&mut b, 52);
    b.write_word(cache + 0x10, old_rows).unwrap();
    b.write_word(cache + 0x14, 1).unwrap();
    let active = filled(&mut b, 24);
    header(&mut b, 0x1010, active, 24, 24);
    let rows = filled(&mut b, 104);
    b.write_word(rows, 100).unwrap();
    b.write_word(rows + 52, 200).unwrap();
    b.write_word(active + 0x10, rows).unwrap();
    b.write_word(active + 0x14, 2).unwrap();
    let originals = filled(&mut b, 4);
    b.write_word(originals, 400).unwrap();
    header(&mut b, 0x1000, originals, 4, 4);
    glass_done(&mut b, 0x1000).unwrap();
    assert_eq!(
        b.calls,
        [
            (300, 0x2c, vec![0, 0]),
            (300, 0, vec![]),
            (200, 0, vec![]),
            (100, 0, vec![]),
            (400, 0x24, vec![1]),
            (400, 0, vec![])
        ]
    );
    assert_eq!(b.read_word(0x1018).unwrap(), active);
    assert_eq!(b.read_word(0x1008).unwrap(), originals);
    assert_eq!(b.memory.allocations().count(), 2);
    glass_free_caller_storage(&mut b, 0x1000).unwrap();
    assert_eq!(b.memory.allocations().count(), 0);
}

#[test]
fn completed_fragments_merge_in_world_space_transfer_to_ring_and_release_once() {
    let mut b = setup();
    glass_init(&mut b, 0x1000, 5).unwrap();
    let entry = filled(&mut b, 24);
    header(&mut b, 0x1010, entry, 24, 24);
    let rows = filled(&mut b, 104);
    b.write_word(entry + 0x10, rows).unwrap();
    b.write_word(entry + 0x14, 2).unwrap();
    let triangle = [
        GlassVertex {
            position: [0., 0., 0.],
            normal: [0., 0., 1.],
            uv: [0., 0.],
        },
        GlassVertex {
            position: [1., 0., 0.],
            normal: [0., 0., 1.],
            uv: [1., 0.],
        },
        GlassVertex {
            position: [0., 1., 0.],
            normal: [0., 0., 1.],
            uv: [0., 1.],
        },
    ];
    for (i, frame) in [0x5000, 0x5400].into_iter().enumerate() {
        b.memory.map_bytes(frame, &[0; 0x300]).unwrap();
        b.write_word(rows + i as u32 * 52, frame).unwrap();
        b.write_word(frame + 0x120, 0x8000).unwrap();
        b.write_word(frame + 0x80, (1. + i as f32 * 2.).to_bits())
            .unwrap();
        let mut world = SMatrix::identity();
        world.elements[0] = 0.;
        world.elements[1] = 1.;
        world.elements[4] = -1.;
        world.elements[5] = 0.;
        world.elements[12] = 1. + i as f32 * 2.;
        for (i, x) in world.elements.into_iter().enumerate() {
            b.write_word(frame + 0x10 + i as u32 * 4, x.to_bits())
                .unwrap();
        }
        b.write_word(frame + 0xac, 0x21).unwrap();
        b.triangles.insert(frame, triangle);
    }
    glass_finish_entry(&mut b, 0x1000, 0).unwrap();
    assert_eq!(b.created.len(), 1);
    assert_eq!(b.installed.len(), 1);
    let (merged, vertices, faces) = &b.installed[0];
    assert_eq!(*faces, [[2, 1, 0], [5, 4, 3]]);
    assert!(vertices.iter().all(|v| v.normal == [0., 0., 1.]));
    let mut world = SMatrix::identity();
    for (i, x) in world.elements.iter_mut().enumerate() {
        *x = f32::from_bits(b.memory.read_word(*merged + 0x10 + i as u32 * 4).unwrap());
    }
    for (i, vertex) in vertices.iter().enumerate() {
        let point = world.try_transform_point(vertex.position).unwrap();
        let input = triangle[i % 3].position;
        let expected = [1. + (i / 3) as f32 * 2. - input[1], input[0], 0.];
        for (actual, expected) in point.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-5);
        }
    }
    assert_eq!(b.read_word(rows).unwrap(), 0);
    assert_eq!(b.read_word(rows + 52).unwrap(), 0);
    assert_eq!(b.read_word(0x1018).unwrap(), entry);
    assert_eq!(b.read_word(0x1024).unwrap(), 0);
    let cache = b.read_word(0x1020).unwrap();
    let pointers = b.read_word(cache + 4).unwrap();
    assert_eq!(b.read_word(pointers).unwrap(), b.created[0]);
    assert!(b.pending.is_empty());
    glass_done(&mut b, 0x1000).unwrap();
    glass_free_caller_storage(&mut b, 0x1000).unwrap();
    for frame in [0x5000, 0x5400, b.created[0]] {
        assert_eq!(
            b.calls
                .iter()
                .filter(|(f, s, _)| *f == frame && *s == 0)
                .count(),
            1
        );
    }
    assert_eq!(b.memory.allocations().count(), 0);
}
