use super::*;
use crate::vehicle_frame_backend::VehicleFrameMemory;
const CAR: u32 = 0x1000;
const ACTOR: u32 = 0x4000;
const DESCRIPTOR: u32 = 0x5000;
const APP: u32 = 0x6000;
const GAME: u32 = 0x6200;
fn set_vec(b: &mut VehicleFrameMemory, a: u32, v: [f32; 3]) {
    for (i, n) in v.into_iter().enumerate() {
        b.write_word(a + i as u32 * 4, n.to_bits()).unwrap();
    }
}
fn vec(b: &mut VehicleFrameMemory, a: u32) -> [f32; 3] {
    std::array::from_fn(|i| f32::from_bits(b.read_word(a + i as u32 * 4).unwrap()))
}
fn setup() -> VehicleFrameMemory {
    let mut b = VehicleFrameMemory::default();
    b.map_bytes(CAR, &vec![0; 0x8000]).unwrap();
    b.map_bytes(0x63788c, &APP.to_le_bytes()).unwrap();
    b.write_word(APP + 0x24, GAME).unwrap();
    for (a, v) in [
        (DESCRIPTOR, 0x40),
        (DESCRIPTOR + 0x44, ACTOR),
        (ACTOR, 0x623918),
        (ACTOR + 0x10, 0x1a),
        (ACTOR + 0x1c, 1),
        (ACTOR + 0x38c, 1),
        (ACTOR + 0x34c, 7),
    ] {
        b.write_word(a, v).unwrap();
    }
    b.write_word(ACTOR + 0x210, 100_f32.to_bits()).unwrap();
    b.write_word(ACTOR + 0x390, 100_f32.to_bits()).unwrap();
    b.write_byte(ACTOR + 0x3be, 1).unwrap();
    set_vec(&mut b, ACTOR + 0x84, [10., 0., 0.]);
    set_vec(&mut b, ACTOR + 0x220, [2., 0., 0.]);
    set_vec(&mut b, ACTOR + 0x22c, [0., 0., 1.]);
    set_vec(&mut b, ACTOR + 0x214, [2., 3., 4.]);
    // Quarter-turn world basis and its inverse; unequal principal inertias
    // make the change of basis observable rather than cancelling trivially.
    let world = [
        0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    let inverse = [
        0., -1., 0., 0., 1., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    for (base, m) in [(ACTOR + 0x18c, world), (ACTOR + 0x1cc, inverse)] {
        for (i, n) in m.into_iter().enumerate() {
            b.write_word(base + i as u32 * 4, f32::to_bits(n)).unwrap();
        }
    }
    b
}
#[test]
fn other_properties_rotate_anisotropic_inertia_and_preserve_filter_output_byte() {
    let mut b = setup();
    let mut damage_allowed = 0xa5;
    assert!(!car_body_collision_flags(&mut b, CAR, DESCRIPTOR, &mut damage_allowed).unwrap());
    assert_eq!(damage_allowed, 0xa5);
    let before = vec(&mut b, ACTOR + 0x220);
    let mut out = VehicleCarOtherBody::default();
    assert!(
        car_body_other_properties(&mut b, DESCRIPTOR, [10., 1., 0.], [0., 0., 1.], &mut out)
            .unwrap()
    );
    assert_eq!(out.arm, [0., 1., 0.]);
    assert_eq!(out.velocity, [3., 0., 0.]);
    assert_eq!(out.mass, 100.);
    assert_eq!(out.inertia_response, [3., 0., 0.]);
    assert_eq!(out.restitution.to_bits(), 0xbfa66666);
    assert_eq!(out.correction_allowed, 0);
    assert_eq!(vec(&mut b, ACTOR + 0x220), before);
}
#[test]
fn impulse_updates_the_live_actor_and_keeps_the_caller_impulse_unchanged() {
    let mut b = setup();
    b.write_word(ACTOR + 0x350, 0xa5a50000).unwrap();
    b.write_word(ACTOR + 0x370, 5_f32.to_bits()).unwrap();
    let mut point = [10., 1., 0.];
    let mut impulse = [10., 0., 0.];
    let mut normal = [1., 0., 0.];
    car_body_apply_other_impulse(
        &mut b,
        CAR,
        DESCRIPTOR,
        &mut point,
        &mut impulse,
        &mut normal,
        1000.,
    )
    .unwrap();
    assert_eq!(vec(&mut b, ACTOR + 0x220), [2.11, 0., 0.]);
    assert_eq!(vec(&mut b, ACTOR + 0x22c), [0., 0., 45.]);
    assert_eq!(b.read_word(ACTOR + 0x1c).unwrap(), 0);
    assert_eq!(b.read_byte(GAME + 0x10c).unwrap(), 1);
    assert_eq!(f32::from_bits(b.read_word(ACTOR + 0x390).unwrap()), 90.);
    assert_eq!(vec(&mut b, ACTOR + 0x3a0), impulse);
    assert_eq!(b.read_word(ACTOR + 0x3b0).unwrap(), (-0_f32).to_bits());
    assert_eq!(b.read_word(ACTOR + 0x350).unwrap(), 0xa5a50007);
    assert_eq!(f32::from_bits(b.read_word(ACTOR + 0x370).unwrap()), 10.);
    assert_eq!(vec(&mut b, ACTOR + 0x33c), point);
    assert_eq!(impulse, [10., 0., 0.]);
}
#[test]
fn immovable_crash_records_damage_without_changing_velocity() {
    let mut b = setup();
    b.write_byte(ACTOR + 0x3be, 0).unwrap();
    b.write_word(ACTOR + 0x390, 5_f32.to_bits()).unwrap();
    assert!(!car_body_crash_collision(&mut b, ACTOR, [10., 1., 0.], [10., 0., 0.]).unwrap());
    assert_eq!(vec(&mut b, ACTOR + 0x220), [2., 0., 0.]);
    assert_eq!(vec(&mut b, ACTOR + 0x22c), [0., 0., 1.]);
    assert_eq!(f32::from_bits(b.read_word(ACTOR + 0x390).unwrap()), -5.);
    assert_eq!(b.read_word(ACTOR + 0x1c).unwrap(), 0);
}

#[test]
fn in_place_transform_preserves_cancellation_order_for_each_column() {
    let mut b = setup();
    let matrix = ACTOR + 0x18c;
    // Each dot product cancels large opposite terms. Adding the unit term
    // before the positive large term would discard it at PC24 precision.
    // Moving translation ahead of those products also changes the result.
    for (row, v) in [
        [16_777_216., 1., 1.],
        [1., 16_777_216., 16_777_216.],
        [-16_777_216., -16_777_216., -16_777_216.],
        [0.5, 0.5, 0.5],
    ]
    .into_iter()
    .enumerate()
    {
        set_vec(&mut b, matrix + row as u32 * 0x10, v);
    }
    assert_eq!(transform(&mut b, matrix, [1.; 3]).unwrap(), [1.5; 3]);
}
