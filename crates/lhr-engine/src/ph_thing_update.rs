//! Original C_PhThing::Update (PC 0x45a7a0). Actor fields and contact history
//! stay in the caller's live address space; frame and sound calls are ordered
//! effects on its existing scene owner.
use crate::ph_thing_response::{ph_thing_norm_squared, PhThingNumber as R};
use crate::vehicle_init::VehicleInitBackend;
use crate::{native_precision, native_transform::SMatrix};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhThingUpdateEffect {
    FrameVirtual {
        frame: u32,
        slot: u32,
        arguments: [u32; 2],
        count: u8,
    },
    UpdateWorld {
        frame: u32,
    },
    SetFrameSector {
        scene: u32,
        frame: u32,
        position: u32,
    },
    SetInactive {
        actor: u32,
        reason: u32,
    },
    LoadSound {
        manager: u32,
        sound: u32,
        source: u32,
    },
}
pub trait PhThingUpdateBackend: VehicleInitBackend {
    fn ph_thing_update_effect(&mut self, effect: PhThingUpdateEffect) -> Result<u32, Self::Error>;
}
#[derive(Debug, PartialEq, Eq)]
pub enum PhThingUpdateError<E> {
    Backend(E),
    StorageRange,
    NumericDomain,
}
type Out<T, E> = Result<T, PhThingUpdateError<E>>;
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    b.read_word(a).map_err(PhThingUpdateError::Backend)
}
fn byte<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u8, B::Error> {
    b.read_byte(a).map_err(PhThingUpdateError::Backend)
}
fn half<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u16, B::Error> {
    Ok(u16::from_le_bytes([byte(b, a)?, byte(b, a + 1)?]))
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    b.write_word(a, v).map_err(PhThingUpdateError::Backend)
}
fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<f32, B::Error> {
    let v = f32::from_bits(word(b, a)?);
    if v.is_finite() {
        Ok(v)
    } else {
        Err(PhThingUpdateError::NumericDomain)
    }
}
fn vector<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<[f32; 3], B::Error> {
    Ok([float(b, a)?, float(b, a + 4)?, float(b, a + 8)?])
}
fn copy<B: VehicleInitBackend>(b: &mut B, src: u32, dst: u32, words: u32) -> Out<(), B::Error> {
    for i in 0..words {
        let v = word(b, src + i * 4)?;
        put(b, dst + i * 4, v)?;
    }
    Ok(())
}
fn effect<B: PhThingUpdateBackend>(b: &mut B, e: PhThingUpdateEffect) -> Out<u32, B::Error> {
    b.ph_thing_update_effect(e)
        .map_err(PhThingUpdateError::Backend)
}
fn virtual_call<B: PhThingUpdateBackend>(
    b: &mut B,
    f: u32,
    slot: u32,
    args: &[u32],
) -> Out<u32, B::Error> {
    let mut arguments = [0; 2];
    arguments[..args.len()].copy_from_slice(args);
    effect(
        b,
        PhThingUpdateEffect::FrameVirtual {
            frame: f,
            slot,
            arguments,
            count: args.len() as u8,
        },
    )
}
fn count<E>(begin: u32, end: u32) -> Out<u32, E> {
    end.checked_sub(begin)
        .filter(|n| n.is_multiple_of(20) && n / 20 <= 1048576)
        .map(|n| n / 20)
        .ok_or(PhThingUpdateError::StorageRange)
}

impl<E> From<&'static str> for PhThingUpdateError<E> {
    fn from(_: &'static str) -> Self {
        Self::NumericDomain
    }
}
fn squared<E>(v: [R; 3], order: [usize; 3]) -> Out<R, E> {
    Ok(v[order[0]]
        .mul(v[order[0]])?
        .add(v[order[1]].mul(v[order[1]])?)?
        .add(v[order[2]].mul(v[order[2]])?)?)
}
fn difference<E>(a: [f32; 3], b: [f32; 3]) -> Out<[R; 3], E> {
    Ok([
        R::load(a[0])?.sub(R::load(b[0])?)?,
        R::load(a[1])?.sub(R::load(b[1])?)?,
        R::load(a[2])?.sub(R::load(b[2])?)?,
    ])
}
fn length_at_least<E>(v: [f32; 3], threshold: f32) -> Out<bool, E> {
    let n = ph_thing_norm_squared(v, [0, 1, 2])?.store()?;
    if (n.to_bits() as i32) < 0x322bcc77 {
        return Ok(false);
    }
    let n = R::load(n)?;
    Ok(n.sub(R::load(1.)?)?.value().abs() < f64::from(1e-8f32)
        || n.sqrt()?.value() >= f64::from(threshold))
}

fn history<B: PhThingUpdateBackend>(b: &mut B, a: u32, ms: u32) -> Out<(), B::Error> {
    let begin = word(b, a + 0x380)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, a + 0x384)?;
    let len = count::<B::Error>(begin, end)?;
    for i in (0..len).rev() {
        let begin = word(b, a + 0x380)?;
        let row = begin + i * 20;
        let life = word(b, row + 16)?.wrapping_sub(ms);
        put(b, row + 16, life)?;
        let begin = word(b, a + 0x380)?;
        let row = begin + i * 20;
        if (word(b, row + 16)? as i32) >= 0 {
            continue;
        }
        let end = word(b, a + 0x384)?;
        let mut src = row + 20;
        while src != end {
            copy(b, src, src - 20, 5)?;
            src += 20;
        }
        let end = word(b, a + 0x384)?;
        put(b, a + 0x384, end - 20)?;
    }
    Ok(())
}
fn wake_or_sleep<B: PhThingUpdateBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    let begin = word(b, a + 0x380)?;
    if begin == 0 {
        return effect(
            b,
            PhThingUpdateEffect::SetInactive {
                actor: a,
                reason: 1,
            },
        )
        .map(|_| ());
    }
    let end = word(b, a + 0x384)?;
    let len = count::<B::Error>(begin, end)?;
    if len == 0 {
        return effect(
            b,
            PhThingUpdateEffect::SetInactive {
                actor: a,
                reason: 1,
            },
        )
        .map(|_| ());
    }
    let begin = word(b, a + 0x380)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, a + 0x384)?;
    let len = count::<B::Error>(begin, end)?;
    for i in (0..len).rev() {
        let begin = word(b, a + 0x380)?;
        let row = begin + i * 20;
        let actor = word(b, row + 12)?;
        let point = ph_thing_contact_position(b, actor)?;
        let old = vector(b, row)?;
        let delta = difference::<B::Error>(old, point)?;
        if squared::<B::Error>(delta, [2, 1, 0])?.value() <= f64::from(0.000016f32) {
            continue;
        }
        let begin = word(b, a + 0x380)?;
        let actor = word(b, begin + i * 20 + 12)?;
        let point = ph_thing_contact_position(b, actor)?;
        for (j, v) in point.into_iter().enumerate() {
            put(b, begin + i * 20 + j as u32 * 4, v.to_bits())?;
        }
        put(b, a + 0x32c, 0)?;
        put(b, a + 0x9c, 0)?;
        break;
    }
    Ok(())
}

/// Original C_actor::GetPos (0x407fe0), including the null-model branch.
pub fn ph_thing_contact_position<B: PhThingUpdateBackend>(
    b: &mut B,
    a: u32,
) -> Out<[f32; 3], B::Error> {
    let frame = word(b, a + 0x68)?;
    if frame == 0 {
        return vector(b, a + 0x24);
    }
    if byte(b, frame + 0xac)? & 0x20 == 0 {
        effect(b, PhThingUpdateEffect::UpdateWorld { frame })?;
    }
    vector(b, frame + 0x40)
}
fn matrix<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    for (i, v) in elements.iter_mut().enumerate() {
        *v = float(b, a + i as u32 * 4)?;
    }
    Ok(SMatrix { elements })
}
/// Frame/matrix refresh helper at PC 0x460220; the Rust suffix is descriptive.
pub fn ph_thing_transform<B: PhThingUpdateBackend>(b: &mut B, a: u32) -> Out<(), B::Error> {
    let frame = word(b, a + 0x68)?;
    if byte(b, frame + 0xac)? & 0x20 == 0 {
        effect(b, PhThingUpdateEffect::UpdateWorld { frame })?;
    }
    copy(b, frame + 0x10, a + 0x10c, 16)?;
    copy(b, frame + 0x10, a + 0x14c, 16)?;
    let (inverse, _) = matrix(b, a + 0x14c)?
        .inverse()
        .map_err(|_| PhThingUpdateError::NumericDomain)?;
    for (i, v) in inverse.elements.into_iter().enumerate() {
        put(b, a + 0x14c + i as u32 * 4, v.to_bits())?;
    }
    copy(b, a + 0x10c, a + 0x18c, 16)?;
    for o in [0x1bc, 0x1c0, 0x1c4] {
        put(b, a + o, 0)?;
    }
    copy(b, a + 0x14c, a + 0x1cc, 16)?;
    for o in [0x1fc, 0x200, 0x204] {
        put(b, a + o, 0)?;
    }
    if byte(b, a + 0x76)? != 0 {
        let point = vector(b, a + 0x78)?;
        let value = matrix(b, a + 0x10c)?
            .try_transform_point(point)
            .map_err(|_| PhThingUpdateError::NumericDomain)?;
        for (i, v) in value.into_iter().enumerate() {
            put(b, a + 0x84 + i as u32 * 4, v.to_bits())?;
        }
    } else {
        let frame = word(b, a + 0x68)?;
        if byte(b, frame + 0xac)? & 0x20 == 0 {
            effect(b, PhThingUpdateEffect::UpdateWorld { frame })?;
        }
        copy(b, frame + 0x40, a + 0x84, 3)?;
    }
    let frame = word(b, a + 0x68)?;
    if byte(b, frame + 0xac)? & 0x20 == 0 {
        effect(b, PhThingUpdateEffect::UpdateWorld { frame })?;
    }
    copy(b, frame + 0x40, a + 0x24, 3)?;
    copy(b, a + 0x10c, a + 0xa0, 3)?;
    copy(b, a + 0x11c, a + 0xac, 3)?;
    copy(b, a + 0x12c, a + 0x30, 3)
}

fn collision_sound<B: PhThingUpdateBackend>(b: &mut B, a: u32, ms: u32) -> Out<(), B::Error> {
    let sound = word(b, a + 0x338)?;
    if sound == 0 || (word(b, a + 0x348)? as i32) < 0 {
        return Ok(());
    }
    let elapsed = word(b, a + 0x374)?.wrapping_add(ms);
    put(b, a + 0x374, elapsed)?;
    let playing = virtual_call(b, sound, 0x54, &[0])? as u8 != 0;
    let mass = float(b, a + 0x210)?;
    let low = R::load(mass)?.mul(R::load(0.4)?)?.store()?;
    let high = R::load(mass)?.mul(R::load(5.6)?)?.store()?;
    if playing && word(b, a + 0x374)? <= 150 {
        return Ok(());
    }
    if float(b, a + 0x370)? <= low {
        return Ok(());
    }
    let index = half(b, a + 0x36c)?;
    let material = half(b, a + 0x350 + u32::from(index) * 2)?;
    if material == 0 || u32::from(material) >= word(b, 0x6d4c28)? {
        return Ok(());
    }
    if byte(b, a + 0x378)? == 0 {
        let sound = word(b, a + 0x338)?;
        if virtual_call(b, sound, 0x54, &[0])? as u8 != 0 {
            return Ok(());
        }
    }
    let sound = word(b, a + 0x338)?;
    virtual_call(b, sound, 0x24, &[0])?;
    let index = half(b, a + 0x36c)?;
    let material = half(b, a + 0x350 + u32::from(index) * 2)?;
    let kind = word(b, a + 0x348)?;
    let materials = word(b, 0x6d4c24)?;
    let source = word(
        b,
        materials + (u32::from(material) * 0x5c + kind) * 4 + 0x130,
    )?;
    let sound = word(b, a + 0x338)?;
    let loaded = effect(
        b,
        PhThingUpdateEffect::LoadSound {
            manager: 0x646f50,
            sound,
            source,
        },
    )? as u8
        != 0;
    if loaded {
        let sound = word(b, a + 0x338)?;
        virtual_call(b, sound, 0x24, &[1])?;
        let impulse = float(b, a + 0x370)?;
        let volume = if impulse > high {
            1.
        } else {
            R::load(impulse)?
                .sub(R::load(low)?)?
                .div(R::load(high)?.sub(R::load(low)?)?)?
                .mul(R::load(0.8)?)?
                .add(R::load(0.19)?)?
                .store()?
        };
        let sound = word(b, a + 0x338)?;
        virtual_call(b, sound, 0x68, &[volume.to_bits()])?;
        let sound = word(b, a + 0x338)?;
        if (byte(b, sound + 0xac)? as i8) >= 0 {
            copy(b, a + 0x33c, sound + 0x80, 3)?;
            let flags = word(b, sound + 0xac)?;
            put(b, sound + 0xac, (flags & 0xfffffedf) | 0x40000000)?;
        }
    }
    put(b, a + 0x374, 0)
}

/// Complete original Update orchestration; every failed effect retains the
/// preceding actor/history/frame mutations and must not be replayed.
pub fn ph_thing_update<B: PhThingUpdateBackend>(b: &mut B, a: u32, ms: u32) -> Out<(), B::Error> {
    let ms = ms.min(83);
    if ms == 0 {
        return Ok(());
    }
    let dt = native_precision::mul(ms as f32, 0.001);
    if byte(b, a + 0x20)? == 0 {
        let velocity = vector(b, a + 0x220)?;
        let moving = length_at_least::<B::Error>(velocity, 0.3)?
            || length_at_least::<B::Error>(vector(b, a + 0x22c)?, 0.1)?;
        if moving {
            put(b, a + 0x32c, 0)?;
        } else {
            let idle = native_precision::add(dt, float(b, a + 0x32c)?);
            put(b, a + 0x32c, idle.to_bits())?;
        }
        let position = vector(b, a + 0x24)?;
        let old = vector(b, a + 0x90)?;
        let delta = difference::<B::Error>(position, old)?;
        if squared::<B::Error>(delta, [1, 2, 0])?.value() >= f64::from(0.04f32) {
            let x = word(b, a + 0x24)?;
            let y = word(b, a + 0x28)?;
            let z = word(b, a + 0x2c)?;
            put(b, a + 0x90, x)?;
            put(b, a + 0x94, y)?;
            put(b, a + 0x9c, 0)?;
            put(b, a + 0x98, z)?;
        } else {
            let elapsed = word(b, a + 0x9c)?.wrapping_add(ms);
            put(b, a + 0x9c, elapsed)?;
        }
        if float(b, a + 0x32c)? > float(b, a + 0x330)? || word(b, a + 0x9c)? > 4000 {
            wake_or_sleep(b, a)?;
        } else {
            history(b, a, ms)?;
        }
        if float(b, a + 0x224)? < -85. {
            effect(
                b,
                PhThingUpdateEffect::SetInactive {
                    actor: a,
                    reason: 2,
                },
            )?;
        }
    }
    let frame = word(b, a + 0x68)?;
    virtual_call(b, frame, 0x18, &[])?;
    let frame = word(b, a + 0x68)?;
    let app = word(b, 0x63788c)?;
    let scene = word(b, app + 0x10)?;
    if byte(b, frame + 0xac)? & 0x20 == 0 {
        effect(b, PhThingUpdateEffect::UpdateWorld { frame })?;
    }
    effect(
        b,
        PhThingUpdateEffect::SetFrameSector {
            scene,
            frame,
            position: frame + 0x40,
        },
    )?;
    ph_thing_transform(b, a)?;
    copy(b, a + 0x24, a + 0xc8, 3)?;
    crate::dynamic_collision_manager::dyn_update(b, 0x647f48, a + 0xb8).map_err(|e| match e {
        crate::dynamic_collision_manager::DynamicCollisionError::Backend(e) => {
            PhThingUpdateError::Backend(e)
        }
        _ => PhThingUpdateError::StorageRange,
    })?;
    collision_sound(b, a, ms)?;
    put(b, a + 0x370, 0)?;
    b.write_byte(a + 0x36c, 0)
        .map_err(PhThingUpdateError::Backend)?;
    b.write_byte(a + 0x36d, 0)
        .map_err(PhThingUpdateError::Backend)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError};
    impl PhThingUpdateBackend for VehicleFrameMemory {
        fn ph_thing_update_effect(
            &mut self,
            _: PhThingUpdateEffect,
        ) -> Result<u32, VehicleMemoryError> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
    }
    const ACTOR: u32 = 0x1000;
    const FRAME: u32 = 0x2000;
    fn setup() -> VehicleFrameMemory {
        let mut b = VehicleFrameMemory::default();
        b.map_bytes(ACTOR, &[0; 0x400]).unwrap();
        b.map_bytes(FRAME, &[0; 0x200]).unwrap();
        put(&mut b, ACTOR + 0x68, FRAME).unwrap();
        put(&mut b, FRAME + 0xac, 0x20).unwrap();
        let mut m = SMatrix::identity();
        m.elements[12..15].copy_from_slice(&[10., -2., 5.]);
        for (i, v) in m.elements.into_iter().enumerate() {
            put(&mut b, FRAME + 0x10 + i as u32 * 4, v.to_bits()).unwrap();
        }
        b
    }
    #[test]
    fn matrix_refresh_publishes_actual_center_and_orientation_without_moving_frame() {
        let mut b = setup();
        b.write_byte(ACTOR + 0x76, 1).unwrap();
        for (i, v) in [1f32, 2., 3.].into_iter().enumerate() {
            put(&mut b, ACTOR + 0x78 + i as u32 * 4, v.to_bits()).unwrap();
        }
        ph_thing_transform(&mut b, ACTOR).unwrap();
        assert_eq!(vector(&mut b, ACTOR + 0x24).unwrap(), [10., -2., 5.]);
        assert_eq!(vector(&mut b, ACTOR + 0x84).unwrap(), [11., 0., 8.]);
        assert_eq!(vector(&mut b, ACTOR + 0xa0).unwrap(), [1., 0., 0.]);
        assert_eq!(vector(&mut b, ACTOR + 0xac).unwrap(), [0., 1., 0.]);
        assert_eq!(vector(&mut b, ACTOR + 0x30).unwrap(), [0., 0., 1.]);
        assert_eq!(vector(&mut b, ACTOR + 0x17c).unwrap(), [-10., 2., -5.]);
        assert_eq!(vector(&mut b, ACTOR + 0x1bc).unwrap(), [0.; 3]);
        assert_eq!(vector(&mut b, ACTOR + 0x1fc).unwrap(), [0.; 3]);
        assert_eq!(vector(&mut b, FRAME + 0x40).unwrap(), [10., -2., 5.]);
    }
    #[test]
    fn actor_position_uses_live_frame_then_original_null_frame_fallback() {
        let mut b = setup();
        assert_eq!(
            ph_thing_contact_position(&mut b, ACTOR).unwrap(),
            [10., -2., 5.]
        );
        put(&mut b, ACTOR + 0x68, 0).unwrap();
        for (i, v) in [-0f32, 3., 4.].into_iter().enumerate() {
            put(&mut b, ACTOR + 0x24 + i as u32 * 4, v.to_bits()).unwrap();
        }
        assert_eq!(
            ph_thing_contact_position(&mut b, ACTOR)
                .unwrap()
                .map(f32::to_bits),
            [-0f32, 3., 4.].map(f32::to_bits)
        );
    }
    #[test]
    fn expired_history_erases_in_order_and_retains_surviving_timer() {
        let mut b = setup();
        let rows = b.allocate(60).unwrap();
        put(&mut b, ACTOR + 0x380, rows).unwrap();
        put(&mut b, ACTOR + 0x384, rows + 60).unwrap();
        for (i, timer) in [u32::MAX, 0, 5].into_iter().enumerate() {
            for j in 0..4 {
                put(&mut b, rows + i as u32 * 20 + j * 4, 100 + i as u32 * 4 + j).unwrap();
            }
            put(&mut b, rows + i as u32 * 20 + 16, timer).unwrap();
        }
        history(&mut b, ACTOR, 1).unwrap();
        assert_eq!(word(&mut b, ACTOR + 0x384).unwrap(), rows + 20);
        for j in 0..4 {
            assert_eq!(word(&mut b, rows + j * 4).unwrap(), 108 + j);
        }
        assert_eq!(word(&mut b, rows + 16).unwrap(), 4);
    }
    #[test]
    fn motion_length_keeps_cutoff_and_does_not_clamp_small_operands() {
        assert!(!length_at_least::<()>([0.; 3], 0.3).unwrap());
        assert!(!length_at_least::<()>([f32::MIN_POSITIVE, 0., 0.], 0.3).unwrap());
        assert!(!length_at_least::<()>([0.299, 0., 0.], 0.3).unwrap());
        assert!(length_at_least::<()>([0.301, 0., 0.], 0.3).unwrap());
        assert!(length_at_least::<()>([0., 1., 0.], 0.3).unwrap());
    }
}
