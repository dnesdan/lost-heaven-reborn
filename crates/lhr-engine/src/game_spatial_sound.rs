//! C_game spatial sound publication (PC 5aca70), StopAllSounds (5acf00),
//! and its live sound phase in Tick (5a5a64..5a5c7a). Frames and vector
//! storage belong to the caller's canonical owners, including failed prefixes.
use crate::vehicle_init::VehicleInitBackend;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameSoundPending {
    Frame(u32),
    Allocation(u32),
}
pub trait GameSpatialSoundBackend: VehicleInitBackend {
    fn game_sound_create(&mut self) -> Result<u32, Self::Error>;
    /// Cache/Open HRESULT; reads the original name only at this source stage.
    fn game_sound_load(&mut self, frame: u32, name: u32) -> Result<u32, Self::Error>;
    fn game_sound_virtual(
        &mut self,
        frame: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error>;
    fn game_sound_track(&mut self, resource: GameSoundPending);
    fn game_sound_untrack(&mut self, resource: GameSoundPending) -> Result<(), Self::Error>;
    fn game_sound_pending_last(&self) -> Option<GameSoundPending>;
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialSoundRequest {
    pub parent: u32,
    pub name: u32,
    pub sound_type: u32,
    pub position: [f32; 3],
    pub range: [f32; 2],
    pub volume: f32,
    pub fade: u8,
    pub fade_step: f32,
    pub looping: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameSpatialSoundError<E> {
    Host(E),
    AllocationFailed,
    NumericDomain,
}
type Out<T, E> = Result<T, GameSpatialSoundError<E>>;
/// Embedded constructor stores from 5472f0 plus the ID/fade resets in
/// 5a0810. Other C_game fields retain their explicit standalone caller state.
pub fn initialize_sound_context<B: VehicleInitBackend>(
    b: &mut B,
    game: u32,
    marker: u8,
) -> Result<(), B::Error> {
    b.write_word(game + 0x2f8, 0)?;
    b.write_byte(game + 0x2f4, marker)?;
    b.write_word(game + 0x2fc, 0)?;
    b.write_word(game + 0x300, 0)?;
    b.write_word(game + 0x304, 0)?;
    b.write_word(game + 0x308, 200)?;
    b.write_word(game + 0x30c, 0)?;
    b.write_word(game + 0x310, 0)?;
    b.write_word(game + 0x304, 0)?;
    b.write_word(game + 0x308, 0x32d)
}
fn word<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<u32, B::Error> {
    b.read_word(address).map_err(GameSpatialSoundError::Host)
}
fn put<B: VehicleInitBackend>(b: &mut B, address: u32, value: u32) -> Out<(), B::Error> {
    b.write_word(address, value)
        .map_err(GameSpatialSoundError::Host)
}
fn call<B: GameSpatialSoundBackend>(
    b: &mut B,
    frame: u32,
    slot: u32,
    args: &[u32],
) -> Out<u32, B::Error> {
    b.game_sound_virtual(frame, slot, args)
        .map_err(GameSpatialSoundError::Host)
}
fn copy_row<B: VehicleInitBackend>(b: &mut B, destination: u32, source: u32) -> Out<(), B::Error> {
    for offset in (0..24).step_by(4) {
        let value = word(b, source + offset)?;
        put(b, destination + offset, value)?;
    }
    Ok(())
}
fn distance(begin: u32, end: u32) -> u32 {
    ((end.wrapping_sub(begin) as i32) / 24) as u32
}
/// The actual 430470 caller inserts one row at end. Other positions/counts
/// are outside this adapter; no general vector-entry parity is claimed.
fn append<B: GameSpatialSoundBackend>(b: &mut B, header: u32, row: u32) -> Out<(), B::Error> {
    let position = word(b, header + 8)?;
    let end = word(b, header + 8)?;
    let capacity = word(b, header + 12)?;
    if distance(end, capacity) >= 1 {
        let end = word(b, header + 8)?;
        if end != 0 {
            copy_row(b, end, row)?;
        }
        let end = word(b, header + 8)?;
        return put(b, header + 8, end.wrapping_add(24));
    }
    let begin = word(b, header + 4)?;
    let size = if begin == 0 { 0 } else { distance(begin, end) };
    let count = size.wrapping_add(size.max(1));
    let allocation = b
        .allocate(if count as i32 >= 0 {
            count.wrapping_mul(24)
        } else {
            0
        })
        .map_err(GameSpatialSoundError::Host)?;
    if allocation == 0 {
        return Err(GameSpatialSoundError::AllocationFailed);
    }
    let resource = GameSoundPending::Allocation(allocation);
    b.game_sound_track(resource);
    let mut cursor = word(b, header + 4)?;
    let mut destination = allocation;
    while cursor != position {
        copy_row(b, destination, cursor)?;
        cursor = cursor.wrapping_add(24);
        destination = destination.wrapping_add(24);
    }
    copy_row(b, destination, row)?;
    destination = destination.wrapping_add(24);
    let end = word(b, header + 8)?;
    let mut cursor = position;
    while cursor != end {
        copy_row(b, destination, cursor)?;
        cursor = cursor.wrapping_add(24);
        destination = destination.wrapping_add(24);
    }
    let old = word(b, header + 4)?;
    b.free(old).map_err(GameSpatialSoundError::Host)?;
    put(
        b,
        header + 12,
        allocation.wrapping_add(count.wrapping_mul(24)),
    )?;
    let begin = word(b, header + 4)?;
    let size = if begin == 0 {
        0
    } else {
        distance(begin, word(b, header + 8)?)
    };
    put(b, header + 4, allocation)?;
    b.game_sound_untrack(resource)
        .map_err(GameSpatialSoundError::Host)?;
    put(
        b,
        header + 8,
        allocation.wrapping_add(size.wrapping_add(1).wrapping_mul(24)),
    )
}

/// Caller provides 24 bytes of original-style stack storage. Unwritten bytes
/// (fade target/rate when disabled, and flag padding) retain the caller fill.
pub fn play_spatial_sound<B: GameSpatialSoundBackend>(
    b: &mut B,
    game: u32,
    scratch: u32,
    request: SpatialSoundRequest,
) -> Out<u32, B::Error> {
    let frame = b.game_sound_create().map_err(GameSpatialSoundError::Host)?;
    if frame == 0 {
        return Ok(u32::MAX);
    }
    let resource = GameSoundPending::Frame(frame);
    b.game_sound_track(resource);
    if b.game_sound_load(frame, request.name)
        .map_err(GameSpatialSoundError::Host)?
        != 0
    {
        call(b, frame, 0, &[])?;
        b.game_sound_untrack(resource)
            .map_err(GameSpatialSoundError::Host)?;
        return Ok(u32::MAX);
    }
    if b.read_byte(frame + 0xac)
        .map_err(GameSpatialSoundError::Host)? as i8
        >= 0
    {
        for (i, value) in request.position.into_iter().enumerate() {
            put(b, frame + 0x80 + i as u32 * 4, value.to_bits())?;
        }
        let flags = word(b, frame + 0xac)?;
        put(b, frame + 0xac, (flags & 0xfffffedf) | 0x40000000)?;
    }
    let parent = if request.parent == 0 {
        let app = word(b, 0x63788c)?;
        let scene = word(b, app + 0x10)?;
        word(b, scene + 0x210)?
    } else {
        request.parent
    };
    call(b, frame, 0x2c, &[parent, 0])?;
    call(b, frame, 0x58, &[request.sound_type])?;
    call(
        b,
        frame,
        0x5c,
        &[
            request.range[0].to_bits(),
            request.range[1].to_bits(),
            0x3ecccccd,
            0x3e99999a,
        ],
    )?;
    call(b, frame, 0x60, &[0x40c90fdb, 0x40c90fdb])?;
    call(b, frame, 0x64, &[0])?;
    call(b, frame, 0x68, &[request.volume.to_bits()])?;
    call(b, frame, 0x18, &[])?;
    call(b, frame, 0x24, &[1])?;
    call(b, frame, 0x6c, &[request.looping])?;
    let id = word(b, game + 0x30c)?;
    put(b, scratch, frame)?;
    put(b, scratch + 4, id)?;
    put(b, game + 0x30c, id.wrapping_add(1))?;
    b.write_byte(scratch + 0x10, request.fade)
        .map_err(GameSpatialSoundError::Host)?;
    put(b, scratch + 0x14, 0)?;
    if request.fade != 0 {
        put(b, scratch + 0xc, request.fade_step.to_bits())?;
        put(b, scratch + 8, request.volume.to_bits())?;
        call(b, frame, 0x68, &[0])?;
        let fades = word(b, game + 0x310)?;
        put(b, game + 0x310, fades.wrapping_add(1))?;
    }
    append(b, game + 0x2f4, scratch)?;
    b.game_sound_untrack(resource)
        .map_err(GameSpatialSoundError::Host)?;
    let end = word(b, game + 0x2fc)?;
    word(b, end.wrapping_sub(0x14))
}
fn frame_at<B: VehicleInitBackend>(b: &mut B, game: u32, offset: u32) -> Out<u32, B::Error> {
    let begin = word(b, game + 0x2f8)?;
    word(b, begin.wrapping_add(offset))
}
fn stop_row<B: GameSpatialSoundBackend>(b: &mut B, game: u32, offset: u32) -> Out<(), B::Error> {
    let frame = frame_at(b, game, offset)?;
    call(b, frame, 0x88, &[0, 1])?;
    let frame = frame_at(b, game, offset)?;
    call(b, frame, 0x2c, &[0, 0])?;
    let frame = frame_at(b, game, offset)?;
    call(b, frame, 0, &[])?;
    Ok(())
}
pub fn stop_all_sounds<B: GameSpatialSoundBackend>(b: &mut B, game: u32) -> Out<(), B::Error> {
    let begin = word(b, game + 0x2f8)?;
    let count = if begin == 0 {
        0
    } else {
        distance(begin, word(b, game + 0x2fc)?)
    };
    let mut offset = count.wrapping_mul(24);
    for _ in 0..count {
        offset = offset.wrapping_sub(24);
        stop_row(b, game, offset)?;
    }
    let begin = word(b, game + 0x2f8)?;
    put(b, game + 0x2fc, begin)
}
/// Logical PC24 precision is tracked. Faded durations above the exact binary32
/// integer range remain explicit numeric frontiers (nonfaded sounds bypass it).
fn fade_value<E>(milliseconds: u32, rate: f32, volume: f32) -> Out<f32, E> {
    use crate::vehicle_dynamics_prepare::{add, mul};
    if milliseconds > 1 << 24 {
        return Err(GameSpatialSoundError::NumericDomain);
    }
    let product =
        mul::<()>(milliseconds as f32, rate).map_err(|_| GameSpatialSoundError::NumericDomain)?;
    add::<()>(product, volume).map_err(|_| GameSpatialSoundError::NumericDomain)
}
pub fn tick_sounds<B: GameSpatialSoundBackend>(
    b: &mut B,
    game: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    if word(b, game + 0x310)? == 0 {
        let elapsed = word(b, game + 0x304)?;
        let period = word(b, game + 0x308)?;
        let elapsed = elapsed.wrapping_add(milliseconds);
        put(b, game + 0x304, elapsed)?;
        if elapsed <= period {
            return Ok(());
        }
        put(b, game + 0x304, elapsed.wrapping_sub(period))?;
    }
    let mut index = 0u32;
    loop {
        let count = crate::vehicle_component_records::vehicle_exhaust_records_size(b, game + 0x2f4)
            .map_err(GameSpatialSoundError::Host)?;
        if index >= count {
            return Ok(());
        }
        let offset = index.wrapping_mul(24);
        let frame = frame_at(b, game, offset)?;
        let mut retire = call(b, frame, 0x54, &[0])? as u8 == 0;
        if !retire {
            let begin = word(b, game + 0x2f8)?;
            let frame = word(b, begin + offset)?;
            if b.read_byte(frame + 0x164)
                .map_err(GameSpatialSoundError::Host)?
                == 0
            {
                let current = call(b, frame, 0x80, &[])?;
                retire = word(b, begin + offset + 0x14)? > current;
                if !retire {
                    let begin = word(b, game + 0x2f8)?;
                    let frame = word(b, begin + offset)?;
                    let current = call(b, frame, 0x80, &[])?;
                    put(b, begin + offset + 0x14, current)?;
                }
            }
        }
        if retire {
            stop_row(b, game, offset)?;
            let end = word(b, game + 0x2fc)?;
            let begin = word(b, game + 0x2f8)?;
            copy_row(b, begin + offset, end.wrapping_sub(24))?;
            let end = word(b, game + 0x2fc)?;
            put(b, game + 0x2fc, end.wrapping_sub(24))?;
            continue;
        }
        let begin = word(b, game + 0x2f8)?;
        let row = begin + offset;
        if b.read_byte(row + 0x10)
            .map_err(GameSpatialSoundError::Host)?
            != 0
        {
            let rate = f32::from_bits(word(b, row + 0xc)?);
            if !rate.is_finite() {
                return Err(GameSpatialSoundError::NumericDomain);
            }
            let frame = word(b, row)?;
            let current = f32::from_bits(word(b, frame + 0x18c)?);
            let value = fade_value(milliseconds, rate, current)?;
            if rate > 0. {
                let target = f32::from_bits(word(b, row + 8)?);
                if !target.is_finite() {
                    return Err(GameSpatialSoundError::NumericDomain);
                }
                let value = if value >= target {
                    b.write_byte(row + 0x10, 0)
                        .map_err(GameSpatialSoundError::Host)?;
                    let count = word(b, game + 0x310)?;
                    put(b, game + 0x310, count.wrapping_sub(1))?;
                    target
                } else {
                    value
                };
                let frame = frame_at(b, game, offset)?;
                call(b, frame, 0x68, &[value.to_bits()])?;
            } else if value <= 0. {
                call(b, frame, 0x88, &[0, 1])?;
                let count = word(b, game + 0x310)?;
                put(b, game + 0x310, count.wrapping_sub(1))?;
            } else {
                call(b, frame, 0x68, &[value.to_bits()])?;
            }
        }
        index = index.wrapping_add(1);
    }
}
pub fn cleanup_sound_pending<B: GameSpatialSoundBackend>(b: &mut B) -> Out<(), B::Error> {
    while let Some(resource) = b.game_sound_pending_last() {
        match resource {
            GameSoundPending::Frame(frame) => {
                call(b, frame, 0x88, &[0, 1])?;
                call(b, frame, 0x2c, &[0, 0])?;
                call(b, frame, 0, &[])?;
            }
            GameSoundPending::Allocation(address) => {
                b.free(address).map_err(GameSpatialSoundError::Host)?
            }
        }
        b.game_sound_untrack(resource)
            .map_err(GameSpatialSoundError::Host)?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "game_spatial_sound_tests.rs"]
mod tests;
