//! PC vehicle sound procedures over the caller's canonical live bytes.
//! EngineSnd (RVA0xebab0) and the dynamic engine procedure (RVA0xebe40)
//! feed the exact InitSounds slots and original per-band parameter tables.
//! These runtime integrations have static primary evidence; native differential
//! admission of these new bodies is separate from the earlier Tick wrappers.
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::runtime_audio::{AudioError, RuntimeAudio};
use crate::vehicle_init::VehicleInitBackend;

#[derive(Debug)]
pub enum VehicleAudioError<E> {
    Host(E),
    Audio(AudioError),
}
type Result<T, E> = std::result::Result<T, VehicleAudioError<E>>;

struct Call<'a, B> {
    b: &'a mut B,
    v: u32,
    a: &'a RuntimeAudio,
}
impl<B: VehicleInitBackend> Call<'_, B> {
    fn byte(&mut self, o: u32) -> Result<u8, B::Error> {
        self.b
            .read_byte(self.v + o)
            .map_err(VehicleAudioError::Host)
    }
    fn word(&mut self, o: u32) -> Result<u32, B::Error> {
        self.b
            .read_word(self.v + o)
            .map_err(VehicleAudioError::Host)
    }
    fn float(&mut self, o: u32) -> Result<f32, B::Error> {
        Ok(f32::from_bits(self.word(o)?))
    }
    fn putbyte(&mut self, o: u32, n: u8) -> Result<(), B::Error> {
        self.b
            .write_byte(self.v + o, n)
            .map_err(VehicleAudioError::Host)
    }
    fn putfloat(&mut self, o: u32, n: f32) -> Result<(), B::Error> {
        self.b
            .write_word(self.v + o, n.to_bits())
            .map_err(VehicleAudioError::Host)
    }
    fn effect(&mut self, o: u32, slot: u32, args: &[u32]) -> Result<u32, B::Error> {
        let handle = self.word(o)?;
        if matches!(slot, 0x24 | 0x88) {
            let flags = self
                .b
                .read_word(handle + 0xac)
                .map_err(VehicleAudioError::Host)?;
            self.b
                .write_word(
                    handle + 0xac,
                    if args[0] != 0 { flags | 1 } else { flags & !1 },
                )
                .map_err(VehicleAudioError::Host)?;
        }
        self.a
            .sound_virtual(handle, slot, args)
            .map_err(VehicleAudioError::Audio)
    }
    fn playing(&mut self, o: u32) -> Result<bool, B::Error> {
        Ok(self.word(o)? != 0 && self.effect(o, 0x54, &[0])? != 0)
    }
    fn off(&mut self, o: u32) -> Result<(), B::Error> {
        if self.word(o)? != 0 {
            self.effect(o, 0x24, &[0])?;
        }
        Ok(())
    }
}

pub fn vehicle_engine_snd<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    enabled: bool,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    let audible = c.byte(0x432)? != 0;
    let mut idle = true;
    if audible {
        for slot in [0x64c, 0x650, 0x92c, 0x930] {
            if c.playing(slot)? {
                idle = false;
            }
        }
    }
    if !enabled {
        if c.byte(0x638)? == 0 {
            return Ok(());
        }
        for offset in sound_slots() {
            if c.word(offset)? != 0 {
                c.effect(offset, 0x88, &[0, 1])?;
            }
        }
        let slot = if c.byte(0x490)? == 0 { 0x930 } else { 0x650 };
        if c.word(slot)? != 0 && audible {
            c.effect(slot, 0x24, &[1])?;
            c.effect(slot, 0x68, &[volume.to_bits()])?;
        }
        for slot in [0x64c, 0x650, 0x92c, 0x930] {
            if c.word(slot)? != 0 {
                c.effect(slot, 0x70, &[0])?;
            }
        }
        for offset in [0xc2c, 0x638, 0x5f0] {
            c.putbyte(offset, 0)?;
        }
        return Ok(());
    }
    if c.byte(0x638)? == 0 && c.byte(0xc2c)? == 0 && idle {
        let slot = if c.byte(0x490)? == 0 { 0x92c } else { 0x64c };
        if c.word(slot)? != 0 && audible {
            c.effect(slot, 0x70, &[0])?;
            c.effect(slot, 0x24, &[1])?;
            c.effect(slot, 0x68, &[volume.to_bits()])?;
        }
        c.putbyte(0x638, 1)?;
    }
    if c.byte(0x638)? == 0 {
        return Ok(());
    }
    let mut ready = true;
    if audible {
        for slot in [0x64c, 0x650, 0x92c, 0x930] {
            if c.playing(slot)? {
                let length = c.effect(slot, 0x84, &[])?;
                if length > 300 {
                    let current = c.effect(slot, 0x80, &[])?;
                    if length.wrapping_sub(current) > 300 {
                        c.putfloat(0xc20, 0.0)?;
                        ready = false;
                    }
                }
            }
        }
    }
    if ready {
        c.putbyte(0xc2c, 1)?;
        let direction = c.byte(0x490)?;
        c.putbyte(0x491, direction)?;
    }
    Ok(())
}

pub fn vehicle_stop_engine_loops<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    audio: &RuntimeAudio,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    for base in [0x658, 0x66c, 0x680] {
        for i in 0..5 {
            if c.word(base + i * 4)? != 0 {
                c.effect(base + i * 4, 0x88, &[0, 1])?;
            }
        }
    }
    for slot in [0x694, 0x938] {
        if c.word(slot)? != 0 {
            c.effect(slot, 0x88, &[0, 1])?;
        }
    }
    for base in [0x93c, 0x950, 0x964] {
        for i in 0..5 {
            if c.word(base + i * 4)? != 0 {
                c.effect(base + i * 4, 0x88, &[0, 1])?;
            }
        }
    }
    if c.word(0x978)? != 0 {
        c.effect(0x978, 0x88, &[0, 1])?;
    }
    Ok(())
}

pub fn vehicle_dynamic_engine_sound<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    seconds: f32,
    audio: &RuntimeAudio,
    volume: &mut f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    if c.byte(0xc2c)? == 0 {
        return Ok(());
    }
    let original_volume = *volume;
    let fade = add(seconds, c.float(0xc20)?).min(0.3);
    c.putfloat(0xc20, fade)?;
    *volume = mul(mul(*volume, fade), 3.333_333_3);
    c.b.write_word(0x634b00, volume.to_bits())
        .map_err(VehicleAudioError::Host)?;
    let idle = c.float(0x5ec)?;
    let maximum = c.float(0x1f9c)?;
    let mut rotation = c.float(0x548)?;
    if rotation < idle {
        rotation = idle;
    } else if sub(maximum, 0.001) < rotation {
        rotation = sub(maximum, 0.001);
    }
    let previous = c.byte(0x491)? != 0;
    let current = c.byte(0x490)? != 0;
    if previous != current {
        for bank in [0x93c, 0x950, 0x964] {
            for i in (0..5).rev() {
                c.off(bank + i * 4 - if previous { 0x2e4 } else { 0 })?;
            }
        }
        for slot in if previous {
            vec![0x694, 0x698]
        } else {
            vec![0x978, 0x97c, 0x938]
        } {
            c.off(slot)?;
        }
        c.putbyte(0x491, u8::from(current))?;
    }
    let inside = c.byte(0x491)? != 0;
    if c.byte(0xc68)? & 0x20 == 0 && !inside {
        if c.word(0x938)? != 0 {
            let knee = c.float(0x9d8)?;
            let mut gain = c.float(0x9dc)?;
            if rotation < knee {
                gain = mul(div(sub(rotation, idle), sub(knee, idle)), gain);
            }
            let rate = add(
                div(
                    sub(rotation, idle),
                    div(sub(maximum, idle), sub(c.float(0x9d4)?, c.float(0x9d0)?)),
                ),
                c.float(0x9d0)?,
            );
            if !c.playing(0x938)? {
                c.effect(0x938, 0x24, &[1])?;
            }
            c.effect(0x938, 0x74, &[rate.to_bits()])?;
            c.effect(0x938, 0x68, &[mul(*volume, gain).to_bits()])?;
        }
    } else {
        let gear = c.word(0x55c)? as i32;
        let shift = if inside { 0 } else { 0x2e4 };
        if gear < 0 {
            for bank in [0x658, 0x66c] {
                for i in (0..5).rev() {
                    c.off(bank + shift + i * 4)?;
                }
            }
        } else {
            for i in (0..5).rev() {
                c.off(0x680 + shift + i * 4)?;
            }
        }
        let throttle = c.float(0x4a0)?;
        let bands = if gear < 0 {
            vec![(
                0x680 + shift,
                0x83c + if inside { 0 } else { 0x2f4 },
                throttle,
                false,
            )]
        } else {
            vec![
                (
                    0x658 + shift,
                    0x6ec + if inside { 0 } else { 0x2f4 },
                    throttle,
                    true,
                ),
                (
                    0x66c + shift,
                    0x794 + if inside { 0 } else { 0x2f4 },
                    sub(1.0, throttle),
                    true,
                ),
            ]
        };
        for (bank, table, power, neutral_flag) in bands {
            for i in (0..5).rev() {
                let slot = bank + i * 4;
                if c.word(slot)? == 0 {
                    continue;
                }
                let p = table + i * 4;
                let low = c.float(p + 0x28)?;
                let high = c.float(p + 0x3c)?;
                let neutral_block = neutral_flag && c.byte(table + 0xa0 + i)? != 0 && gear == 0;
                if rotation < low || rotation >= high || neutral_block {
                    if c.playing(slot)? {
                        c.off(slot)?;
                    }
                    continue;
                }
                let full = c.float(p + 0x78)?;
                let quiet = c.float(p + 0x8c)?;
                let mut gain = add(mul(sub(full, quiet), power), quiet);
                if gain > 0.01 {
                    let fade_in = c.float(p + 0x50)?;
                    let fade_out = c.float(p + 0x64)?;
                    if rotation < fade_in && low != fade_in {
                        gain = mul(div(sub(rotation, low), sub(fade_in, low)), gain);
                    } else if rotation > fade_out && high != fade_out {
                        gain = mul(
                            sub(1.0, div(sub(rotation, fade_out), sub(high, fade_out))),
                            gain,
                        );
                    }
                }
                let min_rate = c.float(p)?;
                let max_rate = c.float(p + 0x14)?;
                let rate = add(
                    div(
                        sub(rotation, low),
                        div(sub(high, low), sub(max_rate, min_rate)),
                    ),
                    min_rate,
                );
                if !c.playing(slot)? {
                    if gain <= 0.01 {
                        continue;
                    }
                    c.effect(slot, 0x24, &[1])?;
                } else if gain < 0.01 {
                    c.off(slot)?;
                    continue;
                }
                c.effect(slot, 0x74, &[rate.to_bits()])?;
                c.effect(slot, 0x68, &[mul(*volume, gain).to_bits()])?;
            }
        }
    }
    let (slot, table) = if inside {
        (0x694, 0x8dc)
    } else {
        (0x978, 0xbd0)
    };
    if c.word(slot)? != 0 {
        let high = c.float(table)?;
        let low = c.float(table + 4)?;
        if rotation < low || rotation >= high {
            if c.playing(slot)? {
                c.off(slot)?;
            }
        } else {
            let gain = mul(
                div(sub(high, rotation), sub(high, low)),
                c.float(table + 8)?,
            );
            if !c.playing(slot)? {
                if gain > 0.01 {
                    c.effect(slot, 0x24, &[1])?;
                    c.effect(slot, 0x68, &[mul(*volume, gain).to_bits()])?;
                }
            } else if gain < 0.01 {
                c.off(slot)?;
            } else {
                c.effect(slot, 0x68, &[mul(*volume, gain).to_bits()])?;
            }
        }
    }
    *volume = original_volume;
    c.b.write_word(0x634b00, original_volume.to_bits())
        .map_err(VehicleAudioError::Host)?;
    Ok(())
}

fn sound_slots() -> impl Iterator<Item = u32> {
    (0x64c..=0x6bc)
        .step_by(4)
        .chain((0..4).rev().flat_map(|i| [0x6c0 + i * 4, 0x6d0 + i * 4]))
        .chain([0x6e0, 0x6e4, 0x6e8])
        .chain((0x92c..=0x9a0).step_by(4))
        .chain((0..4).rev().flat_map(|i| [0x9a4 + i * 4, 0x9b4 + i * 4]))
        .chain([0x9c4, 0x9c8, 0x9cc])
}

/// EnvironmentalSnd, PC RVA0xecde0. Frequency and fade are recovered from
/// primary x87 instructions because the decompiler loses their live stack.
pub fn vehicle_environment_sound<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    let x = c.float(0x1f90)?;
    let y = c.float(0x1f94)?;
    let z = c.float(0x1f98)?;
    let square = add(add(mul(x, x), mul(y, y)), mul(z, z));
    let speed = if (square.to_bits() as i32) < 0x322bcc77 {
        0.0
    } else if f64::from(sub(square, 1.0).abs()) < 1e-8 {
        1.0
    } else {
        sqrt(square)
    };
    let inside = c.byte(0x491)? != 0;
    if !inside && c.byte(0xc68)? & 0x20 == 0 {
        return Ok(());
    }
    let (slot, table) = if inside {
        (0x698, 0x8e8)
    } else {
        (0x97c, 0xbdc)
    };
    speed_envelope(&mut c, slot, table, speed, volume)
}

/// Reached Tick leaf at PC RVA0xed910; its original name is unresolved.
pub fn vehicle_audio_ed910<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    if c.byte(0xc68)? & 0x20 == 0 {
        return Ok(());
    }
    let slot = if c.byte(0x491)? == 0 { 0x988 } else { 0x6a4 };
    if c.word(slot)? != 0 && !c.playing(slot)? && c.byte(0x154)? == 0 {
        c.effect(slot, 0x24, &[1])?;
        c.effect(slot, 0x68, &[volume.to_bits()])?;
    }
    c.putbyte(0x154, 1)
}

/// Impact voice, PC RVA0xee950.
pub fn vehicle_impact_voice<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    let slot = if c.byte(0x491)? == 0 { 0x99c } else { 0x6b8 };
    if c.word(slot)? != 0 {
        c.effect(slot, 0x24, &[1])?;
        c.effect(slot, 0x68, &[volume.to_bits()])?;
    }
    Ok(())
}

pub fn vehicle_failed_start_sound<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    let first_busy = c.playing(0x654)?;
    if c.playing(0x934)? || first_busy {
        return Ok(());
    }
    let slot = if c.byte(0x491)? == 0 { 0x934 } else { 0x654 };
    if c.word(slot)? != 0 {
        c.effect(slot, 0x24, &[1])?;
        c.effect(slot, 0x68, &[volume.to_bits()])?;
    }
    Ok(())
}

pub fn vehicle_horn<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    enabled: bool,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    switch_sound_pair(
        &mut Call {
            b,
            v: vehicle,
            a: audio,
        },
        0x69c,
        0x980,
        enabled,
        volume,
    )
}
pub fn vehicle_siren<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    enabled: bool,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    switch_sound_pair(
        &mut Call {
            b,
            v: vehicle,
            a: audio,
        },
        0x6a0,
        0x984,
        enabled,
        volume,
    )
}
fn switch_sound_pair<B: VehicleInitBackend>(
    c: &mut Call<'_, B>,
    inside: u32,
    outside: u32,
    enabled: bool,
    volume: f32,
) -> Result<(), B::Error> {
    if !enabled {
        c.off(inside)?;
        return c.off(outside);
    }
    let (active, inactive) = if c.byte(0x491)? == 0 {
        (outside, inside)
    } else {
        (inside, outside)
    };
    if c.word(active)? != 0 && !c.playing(active)? {
        c.effect(active, 0x24, &[1])?;
        c.effect(active, 0x68, &[volume.to_bits()])?;
    }
    c.off(inactive)
}

/// PC RVA0xed160, the speed-dependent flat-tyre loop selected by Update.
pub fn vehicle_flat_tyre_sound<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    audible: bool,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    if !audible {
        c.off(0x6e8)?;
        return c.off(0x9cc);
    }
    let speed = add(
        add(
            mul(c.float(0xc44)?, c.float(0x1f98)?),
            mul(c.float(0xc40)?, c.float(0x1f94)?),
        ),
        mul(c.float(0xc3c)?, c.float(0x1f90)?),
    )
    .abs();
    let inside = c.byte(0x491)? != 0;
    if !inside && c.byte(0xc68)? & 0x20 == 0 {
        return Ok(());
    }
    let (slot, table) = if inside {
        (0x6e8, 0x900)
    } else {
        (0x9cc, 0xbf4)
    };
    speed_envelope(&mut c, slot, table, speed, volume)
}

fn speed_envelope<B: VehicleInitBackend>(
    c: &mut Call<'_, B>,
    slot: u32,
    table: u32,
    speed: f32,
    volume: f32,
) -> Result<(), B::Error> {
    if c.word(slot)? == 0 {
        return Ok(());
    }
    let low = c.float(table + 8)?;
    let high = c.float(table + 12)?;
    let end = c.float(table + 16)?;
    let speed = speed.min(high);
    if speed < low {
        if c.playing(slot)? {
            c.off(slot)?;
        }
        return Ok(());
    }
    let gain = if speed < end && end != low {
        div(sub(speed, low), sub(end, low))
    } else {
        1.0
    };
    let min_rate = c.float(table)?;
    let max_rate = c.float(table + 4)?;
    let rate = add(
        div(
            sub(speed, low),
            div(sub(high, low), sub(max_rate, min_rate)),
        ),
        min_rate,
    );
    if !c.playing(slot)? {
        if gain <= 0.01 {
            return Ok(());
        }
        c.effect(slot, 0x24, &[1])?;
    } else if gain < 0.01 {
        return c.off(slot);
    }
    c.effect(slot, 0x74, &[rate.to_bits()])?;
    c.effect(slot, 0x68, &[mul(volume, gain).to_bits()])?;
    Ok(())
}

/// SkidSnd, PC RVA0xed4f0. Uses actual wheel records, original material lookup
/// and authored smoothing; no speed-only synthetic tyre sound is introduced.
pub fn vehicle_skid_sound<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    seconds: f32,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    let count = c.word(0x4c4)?;
    let wheels = c.word(0xc38)?;
    let mut loudest = 0.0f32;
    for i in (0..count).rev() {
        let wheel =
            c.b.read_word(wheels + i * 4)
                .map_err(VehicleAudioError::Host)?;
        let flags =
            c.b.read_word(wheel + 0x120)
                .map_err(VehicleAudioError::Host)?;
        if flags & 8 == 0 {
            continue;
        }
        let material =
            c.b.read_word(wheel + 0xb4)
                .map_err(VehicleAudioError::Host)?;
        let map =
            c.b.read_byte(0x63d8b0 + material)
                .map_err(VehicleAudioError::Host)?;
        let table = c.b.read_word(0x63d9b4).map_err(VehicleAudioError::Host)?;
        if c.b
            .read_byte(table + u32::from(map) * 0x30 + 0x2c)
            .map_err(VehicleAudioError::Host)?
            == 0
        {
            continue;
        }
        let read = |c: &mut Call<'_, B>, offset| -> Result<f32, B::Error> {
            Ok(f32::from_bits(
                c.b.read_word(wheel + offset)
                    .map_err(VehicleAudioError::Host)?,
            ))
        };
        let mut gain = if flags & 0x20 != 0 {
            1.0
        } else {
            let amount = read(&mut c, 0x108)?;
            let full = read(&mut c, 0x178)?;
            let low = read(&mut c, 0x174)?;
            if amount > full {
                1.0
            } else if amount > low {
                div(sub(amount, low), sub(full, low)).max(0.0)
            } else {
                0.0
            }
        };
        let x = read(&mut c, 0x4c)?;
        let y = read(&mut c, 0x50)?;
        let z = read(&mut c, 0x54)?;
        let speed_squared = add(add(mul(x, x), mul(y, y)), mul(z, z));
        if speed_squared < 4.0 {
            gain = mul(mul(speed_squared, gain), 0.25);
        }
        if flags & 0x80 != 0 {
            let gear = c.word(0x55c)?;
            let wheel_speed = mul(
                div(
                    c.float(0x548)?,
                    c.float(0x570u32.wrapping_add(gear.wrapping_mul(4)))?,
                ),
                c.float(0x5ac)?,
            );
            let low = add(c.float(0x188)?, c.float(0x59c)?);
            if wheel_speed > low {
                let high = add(c.float(0x18c)?, c.float(0x59c)?);
                let spin = if wheel_speed > high {
                    1.0
                } else {
                    div(sub(wheel_speed, low), c.float(0x18c)?)
                };
                if gain < spin {
                    gain = spin;
                }
            }
        }
        if gain > loudest {
            loudest = gain;
        }
        if loudest > 0.99999 {
            loudest = 1.0;
            break;
        }
    }
    let step = mul(seconds, c.float(0x190)?);
    let old = c.float(0x194)?;
    let mut gain = if step <= sub(loudest, old) {
        add(step, old)
    } else {
        loudest
    };
    if gain >= 0.99999 {
        gain = 1.0;
    }
    c.putfloat(0x194, gain)?;
    if gain >= 0.01 {
        let (active, inactive) = if c.byte(0x491)? == 0 {
            (0x994, 0x6b0)
        } else {
            (0x6b0, 0x994)
        };
        if c.word(active)? != 0 {
            if !c.playing(active)? {
                c.effect(active, 0x24, &[1])?;
            }
            c.effect(active, 0x68, &[mul(volume, gain).to_bits()])?;
        }
        c.off(inactive)?;
    } else {
        c.off(0x6b0)?;
        c.off(0x994)?;
    }
    Ok(())
}

/// Original collision voice selection, PC 0x4edca0. The authored threshold
/// tables select one of four paired voices in the current acoustic bank.
pub fn vehicle_collision_sound<B: VehicleInitBackend>(
    b: &mut B,
    vehicle: u32,
    strength: f32,
    grounded: bool,
    audio: &RuntimeAudio,
    volume: f32,
) -> Result<(), B::Error> {
    let mut c = Call {
        b,
        v: vehicle,
        a: audio,
    };
    let (thresholds, voices, grounded_flags) = if c.byte(0x491)? != 0 {
        (0x918, 0x6c0, 0x928)
    } else {
        (0xc0c, 0x9a4, 0xc1c)
    };
    let minimum = c.float(thresholds)?;
    // Native FCOMP rejects unordered inputs at the first threshold as well.
    if strength.partial_cmp(&minimum) != Some(std::cmp::Ordering::Greater) {
        return Ok(());
    }
    let mut band = 3;
    for i in 1..4 {
        if strength < c.float(thresholds + i * 4)? {
            band = i - 1;
            break;
        }
    }
    if c.byte(grounded_flags + band)? != 0 && grounded {
        return Ok(());
    }
    let primary = voices + band * 4;
    let secondary = primary + 0x10;
    let (chosen, other) = if c.word(primary)? != 0 {
        if c.word(secondary)? != 0 {
            let seed = c.b.read_word(0x63a200).map_err(VehicleAudioError::Host)?;
            let next = seed.wrapping_mul(214013).wrapping_add(2531011);
            c.b.write_word(0x63a200, next)
                .map_err(VehicleAudioError::Host)?;
            let random = ((next >> 16) & 0x7fff) as i32;
            if random >= 0x3fff {
                (secondary, Some(primary))
            } else {
                (primary, Some(secondary))
            }
        } else {
            (primary, None)
        }
    } else {
        if c.word(secondary)? == 0 {
            return Ok(());
        }
        (secondary, None)
    };
    if c.effect(chosen, 0x54, &[0])? != 0 {
        return Ok(());
    }
    if let Some(other) = other {
        if c.effect(other, 0x54, &[0])? != 0 && c.effect(other, 0x80, &[])? < 800 {
            return Ok(());
        }
    }
    c.effect(chosen, 0x24, &[1])?;
    c.effect(chosen, 0x68, &[volume.to_bits()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::VehicleFrameMemory;
    struct Fixture {
        memory: VehicleFrameMemory,
        audio: RuntimeAudio,
        directory: std::path::PathBuf,
    }
    impl Fixture {
        fn new(label: &str) -> Self {
            let directory = std::env::temp_dir()
                .join(format!("lhr-vehicle-audio-{label}-{}", std::process::id()));
            std::fs::create_dir(&directory).unwrap();
            let mut bytes = b"RIFF".to_vec();
            bytes.extend(44u32.to_le_bytes());
            bytes.extend(b"WAVEfmt ");
            bytes.extend(16u32.to_le_bytes());
            bytes.extend(1u16.to_le_bytes());
            bytes.extend(1u16.to_le_bytes());
            bytes.extend(4u32.to_le_bytes());
            bytes.extend(8u32.to_le_bytes());
            bytes.extend(2u16.to_le_bytes());
            bytes.extend(16u16.to_le_bytes());
            bytes.extend(b"data");
            bytes.extend(8u32.to_le_bytes());
            for sample in [4096i16, 8192, 4096, -8192] {
                bytes.extend(sample.to_le_bytes());
            }
            std::fs::write(directory.join("loop.wav"), bytes).unwrap();
            let installation = lhr_assets::Installation::open(&directory).unwrap();
            let audio = RuntimeAudio::default();
            audio.create_sound_at(0x7000).unwrap();
            audio.load_sound(0x7000, "loop.wav", &installation).unwrap();
            audio.sound_virtual(0x7000, 0x58, &[3]).unwrap();
            let mut memory = VehicleFrameMemory::default();
            memory.map_bytes(0x1000, &vec![0; 0x3000]).unwrap();
            memory.map_bytes(0x7000, &[0; 0x200]).unwrap();
            memory
                .map_bytes(0x634b00, &1f32.to_bits().to_le_bytes())
                .unwrap();
            Self {
                memory,
                audio,
                directory,
            }
        }
        fn add_sound(&mut self, handle: u32) {
            let installation = lhr_assets::Installation::open(&self.directory).unwrap();
            self.audio.create_sound_at(handle).unwrap();
            self.audio
                .load_sound(handle, "loop.wav", &installation)
                .unwrap();
            self.audio.sound_virtual(handle, 0x58, &[3]).unwrap();
            self.memory.map_bytes(handle, &[0; 0x200]).unwrap();
        }
        fn word(&mut self, offset: u32, value: u32) {
            self.memory.write_word(0x1000 + offset, value).unwrap();
        }
        fn float(&mut self, offset: u32, value: f32) {
            self.word(offset, value.to_bits());
        }
        fn byte(&mut self, offset: u32, value: u8) {
            self.memory.write_byte(0x1000 + offset, value).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn starter_gates_engine_until_final_300ms_and_stop_clears_actual_frame_on() {
        let mut f = Fixture::new("starter");
        f.word(0x64c, 0x7000);
        f.byte(0x432, 1);
        f.byte(0x490, 1);
        vehicle_engine_snd(&mut f.memory, 0x1000, true, &f.audio, 1.0).unwrap();
        assert_eq!(f.memory.read_byte(0x1000 + 0x638).unwrap(), 1);
        assert_eq!(f.memory.read_byte(0x1000 + 0xc2c).unwrap(), 0);
        assert_eq!(f.memory.read_word(0x7000 + 0xac).unwrap() & 1, 1);
        let mut output = [0.0; 6];
        f.audio.mix_stereo(4, &mut output).unwrap();
        assert!(output.iter().any(|v| *v != 0.0));
        vehicle_engine_snd(&mut f.memory, 0x1000, true, &f.audio, 1.0).unwrap();
        assert_eq!(f.memory.read_byte(0x1000 + 0xc2c).unwrap(), 1);
        vehicle_engine_snd(&mut f.memory, 0x1000, false, &f.audio, 1.0).unwrap();
        assert!(!f.audio.sound(0x7000).unwrap().is_playing());
        assert_eq!(f.memory.read_word(0x7000 + 0xac).unwrap() & 1, 0);
        assert_eq!(f.memory.read_byte(0x1000 + 0xc2c).unwrap(), 0);
    }

    #[test]
    fn authored_engine_band_controls_rate_gain_fade_and_reverse_cleanup() {
        let mut f = Fixture::new("band");
        f.word(0x658, 0x7000);
        for offset in [0x490, 0x491, 0xc2c] {
            f.byte(offset, 1);
        }
        f.byte(0xc68, 0x20);
        f.word(0x55c, 1);
        for (offset, value) in [
            (0x5ec, 100.0),
            (0x1f9c, 1000.0),
            (0x548, 550.0),
            (0x4a0, 0.5),
            (0x6ec, 0.5),
            (0x700, 1.5),
            (0x714, 100.0),
            (0x728, 1000.0),
            (0x73c, 100.0),
            (0x750, 1000.0),
            (0x764, 1.0),
            (0x778, 0.2),
        ] {
            f.float(offset, value);
        }
        f.audio.sound_virtual(0x7000, 0x6c, &[1]).unwrap();
        let mut volume = 1.0;
        vehicle_dynamic_engine_sound(&mut f.memory, 0x1000, 0.15, &f.audio, &mut volume).unwrap();
        let sound = f.audio.sound(0x7000).unwrap();
        assert!(sound.is_playing());
        assert_eq!(sound.frequency, 1.0);
        assert!((sound.volume - 0.3).abs() < 1e-6);
        assert_eq!(volume, 1.0);
        f.word(0x55c, u32::MAX);
        vehicle_dynamic_engine_sound(&mut f.memory, 0x1000, 0.016, &f.audio, &mut volume).unwrap();
        assert!(!f.audio.sound(0x7000).unwrap().is_playing());
    }
    #[test]
    fn collision_threshold_banks_and_grounded_flags_use_actual_voice_slots() {
        let mut f = Fixture::new("collision-banks");
        for (inside, table, base, flags) in
            [(false, 0xc0c, 0x9a4, 0xc1c), (true, 0x918, 0x6c0, 0x928)]
        {
            f.byte(0x491, u8::from(inside));
            for (i, value) in [10., 20., 30., 40.].into_iter().enumerate() {
                f.float(table + i as u32 * 4, value);
            }
            for (band, strength) in [(0, 15.), (1, 20.), (2, 30.), (3, 40.)] {
                for i in 0..4 {
                    f.word(base + i * 4, 0);
                    f.byte(flags + i, 0);
                }
                f.word(base + band * 4, 0x7000);
                vehicle_collision_sound(&mut f.memory, 0x1000, 10., false, &f.audio, 0.35).unwrap();
                assert!(!f.audio.sound(0x7000).unwrap().is_playing());
                f.byte(flags + band, 1);
                vehicle_collision_sound(&mut f.memory, 0x1000, strength, true, &f.audio, 0.35)
                    .unwrap();
                assert!(!f.audio.sound(0x7000).unwrap().is_playing());
                vehicle_collision_sound(&mut f.memory, 0x1000, strength, false, &f.audio, 0.35)
                    .unwrap();
                let sound = f.audio.sound(0x7000).unwrap();
                assert!(sound.is_playing());
                assert_eq!(sound.volume, 0.35);
                assert_eq!(f.memory.read_word(0x70ac).unwrap() & 1, 1);
                let mut output = [0.; 2];
                f.audio.mix_stereo(4, &mut output).unwrap();
                assert!(output.iter().any(|sample| *sample != 0.));
                f.audio.sound_virtual(0x7000, 0x24, &[0]).unwrap();
            }
        }
        // The single-voice cases above deliberately have no RNG mapping.
    }
    #[test]
    fn collision_pair_draws_live_rng_and_allows_other_voice_at_exactly_800ms() {
        let mut f = Fixture::new("collision-pair");
        f.add_sound(0x7200);
        f.memory.map_bytes(0x63a200, &0u32.to_le_bytes()).unwrap();
        f.byte(0x491, 1);
        for (i, value) in [10., 20., 30., 40.].into_iter().enumerate() {
            f.float(0x918 + i as u32 * 4, value);
        }
        f.word(0x6c0, 0x7000);
        f.word(0x6d0, 0x7200);
        vehicle_collision_sound(&mut f.memory, 0x1000, 15., false, &f.audio, 0.5).unwrap();
        assert!(f.audio.sound(0x7000).unwrap().is_playing());
        assert_eq!(f.memory.read_word(0x63a200).unwrap(), 2531011);
        f.audio.sound_virtual(0x7000, 0x70, &[750]).unwrap();
        f.memory.write_word(0x63a200, 50000).unwrap();
        vehicle_collision_sound(&mut f.memory, 0x1000, 15., false, &f.audio, 0.5).unwrap();
        assert!(!f.audio.sound(0x7200).unwrap().is_playing());
        let next = 50000u32.wrapping_mul(214013).wrapping_add(2531011);
        assert_eq!(f.memory.read_word(0x63a200).unwrap(), next);
        f.audio.sound_virtual(0x7000, 0x70, &[800]).unwrap();
        assert_eq!(f.audio.sound_virtual(0x7000, 0x80, &[]).unwrap(), 800);
        f.memory.write_word(0x63a200, 50000).unwrap();
        vehicle_collision_sound(&mut f.memory, 0x1000, 15., false, &f.audio, 0.25).unwrap();
        assert!(f.audio.sound(0x7000).unwrap().is_playing());
        assert!(f.audio.sound(0x7200).unwrap().is_playing());
        assert_eq!(f.audio.sound(0x7200).unwrap().volume, 0.25);
        // A newly selected voice already playing must keep its cursor.
        f.audio.sound_virtual(0x7200, 0x70, &[250]).unwrap();
        f.memory.write_word(0x63a200, 50000).unwrap();
        vehicle_collision_sound(&mut f.memory, 0x1000, 15., false, &f.audio, 0.75).unwrap();
        assert_eq!(f.audio.sound(0x7200).unwrap().current_time_ms(), 250);
        assert_eq!(f.audio.sound(0x7200).unwrap().volume, 0.25);
    }
}
