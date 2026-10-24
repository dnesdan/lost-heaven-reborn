use super::*;
use crate::{
    vehicle_dynamics_body_response::{
        vehicle_body_static_response, VehicleBodyStaticContact, VehicleBodyStaticEffect,
        VehicleBodyStaticResponseBackend,
    },
    vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError},
    vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    },
};
const BASE: u32 = 0x1000_0000;
struct Backend {
    memory: VehicleFrameMemory,
    visited: Vec<u32>,
    rewrite_order: bool,
    shrink_fine: bool,
    additional_damage: bool,
    effects: Vec<VehicleBodyStaticEffect>,
}
impl Backend {
    fn new() -> Self {
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(BASE, &vec![0; 65536]).unwrap();
        memory.map_bytes(0x63d000, &[0; 4096]).unwrap();
        let mut b = Self {
            memory,
            visited: Vec::new(),
            rewrite_order: false,
            shrink_fine: false,
            additional_damage: false,
            effects: Vec::new(),
        };
        for header in [0x63d9fc, 0x63d9ec] {
            let begin = BASE + if header == 0x63d9fc { 0xb000 } else { 0xb200 };
            b.w(header + 4, begin);
            b.w(header + 8, begin);
            b.w(header + 12, begin + 0x100);
        }
        b
    }
    fn w(&mut self, address: u32, value: u32) {
        self.memory.write_word(address, value).unwrap();
    }
    fn f(&mut self, offset: u32, value: f32) {
        self.w(BASE + offset, value.to_bits());
    }
    fn matrix(&mut self, offset: u32) {
        for i in 0..16 {
            self.f(
                offset + i * 4,
                if [0, 5, 10, 15].contains(&i) { 1. } else { 0. },
            );
        }
    }
    fn clear_order(&mut self, header: u32) -> Result<(), VehicleMemoryError> {
        let begin = self.read_word(header + 4)?;
        self.write_word(header + 8, begin)
    }
}
impl VehicleInitBackend for Backend {
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
    fn free(&mut self, a: u32) -> Result<(), Self::Error> {
        self.memory.free(a)
    }
    fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
        self.memory.allocate(n)
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
impl VehicleBodyCollisionBackend for Backend {
    fn body_collision_initialize(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn body_collision_clear_order(&mut self) -> Result<(), Self::Error> {
        self.clear_order(0x63d9fc)
    }
    fn body_collision_insert(
        &mut self,
        h: u32,
        p: u32,
        index: u32,
        d: f32,
    ) -> Result<(), Self::Error> {
        let begin = self.read_word(h + 4)?;
        let end = self.read_word(h + 8)?;
        let at = begin + p * 8;
        let mut cursor = end;
        while cursor > at {
            cursor -= 4;
            let value = self.read_word(cursor)?;
            self.write_word(cursor + 8, value)?;
        }
        self.write_word(at, index)?;
        self.write_word(at + 4, d.to_bits())?;
        self.write_word(h + 8, end + 8)?;
        if self.shrink_fine && h == 0x63d9ec && end + 8 - begin == 16 {
            self.w(BASE + 0x7008, BASE + 0x903c);
        }
        Ok(())
    }
    fn body_collision_update_world(&mut self, _: u32) -> Result<(), Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn body_collision_filter(&mut self, _: VehicleBodyCollisionFilter) -> Result<(), Self::Error> {
        Ok(())
    }
    fn body_collision_sphere(&mut self, q: VehicleBodySphereSweep) -> Result<bool, Self::Error> {
        self.visited.push(q.record);
        if self.rewrite_order && self.visited.len() == 1 {
            // A query callback/nested owner replaces the shared scratch vector.
            // The parent's remaining count survives; its next begin load must
            // see these new canonical entries instead of a detached ordering.
            self.w(BASE + 0xb800, 1);
            self.w(BASE + 0xb808, 0);
            self.w(0x63da00, BASE + 0xb800);
            self.w(0x63da04, BASE + 0xb810);
        }
        Ok(false)
    }
    fn body_collision_volume(&mut self, _: VehicleBodyVolumeSweep) -> Result<bool, Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
}
impl VehicleBodyStaticResponseBackend for Backend {
    fn body_static_effect(
        &mut self,
        vehicle: u32,
        effect: &mut VehicleBodyStaticEffect,
    ) -> Result<(), Self::Error> {
        self.effects.push(*effect);
        if matches!(effect, VehicleBodyStaticEffect::Damage { .. }) {
            let flags = self.read_word(vehicle + 0xc68)?;
            self.write_word(
                vehicle + 0xc68,
                if self.additional_damage {
                    flags | 0x4000
                } else {
                    flags & !0x4000
                },
            )?;
        }
        Ok(())
    }
    fn body_static_damage_timer(
        &mut self,
        _: u32,
        _: u32,
        _: [f32; 3],
        _: u32,
    ) -> Result<bool, Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
}
fn call() -> VehicleBodyCollisionCall {
    VehicleBodyCollisionCall {
        collision_records: BASE + 0x4ac,
        correction_first: [0.; 3],
        correction_second: [0.; 3],
        auxiliary_scalar: 0.,
        auxiliary_vector: [0.; 3],
        previous_up: [0., 1., 0.],
        milliseconds: 16,
    }
}
#[test]
fn parent_traversal_observes_callback_replacement_of_shared_order() {
    let mut b = Backend::new();
    b.rewrite_order = true;
    b.w(BASE + 0x34c, BASE + 0x6000);
    b.w(BASE + 0x60ac, 0x20);
    for m in [0x6010, 0xde8, 0xd28] {
        b.matrix(m);
    }
    b.f(0x1f90, 1.);
    b.w(BASE + 0x4b0, BASE + 0x7000);
    b.w(BASE + 0x4b4, BASE + 0x70e8);
    for i in 0..2 {
        b.f(0x7000 + i * 0x74 + 0x24, i as f32);
        b.f(0x7000 + i * 0x74 + 0x54, 1.);
    }
    assert_eq!(
        vehicle_body_collision_no_contact(&mut b, BASE, &mut call()),
        Ok(VehicleBodyCollisionOutcome::NoContact)
    );
    assert_eq!(b.visited, [BASE + 0x7074, BASE + 0x7074]);
}
#[test]
fn damage_callback_controls_the_following_live_additional_damage_test() {
    for enable in [false, true] {
        let mut b = Backend::new();
        b.additional_damage = enable;
        for m in [0xd68, 0xda8] {
            b.matrix(m);
        }
        b.write_byte(BASE + 0x13c, 1).unwrap();
        b.f(0x390, 1.5);
        b.f(0x38c, 0.);
        b.f(0x5f4, 1000.);
        for o in [0x438, 0x43c, 0x440] {
            b.f(o, 0.001);
        }
        b.f(0x1f90, 0.5);
        b.f(0x1f94, -2.);
        b.f(0x1f98, 0.25);
        b.f(0x59c, 1.);
        b.w(BASE + 0xc68, if enable { 0x8000 } else { 0xc000 });
        let hit = VehicleBodyStaticContact {
            sphere_record: BASE + 0x9000,
            descriptor: BASE + 0x8000,
            previous_center: [0., 1., 0.],
            resolved_center: [0., 1.25, 0.],
            normal: [0., 1., 0.],
            radius: 0.5,
        };
        assert_eq!(
            vehicle_body_static_response(&mut b, BASE, hit, &mut call()),
            Ok(true)
        );
        assert!(matches!(
            b.effects[0],
            VehicleBodyStaticEffect::Damage { .. }
        ));
        assert_eq!(
            b.effects
                .iter()
                .any(|e| matches!(e, VehicleBodyStaticEffect::AdditionalDamage { .. })),
            enable
        );
    }
}

impl VehicleBodyCollisionResponseBackend for Backend {
    fn body_collision_clear_fine_order(&mut self) -> Result<(), Self::Error> {
        self.clear_order(0x63d9ec)
    }
    fn body_collision_fine_sphere(
        &mut self,
        query: VehicleBodySphereSweep,
    ) -> Result<Option<VehicleBodySweepHit>, Self::Error> {
        self.visited.push(query.record);
        Ok(None)
    }
    fn body_collision_map_descriptor(
        &mut self,
        _: u32,
        _: u32,
        _: u32,
        _: &mut VehicleBodySweepHit,
    ) -> Result<u32, Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
    fn body_collision_nested(
        &mut self,
        _: u32,
        _: u32,
        _: &mut VehicleBodyCollisionCall,
    ) -> Result<bool, Self::Error> {
        Err(VehicleMemoryError::FrameAdapterRequired)
    }
}
#[test]
fn fine_traversal_reloads_record_count_after_insertion_effects() {
    let mut b = Backend::new();
    b.shrink_fine = true;
    b.matrix(0xde8);
    b.w(BASE + 0x7004, BASE + 0x9000);
    b.w(BASE + 0x7008, BASE + 0x9078);
    for i in 0..2 {
        b.f(0x9004 + i * 0x3c, i as f32);
        b.f(0x9034 + i * 0x3c, 0.5);
    }
    assert_eq!(
        fine_response(
            &mut b,
            BASE,
            BASE + 0x7000,
            SMatrix::identity(),
            [1000., 0., 0.],
            &mut call()
        ),
        Ok(false)
    );
    assert_eq!(b.visited, [BASE + 0x9000]);
    assert_eq!(order_count(&mut b, 0x63d9ec), Ok(2));
}
