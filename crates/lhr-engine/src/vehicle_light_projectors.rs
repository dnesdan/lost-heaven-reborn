//! Game projector construction helpers, PC RVAs0xc2440 and0xc29f0.
//! Driver/cache/virtual methods are real resource boundaries, with no defaults.
use super::*;
use crate::vehicle_component_records::vehicle_ballast_records_insert;

pub trait VehicleLightResourceBackend: VehicleAxisUpdateBackend {
    /// Driver virtual+0x4c, visual subtype PROJ=0x4a4f5250.
    fn create_vehicle_light_visual(&mut self, subtype: u32) -> Result<u32, Self::Error>;
    /// Driver virtual+0x50, frame class2.
    fn create_vehicle_light_frame(&mut self, kind: u32) -> Result<u32, Self::Error>;
    /// Actual frame/projector/light/texture vtable call. `args` excludes this;
    /// addresses are inert backend storage. Unknown effects must be unavailable.
    fn vehicle_light_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error>;
    /// Global647ee0 virtual+4(this,name,emptyName,0,&texture). Returns the native
    /// status and retained texture identity, without inventing cache success.
    fn vehicle_light_texture_cache(&mut self, name: &[u8]) -> Result<(u32, u32), Self::Error>;
}
type R<B, T> = Result<T, VehicleLightInitUnavailable<<B as VehicleInitBackend>::Error>>;
fn a(x: u32, y: u32) -> u32 {
    x.wrapping_add(y)
}
fn get<B: VehicleInitBackend>(b: &mut B, p: u32) -> R<B, u32> {
    b.read_word(p).map_err(Into::into)
}
fn set<B: VehicleInitBackend>(b: &mut B, p: u32, v: u32) -> R<B, ()> {
    b.write_word(p, v).map_err(Into::into)
}
fn vector<B: VehicleInitBackend>(b: &mut B, p: u32) -> R<B, [u32; 3]> {
    let mut v = [0; 3];
    for (i, x) in v.iter_mut().enumerate() {
        *x = get(b, a(p, i as u32 * 4))?;
    }
    Ok(v)
}
fn call<B: VehicleLightResourceBackend>(b: &mut B, f: u32, slot: u32, args: &[u32]) -> R<B, u32> {
    b.vehicle_light_virtual(f, slot, args).map_err(Into::into)
}
fn cstring<B: VehicleInitBackend>(b: &mut B, p: u32) -> R<B, Vec<u8>> {
    let mut out = Vec::new();
    let mut p = p;
    loop {
        let byte = b.read_byte(p)?;
        if byte == 0 {
            break;
        }
        out.push(byte);
        p = a(p, 1);
    }
    Ok(out)
}
fn world<B: VehicleLightResourceBackend>(b: &mut B, f: u32) -> R<B, ()> {
    if b.read_byte(a(f, 0xac))? & 0x20 == 0 {
        b.update_model_world_matrix(f)?;
    }
    Ok(())
}
fn position<B: VehicleLightResourceBackend>(b: &mut B, d: u32, s: u32) -> R<B, ()> {
    world(b, s)?;
    if b.read_byte(a(d, 0xac))? & 0x80 == 0 {
        for i in 0..3 {
            let v = get(b, a(s, 0x40 + i * 4))?;
            set(b, a(d, 0x80 + i * 4), v)?;
        }
        let flags = get(b, a(d, 0xac))?;
        set(b, a(d, 0xac), (flags & 0xffff_fedf) | 0x4000_0000)?;
    }
    Ok(())
}
fn direction<B: VehicleLightResourceBackend>(b: &mut B, f: u32) -> R<B, [u32; 3]> {
    world(b, f)?;
    super::components::normalize_direction::<B::Error>(vector(b, a(f, 0x30))?)
        .map_err(|_| VehicleLightInitUnavailable::NumericDomain)
}
fn configure_visual<B: VehicleLightResourceBackend>(
    b: &mut B,
    visual: u32,
    frame: u32,
    scratch: u32,
) -> R<B, ()> {
    position(b, visual, frame)?;
    let dir = direction(b, frame)?;
    b.set_axis_frame_direction(visual, dir)?;
    for (off, v) in [
        (0x90, 0x4100_0000),
        (0x94, 0x40a0_0000),
        (0x98, 0x41a0_0000),
    ] {
        set(b, a(visual, off), v)?;
    }
    if b.read_byte(a(visual, 0xac))? & 8 == 0 {
        b.materialize_frame_quaternion(visual)?;
    }
    let flags = get(b, a(visual, 0xac))?;
    set(b, a(visual, 0xac), (flags & 0xffff_fecb) | 0x4000_0008)?;
    for i in 0..3 {
        set(b, a(scratch, i * 4), 0x3f00_0000)?;
    }
    call(b, visual, 0x84, &[scratch])?;
    call(b, visual, 0x88, &[4])?;
    call(b, visual, 0x2c, &[0, 0])?;
    Ok(())
}
fn configure_light<B: VehicleLightResourceBackend>(
    b: &mut B,
    light: u32,
    frame: u32,
    dir: [u32; 3],
    scratch: u32,
) -> R<B, ()> {
    position(b, light, frame)?;
    b.set_axis_frame_direction(light, dir)?;
    for (slot, args) in [
        (0x50, &[2][..]),
        (0x60, &[0x11][..]),
        (0x5c, &[0x4000_0000][..]),
        (0x64, &[0x41c8_0000, 0x4270_0000][..]),
        (0x68, &[0x3f05_1eb8, 0x3f9c_28f6][..]),
    ] {
        call(b, light, slot, args)?;
    }
    for (i, v) in [0x3f44_dd2f, 0x3f2c_cccd, 0x3f0a_7efa]
        .into_iter()
        .enumerate()
    {
        set(b, a(scratch, i as u32 * 4), v)?;
    }
    call(b, light, 0x58, &[scratch])?;
    call(b, light, 0x2c, &[0, 0])?;
    call(b, light, 0x24, &[0])?;
    call(b, light, 0x18, &[])?;
    Ok(())
}
/// Original single-texture helper. `stack` is its ESP after SUB0xa0 and the
/// first two saved registers. The caller supplies real scratch storage.
pub fn vehicle_create_light_projector<B: VehicleLightResourceBackend>(
    b: &mut B,
    _vehicle: u32,
    frame: u32,
    output: u32,
    stack: u32,
) -> R<B, u32> {
    let properties = get(b, a(frame, 12))?;
    let bytes = if properties == 0 {
        Vec::new()
    } else {
        cstring(b, properties)?
    };
    if bytes.len() <= 2 {
        return Ok(0);
    }
    let Some(open) = bytes.iter().position(|c| *c == b'[') else {
        return Ok(0);
    };
    let suffix = &bytes[open + 1..];
    let Some(close) = suffix.iter().rposition(|c| *c == b']') else {
        return Ok(0);
    };
    if close == 0 {
        return Ok(0);
    }
    // The original copies the complete suffix before cutting at its last ].
    for (i, v) in suffix.iter().copied().chain([0]).enumerate() {
        b.write_byte(a(stack, 0x28 + i as u32), v)?;
    }
    b.write_byte(a(stack, 0x28 + close as u32), 0)?;
    let visual = b.create_vehicle_light_visual(0x4a4f_5250)?;
    if visual == 0 {
        return Ok(0);
    }
    configure_visual(b, visual, frame, a(stack, 0x1c))?;
    let (status, texture) = b.vehicle_light_texture_cache(&suffix[..close])?;
    if status == 0 {
        call(b, visual, 0x70, &[texture])?;
        call(b, texture, 0, &[])?;
    }
    call(b, visual, 0x78, &[0])?;
    call(b, visual, 0x24, &[0])?;
    call(b, visual, 0x18, &[])?;
    let light = b.create_vehicle_light_frame(2)?;
    set(b, output, light)?;
    if light != 0 {
        let dir = direction(b, frame)?;
        configure_light(b, light, frame, dir, a(stack, 0x10))?;
    }
    Ok(visual)
}
/// Original triple-texture helper. Successful cache references move into the
/// vehicle's 28-byte projector record, without setting its visual texture yet.
pub fn vehicle_create_shared_projector<B: VehicleLightResourceBackend>(
    b: &mut B,
    vehicle: u32,
    frame: u32,
    output: u32,
    stack: u32,
) -> R<B, u32> {
    let properties = get(b, a(frame, 12))?;
    let bytes = if properties == 0 {
        Vec::new()
    } else {
        cstring(b, properties)?
    };
    if bytes.len() <= 2 {
        return Ok(0);
    }
    let Some(open) = bytes.iter().position(|c| *c == b'[') else {
        return Ok(0);
    };
    let suffix = &bytes[open + 1..];
    let Some(first) = suffix.iter().position(|c| *c == b',') else {
        return Ok(0);
    };
    let after = &suffix[first + 1..];
    let Some(second) = after.iter().position(|c| *c == b',') else {
        return Ok(0);
    };
    let last = &after[second + 1..];
    let Some(close) = last.iter().position(|c| *c == b']') else {
        return Ok(0);
    };
    let names = [&suffix[..first], &after[..second], &last[..close]];
    for (offset, name) in [0x5c, 0x7c, 0x9c].into_iter().zip(names) {
        for (i, byte) in name.iter().copied().chain([0]).enumerate() {
            b.write_byte(a(stack, offset + i as u32), byte)?;
        }
    }
    let visual = b.create_vehicle_light_visual(0x4a4f_5250)?;
    if visual == 0 {
        return Ok(0);
    }
    configure_visual(b, visual, frame, a(stack, 0x30))?;
    let mut textures = [0; 3];
    for i in 0..3 {
        let (status, texture) = b.vehicle_light_texture_cache(names[i])?;
        if status != 0 {
            call(b, visual, 0, &[])?;
            for texture in &textures[..i] {
                call(b, *texture, 0, &[])?;
            }
            return Ok(0);
        }
        textures[i] = texture;
    }
    call(b, visual, 0x78, &[0])?;
    call(b, visual, 0x24, &[0])?;
    call(b, visual, 0x18, &[])?;
    let light = b.create_vehicle_light_frame(2)?;
    set(b, output, light)?;
    if light != 0 {
        let dir = vector(b, a(vehicle, 0xc3c))?;
        configure_light(b, light, frame, dir, a(stack, 0x30))?;
    }
    let record = a(stack, 0x40);
    for off in (0..28).step_by(4) {
        set(b, a(record, off), 0)?;
    }
    for (off, value) in [
        (0, frame),
        (4, visual),
        (8, light),
        (12, textures[0]),
        (16, textures[1]),
        (20, textures[2]),
    ] {
        set(b, a(record, off), value)?;
    }
    let end = get(b, a(vehicle, 0x114))?;
    vehicle_ballast_records_insert(b, a(vehicle, 0x10c), end, 1, record)?;
    Ok(visual)
}
