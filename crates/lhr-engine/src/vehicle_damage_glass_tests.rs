use super::*;
use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
use crate::vehicle_init::{
    VehicleInitBackend, VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery,
    VehicleInitUnavailable,
};

#[derive(Default)]
struct Owner {
    memory: VehicleFrameMemory,
    calls: Vec<(u32, u32, Vec<u32>)>,
    callbacks: Vec<VehicleDamageGlassCallback>,
    breaks: Vec<VehicleGlassBreakCall>,
    fail_callback: bool,
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
impl VehicleDamageMeshBackend for Owner {
    fn damage_virtual(&mut self, object: u32, slot: u32, args: &[u32]) -> Result<u32, Self::Error> {
        self.calls.push((object, slot, args.to_vec()));
        match (object, slot, args) {
            (0x5000, 4, [0]) => Ok(0x5100),
            (0x5100, 0x14, [0]) => Ok(0x5200),
            (0x5200, 0, [0x5300]) => Ok(0),
            _ => panic!("unexpected glass virtual"),
        }
    }
    fn damage_update_world(&mut self, _: u32) -> Result<(), Self::Error> {
        panic!("unexpected update")
    }
    fn damage_rand(&mut self) -> Result<i32, Self::Error> {
        panic!("unexpected random")
    }
    fn damage_notification(
        &mut self,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: u16,
        _: [f32; 3],
        _: [f32; 3],
        _: bool,
    ) -> Result<(), Self::Error> {
        panic!("unexpected notification")
    }
    fn damage_break_glass(&mut self, _: u32, _: u32, _: [f32; 3]) -> Result<(), Self::Error> {
        panic!("unexpected recursion")
    }
    fn damage_detach_part(
        &mut self,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: [f32; 3],
        _: u32,
    ) -> Result<(), Self::Error> {
        panic!("unexpected detach")
    }
    fn damage_lamp_switch(&mut self, _: u32, _: u32, _: u32) -> Result<(), Self::Error> {
        panic!("unexpected lamp")
    }
}
impl VehicleDamageGlassBackend for Owner {
    fn damage_glass_callback(
        &mut self,
        call: VehicleDamageGlassCallback,
    ) -> Result<(), Self::Error> {
        assert_eq!(self.read_byte(0x400e).unwrap(), 0xa5);
        self.callbacks.push(call);
        if self.fail_callback {
            self.read_word(0xffff0000)?;
        }
        Ok(())
    }
}
impl CarDamageGlassBackend for Owner {
    fn car_glass_break(&mut self, call: VehicleGlassBreakCall) -> Result<(), Self::Error> {
        self.breaks.push(call);
        Ok(())
    }
}
fn setup(health: f32) -> Owner {
    let mut b = Owner::default();
    b.memory.map_bytes(0x1000, &vec![0; 0x2300]).unwrap();
    b.memory.map_bytes(0x4000, &[0; 0x100]).unwrap();
    for (a, v) in [
        (0x1218, 0x4000),
        (0x4004, 0x5000),
        (0x4008, 0x5300),
        (0x1300, 0x4262f0),
        (0x1c74, 0x0f90),
        (0x401c, 100f32.to_bits()),
        (0x4020, health.to_bits()),
    ] {
        b.write_word(a, v).unwrap();
    }
    b.write_byte(0x400e, 0xa4).unwrap();
    for (i, c) in b"broken.wav\0".iter().copied().enumerate() {
        b.write_byte(0x2cb0 + i as u32, c).unwrap();
    }
    b
}
#[test]
fn damaged_glass_swaps_material_before_exhausted_flag_and_callback() {
    let mut b = setup(0.);
    vehicle_damage_glass(&mut b, 0x1000, 0, [1., 2., 3.]).unwrap();
    assert_eq!(
        b.calls,
        [
            (0x5000, 4, vec![0]),
            (0x5100, 0x14, vec![0]),
            (0x5200, 0, vec![0x5300])
        ]
    );
    assert_eq!(
        b.callbacks,
        [VehicleDamageGlassCallback {
            function: 0x4262f0,
            context: 0x0f90,
            index: 0,
            direction: [1., 2., 3.],
            sound_name: Some(0x2cb0)
        }]
    );
    assert_eq!(b.read_byte(0x400e).unwrap(), 0xa5);
}
#[test]
fn partial_health_only_changes_material_and_empty_alternate_name_is_null() {
    let mut b = setup(50.);
    vehicle_damage_glass(&mut b, 0x1000, 0, [0.; 3]).unwrap();
    assert_eq!(b.calls.len(), 3);
    assert!(b.callbacks.is_empty());
    assert_eq!(b.read_byte(0x400e).unwrap(), 0xa4);
    b.write_byte(0x1491, 1).unwrap();
    b.write_word(0x4020, (-0f32).to_bits()).unwrap();
    vehicle_damage_glass(&mut b, 0x1000, 0, [0.; 3]).unwrap();
    assert_eq!(b.callbacks[0].sound_name, None);
}
#[test]
fn callback_error_retains_prior_material_calls_and_exhausted_flag() {
    let mut b = setup(-1.);
    b.fail_callback = true;
    assert!(matches!(
        vehicle_damage_glass(&mut b, 0x1000, 0, [0.; 3]),
        Err(VehicleDamageGlassError::Host(_))
    ));
    assert_eq!(b.calls.len(), 3);
    assert_eq!(b.callbacks.len(), 1);
    assert_eq!(b.read_byte(0x400e).unwrap(), 0xa5);
}

#[test]
fn car_callback_resolves_the_same_part_and_default_or_explicit_sound() {
    let mut b = setup(0.);
    b.memory
        .map_bytes(0x63788c, &0x6000u32.to_le_bytes())
        .unwrap();
    b.memory.map_bytes(0x6000, &[0; 0x100]).unwrap();
    b.write_word(0x6024, 0x7000).unwrap();
    b.write_word(0x1218, 0x4000).unwrap();
    b.write_word(0x4034, 0x8000).unwrap();
    let mut call = VehicleDamageGlassCallback {
        function: 0x4262f0,
        context: 0x0f90,
        index: 1,
        direction: [0.25, -0.5, 1.],
        sound_name: None,
    };
    // Context+288 is the same live row header as embedded vehicle+218.
    car_damage_glass_callback(&mut b, call).unwrap();
    assert_eq!(
        b.breaks[0],
        VehicleGlassBreakCall {
            manager: 0x7208,
            frame: 0x8000,
            direction: Some(call.direction),
            sound_name: 0x6323b8,
            retain_original: true,
        }
    );
    call.sound_name = Some(0x2cb0);
    car_damage_glass_callback(&mut b, call).unwrap();
    assert_eq!(b.breaks[1].sound_name, 0x2cb0);
}
