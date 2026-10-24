//! Original CreateCrashModel (PC0x431710) and DoChange (PC0x431a00).
//! Frame identities are checked by their owning world registry, not cast into
//! car-memory pointers. Actor creation remains an explicit original worker.
use crate::{game_crash_registry::CrashRegistryError, vehicle_init::VehicleInitBackend};

#[derive(Clone, Debug, PartialEq)]
pub struct VehicleCreateCrashActorsCall {
    pub source_frame: u32,
    pub point: [f32; 3],
    pub velocity: [f32; 3],
    pub strength: f32,
    pub filename: Vec<u8>,
    pub change_filename: bool,
    pub byte_argument: u8,
    pub ancestor_depth: i32,
    pub recursive_flag: bool,
    pub actor_mask: u8,
}
pub trait VehicleCrashModelBackend: VehicleInitBackend {
    fn crash_frame_kind(&mut self, frame: u32) -> Result<u32, Self::Error>;
    fn crash_frame_parent(&mut self, frame: u32) -> Result<Option<u32>, Self::Error>;
    fn crash_frame_children(&mut self, frame: u32) -> Result<Vec<u32>, Self::Error>;
    fn crash_frame_properties(&mut self, frame: u32) -> Result<Vec<u8>, Self::Error>;
    fn crash_model_filename(&mut self, frame: u32) -> Result<Vec<u8>, Self::Error>;
    fn crash_model_collisions_off(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn crash_create_actors(
        &mut self,
        call: VehicleCreateCrashActorsCall,
    ) -> Result<bool, Self::Error>;
    fn crash_set_on(&mut self, frame: u32, on: bool) -> Result<(), Self::Error>;
    fn crash_debustize(&mut self, frame: u32) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum VehicleCrashModelError<E> {
    Host(E),
    Registry(CrashRegistryError<E>),
    NumericDomain,
    StringExtent,
    NullAllocation,
}
type Out<B, T> = Result<T, VehicleCrashModelError<<B as VehicleInitBackend>::Error>>;
fn word<B: VehicleInitBackend>(b: &mut B, address: u32) -> Out<B, u32> {
    b.read_word(address).map_err(VehicleCrashModelError::Host)
}
fn write<B: VehicleInitBackend>(b: &mut B, address: u32, value: u8) -> Out<B, ()> {
    b.write_byte(address, value)
        .map_err(VehicleCrashModelError::Host)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleCrashChange {
    pub change_filename: bool,
    pub ancestor_depth: i32,
    pub actor_mask: u8,
}
/// Original temporary property-string copy, uppercase semicolon section and
/// P/Z/dot interpretation. The caller initializes actor_mask to1.
pub fn vehicle_crash_do_change<B: VehicleCrashModelBackend>(
    b: &mut B,
    mut frame: u32,
) -> Out<B, VehicleCrashChange> {
    if b.crash_frame_kind(frame)
        .map_err(VehicleCrashModelError::Host)?
        == 9
    {
        let children = b
            .crash_frame_children(frame)
            .map_err(VehicleCrashModelError::Host)?;
        for child in children.into_iter().rev() {
            if b.crash_frame_kind(child)
                .map_err(VehicleCrashModelError::Host)?
                == 1
            {
                frame = child;
                break;
            }
        }
    }
    let mut result = VehicleCrashChange {
        change_filename: true,
        ancestor_depth: 0,
        actor_mask: 1,
    };
    let mut bytes = b
        .crash_frame_properties(frame)
        .map_err(VehicleCrashModelError::Host)?;
    bytes.truncate(bytes.iter().position(|x| *x == 0).unwrap_or(bytes.len()));
    let size = u32::try_from(bytes.len())
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or(VehicleCrashModelError::StringExtent)?;
    let allocation = b.allocate(size).map_err(VehicleCrashModelError::Host)?;
    if allocation == 0 {
        return Err(VehicleCrashModelError::NullAllocation);
    }
    for (index, value) in bytes.iter().copied().chain([0]).enumerate() {
        write(b, allocation + index as u32, value)?;
    }
    if let Some(begin) = bytes.iter().position(|x| *x == b';') {
        for (index, value) in bytes.iter_mut().enumerate().skip(begin) {
            *value = value.to_ascii_uppercase();
            write(b, allocation + index as u32, *value)?;
        }
        let end = bytes[begin + 1..]
            .iter()
            .position(|x| *x == b';')
            .map(|i| i + begin + 1)
            .unwrap_or(bytes.len());
        if end < bytes.len() {
            write(b, allocation + end as u32, 0)?;
        }
        let section = &bytes[begin..end];
        result.change_filename = !section.contains(&b'.');
        if section.contains(&b'Z') {
            result.actor_mask = 0;
        }
        if let Some(position) = section.iter().position(|x| *x == b'P') {
            let digits = &section[position + 1..];
            let length = digits.iter().take_while(|x| x.is_ascii_digit()).count();
            let mut integer = 0_u32;
            for &digit in &digits[..length] {
                integer = integer
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u32::from(digit - b'0')))
                    .ok_or(VehicleCrashModelError::NumericDomain)?;
            }
            // This admitted integer range converts exactly through the
            // original atof/FTOL path; larger decimal parser profiles remain
            // unavailable rather than inventing overflow behavior.
            if integer > 1 << 24 {
                return Err(VehicleCrashModelError::NumericDomain);
            }
            result.ancestor_depth = integer as i32;
        }
    }
    b.free(allocation).map_err(VehicleCrashModelError::Host)?;
    Ok(result)
}
fn game<B: VehicleInitBackend>(b: &mut B) -> Out<B, u32> {
    let root = word(b, 0x63788c)?;
    word(b, root + 0x24)
}
/// Original CreateCrashModel frame/model orchestration. The actor worker's
/// false result is intentionally ignored, as in the primary caller.
pub fn vehicle_create_crash_model<B: VehicleCrashModelBackend>(
    b: &mut B,
    mut frame: u32,
    point: [f32; 3],
    velocity: [f32; 3],
    strength: f32,
) -> Out<B, bool> {
    if frame == 0 {
        return Ok(false);
    }
    let change = vehicle_crash_do_change(b, frame)?;
    while b
        .crash_frame_kind(frame)
        .map_err(VehicleCrashModelError::Host)?
        != 9
    {
        let Some(parent) = b
            .crash_frame_parent(frame)
            .map_err(VehicleCrashModelError::Host)?
        else {
            return Ok(false);
        };
        frame = parent;
    }
    let owner = game(b)?;
    crate::game_crash_registry::del_semaphore(b, owner, frame)
        .map_err(VehicleCrashModelError::Registry)?;
    let owner = game(b)?;
    crate::game_crash_registry::del_telephones(b, owner, frame)
        .map_err(VehicleCrashModelError::Registry)?;
    b.crash_model_collisions_off(frame)
        .map_err(VehicleCrashModelError::Host)?;
    let mut filename = b
        .crash_model_filename(frame)
        .map_err(VehicleCrashModelError::Host)?;
    filename.truncate(
        filename
            .iter()
            .position(|x| *x == 0)
            .unwrap_or(filename.len()),
    );
    if filename.len() >= 128 {
        return Err(VehicleCrashModelError::StringExtent);
    }
    let begin = filename
        .iter()
        .rposition(|x| *x == b'\\')
        .map(|n| n + 1)
        .unwrap_or(0);
    let mut filename = filename[begin..].to_vec();
    if change.change_filename {
        if let Some(dot) = filename.iter().position(|x| *x == b'.') {
            filename.truncate(dot);
        }
        if begin + filename.len() + 7 >= 128 {
            return Err(VehicleCrashModelError::StringExtent);
        }
        filename.extend_from_slice(b"_c1.i3d");
    }
    let _ = b
        .crash_create_actors(VehicleCreateCrashActorsCall {
            source_frame: frame,
            point,
            velocity,
            strength,
            filename,
            change_filename: change.change_filename,
            byte_argument: 0,
            ancestor_depth: change.ancestor_depth,
            recursive_flag: false,
            actor_mask: change.actor_mask,
        })
        .map_err(VehicleCrashModelError::Host)?;
    let owner = game(b)?;
    crate::game_crash_registry::retain_hidden_model(b, owner, frame)
        .map_err(VehicleCrashModelError::Registry)?;
    b.crash_set_on(frame, false)
        .map_err(VehicleCrashModelError::Host)?;
    b.crash_debustize(frame)
        .map_err(VehicleCrashModelError::Host)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError},
        vehicle_init::*,
    };
    struct Backend {
        memory: VehicleFrameMemory,
        effects: Vec<&'static str>,
        call: Option<VehicleCreateCrashActorsCall>,
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
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.memory.allocate(n)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.memory.free(a)
        }
        fn collect_frame(
            &mut self,
            v: &mut VehicleInitFrameVector,
            f: u32,
        ) -> Result<(), Self::Error> {
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
    impl Backend {
        fn string(&mut self, a: u32) -> Result<Vec<u8>, VehicleMemoryError> {
            if a == 0 {
                return Ok(vec![]);
            }
            let mut out = vec![];
            for i in 0..256 {
                let c = self.read_byte(a + i)?;
                if c == 0 {
                    return Ok(out);
                }
                out.push(c);
            }
            Err(VehicleMemoryError::Capacity)
        }
    }
    impl VehicleCrashModelBackend for Backend {
        fn crash_frame_kind(&mut self, f: u32) -> Result<u32, Self::Error> {
            self.read_word(f + 0x110)
        }
        fn crash_frame_parent(&mut self, f: u32) -> Result<Option<u32>, Self::Error> {
            let p = self.read_word(f + 0x120)?;
            Ok((p != 0).then_some(p))
        }
        fn crash_frame_children(&mut self, f: u32) -> Result<Vec<u32>, Self::Error> {
            let p = self.read_word(f + 0x114)?;
            let n = self.read_word(f + 0x11c)?;
            (0..n).map(|i| self.read_word(p + i * 4)).collect()
        }
        fn crash_frame_properties(&mut self, f: u32) -> Result<Vec<u8>, Self::Error> {
            let p = self.read_word(f + 0xc)?;
            self.string(p)
        }
        fn crash_model_filename(&mut self, f: u32) -> Result<Vec<u8>, Self::Error> {
            let p = self.read_word(f + 0x154)?;
            self.string(p)
        }
        fn crash_model_collisions_off(&mut self, _: u32) -> Result<(), Self::Error> {
            self.effects.push("collisions_off");
            Ok(())
        }
        fn crash_create_actors(
            &mut self,
            c: VehicleCreateCrashActorsCall,
        ) -> Result<bool, Self::Error> {
            self.effects.push("actors");
            self.call = Some(c);
            Ok(false)
        }
        fn crash_set_on(&mut self, f: u32, on: bool) -> Result<(), Self::Error> {
            self.effects.push("on");
            self.write_word(f + 0xac, u32::from(on))
        }
        fn crash_debustize(&mut self, _: u32) -> Result<(), Self::Error> {
            self.effects.push("debustize");
            Ok(())
        }
    }
    fn fixture(properties: &[u8]) -> Backend {
        let mut b = Backend {
            memory: VehicleFrameMemory::default(),
            effects: vec![],
            call: None,
        };
        for f in [0x1000, 0x2000, 0x3000] {
            b.memory.map_bytes(f, &[0; 0x200]).unwrap();
        }
        b.memory.map_bytes(0x4000, b";P999;\0").unwrap();
        let mut p = properties.to_vec();
        p.push(0);
        b.memory.map_bytes(0x4100, &p).unwrap();
        b.memory
            .map_bytes(0x5000, b"models\\synthetic.i3d\0")
            .unwrap();
        b.memory.map_bytes(0x6000, &[0; 8]).unwrap();
        b.memory.map_bytes(0x7000, &[0; 0x28]).unwrap();
        b.memory.map_bytes(0x8000, &[0; 0x300]).unwrap();
        b.memory
            .map_bytes(0x63788c, &0x7000_u32.to_le_bytes())
            .unwrap();
        for (a, v) in [
            (0x1110, 9),
            (0x2110, 1),
            (0x3110, 1),
            (0x211c, 0),
            (0x311c, 0),
            (0x1114, 0x6000),
            (0x111c, 2),
            (0x200c, 0x4000),
            (0x300c, 0x4100),
            (0x1154, 0x5000),
            (0x6000, 0x2000),
            (0x6004, 0x3000),
            (0x7024, 0x8000),
            (0x10ac, 1),
        ] {
            b.write_word(a, v).unwrap();
        }
        b
    }
    #[test]
    fn do_change_uses_last_mesh_and_original_property_markers() {
        let mut b = fixture(b"outside; p12z.;ignored");
        let changed = vehicle_crash_do_change(&mut b, 0x1000).unwrap();
        assert_eq!(
            changed,
            VehicleCrashChange {
                change_filename: false,
                ancestor_depth: 12,
                actor_mask: 0
            }
        );
        assert_eq!(b.memory.allocations().count(), 0);
        assert!(b.effects.is_empty());
    }
    #[test]
    fn model_crash_keeps_original_even_when_actor_factory_returns_false() {
        let mut b = fixture(b";M25I2;");
        assert!(
            vehicle_create_crash_model(&mut b, 0x1000, [1., 2., 3.], [4., 5., 6.], 2.).unwrap()
        );
        assert_eq!(b.effects, ["collisions_off", "actors", "on", "debustize"]);
        let call = b.call.take().unwrap();
        assert_eq!(call.filename, b"synthetic_c1.i3d");
        assert_eq!(call.point, [1., 2., 3.]);
        assert_eq!(call.velocity, [4., 5., 6.]);
        assert!(call.change_filename);
        assert_eq!(call.actor_mask, 1);
        assert_eq!(call.ancestor_depth, 0);
        let kept = b.read_word(0x8014).unwrap();
        assert_eq!(b.read_word(kept).unwrap(), 0x1000);
        assert_eq!(b.read_word(0x10ac).unwrap(), 0);
        assert_eq!(b.memory.allocations().count(), 1);
    }
    #[test]
    fn null_or_model_less_frames_do_not_touch_game_registries() {
        let mut b = fixture(b"");
        assert!(!vehicle_create_crash_model(&mut b, 0, [0.; 3], [0.; 3], 2.).unwrap());
        b.write_word(0x1110, 1).unwrap();
        assert!(!vehicle_create_crash_model(&mut b, 0x1000, [0.; 3], [0.; 3], 2.).unwrap());
        assert!(b.effects.is_empty());
        assert_eq!(b.memory.allocations().count(), 0);
    }
}
