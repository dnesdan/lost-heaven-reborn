use super::*;
use crate::native_precision::{current_precision, Pc24PrecisionState};
use crate::vehicle_init::{
    VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
};
use std::collections::{BTreeMap, VecDeque};

const BASE: u32 = 0x7d00_0000;

#[derive(Clone, Debug, PartialEq)]
enum Event {
    ReadByte(u32),
    ReadWord(u32),
    WriteByte(u32, u8),
    WriteWord(u32, u32),
    Effect(VehicleTickEffect),
    Gear(i32),
    MotorForce(u32, u32),
    MotorRot(u32, u32),
    Engine(u32, u32, u32),
    MaximumMoment(u32, i32, bool),
}
impl Event {
    fn is_callback(&self) -> bool {
        !matches!(
            self,
            Self::ReadByte(_) | Self::ReadWord(_) | Self::WriteByte(..) | Self::WriteWord(..)
        )
    }
}

#[derive(Clone)]
enum Change {
    Word(usize, u32),
    Byte(usize, u8),
}

struct Prefix {
    bytes: Vec<u8>,
    precision: bool,
}

struct Backend {
    bytes: Vec<u8>,
    events: Vec<Event>,
    prefixes: Vec<Prefix>,
    capture_prefixes: bool,
    fail_event: Option<usize>,
    callback_count: usize,
    changes: BTreeMap<usize, Vec<Change>>,
    fail_callback_after: Option<usize>,
    force_results: VecDeque<f32>,
    redirect_after_read: Option<(u32, u32)>,
}
impl Backend {
    fn new(mode: u32) -> Self {
        let mut b = Self {
            bytes: vec![0; 0x8000],
            events: Vec::new(),
            prefixes: Vec::new(),
            capture_prefixes: false,
            fail_event: None,
            callback_count: 0,
            changes: BTreeMap::new(),
            fail_callback_after: None,
            force_results: VecDeque::from([2.0, 6.0]),
            redirect_after_read: None,
        };
        b.put(0x1a4, 1);
        b.put(0xb8, mode);
        for offset in [0x44, 0x548, 0x5ac, 0x570, 0x574] {
            b.float(offset, 1.0);
        }
        b.float(0x3a4, 0.25);
        b.float(0x3a8, -0.5);
        b.bytes[0x4ab] = 1;
        b.float(0x5e0, 0.5);
        b
    }
    fn put(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn float(&mut self, offset: usize, value: f32) {
        self.put(offset, value.to_bits());
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn record(&mut self, event: Event) -> Result<(), u32> {
        let index = self.events.len();
        if self.capture_prefixes {
            self.prefixes.push(Prefix {
                bytes: self.bytes.clone(),
                precision: current_precision().unwrap(),
            });
        }
        self.events.push(event);
        if self.fail_event == Some(index) {
            Err(index as u32)
        } else {
            Ok(())
        }
    }
    // Spies apply explicit edits to the same storage used by the tick.
    fn callback(&mut self, event: Event) -> Result<(), u32> {
        self.record(event)?;
        let index = self.callback_count;
        self.callback_count += 1;
        if let Some(changes) = self.changes.remove(&index) {
            for change in changes {
                match change {
                    Change::Word(offset, value) => self.put(offset, value),
                    Change::Byte(offset, value) => self.bytes[offset] = value,
                }
            }
        }
        if self.fail_callback_after == Some(index) {
            Err(0xf000_0000 | index as u32)
        } else {
            Ok(())
        }
    }
    fn callbacks(&self) -> Vec<Event> {
        self.events
            .iter()
            .filter(|event| event.is_callback())
            .cloned()
            .collect()
    }
}

impl VehicleInitBackend for Backend {
    type Error = u32;
    fn read_byte(&mut self, address: u32) -> Result<u8, u32> {
        self.record(Event::ReadByte(address))?;
        self.bytes
            .get(address.wrapping_sub(BASE) as usize)
            .copied()
            .ok_or(address)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, u32> {
        self.record(Event::ReadWord(address))?;
        let offset = address.wrapping_sub(BASE) as usize;
        let value = self
            .bytes
            .get(offset..offset + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            .ok_or(address)?;
        match self.redirect_after_read {
            Some((trigger, root)) if trigger == address => {
                self.put(0xc38, root);
                self.redirect_after_read = None;
            }
            _ => {}
        }
        Ok(value)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), u32> {
        self.record(Event::WriteByte(address, value))?;
        *self
            .bytes
            .get_mut(address.wrapping_sub(BASE) as usize)
            .ok_or(address)? = value;
        Ok(())
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), u32> {
        self.record(Event::WriteWord(address, value))?;
        let offset = address.wrapping_sub(BASE) as usize;
        self.bytes
            .get_mut(offset..offset + 4)
            .ok_or(address)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn free(&mut self, address: u32) -> Result<(), u32> {
        Err(address)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, u32> {
        Err(bytes)
    }
    fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, frame: u32) -> Result<(), u32> {
        Err(frame)
    }
    fn enumerate(
        &mut self,
        model: u32,
        _: VehicleInitQuery,
        _: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<u32>> {
        Err(VehicleInitUnavailable::Effect(model))
    }
}
impl VehicleTickBackend for Backend {
    fn tick_effect(&mut self, vehicle: u32, effect: VehicleTickEffect) -> Result<(), u32> {
        assert_eq!(vehicle, BASE);
        self.callback(Event::Effect(effect))
    }
    fn tick_set_gear(&mut self, vehicle: u32, gear: i32) -> Result<(), u32> {
        assert_eq!(vehicle, BASE);
        self.callback(Event::Gear(gear))
    }
    fn tick_motor_force(&mut self, vehicle: u32, dt: f32, power: f32) -> Result<f32, u32> {
        assert_eq!(vehicle, BASE);
        self.callback(Event::MotorForce(dt.to_bits(), power.to_bits()))?;
        self.force_results.pop_front().ok_or(0xffff_ffff)
    }
    fn tick_motor_rot(&mut self, vehicle: u32, dt: f32, force: f32) -> Result<(), u32> {
        assert_eq!(vehicle, BASE);
        self.callback(Event::MotorRot(dt.to_bits(), force.to_bits()))
    }
    fn tick_engine(&mut self, vehicle: u32, dt: f32, first: f32, second: f32) -> Result<(), u32> {
        assert_eq!(vehicle, BASE);
        self.callback(Event::Engine(
            dt.to_bits(),
            first.to_bits(),
            second.to_bits(),
        ))
    }
    fn tick_maximum_moment(
        &mut self,
        vehicle: u32,
        rotation: f32,
        gear: i32,
        reverse: bool,
    ) -> Result<Option<f32>, u32> {
        assert_eq!(vehicle, BASE);
        self.callback(Event::MaximumMoment(rotation.to_bits(), gear, reverse))?;
        Ok(Some(2.0))
    }
}

fn run(b: &mut Backend, milliseconds: u32) -> (Out<(), u32>, bool) {
    let mut precision = Pc24PrecisionState::default();
    let result = precision.run(|| vehicle_tick(b, BASE, milliseconds));
    (result, precision.precision_raised())
}

fn callback_fixture() -> Backend {
    let mut b = Backend::new(2);
    for (offset, value) in [
        (0x540, 1.0),
        (0x544, 3.0),
        (0xc30, 10.0),
        (0x548, 2.0),
        (0x1f9c, 4.0),
        (0x498, 1.0),
        (0x434, 1.0),
        (0x5a4, 7.0),
    ] {
        b.float(offset, value);
    }
    b.bytes[0x432] = 1;
    b.put(0xc68, 0x20);
    b.changes
        .insert(0, vec![Change::Word(0xb8, 1), Change::Byte(0xc2c, 1)]);
    b.changes.insert(
        1,
        vec![
            Change::Word(0x3a4, (-0.5_f32).to_bits()),
            Change::Word(0x3a8, 0.25_f32.to_bits()),
        ],
    );
    b.changes
        .insert(2, vec![Change::Word(0x5e0, 0.75_f32.to_bits())]);
    b.changes.insert(
        3,
        vec![
            Change::Word(0x540, 5.0_f32.to_bits()),
            Change::Word(0x544, (-0.75_f32).to_bits()),
        ],
    );
    b.changes
        .insert(4, vec![Change::Word(0x44, 2.0_f32.to_bits())]);
    b.changes.insert(5, vec![Change::Byte(0x5f0, 1)]);
    b.changes.insert(
        6,
        vec![
            Change::Word(0x1a4, 11),
            Change::Word(0x4c4, 3),
            Change::Word(0xc38, BASE + 0x3000),
            Change::Word(0x320, 1.0_f32.to_bits()),
            Change::Word(0x324, 2.0_f32.to_bits()),
            Change::Word(0x328, 3.0_f32.to_bits()),
            Change::Word(0x1a8, 1000),
            Change::Word(4, 4001),
        ],
    );
    for (index, flags) in [8, 0x18, 8].into_iter().enumerate() {
        let wheel = 0x4000 + index * 0x200;
        b.put(0x3000 + index * 4, BASE + wheel as u32);
        b.put(wheel + 0x120, flags);
    }
    b
}

#[test]
fn callbacks_feed_later_phases_without_replacing_earlier_locals() {
    let mut b = callback_fixture();
    b.capture_prefixes = true;
    assert_eq!(run(&mut b, 1000).0, Ok(()));
    let dt = 1.0_f32.to_bits();
    assert_eq!(
        b.callbacks(),
        [
            Event::Effect(VehicleTickEffect::SteeringWheels { seconds: 1.0 }),
            Event::Effect(VehicleTickEffect::ProcedureEbe40 { seconds: 1.0 }),
            Event::Effect(VehicleTickEffect::ProcedureEd910),
            Event::MotorForce(dt, 0.5_f32.to_bits()),
            Event::MotorForce(dt, 0.75_f32.to_bits()),
            Event::Engine(dt, (-2.0_f32).to_bits(), (-6.0_f32).to_bits()),
            Event::Effect(VehicleTickEffect::EngineSound { enabled: false }),
        ]
    );
    // Fuel consumes the prior power channels, before the callbacks publish inputs.
    assert_eq!(b.word(0xc30), 8.3_f32.to_bits());
    assert_eq!(b.word(0x3bc), (-0.5_f32).to_bits());
    assert_eq!(b.word(0x3c0), 0.25_f32.to_bits());
    assert_eq!(b.word(0x5e4), 0.75_f32.to_bits());
    let engine = b
        .events
        .iter()
        .position(|event| matches!(event, Event::Engine(..)))
        .unwrap();
    for (offset, expected) in [(0x5a8, 7.0_f32), (0x54c, 2.0), (0x5a0, 0.0)] {
        let bytes = &b.prefixes[engine].bytes[offset..offset + 4];
        assert_eq!(bytes, expected.to_le_bytes());
    }
    let reads: Vec<_> = b
        .events
        .iter()
        .filter_map(|event| match *event {
            Event::ReadWord(a) if [BASE + 0x4120, BASE + 0x4320, BASE + 0x4520].contains(&a) => {
                Some(a)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        reads,
        [
            BASE + 0x4520,
            BASE + 0x4320,
            BASE + 0x4120,
            BASE + 0x4520,
            BASE + 0x4320,
        ]
    );
    let position = b
        .events
        .iter()
        .position(|event| *event == Event::WriteWord(BASE + 8, 1.0_f32.to_bits()))
        .unwrap();
    assert_eq!(
        &b.events[position..],
        [
            Event::WriteWord(BASE + 8, 1.0_f32.to_bits()),
            Event::ReadWord(BASE + 0x324),
            Event::WriteWord(BASE + 0xc, 2.0_f32.to_bits()),
            Event::WriteWord(BASE + 4, 0),
            Event::ReadWord(BASE + 0x328),
            Event::WriteWord(BASE + 0x10, 3.0_f32.to_bits()),
            Event::ReadWord(BASE + 0x1a8),
            Event::WriteWord(BASE + 0x1a8, 2000),
            Event::ReadWord(BASE + 0x1a4),
            Event::ReadWord(BASE + 0x1a4),
            Event::WriteWord(BASE + 0x1a4, 10),
        ]
    );
}

fn coasting_fixture() -> Backend {
    let mut b = Backend::new(0);
    b.put(0x1a4, 0);
    b.put(0x55c, 1);
    b.float(0x3a4, 0.0);
    b.float(0x3a8, 0.0);
    b.float(0x5ec, 1.0);
    b.bytes[0x5f0] = 1;
    b.put(0x320, 0x7fc0_1234);
    b.put(0x5e8, 0x20);
    b.put(4, 91);
    b.put(0x5a8, 0x1234_5678);
    b.put(0x1a8, 990);
    b.changes
        .insert(1, vec![Change::Word(0x548, 0.5_f32.to_bits())]);
    b
}

#[test]
fn coasting_finishes_the_tick_before_clutch_engine_and_tail() {
    for (rpm, stop, elapsed, ms, expected_elapsed, flags) in [
        (2.0_f32, 0, 77, 16, 77, 1),
        (0.5, 1, 77, 16, 77, 0),
        (0.5, 0, 990, 16, 1006, 2),
        (0.5, 0, u32::MAX - 7, 16, 8, 0),
        (0.5, 0, 1000, 0, 1001, 2),
    ] {
        let mut b = coasting_fixture();
        b.bytes[0x2c0] = stop;
        b.put(0x1a8, elapsed);
        b.changes
            .insert(1, vec![Change::Word(0x548, rpm.to_bits())]);
        assert_eq!(run(&mut b, ms).0, Ok(()));
        let dt = if ms == 0 { 0.001_f32 } else { 0.016_f32 }.to_bits();
        assert_eq!(
            b.callbacks(),
            [
                Event::MotorForce(dt, 0.0_f32.to_bits()),
                Event::MotorRot(dt, 2.0_f32.to_bits()),
            ]
        );
        assert_eq!(b.word(0x1a8), expected_elapsed);
        assert_eq!(b.word(0x1a4), flags);
        assert_eq!(b.word(4), 91);
        assert_eq!(b.word(0x5a8), 0x1234_5678);
        assert_eq!(b.bytes[0x5f0], 1);
        assert!(!b.events.contains(&Event::ReadWord(BASE + 0x320)));
    }
}

#[test]
fn raw_targets_and_unknown_mode_keep_their_original_publication_prefix() {
    for mode in [0, 3] {
        let mut b = Backend::new(mode);
        b.put(0x1a4, 0x0100_0001);
        let payload = 0x7fc0_1234;
        b.put(0x3a8, payload);
        b.put(0x5e0, payload);
        let mut expected = b.bytes.clone();
        for (offset, value) in [
            (0x1a4, 1),
            (0x544, payload),
            (0x540, 0.25_f32.to_bits()),
            (0x5e4, payload),
            (0x3bc, 0.25_f32.to_bits()),
            (0x3c0, payload),
        ] {
            expected[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let result = run(&mut b, 1).0;
        if mode == 3 {
            assert_eq!(result, Err(VehicleTickUnavailable::UnrecoveredMode(3)));
            assert_eq!(b.bytes, expected);
            assert!(b.callbacks().is_empty());
        } else {
            assert_eq!(result, Ok(()));
            for offset in [0x544, 0x3c0, 0x5e4] {
                assert_eq!(b.word(offset), payload);
            }
        }
    }
}

fn gear_fixture() -> Backend {
    let mut b = Backend::new(0);
    b.put(0x55c, 1);
    b.put(0x568, 3);
    b.bytes[0xc2c] = 1;
    b.bytes[0x4cc] = 1;
    b.bytes[0x2c0] = 1;
    b.bytes[0x394] = 1;
    b.put(0x5e8, 0x20);
    for (offset, value) in [
        (0xc30, 10.0),
        (0x1f9c, 1.0),
        (0x3a4, 0.5),
        (0x5dc, 1.0),
        (0x4d8, 0.5),
        (0x5ec, 2.0),
        (0x57c, 1.0),
        (0x578, 1.0),
        (0x510, 1.0),
        (0x4dc, 3.0),
    ] {
        b.float(offset, value);
    }
    b.changes.insert(
        0,
        vec![
            Change::Word(0x55c, 3),
            Change::Word(0x574, 2.0_f32.to_bits()),
        ],
    );
    b
}

#[test]
fn automatic_gear_keeps_its_next_index_across_the_gear_callback() {
    let mut b = gear_fixture();
    assert_eq!(run(&mut b, 1).0, Ok(()));
    let dt = 0.001_f32.to_bits();
    assert_eq!(
        b.callbacks(),
        [
            Event::Gear(2),
            Event::MotorForce(dt, 0.5_f32.to_bits()),
            Event::Gear(3),
            Event::Engine(dt, 1.0_f32.to_bits(), 1.0_f32.to_bits()),
        ]
    );
    let start = b
        .events
        .iter()
        .position(|event| matches!(event, Event::MotorForce(..)))
        .unwrap();
    let end = b
        .events
        .iter()
        .position(|event| *event == Event::Gear(3))
        .unwrap();
    let reads: Vec<_> = b.events[start..end]
        .iter()
        .filter_map(|event| match *event {
            Event::ReadWord(a) if [BASE + 0x574, BASE + 0x578, BASE + 0x57c].contains(&a) => {
                Some(a)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        reads,
        [
            BASE + 0x57c,
            BASE + 0x57c,
            BASE + 0x578,
            BASE + 0x574,
            BASE + 0x578
        ]
    );
}

#[test]
fn contact_passes_reload_the_pointer_table_and_keep_empty_list_transitions() {
    let mut b = Backend::new(0);
    b.put(0x1a4, 9);
    b.put(0x4c4, 2);
    b.put(0xc38, BASE + 0x3000);
    for (offset, value) in [
        (0x3000, BASE + 0x4000),
        (0x3004, BASE + 0x4200),
        (0x3100, BASE + 0x4400),
        (0x3104, BASE + 0x4600),
        (0x4120, 8),
        (0x4320, 8),
        (0x4520, 8),
        (0x4720, 0x18),
    ] {
        b.put(offset, value);
    }
    b.redirect_after_read = Some((BASE + 0x4320, BASE + 0x3100));
    assert_eq!(run(&mut b, 1).0, Ok(()));
    let reads: Vec<_> = b
        .events
        .iter()
        .filter_map(|event| match *event {
            Event::ReadWord(a) if (BASE + 0x4000..BASE + 0x5000).contains(&a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(reads, [BASE + 0x4320, BASE + 0x4520, BASE + 0x4720]);
    assert_eq!(b.word(0x1a4), 9);
    for (flags, expected) in [(1, 9), (9, 1)] {
        let mut b = Backend::new(0);
        b.put(0x1a4, flags);
        assert_eq!(run(&mut b, 1).0, Ok(()));
        assert_eq!(b.word(0x1a4), expected);
    }
}

#[test]
fn time_conversion_does_not_raise_precision_before_initial_stores_complete() {
    let make = || {
        let mut b = Backend::new(0);
        b.bytes[0x1fa8] = 1;
        b.bytes[0xc2c] = 9;
        b
    };
    let mut completed = make();
    assert_eq!(run(&mut completed, 3).0, Ok(()));
    for (event, expected_precision) in [
        (Event::WriteWord(BASE + 0x1a4, 1), false),
        (Event::ReadWord(BASE + 0xb8), true),
    ] {
        let index = completed.events.iter().position(|e| *e == event).unwrap();
        let mut b = make();
        b.fail_event = Some(index);
        assert_eq!(
            run(&mut b, 3),
            (
                Err(VehicleTickUnavailable::Effect(index as u32)),
                expected_precision
            )
        );
        assert_eq!(b.bytes[0xc2c], 0);
        assert_eq!(b.bytes[0x1fa9], 9);
        assert_eq!(b.word(0x3a4), 0);
        assert_eq!(b.word(0x3a8), 0);
    }
}

#[test]
fn each_dependency_failure_retains_exactly_the_completed_prefix() {
    for (make, ms) in [
        (callback_fixture as fn() -> Backend, 1000),
        (gear_fixture, 1),
        (coasting_fixture, 16),
    ] {
        let mut completed = make();
        completed.capture_prefixes = true;
        assert_eq!(run(&mut completed, ms).0, Ok(()));
        for (index, prefix) in completed.prefixes.iter().enumerate() {
            let mut b = make();
            b.fail_event = Some(index);
            assert_eq!(
                run(&mut b, ms),
                (
                    Err(VehicleTickUnavailable::Effect(index as u32)),
                    prefix.precision
                )
            );
            assert_eq!(b.events, completed.events[..=index]);
            assert_eq!(b.bytes, prefix.bytes);
        }
    }
}

#[test]
fn wedged_clutch_copy_path_self_sustains_under_full_throttle() {
    // Probe wedge at [-1314,-227] (C:\Temp\probe_clutch.log, Game.exe
    // SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27):
    // gear 1/1, full power channel, no brake, engine on, motor ~43.4 rad/s
    // against static wheels, clutch progress 0x5dc ~0.06 with flags 0x2c
    // (LimitedByTarget, no Engaging bit), target 0x5cc equal to the
    // progress. One full 16 ms tick with re-applied full throttle leaves
    // the clutch disengaged: the ported tick.clutch() takes the 0x5cc>=0
    // copy path, so the low progress self-sustains. This pins ported
    // behavior only; it claims nothing about the native body (no overlay
    // entry, no REA evidence ID).
    let mut b = Backend::new(0);
    // Fuel/engine prerequisites (callback_fixture pattern) so the tick
    // reaches the clutch phase with the throttle intact.
    b.float(0xc30, 10.0);
    b.float(0x498, 1.0);
    b.float(0x434, 1.0);
    b.bytes[0x432] = 1;
    b.put(0xc68, 0x20);
    b.bytes[0xc2c] = 1;
    // Probe-seeded wedge snapshot (first boxed window: 0x55c=0x1,
    // 0x560=0x1, 0x3a4=1.0, 0x3a8=0, 0x5b8=0, 0x3cc=0, 0x59c=0x3c9539e4,
    // 0x548=0x422daf12, 0x5e0=1.0, 0x5dc=0x3d73287b, 0x5e8=0x2c,
    // 0x540=1.0, 0x544=0, 0x494=0, 0x5e4=1.0, 0x5cc=0x3d73287b,
    // 0x5c8=0x422d5556, 0x5c4=0x5c0=0x5ec=0x41555556, 0x1a4=0x1).
    // Probe 0x59c ~0.018 reads as standstill; every speed threshold on
    // the path (0.5/0.7/1.0) sits far above it.
    b.put(0x55c, 1);
    b.put(0x560, 1);
    b.float(0x3a4, 1.0);
    b.float(0x3a8, 0.0);
    b.float(0x540, 1.0);
    b.put(0x24, 0x3dde_b334);
    b.put(0x548, 0x422d_af12);
    b.put(0x5ec, 0x4155_5556);
    b.put(0x5c8, 0x422d_5556);
    b.put(0x5c4, 0x4155_5556);
    b.put(0x5c0, 0x4155_5556);
    b.put(0x5dc, 0x3d73_287b);
    b.put(0x5cc, 0x3d73_287b);
    b.float(0x5e4, 1.0);
    b.float(0x5e0, 1.0);
    b.put(0x5d4, 0x4055_5555);
    b.put(0x5d8, 0x40a0_0000);
    b.put(0x5e8, 0x2c);
    b.put(0x1a4, 1);
    b.put(0x59c, 0x3c95_39e4);
    // Host fixtures for inputs the wedge census does not capture (redline
    // threshold, transmission ratio); documented here, never presented
    // as taxi values.
    b.float(0x1f9c, 600.0);
    b.float(0x488, 200.0);
    b.float(0x5d0, 4.0);
    assert_eq!(run(&mut b, 16).0, Ok(()));
    // No re-engage: the copy path preserves the disengaged progress and
    // the LimitedByTarget flags, and the gear/throttle survive the tick.
    assert_eq!(b.word(0x5dc), 0x3d73_287b);
    assert_eq!(b.word(0x5cc), 0x3d73_287b);
    assert_eq!(b.word(0x5e8), 0x2c);
    assert_eq!(b.word(0x55c), 1);
    assert_eq!(b.word(0x3a4), 1.0_f32.to_bits());
}

#[test]
fn wedged_clutch_negative_target_takes_recompute_path() {
    // Contrast state from the same wedge (third boxed window:
    // 0x5dc=0x3da2691f with 0x5cc=-1.0, motor 0x41e5781e): the ported
    // tick.clutch() takes the recompute arm instead of the copy arm. The
    // absolute value below depends on the mocked force total (2.0) and
    // the fixture transmission ratio (4.0); the pinned fact is the arm
    // taken, not a native outcome (no overlay entry, no REA evidence ID).
    let mut b = Backend::new(0);
    b.float(0xc30, 10.0);
    b.float(0x498, 1.0);
    b.float(0x434, 1.0);
    b.bytes[0x432] = 1;
    b.put(0xc68, 0x20);
    b.bytes[0xc2c] = 1;
    b.put(0x55c, 1);
    b.put(0x560, 1);
    b.float(0x3a4, 1.0);
    b.float(0x3a8, 0.0);
    b.float(0x540, 1.0);
    b.put(0x24, 0x3dde_b334);
    b.put(0x548, 0x41e5_781e);
    b.put(0x5ec, 0x4155_5556);
    b.put(0x5c8, 0x422d_5556);
    b.put(0x5c4, 0x4155_5556);
    b.put(0x5c0, 0x4155_5556);
    b.put(0x5dc, 0x3da2_691f);
    b.put(0x5cc, 0xbf80_0000);
    b.float(0x5e4, 1.0);
    b.float(0x5e0, 1.0);
    b.put(0x5d4, 0x4055_5555);
    b.put(0x5d8, 0x40a0_0000);
    b.put(0x5e8, 0x2c);
    b.put(0x1a4, 1);
    b.put(0x59c, 0x3cd9_9cb9);
    b.float(0x1f9c, 600.0);
    b.float(0x488, 200.0);
    b.float(0x5d0, 4.0);
    assert_eq!(run(&mut b, 16).0, Ok(()));
    // Recompute arm: total/(2*denominator) with the mocked total, so the
    // progress rises instead of self-sustaining, while the negative
    // target is left in place (this arm publishes 0x5dc only). Flags keep
    // LimitedByTarget.
    assert_eq!(b.word(0x5dc), 0.25_f32.to_bits());
    assert_eq!(b.word(0x5cc), 0xbf80_0000);
    assert_eq!(b.word(0x5e8), 0x2c);
    assert_eq!(b.word(0x55c), 1);
}

#[test]
fn wedged_clutch_throttle_lift_snaps_progress_to_max_and_requests_engage() {
    // Same wedge snapshot as the copy-path trap, but with the throttle
    // released (0x3a4 = 0.0, so power 0x540 <= threshold 0x24): the ported
    // tick.clutch() takes the power<=threshold arm, snaps 0x5dc to the max
    // 0x5e4, clears the stall bits and requests Engaging (bit0). This pins
    // the native-modelled re-engage entry the probe choreography can drive
    // without touching ported logic (no overlay entry, no REA evidence ID).
    let mut b = Backend::new(0);
    b.float(0xc30, 10.0);
    b.float(0x498, 1.0);
    b.float(0x434, 1.0);
    b.bytes[0x432] = 1;
    b.put(0xc68, 0x20);
    b.bytes[0xc2c] = 1;
    b.put(0x55c, 1);
    b.put(0x560, 1);
    b.float(0x3a4, 0.0);
    b.float(0x3a8, 0.0);
    b.float(0x540, 0.0);
    b.put(0x24, 0x3dde_b334);
    b.put(0x548, 0x422d_af12);
    b.put(0x5ec, 0x4155_5556);
    b.put(0x5c8, 0x422d_5556);
    b.put(0x5c4, 0x4155_5556);
    b.put(0x5c0, 0x4155_5556);
    b.put(0x5dc, 0x3d73_287b);
    b.put(0x5cc, 0x3d73_287b);
    b.float(0x5e4, 1.0);
    b.float(0x5e0, 1.0);
    b.put(0x5d4, 0x4055_5555);
    b.put(0x5d8, 0x40a0_0000);
    b.put(0x5e8, 0x2c);
    b.put(0x1a4, 1);
    b.put(0x59c, 0x3c95_39e4);
    b.float(0x1f9c, 600.0);
    b.float(0x488, 200.0);
    b.float(0x5d0, 4.0);
    assert_eq!(run(&mut b, 16).0, Ok(()));
    // Clutch snapped to max, target parked at -1.0, Engaging requested
    // (bit0) with the stall bits cleared; gear and lifted throttle survive.
    assert_eq!(b.word(0x5dc), 1.0_f32.to_bits());
    assert_eq!(b.word(0x5cc), 0xbf80_0000);
    assert_eq!(b.word(0x5e8), 0x21);
    assert_eq!(b.word(0x55c), 1);
    assert_eq!(b.word(0x540), 0.0_f32.to_bits());
}

#[test]
fn wedged_clutch_even_motion_flag_defers_snap_by_one_tick() {
    // Live wedge snapshots show 0x1a4 both set (0x1) and clear (0x2). With
    // bit0 clear the tick takes the early coasting arm: it republishes the
    // motor rotation, sets bit0 and returns Complete, so clutch() never
    // runs and the snap waits for the next tick. (prepare_motion_state
    // clears bit1 every tick, so an even seed lands on 0x1, not 0x3.)
    // This pins that the lift needs at least two coast ticks from an
    // even flag, not that it is refused (no overlay entry, no REA
    // evidence ID).
    let mut b = Backend::new(0);
    b.float(0xc30, 10.0);
    b.float(0x498, 1.0);
    b.float(0x434, 1.0);
    b.bytes[0x432] = 1;
    b.put(0xc68, 0x20);
    b.bytes[0xc2c] = 1;
    b.put(0x55c, 1);
    b.put(0x560, 1);
    b.float(0x3a4, 0.0);
    b.float(0x3a8, 0.0);
    b.float(0x540, 0.0);
    b.put(0x24, 0x3dde_b334);
    b.put(0x548, 0x422d_af12);
    b.put(0x5ec, 0x4155_5556);
    b.put(0x5c8, 0x422d_5556);
    b.put(0x5c4, 0x4155_5556);
    b.put(0x5c0, 0x4155_5556);
    b.put(0x5dc, 0x3d73_287b);
    b.put(0x5cc, 0x3d73_287b);
    b.float(0x5e4, 1.0);
    b.float(0x5e0, 1.0);
    b.put(0x5d4, 0x4055_5555);
    b.put(0x5d8, 0x40a0_0000);
    b.put(0x5e8, 0x2c);
    b.put(0x1a4, 2);
    b.put(0x59c, 0x3c95_39e4);
    b.float(0x1f9c, 600.0);
    b.float(0x488, 200.0);
    b.float(0x5d0, 4.0);
    assert_eq!(run(&mut b, 16).0, Ok(()));
    // Early arm: clutch untouched, motion bit set for the next tick.
    assert_eq!(b.word(0x5dc), 0x3d73_287b);
    assert_eq!(b.word(0x1a4), 1);
    // Second coast tick reaches clutch() and snaps like the odd-flag case.
    assert_eq!(run(&mut b, 16).0, Ok(()));
    assert_eq!(b.word(0x5dc), 1.0_f32.to_bits());
    assert_eq!(b.word(0x5cc), 0xbf80_0000);
    assert_eq!(b.word(0x5e8), 0x21);
}

#[test]
fn a_failed_callback_retains_its_own_mutations_and_stops_later_phases() {
    let mut completed = callback_fixture();
    completed.capture_prefixes = true;
    assert_eq!(run(&mut completed, 1000).0, Ok(()));
    for callback in [3, 5] {
        let event = completed
            .events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.is_callback())
            .nth(callback)
            .unwrap()
            .0;
        let mut b = callback_fixture();
        b.fail_callback_after = Some(callback);
        assert_eq!(
            run(&mut b, 1000).0,
            Err(VehicleTickUnavailable::Effect(
                0xf000_0000 | callback as u32
            ))
        );
        assert_eq!(b.events, completed.events[..=event]);
        assert_eq!(b.bytes, completed.prefixes[event + 1].bytes);
        if callback == 3 {
            assert_eq!(b.word(0x540), 5.0_f32.to_bits());
            assert_eq!(b.word(0x544), (-0.75_f32).to_bits());
        } else {
            assert_eq!(b.bytes[0x5f0], 1);
        }
    }
}
