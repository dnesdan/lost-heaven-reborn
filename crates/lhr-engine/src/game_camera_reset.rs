//! Borrowed PC G_Camera::Reset over the actual C_game allocation.
//!
//! mafia-re f42a25d, game/camera/g_camera.cpp, Game.exe 0x5ed040.
//! G_Camera and cameraParams are PS2 names cross-checked to this PC entry;
//! offsets here are PC evidence, never inferred from the PS2 member layout.
//! This storage adapter does not create a second camera or Scene owner.

pub const CAMERA_BYTES: usize = 0xa0;
const FIRST_CAMERA_VALUES: u32 = 0x638708;

#[derive(Debug, PartialEq, Eq)]
pub enum CameraResetError<E> {
    UnavailableOwner,
    Profile(E),
}

/// Read the setting when Reset reaches it. The source is the live load/save
/// owner, not a saved constructor input or a caller supplied camera default.
pub trait CameraProfile {
    type Error;
    fn camera_setting(&mut self) -> Result<u32, Self::Error>;
}

struct CameraStorage<'a> {
    bytes: &'a mut [u8],
}
impl CameraStorage<'_> {
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn set_word(&mut self, offset: usize, word: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    fn set_byte(&mut self, offset: usize, byte: u8) {
        self.bytes[offset] = byte;
    }
    fn clear_pushed_states(&mut self) {
        // std::vector<G_CameraPushPop>::clear: end = begin. The vector is
        // retained, including its capacity and pointed-to allocation.
        let begin = self.word(0x6c);
        self.set_word(0x70, begin);
    }
    fn before_profile_read(&mut self) {
        for offset in [4, 8, 0xc, 0] {
            self.set_word(offset, 0);
        }
    }
    fn after_profile_read(&mut self, camera: u32, look_from_eyes: &mut u32) {
        self.set_word(0x10, camera);
        // Original reads its just-written member for this second store.
        self.set_word(0x14, self.word(0x10));
        for offset in [0x20, 0x21, 0x22] {
            self.set_byte(offset, 0);
        }
        self.set_word(0x24, 5.0f32.to_bits());
        self.set_word(0x28, 0.5f32.to_bits());
        for offset in [0x2c, 0x30] {
            self.set_word(offset, 0);
        }
        self.set_byte(0x58, 0);
        self.set_word(0x60, 0);
        self.clear_pushed_states();
        self.set_word(0x8c, 0);
        self.set_word(0x90, 0.2f32.to_bits());
        self.set_word(0x1c, FIRST_CAMERA_VALUES);
        self.set_byte(0x94, 0);
        self.clear_pushed_states();
        *look_from_eyes = 0.0f32.to_bits();
    }
}

/// PC Reset on a borrowed embedded camera. A profile dependency failure leaves
/// the first four stores visible. Only an actual owned 0xa0-byte camera slice
/// is admitted; `MissionComponent` supplies that slice from its Game allocation.
pub fn reset<P: CameraProfile>(
    camera: &mut [u8],
    profile: &mut P,
    look_from_eyes: &mut u32,
) -> Result<(), CameraResetError<P::Error>> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraResetError::UnavailableOwner);
    }
    let mut view = CameraStorage { bytes: camera };
    view.before_profile_read();
    let choice = profile
        .camera_setting()
        .map_err(CameraResetError::Profile)?;
    view.after_profile_read(choice, look_from_eyes);
    Ok(())
}

/// Constructor-only value path: the caller has already obtained the live
/// setting before entering this embedded constructor. This has no backend
/// callback boundary; later Reset calls use `reset` above.
pub(crate) fn reset_from_value(camera: &mut [u8], choice: u32, look_from_eyes: &mut u32) {
    assert_eq!(camera.len(), CAMERA_BYTES);
    let mut view = CameraStorage { bytes: camera };
    view.before_profile_read();
    view.after_profile_read(choice, look_from_eyes);
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Profile {
        value: Result<u32, &'static str>,
        reads: usize,
    }
    impl CameraProfile for Profile {
        type Error = &'static str;
        fn camera_setting(&mut self) -> Result<u32, Self::Error> {
            self.reads += 1;
            self.value
        }
    }
    fn word(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }
    #[test]
    fn reset_reads_live_profile_and_keeps_vector_capacity_and_unknown_fields() {
        let mut camera = [0xa5; CAMERA_BYTES];
        camera[0x6c..0x70].copy_from_slice(&0x1234u32.to_le_bytes());
        camera[0x70..0x74].copy_from_slice(&0x5678u32.to_le_bytes());
        camera[0x74..0x78].copy_from_slice(&0x9abcu32.to_le_bytes());
        let mut look = 0x8888_8888;
        let mut profile = Profile {
            value: Ok(7),
            reads: 0,
        };
        reset(&mut camera, &mut profile, &mut look).unwrap();
        assert_eq!(profile.reads, 1);
        assert_eq!((word(&camera, 0x10), word(&camera, 0x14)), (7, 7));
        assert_eq!(word(&camera, 0x1c), FIRST_CAMERA_VALUES);
        assert_eq!((word(&camera, 0x70), word(&camera, 0x74)), (0x1234, 0x9abc));
        assert_eq!(word(&camera, 0x90), 0.2f32.to_bits());
        assert_eq!(camera[0x18], 0xa5);
        assert_eq!(camera[0x34], 0xa5);
        assert_eq!(look, 0);
        profile.value = Ok(9);
        reset(&mut camera, &mut profile, &mut look).unwrap();
        assert_eq!((word(&camera, 0x10), word(&camera, 0x14)), (9, 9));
        assert_eq!(profile.reads, 2);
    }
    #[test]
    fn failed_profile_read_retains_only_the_four_initial_stores() {
        let mut camera = [0xa5; CAMERA_BYTES];
        let mut look = 0xfeed_face;
        let mut profile = Profile {
            value: Err("profile unavailable"),
            reads: 0,
        };
        assert_eq!(
            reset(&mut camera, &mut profile, &mut look),
            Err(CameraResetError::Profile("profile unavailable"))
        );
        let mut expected = [0xa5; CAMERA_BYTES];
        for offset in [4, 8, 0xc, 0] {
            expected[offset..offset + 4].fill(0);
        }
        assert_eq!(camera, expected);
        assert_eq!(look, 0xfeed_face);
    }
}
