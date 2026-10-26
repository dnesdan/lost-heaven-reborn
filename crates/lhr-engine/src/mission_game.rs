//! C_game constructor (Game RVA0x1472f0), destructor0x147a60,
//! and constructor-empty raw member ownership.
//! Game SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Unknown fields retain their seed; vector controls distinguish caller stack
//! residue from embedded constructors' saved-this identity bytes.
#[path = "game_camera_reset.rs"]
pub(super) mod camera;
pub use camera::{CameraProfile, CameraResetError};
#[derive(Clone, Copy, Debug)]
pub struct GameConstructorInputs {
    pub vector_control: u8,
    pub embedded_4c_control: u8,
}
/// Original shared global fields touched by the constructor's helper5ed040.
#[derive(Clone, Copy, Debug)]
pub struct GameConstructorGlobals {
    pub word_6d4b1c: u32,
    pub word_6bdb10: u32,
}
fn put(bytes: &mut [u8], at: usize, x: u32) {
    bytes[at..at + 4].copy_from_slice(&x.to_le_bytes());
}
fn vector(bytes: &mut [u8], at: usize, control: u8) {
    bytes[at] = control;
    for n in [4, 8, 12] {
        put(bytes, at + n, 0);
    }
}
pub(crate) fn construct(
    bytes: &mut [u8],
    identity: u32,
    inputs: GameConstructorInputs,
    globals: &mut GameConstructorGlobals,
) {
    for at in [
        0, 0x10, 0x20, 0x30, 0xec, 0xfc, 0x110, 0x120, 0x130, 0x140, 0x150, 0x168, 0x17c, 0x1a4,
        0x1b4, 0x1c4, 0x1d4, 0x1e4, 0x1f4, 0x208, 0x218, 0x240, 0x250, 0x280, 0x290, 0x2a0, 0x2c4,
        0x2d4, 0x2e4, 0x2f4, 0x314, 0x74c, 0x29ec, 0x29fc, 0x2adc, 0x2aec, 0x2afc, 0x2d68, 0x2d78,
        0x2e50, 0x2e60, 0x2e70, 0x2e80, 0x2e90, 0x2ee4, 0x2f18, 0x2f28, 0x2f40, 0x2f7c, 0x2f8c,
        0x2fa0, 0x2fc4, 0x3618, 0x3674, 0x3688,
    ] {
        vector(bytes, at, inputs.vector_control);
    }
    bytes[0x40] = 0;
    for at in [
        0x18c, 0x194, 0x26c, 0x2b0, 0x304, 0x2eb0, 0x2eb8, 0x2ebc, 0x2ec0, 0x2ec8, 0x2ecc, 0x2ed0,
        0x2ed4,
    ] {
        put(bytes, at, 0);
    }
    for at in [0x190, 0x198, 0x2b4, 0x308] {
        put(bytes, at, 200);
    }
    bytes[0x2ec4] = 0;
    put(bytes, 0x2bc, 0x624b50);
    put(bytes, 0x2c0, 0);
    for row in 0..100 {
        bytes[0x760 + row * 0x58..0x760 + row * 0x58 + 0x48].fill(0);
    }
    // Embedded helper5ecf90 constructs two vectors, then G_Camera::Reset
    // writes into that actual embedded owner and the global look variable.
    let at = 0x4c;
    vector(bytes, at + 0x68, inputs.embedded_4c_control);
    vector(bytes, at + 0x78, inputs.embedded_4c_control);
    camera::reset_from_value(
        &mut bytes[at..at + camera::CAMERA_BYTES],
        globals.word_6d4b1c,
        &mut globals.word_6bdb10,
    );
    // Inlined constructors at4a0320,5604f0 and499b40 read their own savedthis.
    let at = 0x2b18;
    let control = (identity.wrapping_add(at as u32) >> 24) as u8;
    vector(bytes, at, control);
    vector(bytes, at + 0x10, control);
    put(bytes, at + 0x20, 0);
    let at = 0x2d90;
    vector(bytes, at, (identity.wrapping_add(at as u32) >> 24) as u8);
    let at = 0x2da0;
    for n in [4, 0x14, 0x1c, 0x24] {
        put(bytes, at + n, 0);
    }
    for n in [8, 0x18, 0x20, 0x28] {
        put(bytes, at + n, 200);
    }
    for n in [0x3c, 0x4c, 0x5c, 0x6c, 0x7c, 0x8c, 0x9c] {
        vector(
            bytes,
            at + n,
            (identity.wrapping_add(at as u32) >> 24) as u8,
        );
    }
}
/// Empty owned member destruction. Populated stacks/embedded owners need their
/// real element destructors; no pointer is interpreted as a host allocation.
pub(crate) fn destroy_empty<E>(
    bytes: &mut [u8],
    mut free: impl FnMut(usize, u32) -> Result<(), E>,
) -> Result<(), E> {
    // InventoryStackClear, CharacterStackClear and FloatStackClear run first.
    for (begin, end) in [(0x2e54, 0x2e58), (0x2e64, 0x2e68), (0x2e74, 0x2e78)] {
        let value = u32::from_le_bytes(bytes[begin..begin + 4].try_into().unwrap());
        // The callback validates the owned empty-member admission before any
        // native element traversal, without manufacturing an allocator call.
        if value != 0 {
            free(begin, value)?;
        }
        put(bytes, end, value);
    }
    for offset in [
        0x368c, 0x3678, 0x361c, 0x2fc8, 0x2fa4, 0x2f90, 0x2f80, 0x2f44, 0x2f2c, 0x2f1c, 0x2ee8,
        0x2e94, 0x2e84, 0x2e74, 0x2e64, 0x2e54,
        // Embedded499c00 and412000, then the two preceding vectors.
        0x2e40, 0x2e30, 0x2e20, 0x2e10, 0x2e00, 0x2df0, 0x2de0, 0x2d94, 0x2d7c, 0x2d6c,
        // Embedded444670 (4a0320's two vectors).
        0x2b2c, 0x2b1c, 0x2b00, 0x2af0, 0x2ae0, 0x2a00, 0x29f0, 0x750, 0x318, 0x2f8, 0x2e8, 0x2d8,
        0x2c8, // Embedded499290 has a null+4 pointer, so no free here.
        0x2a4, 0x294, 0x284,
        // Embedded5479e0 and5483d0 each destroy their second vector first.
        0x254, 0x244, 0x21c, 0x20c, 0x1f8, 0x1e8, 0x1d8, 0x1c8, 0x1b8, 0x1a8, 0x180, 0x16c, 0x154,
        0x144, 0x134, 0x124, 0x114, 0x100, 0xf0,
        // Embedded5ed000, then the original first four vectors.
        0xc8, 0xb8, 0x34, 0x24, 0x14, 4,
    ] {
        if offset == 0x2a4 {
            put(bytes, 0x2bc, 0x624b50);
            let pointer = u32::from_le_bytes(bytes[0x2c0..0x2c4].try_into().unwrap());
            if pointer != 0 {
                free(0x2c0, pointer)?;
            }
            put(bytes, 0x2c0, 0);
        }
        let value = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        free(offset, value)?;
        for n in [0, 4, 8] {
            put(bytes, offset + n, 0);
        }
    }
    Ok(())
}
