use super::*;
use crate::{
    vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError},
    vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
        VehicleUninitializedStorage,
    },
};

const VEHICLE: u32 = 0x1000;
const BYTES: usize = 0xc6c;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    ReadByte(u32),
    ReadWord(u32),
    WriteByte(u32, u8),
    WriteWord(u32, u32),
    Sound(u32, bool),
    FailedStart(u32),
}

#[derive(Debug, PartialEq, Eq)]
enum Error {
    Storage(VehicleMemoryError),
    Injected,
    Callback,
    UnexpectedOperation,
}
impl From<VehicleUninitializedStorage> for Error {
    fn from(error: VehicleUninitializedStorage) -> Self {
        Self::Storage(VehicleMemoryError::Uninitialized(error.address))
    }
}

#[derive(Clone, Copy, Default)]
struct Callback {
    engine_state: Option<u8>,
    marker: Option<u8>,
    fail_after_mutation: bool,
}

struct Backend {
    memory: VehicleFrameMemory,
    mapped_base: u32,
    mapped_len: usize,
    events: Vec<Event>,
    prefixes: Vec<Vec<u8>>,
    fail_at: Option<usize>,
    sound: Callback,
    failed_start: Callback,
}

impl Backend {
    fn mapped(base: u32, bytes: &[u8]) -> Self {
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(base, bytes).unwrap();
        Self {
            memory,
            mapped_base: base,
            mapped_len: bytes.len(),
            events: Vec::new(),
            prefixes: Vec::new(),
            fail_at: None,
            sound: Callback::default(),
            failed_start: Callback::default(),
        }
    }

    fn new() -> Self {
        let mut backend = Self::mapped(VEHICLE, &vec![0xa5; BYTES]);
        backend.byte(0xc2c, 0);
        backend.byte(0x432, 0);
        backend.word(0xc68, 0);
        backend.word(0xc30, 1.0f32.to_bits());
        backend.word(0x1b4, 1.0f32.to_bits());
        backend
    }

    fn byte(&mut self, offset: u32, value: u8) {
        self.memory.write_byte(VEHICLE + offset, value).unwrap();
    }

    fn word(&mut self, offset: u32, value: u32) {
        self.memory.write_word(VEHICLE + offset, value).unwrap();
    }

    fn snapshot(&mut self) -> Vec<u8> {
        (0..self.mapped_len)
            .map(|i| self.memory.read_byte(self.mapped_base + i as u32).unwrap())
            .collect()
    }

    fn record(&mut self, event: Event) -> Result<(), Error> {
        let before = self.snapshot();
        self.prefixes.push(before);
        self.events.push(event);
        if self.fail_at == Some(self.events.len()) {
            Err(Error::Injected)
        } else {
            Ok(())
        }
    }

    fn callback(&mut self, event: Event, action: Callback) -> Result<(), Error> {
        self.record(event)?;
        if let Some(state) = action.engine_state {
            self.byte(0xc2c, state);
        }
        if let Some(marker) = action.marker {
            self.byte(0x20, marker);
        }
        if action.fail_after_mutation {
            Err(Error::Callback)
        } else {
            Ok(())
        }
    }
}

impl VehicleInitBackend for Backend {
    type Error = Error;

    fn read_byte(&mut self, address: u32) -> Result<u8, Error> {
        self.record(Event::ReadByte(address))?;
        self.memory.read_byte(address).map_err(Error::Storage)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Error> {
        self.record(Event::ReadWord(address))?;
        self.memory.read_word(address).map_err(Error::Storage)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Error> {
        self.record(Event::WriteByte(address, value))?;
        self.memory
            .write_byte(address, value)
            .map_err(Error::Storage)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Error> {
        self.record(Event::WriteWord(address, value))?;
        self.memory
            .write_word(address, value)
            .map_err(Error::Storage)
    }
    fn allocate(&mut self, _: u32) -> Result<u32, Error> {
        Err(Error::UnexpectedOperation)
    }
    fn free(&mut self, _: u32) -> Result<(), Error> {
        Err(Error::UnexpectedOperation)
    }
    fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), Error> {
        Err(Error::UnexpectedOperation)
    }
    fn enumerate(
        &mut self,
        _: u32,
        _: VehicleInitQuery,
        _: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Error>> {
        Err(VehicleInitUnavailable::Effect(Error::UnexpectedOperation))
    }
}

impl VehicleEngineStateBackend for Backend {
    fn engine_state_sound(&mut self, vehicle: u32, enabled: bool) -> Result<(), Error> {
        self.callback(Event::Sound(vehicle, enabled), self.sound)
    }
    fn engine_state_failed_start_sound(&mut self, vehicle: u32) -> Result<(), Error> {
        self.callback(Event::FailedStart(vehicle), self.failed_start)
    }
}

fn expected_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn force_retains_both_fade_stores_and_never_reads_or_calls_sound() {
    for enabled in [false, true] {
        let mut b = Backend::new();
        let mut expected = b.snapshot();
        let mut events = vec![
            Event::WriteWord(VEHICLE + 0xc20, 5000.0f32.to_bits()),
            Event::WriteByte(VEHICLE + 0x5f0, 0),
            Event::WriteByte(VEHICLE + 0xc2c, u8::from(enabled)),
            Event::WriteByte(VEHICLE + 0x638, u8::from(enabled)),
        ];
        if enabled {
            events.push(Event::WriteWord(VEHICLE + 0xc20, 0));
        }
        expected_word(
            &mut expected,
            0xc20,
            if enabled { 0 } else { 5000.0f32.to_bits() },
        );
        expected[0x5f0] = 0;
        expected[0xc2c] = u8::from(enabled);
        expected[0x638] = u8::from(enabled);
        assert_eq!(
            vehicle_set_engine_on(&mut b, VEHICLE, enabled, true),
            Ok(true)
        );
        assert_eq!(b.events, events);
        assert_eq!(b.snapshot(), expected);
    }
}

#[test]
fn exact_state_noops_need_only_one_mapped_byte() {
    for enabled in [false, true] {
        let mut b = Backend::mapped(VEHICLE + 0xc2c, &[u8::from(enabled)]);
        assert_eq!(
            vehicle_set_engine_on(&mut b, VEHICLE, enabled, false),
            Ok(true)
        );
        assert_eq!(b.events, [Event::ReadByte(VEHICLE + 0xc2c)]);
        assert_eq!(b.snapshot(), [u8::from(enabled)]);
    }
}

#[test]
fn noncanonical_nonzero_state_is_not_an_already_enabled_noop() {
    for state in [2, 255] {
        let mut b = Backend::new();
        b.byte(0xc2c, state);
        let expected = b.snapshot();
        assert_eq!(
            vehicle_set_engine_on(&mut b, VEHICLE, true, false),
            Ok(true)
        );
        assert_eq!(
            b.events,
            [
                Event::ReadByte(VEHICLE + 0xc2c),
                Event::ReadWord(VEHICLE + 0xc68),
                Event::ReadWord(VEHICLE + 0xc30),
                Event::Sound(VEHICLE, true),
                Event::ReadByte(VEHICLE + 0xc2c),
            ]
        );
        assert_eq!(b.snapshot(), expected);
    }
}

#[test]
fn start_guards_keep_unordered_values_and_short_circuit_reads() {
    let threshold = 0.02f32.to_bits();
    let fuel_cases = [
        (f32::NEG_INFINITY, false),
        (-0.0, false),
        (0.0, false),
        (f32::from_bits(threshold - 1), false),
        (0.02, false),
        (f32::from_bits(threshold + 1), true),
        (1.0, true),
        (f32::INFINITY, true),
        (f32::from_bits(0x7fc1_2345), false),
    ];
    let damage_cases = [
        (f32::NEG_INFINITY, false),
        (-0.0, false),
        (0.0, false),
        (1.0, true),
        (f32::INFINITY, true),
        (f32::from_bits(0x7fc5_4321), false),
    ];
    for check_damage in [false, true] {
        for (fuel, fuel_ok) in fuel_cases {
            for (damage, damage_ok) in damage_cases {
                for audible in [0, 7] {
                    let mut b = Backend::new();
                    b.word(0xc68, if check_damage { 0x4000 } else { 0 });
                    b.word(0xc30, fuel.to_bits());
                    b.word(0x1b4, damage.to_bits());
                    b.byte(0x432, audible);
                    b.sound.engine_state = Some(1);
                    let mut expected = b.snapshot();
                    let mut events = vec![
                        Event::ReadByte(VEHICLE + 0xc2c),
                        Event::ReadWord(VEHICLE + 0xc68),
                        Event::ReadWord(VEHICLE + 0xc30),
                    ];
                    if fuel_ok && check_damage {
                        events.push(Event::ReadWord(VEHICLE + 0x1b4));
                    }
                    if !fuel_ok || check_damage && !damage_ok {
                        events.push(Event::ReadByte(VEHICLE + 0x432));
                        if audible != 0 {
                            events.push(Event::FailedStart(VEHICLE));
                        }
                    } else {
                        events.push(Event::Sound(VEHICLE, true));
                        events.push(Event::ReadByte(VEHICLE + 0xc2c));
                        expected[0xc2c] = 1;
                    }
                    assert_eq!(
                        vehicle_set_engine_on(&mut b, VEHICLE, true, false),
                        Ok(true)
                    );
                    assert_eq!(b.events, events);
                    assert_eq!(b.snapshot(), expected);
                }
            }
        }
    }
}

#[test]
fn sound_callback_state_is_reread_before_clearing_power() {
    for enabled in [false, true] {
        for after_sound in [0, 1, 2, 255] {
            let mut b = Backend::new();
            b.byte(0xc2c, u8::from(!enabled));
            b.sound.engine_state = Some(after_sound);
            b.sound.marker = Some(0x77);
            let mut expected = b.snapshot();
            expected[0xc2c] = after_sound;
            expected[0x20] = 0x77;
            let mut events = vec![Event::ReadByte(VEHICLE + 0xc2c)];
            if enabled {
                events.push(Event::ReadWord(VEHICLE + 0xc68));
                events.push(Event::ReadWord(VEHICLE + 0xc30));
            }
            events.push(Event::Sound(VEHICLE, enabled));
            events.push(Event::ReadByte(VEHICLE + 0xc2c));
            if after_sound == 0 {
                expected_word(&mut expected, 0x3a4, 0);
                expected_word(&mut expected, 0x3a8, 0);
                expected[0x5f0] = 0;
                events.extend([
                    Event::WriteWord(VEHICLE + 0x3a4, 0),
                    Event::WriteWord(VEHICLE + 0x3a8, 0),
                    Event::WriteByte(VEHICLE + 0x5f0, 0),
                ]);
            }
            assert_eq!(
                vehicle_set_engine_on(&mut b, VEHICLE, enabled, false),
                Ok(true)
            );
            assert_eq!(b.events, events);
            assert_eq!(b.snapshot(), expected);
        }
    }
}

#[test]
fn stop_does_not_require_fuel_or_damage_storage() {
    let mut b = Backend::mapped(VEHICLE + 0xc2c, &[1]);
    assert_eq!(
        vehicle_set_engine_on(&mut b, VEHICLE, false, false),
        Ok(true)
    );
    assert_eq!(
        b.events,
        [
            Event::ReadByte(VEHICLE + 0xc2c),
            Event::Sound(VEHICLE, false),
            Event::ReadByte(VEHICLE + 0xc2c),
        ]
    );
    assert_eq!(b.snapshot(), [1]);
}

#[test]
fn callback_errors_keep_their_mutations_and_stop_all_following_accesses() {
    for failed_start in [false, true] {
        let mut b = Backend::new();
        let action = Callback {
            engine_state: Some(9),
            marker: Some(0x42),
            fail_after_mutation: true,
        };
        b.sound = action;
        b.failed_start = action;
        if failed_start {
            b.word(0xc30, 0);
            b.byte(0x432, 1);
        }
        let mut expected = b.snapshot();
        expected[0xc2c] = 9;
        expected[0x20] = 0x42;
        let mut events = vec![
            Event::ReadByte(VEHICLE + 0xc2c),
            Event::ReadWord(VEHICLE + 0xc68),
            Event::ReadWord(VEHICLE + 0xc30),
        ];
        if failed_start {
            events.push(Event::ReadByte(VEHICLE + 0x432));
            events.push(Event::FailedStart(VEHICLE));
        } else {
            events.push(Event::Sound(VEHICLE, true));
        }
        assert_eq!(
            vehicle_set_engine_on(&mut b, VEHICLE, true, false),
            Err(Error::Callback)
        );
        assert_eq!(b.events, events);
        assert_eq!(b.snapshot(), expected);
    }
}

fn failure_case(case: usize) -> (Backend, bool, bool) {
    let mut b = Backend::new();
    let (enabled, force) = match case {
        0 => (false, true),
        1 => (true, true),
        2 => {
            b.word(0xc68, 0x4000);
            b.sound.marker = Some(0x55);
            (true, false)
        }
        3 => {
            b.word(0xc30, 0);
            b.byte(0x432, 1);
            (true, false)
        }
        4 => {
            b.byte(0xc2c, 1);
            b.sound.engine_state = Some(0);
            (false, false)
        }
        _ => unreachable!(),
    };
    (b, enabled, force)
}

#[test]
fn every_failure_keeps_exactly_the_completed_access_prefix() {
    for case in 0..5 {
        let (mut successful, enabled, force) = failure_case(case);
        assert_eq!(
            vehicle_set_engine_on(&mut successful, VEHICLE, enabled, force),
            Ok(true)
        );
        for fail_at in 1..=successful.events.len() {
            let (mut failing, _, _) = failure_case(case);
            failing.fail_at = Some(fail_at);
            assert_eq!(
                vehicle_set_engine_on(&mut failing, VEHICLE, enabled, force),
                Err(Error::Injected)
            );
            assert_eq!(failing.events, successful.events[..fail_at]);
            assert_eq!(failing.snapshot(), successful.prefixes[fail_at - 1]);
        }
    }
}

#[test]
fn partial_word_failure_preserves_bytes_written_by_the_live_memory_backend() {
    let fade = 5000.0f32.to_le_bytes();
    for count in 1..4 {
        let mut b = Backend::mapped(VEHICLE + 0xc20, &vec![0xa5; count]);
        assert_eq!(
            vehicle_set_engine_on(&mut b, VEHICLE, true, true),
            Err(Error::Storage(VehicleMemoryError::Address(
                VEHICLE + 0xc20 + count as u32
            )))
        );
        assert_eq!(
            b.events,
            [Event::WriteWord(VEHICLE + 0xc20, 5000.0f32.to_bits())]
        );
        assert_eq!(b.snapshot(), fade[..count]);
    }
}
