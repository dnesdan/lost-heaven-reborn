use super::*;
use crate::vehicle_init::*;
use std::collections::{BTreeMap, BTreeSet};
const GAME: u32 = 0x1000;
const SCRATCH: u32 = 0x3000;
#[derive(Default)]
struct Backend {
    bytes: BTreeMap<u32, u8>,
    allocations: BTreeSet<u32>,
    frames: BTreeSet<u32>,
    pending: Vec<GameSoundPending>,
    calls: Vec<(u32, u32, Vec<u32>)>,
    next: u32,
    next_frame: u32,
    fail_slot: Option<u32>,
    playing: bool,
}
impl Backend {
    fn new() -> Self {
        let mut b = Self {
            next: 0x10000,
            next_frame: 0x4000,
            playing: true,
            ..Self::default()
        };
        for a in GAME..GAME + 0x400 {
            b.bytes.insert(a, 0);
        }
        for a in SCRATCH..SCRATCH + 24 {
            b.bytes.insert(a, 0xa5);
        }
        initialize_sound_context(&mut b, GAME, 0xa5).unwrap();
        b
    }
    fn play(&mut self, fade: u8, rate: f32) -> u32 {
        play_spatial_sound(
            self,
            GAME,
            SCRATCH,
            SpatialSoundRequest {
                parent: 0x9000,
                name: 0x123456,
                sound_type: 1,
                position: [1., 2., 3.],
                range: [1., 25.],
                volume: 1.,
                fade,
                fade_step: rate,
                looping: 0,
            },
        )
        .unwrap()
    }
    fn row(&mut self) -> u32 {
        self.read_word(GAME + 0x2f8).unwrap()
    }
}
impl VehicleInitBackend for Backend {
    type Error = &'static str;
    fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
        self.bytes.get(&a).copied().ok_or("unmapped")
    }
    fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
        let mut v = [0; 4];
        for (i, p) in v.iter_mut().enumerate() {
            *p = self.read_byte(a + i as u32)?;
        }
        Ok(u32::from_le_bytes(v))
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
        self.bytes.insert(a, v);
        Ok(())
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
        for (i, v) in v.to_le_bytes().into_iter().enumerate() {
            self.write_byte(a + i as u32, v)?;
        }
        Ok(())
    }
    fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
        let p = self.next;
        self.next += n + 16;
        self.allocations.insert(p);
        for a in p..p + n {
            self.bytes.insert(a, 0xa5);
        }
        Ok(p)
    }
    fn free(&mut self, p: u32) -> Result<(), Self::Error> {
        if p != 0 && !self.allocations.remove(&p) {
            return Err("unowned allocation");
        }
        Ok(())
    }
    fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), Self::Error> {
        Err("unused")
    }
    fn enumerate(
        &mut self,
        _: u32,
        _: VehicleInitQuery,
        _: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        Err(VehicleInitUnavailable::Effect("unused"))
    }
}
impl GameSpatialSoundBackend for Backend {
    fn game_sound_create(&mut self) -> Result<u32, Self::Error> {
        let p = self.next_frame;
        self.next_frame += 0x400;
        self.frames.insert(p);
        for a in p..p + 0x200 {
            self.bytes.insert(a, 0);
        }
        self.write_word(p + 0x18c, 1f32.to_bits())?;
        Ok(p)
    }
    fn game_sound_load(&mut self, _: u32, name: u32) -> Result<u32, Self::Error> {
        assert_eq!(name, 0x123456);
        Ok(0)
    }
    fn game_sound_virtual(&mut self, f: u32, slot: u32, args: &[u32]) -> Result<u32, Self::Error> {
        self.calls.push((f, slot, args.to_vec()));
        if self.fail_slot == Some(slot) {
            return Err("injected sound effect failure");
        }
        match slot {
            0 => {
                if !self.frames.remove(&f) {
                    return Err("unowned frame");
                }
            }
            0x54 => return Ok(u32::from(self.playing)),
            0x80 => return Ok(42),
            0x68 => self.write_word(f + 0x18c, args[0])?,
            _ => (),
        }
        Ok(0)
    }
    fn game_sound_track(&mut self, p: GameSoundPending) {
        self.pending.push(p);
    }
    fn game_sound_untrack(&mut self, p: GameSoundPending) -> Result<(), Self::Error> {
        let i = self
            .pending
            .iter()
            .rposition(|v| *v == p)
            .ok_or("unowned pending resource")?;
        self.pending.remove(i);
        Ok(())
    }
    fn game_sound_pending_last(&self) -> Option<GameSoundPending> {
        self.pending.last().copied()
    }
}
#[test]
fn nonfaded_rows_preserve_caller_bytes_and_cleanup_releases_each_frame() {
    let mut b = Backend::new();
    for id in 0..3 {
        assert_eq!(b.play(0, 0.001), id);
    }
    let row = b.row();
    for a in row + 8..row + 16 {
        assert_eq!(b.read_byte(a).unwrap(), 0xa5);
    }
    for a in row + 17..row + 20 {
        assert_eq!(b.read_byte(a).unwrap(), 0xa5);
    }
    assert!(b.pending.is_empty());
    assert_eq!(b.frames.len(), 3);
    stop_all_sounds(&mut b, GAME).unwrap();
    assert!(b.frames.is_empty());
    let begin = b.row();
    b.free(begin).unwrap();
    assert!(b.allocations.is_empty());
    let released: Vec<_> = b
        .calls
        .iter()
        .filter(|(_, s, _)| *s == 0)
        .map(|(f, _, _)| *f)
        .collect();
    assert_eq!(released, [0x4800, 0x4400, 0x4000]);
}
#[test]
fn failed_effect_keeps_factory_frame_until_explicit_pending_cleanup() {
    let mut b = Backend::new();
    b.fail_slot = Some(0x58);
    let result = play_spatial_sound(
        &mut b,
        GAME,
        SCRATCH,
        SpatialSoundRequest {
            parent: 0x9000,
            name: 0x123456,
            sound_type: 1,
            position: [0.; 3],
            range: [1., 25.],
            volume: 1.,
            fade: 0,
            fade_step: 0.001,
            looping: 0,
        },
    );
    assert!(matches!(
        result,
        Err(GameSpatialSoundError::Host("injected sound effect failure"))
    ));
    assert_eq!(b.pending, [GameSoundPending::Frame(0x4000)]);
    b.fail_slot = None;
    cleanup_sound_pending(&mut b).unwrap();
    assert!(b.frames.is_empty());
    assert!(b.allocations.is_empty());
    assert!(b.pending.is_empty());
}
#[test]
fn initialized_period_uses_strict_unsigned_gate_and_live_swap_last_retirement() {
    let mut b = Backend::new();
    for _ in 0..3 {
        b.play(0, 0.001);
    }
    b.playing = false;
    assert_eq!(b.read_word(GAME + 0x308).unwrap(), 813);
    tick_sounds(&mut b, GAME, 813).unwrap();
    assert_eq!(b.frames.len(), 3);
    tick_sounds(&mut b, GAME, 1).unwrap();
    assert!(b.frames.is_empty());
    assert_eq!(b.read_word(GAME + 0x304).unwrap(), 1);
    assert_eq!(b.read_word(GAME + 0x2fc).unwrap(), b.row());
    let released: Vec<_> = b
        .calls
        .iter()
        .filter(|(_, s, _)| *s == 0)
        .map(|(f, _, _)| *f)
        .collect();
    assert_eq!(released, [0x4000, 0x4800, 0x4400]);
}
#[test]
fn zero_fade_rate_keeps_active_fade_even_when_volume_equals_target() {
    let mut b = Backend::new();
    b.play(1, 0.);
    let row = b.row();
    let frame = b.read_word(row).unwrap();
    b.write_word(frame + 0x18c, 1f32.to_bits()).unwrap();
    tick_sounds(&mut b, GAME, 16).unwrap();
    assert_eq!(b.read_byte(row + 0x10).unwrap(), 1);
    assert_eq!(b.read_word(GAME + 0x310).unwrap(), 1);
    assert_eq!(b.read_word(frame + 0x18c).unwrap(), 1f32.to_bits());
}
